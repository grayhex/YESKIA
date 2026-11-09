use std::path::{Path, PathBuf};
use yeskia_discovery::fake::FakeTopology;
use yeskia_discovery::model::{StorageSummary, WpdListing};
use yeskia_lab::report::{self, Evidence, FindingEntry};
use yeskia_lab::{analyze, fixture, inventory, redact, scan};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn load_report(rel: &str) -> report::ExperimentReport {
    report::parse(&std::fs::read_to_string(repo_root().join(rel)).unwrap()).unwrap()
}

const CASE: &str = "docs/protocol/cases/CASE-0001-synthetic-enumerate-identify.report.json";

fn rules(r: &report::ExperimentReport) -> Vec<String> {
    report::validate(r)
        .into_iter()
        .map(|v| format!("{}: {}", v.path, v.rule))
        .collect()
}

// ---------- отчёт ----------

#[test]
fn the_synthetic_case_is_a_valid_v1_report() {
    let r = load_report(CASE);
    assert_eq!(r.schema, report::REPORT_SCHEMA);
    assert!(r.synthetic);
    assert_eq!(rules(&r), Vec::<String>::new());
}

#[test]
fn raw_captures_and_missing_sections_are_rejected() {
    let mut r = load_report(CASE);
    r.capture.raw_committed = true;
    r.known.clear();
    r.unknown.clear();
    r.next_experiment.clear();
    let v = rules(&r).join("\n");
    for needle in ["raw_committed", "known", "unknown", "next_experiment"] {
        assert!(v.contains(needle), "{needle}: {v}");
    }
}

#[test]
fn protocol_support_claims_need_adapter_verification() {
    let mut r = load_report(CASE);
    r.findings.push(FindingEntry {
        statement: "Протокол PC Suite поддерживается".into(),
        evidence: Evidence::Observed,
        adapter_ref: None,
    });
    assert!(
        rules(&r).iter().any(|x| x.contains("adapter-verified")),
        "{:?}",
        rules(&r)
    );

    // отрицание не считается заявлением
    let mut ok = load_report(CASE);
    ok.findings.push(FindingEntry {
        statement: "Поддержка PIM не подтверждена".into(),
        evidence: Evidence::Inferred,
        adapter_ref: None,
    });
    assert_eq!(rules(&ok), Vec::<String>::new());

    // синтетический отчёт не может быть adapter-verified, а реальный — без ссылки на адаптер
    let mut syn = load_report(CASE);
    syn.findings.push(FindingEntry {
        statement: "Идентификация работает".into(),
        evidence: Evidence::AdapterVerified,
        adapter_ref: Some("adapter:mtp/device-fp".into()),
    });
    assert!(rules(&syn).iter().any(|x| x.contains("синтетический")));
    let mut real = load_report(CASE);
    real.synthetic = false;
    real.redaction.reviewed_by_human = true;
    real.findings.push(FindingEntry {
        statement: "Идентификация работает".into(),
        evidence: Evidence::AdapterVerified,
        adapter_ref: None,
    });
    assert!(rules(&real).iter().any(|x| x.contains("adapter_ref")));
}

#[test]
fn private_data_and_placeholders_are_rejected_in_any_string() {
    let mut r = load_report(CASE);
    r.setup.firmware = "серийник: ABC123456789".into();
    r.known.push("IMEI 356938035643809".into());
    r.title = "TODO: название".into();
    let v = rules(&r).join("\n");
    assert!(
        v.contains("setup.firmware") && v.contains("приватные"),
        "{v}"
    );
    assert!(v.contains("known[2]"), "{v}");
    assert!(v.contains("TODO"), "{v}");
}

#[test]
fn real_reports_need_human_review_of_redaction() {
    let mut r = load_report(CASE);
    r.synthetic = false;
    assert!(rules(&r).iter().any(|x| x.contains("reviewed_by_human")));
    r.redaction.reviewed_by_human = true;
    r.capture.raw_stored_locally = true;
    assert_eq!(rules(&r), Vec::<String>::new());
}

#[test]
fn test_data_and_retention_rules() {
    let mut r = load_report(CASE);
    r.test_data.user_data_backed_up = false;
    r.capture.retention_days = 400;
    let v = rules(&r).join("\n");
    assert!(
        v.contains("test_data") && v.contains("retention_days"),
        "{v}"
    );
}

// ---------- реплей-тесты на синтетических фикстурах ----------

fn analysis_json(name: &str) -> serde_json::Value {
    let text =
        std::fs::read_to_string(repo_root().join("docs/protocol/fixtures").join(name)).unwrap();
    serde_json::to_value(analyze::analyze(&fixture::parse(&text).unwrap())).unwrap()
}

fn golden(name: &str) -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("docs/protocol/fixtures").join(name)).unwrap(),
    )
    .unwrap()
}

#[test]
fn replay_matches_golden_analysis() {
    assert_eq!(
        analysis_json("synthetic-session-a.jsonl"),
        golden("synthetic-session-a.analysis.golden.json")
    );
    assert_eq!(
        analysis_json("synthetic-session-b.jsonl"),
        golden("synthetic-session-b.analysis.golden.json")
    );
}

#[test]
fn replay_matches_golden_diff() {
    let load = |n: &str| {
        analyze::analyze(
            &fixture::parse(
                &std::fs::read_to_string(repo_root().join("docs/protocol/fixtures").join(n))
                    .unwrap(),
            )
            .unwrap(),
        )
    };
    let d = analyze::diff_sessions(
        &load("synthetic-session-a.jsonl"),
        &load("synthetic-session-b.jsonl"),
    );
    assert_eq!(
        serde_json::to_value(&d).unwrap(),
        golden("synthetic-a-vs-b.diff.golden.json")
    );
}

#[test]
fn analyzer_finds_exchanges_retries_resets_and_framing() {
    let load = |n: &str| {
        analyze::analyze(
            &fixture::parse(
                &std::fs::read_to_string(repo_root().join("docs/protocol/fixtures").join(n))
                    .unwrap(),
            )
            .unwrap(),
        )
    };
    let a = load("synthetic-session-a.jsonl");
    assert_eq!(a.exchanges.len(), 3);
    assert_eq!(a.exchanges[1].retries, 1);
    assert_eq!(a.exchanges[0].marker.as_deref(), Some("UI: connect"));
    assert_eq!(a.exchanges[1].latency_ms, Some(45));
    let fr = a.framing.expect("поле длины найдено");
    assert_eq!((fr.offset, fr.size, fr.matched, fr.total), (1, 1, 7, 7));

    let b = load("synthetic-session-b.jsonl");
    assert_eq!(b.resets_t_ms, vec![130]);
    assert!(b.exchanges[2].interrupted_by_reset);
}

#[test]
fn analyzer_never_claims_support_and_marks_everything_inferred() {
    for n in ["synthetic-session-a.jsonl", "synthetic-session-b.jsonl"] {
        let a = analysis_json(n);
        for h in a["hypotheses"].as_array().unwrap() {
            assert_eq!(h["evidence"], "inferred");
            let s = h["statement"].as_str().unwrap().to_lowercase();
            assert!(!s.contains("поддерж") && !s.contains("support"), "{s}");
        }
    }
}

#[test]
fn diff_flags_only_the_bytes_that_change_between_sessions() {
    let load = |n: &str| {
        analyze::analyze(
            &fixture::parse(
                &std::fs::read_to_string(repo_root().join("docs/protocol/fixtures").join(n))
                    .unwrap(),
            )
            .unwrap(),
        )
    };
    let d = analyze::diff_sessions(
        &load("synthetic-session-a.jsonl"),
        &load("synthetic-session-b.jsonl"),
    );
    assert_eq!(d.dynamic.len(), 2);
    assert_eq!(
        (
            d.dynamic[0].exchange,
            d.dynamic[0].side,
            d.dynamic[0].offsets.clone()
        ),
        (1, "response", vec![4, 5, 6, 7])
    );
    assert_eq!(
        (
            d.dynamic[1].exchange,
            d.dynamic[1].side,
            d.dynamic[1].offsets.clone()
        ),
        (2, "request", vec![5])
    );
}

#[test]
fn fixtures_with_bad_schema_or_hex_are_rejected() {
    assert!(fixture::parse("").is_err());
    assert!(fixture::parse(
        r#"{"schema":"x","synthetic":true,"channel":"usb_raw","description":"d"}"#
    )
    .is_err());
    let bad_hex = format!(
        "{}\n{}\n",
        r#"{"schema":"yeskia.lab.fixture/1","synthetic":true,"channel":"usb_raw","description":"d"}"#,
        r#"{"t_ms":1,"dir":"out","ep":"0x02","kind":"bulk","hex":"zz"}"#
    );
    assert!(fixture::parse(&bad_hex).is_err());
}

// ---------- scan ----------

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("yeskia-lab-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("docs/protocol/cases")).unwrap();
    d
}

#[test]
fn scan_passes_on_the_real_repository() {
    let root = repo_root();
    let mut p = scan::scan_forbidden_files(&root);
    p.extend(scan::scan_protocol_dir(&root, &root.join("docs/protocol")));
    assert!(p.is_empty(), "{p:?}");
}

#[test]
fn scan_rejects_raw_captures_private_text_real_fixtures_and_bad_reports() {
    let root = tmp("scan");
    std::fs::write(root.join("capture.pcapng"), b"x").unwrap();
    std::fs::write(
        root.join("docs/protocol/note.md"),
        "mac aa:bb:cc:dd:ee:ff и IMEI 356938035643809",
    )
    .unwrap();
    let real_fixture = format!(
        "{}\n",
        r#"{"schema":"yeskia.lab.fixture/1","synthetic":false,"channel":"usb_raw","description":"d"}"#
    );
    std::fs::write(root.join("docs/protocol/real.jsonl"), real_fixture).unwrap();
    std::fs::write(root.join("docs/protocol/cases/x.report.json"), "{}").unwrap();
    let mut p = scan::scan_forbidden_files(&root);
    p.extend(scan::scan_protocol_dir(&root, &root.join("docs/protocol")));
    let all = p
        .iter()
        .map(|x| format!("{} {}", x.file, x.detail))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(all.contains("capture.pcapng"), "{all}");
    assert!(
        all.contains("note.md") && all.contains("приватные"),
        "{all}"
    );
    assert!(
        all.contains("real.jsonl") && all.contains("синтетические"),
        "{all}"
    );
    assert!(
        all.contains("x.report.json") && all.contains("не отчёт"),
        "{all}"
    );
    // сами значения в сообщениях не раскрываются
    assert!(!all.contains("356938035643809") && !all.contains("aa:bb:cc"));
    let _ = std::fs::remove_dir_all(root);
}

// ---------- обезличивание кадров ----------

#[test]
fn fixture_payload_masking_hides_text_but_keeps_structure() {
    let mut data = vec![0x7E, 0x14, 0x01, 0x00];
    data.extend_from_slice(b"Ivan Petrov");
    data.extend_from_slice(&[0xAA; 4]);
    let masked = redact::mask_payload(&data, 4);
    assert_eq!(masked.len(), data.len());
    assert_eq!(&masked[..4], &data[..4]);
    assert!(!String::from_utf8_lossy(&masked).contains("Ivan"));
    assert_eq!(
        &masked[masked.len() - 4..],
        &[0xAA; 4],
        "бинарные поля сохранены для анализа framing"
    );
}

// ---------- инвентаризация ----------

#[test]
fn inventory_skeleton_is_private_and_not_valid_until_completed() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, "SER0123456789AB", "WinUSB");
    t.wpd(
        &usb,
        "Nokia E52",
        WpdListing::Ok {
            storages: vec![StorageSummary { object_count: 1 }],
        },
    );
    let r = inventory::skeleton_from_snapshot(&t.snap, "2026-11-09", |_| Ok("Nokia E52".into()));
    let json = serde_json::to_string(&r).unwrap();
    for secret in ["SER0123456789AB", "VID_0421", "WPDBUSENUM", "usb#vid"] {
        assert!(!json.contains(secret), "утечка {secret}: {json}");
    }
    assert_eq!(r.setup.usb_vid_pid, "0421:0223");
    assert!(r.findings.iter().all(|f| f.evidence == Evidence::Observed));
    assert!(r.findings.iter().any(|f| f.statement.contains("Nokia E52")));
    // пока человек не заполнил TODO и не проверил редакцию — отчёт не проходит проверку
    let v = rules(&r).join("\n");
    assert!(
        v.contains("TODO") && v.contains("reviewed_by_human") && v.contains("test_data"),
        "{v}"
    );
}

#[test]
fn inventory_without_a_device_says_so() {
    let t = FakeTopology::new();
    let r = inventory::skeleton_from_snapshot(&t.snap, "2026-11-09", |_| Err("n/a".into()));
    assert!(r.findings[0].statement.contains("не обнаружено"));
    assert!(r.unknown.len() >= 2);
}
