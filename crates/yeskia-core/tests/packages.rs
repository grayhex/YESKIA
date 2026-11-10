//! Библиотека пакетов и передача на телефон (issue #8).

use std::path::PathBuf;
use std::sync::Arc;
use yeskia_core::device::*;
use yeskia_core::diagnostics::Diagnostics;
use yeskia_core::engine::Engine;
use yeskia_core::error::ErrorCode;
use yeskia_core::library::{Library, Provenance, Query};
use yeskia_core::mock::{MockTransport, MOCK_DEVICE_ID};
use yeskia_core::operations::*;
use yeskia_core::pkg::inspect::Target;
use yeskia_core::storage::Storage;

const UNSIGNED: &[u8] = include_bytes!("fixtures/pkg/sdk_probe_package.sis");
const SIGNED: &[u8] = include_bytes!("fixtures/pkg/sdk_probe_package_signed.sis");
const AT: i64 = 1_794_268_800;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-pkg-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn lib(tag: &str) -> (Library, PathBuf) {
    let d = tmp(tag);
    (Library::open(&d).unwrap(), d)
}

fn prov(source: &str) -> Provenance {
    Provenance {
        source: source.into(),
        ..Provenance::default()
    }
}

fn engine() -> (Arc<Engine>, MockTransport, DeviceId) {
    let mock = MockTransport::new();
    let e = Engine::new(
        vec![Arc::new(mock.clone())],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    let id = DeviceId::new(MOCK_DEVICE_ID);
    e.connect(&id).unwrap();
    (Arc::new(e), mock, id)
}

// ---------- библиотека ----------

#[test]
fn import_hashes_dedupes_and_reads_metadata() {
    let (l, _d) = lib("import");
    let it = l.import("probe.sis", UNSIGNED, prov("тест")).unwrap();
    assert_eq!(it.id, yeskia_core::fsutil::sha256_hex(UNSIGNED));
    assert_eq!(it.status, "ok");
    assert_eq!(it.uid.as_deref(), Some("0xe0000809"));
    assert_eq!(it.title.as_deref(), Some("Symbian E32 Probe"));
    assert_eq!(it.vendor.as_deref(), Some("Symbian research"));
    assert_eq!(it.version.as_deref(), Some("1.0.0"));
    assert_eq!(it.signed, Some(false));
    assert_eq!(
        it.provenance.rights, "unknown",
        "права по умолчанию неизвестны"
    );
    // тот же файл под другим именем — одна запись, новое происхождение
    let again = l
        .import(
            "copy.sisx",
            UNSIGNED,
            Provenance {
                source: "карта".into(),
                rights: "user-confirmed".into(),
                ..Provenance::default()
            },
        )
        .unwrap();
    assert_eq!(again.id, it.id);
    assert_eq!(l.list(&Query::default()).unwrap().len(), 1);
    assert_eq!(
        l.get(&it.id, None).unwrap().provenance.rights,
        "user-confirmed"
    );
    let signed = l.import("signed.sis", SIGNED, prov("")).unwrap();
    assert_eq!(signed.signed, Some(true));
    assert_eq!(l.list(&Query::default()).unwrap().len(), 2);
}

#[test]
fn names_are_validated_and_paths_stripped() {
    let (l, d) = lib("names");
    let it = l.import("..\\..\\evil\\a.sis", UNSIGNED, prov("")).unwrap();
    assert_eq!(it.name, "a.sis", "путь отброшен, остаётся имя файла");
    assert!(
        l.import("con.sis", UNSIGNED, prov("")).is_err(),
        "зарезервированное имя Windows"
    );
    assert!(
        l.import("a.exe", UNSIGNED, prov("")).is_err(),
        "только пакеты"
    );
    assert!(l.import("a.sis", b"", prov("")).is_err());
    // на диске файл лежит под хешем, без расширения
    let files: Vec<_> = std::fs::read_dir(d.join("files"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(files, vec![it.id.clone()]);
    assert!(
        l.get("../../etc/passwd", None).is_err(),
        "id — только hex SHA-256"
    );
    assert!(l.remove("..").is_err());
}

#[test]
fn broken_files_are_kept_with_a_status_not_as_ok() {
    let (l, _d) = lib("broken");
    let junk = l
        .import("junk.sis", b"definitely not a sis file....", prov(""))
        .unwrap();
    assert_eq!(junk.status, "unreadable");
    assert!(junk.error.is_some());
    let mut bad = UNSIGNED.to_vec();
    let at = bad.windows(4).position(|w| w == b"EPOC").unwrap() + 40;
    bad[at] ^= 0xFF;
    let c = l.import("bad.sis", &bad, prov("")).unwrap();
    assert_eq!(c.status, "corrupt");
    // повреждённый и нечитаемый нельзя передать на телефон
    let dir = tmp("stage-bad");
    assert!(l.stage_for_transfer(&junk.id, &dir).is_err());
    assert!(l.stage_for_transfer(&c.id, &dir).is_err());
    // и они не считаются поставщиками зависимостей
    assert!(l.provided().unwrap().is_empty());
}

#[test]
fn search_and_filters() {
    let (l, _d) = lib("search");
    l.import("probe.sis", UNSIGNED, prov("")).unwrap();
    l.import("подписан.sis", SIGNED, prov("")).unwrap();
    let q = |t: &str| Query {
        text: t.into(),
        ..Query::default()
    };
    assert_eq!(l.list(&q("probe.sis")).unwrap().len(), 1);
    assert_eq!(
        l.list(&q("probe")).unwrap().len(),
        2,
        "и по названию пакета"
    );
    assert_eq!(
        l.list(&q("SYMBIAN research")).unwrap().len(),
        2,
        "по вендору, без учёта регистра"
    );
    assert_eq!(l.list(&q("подпис")).unwrap().len(), 1, "кириллица в имени");
    assert_eq!(l.list(&q("0xe0000809")).unwrap().len(), 2);
    assert_eq!(l.list(&q("нет такого")).unwrap().len(), 0);
    assert_eq!(
        l.list(&Query {
            format: Some("jar".into()),
            ..Query::default()
        })
        .unwrap()
        .len(),
        0
    );
}

#[test]
fn model_fit_uses_declared_platform_and_never_claims_installation() {
    let (l, _d) = lib("fit");
    let it = l.import("probe.sis", UNSIGNED, prov("")).unwrap();
    // фикстуры не объявляют платформу → fit неизвестен для любой модели
    let items = l
        .list(&Query {
            model: Some("nokia-e52".into()),
            ..Query::default()
        })
        .unwrap();
    assert_eq!(items[0].fit.as_deref(), Some("unknown"));
    assert_eq!(
        l.list(&Query {
            model: Some("nokia-e52".into()),
            only_fitting: true,
            ..Query::default()
        })
        .unwrap()
        .len(),
        0
    );
    assert_eq!(
        l.get(&it.id, None).unwrap().fit,
        None,
        "модель не выбрана — подгонки нет"
    );
    assert_eq!(
        l.list(&Query {
            model: Some("nokia-7610".into()),
            ..Query::default()
        })
        .unwrap()[0]
            .fit
            .as_deref(),
        Some("unknown")
    );
}

#[test]
fn sets_are_ordered_unique_and_cascade() {
    let (l, _d) = lib("sets");
    let a = l.import("a.sis", UNSIGNED, prov("")).unwrap();
    let b = l.import("b.sis", SIGNED, prov("")).unwrap();
    let s = l.create_set("  Мой набор  ").unwrap();
    assert_eq!(s.name, "Мой набор");
    assert!(
        l.create_set("Мой набор").is_err(),
        "имена наборов уникальны"
    );
    assert!(l.create_set("   ").is_err());
    l.set_add(s.id, &b.id).unwrap();
    l.set_add(s.id, &a.id).unwrap();
    l.set_add(s.id, &a.id).unwrap(); // повтор — без дубля
    assert_eq!(l.sets().unwrap()[0].items, vec![b.id.clone(), a.id.clone()]);
    assert_eq!(
        l.list(&Query {
            set: Some(s.id),
            ..Query::default()
        })
        .unwrap()
        .len(),
        2
    );
    assert!(l.set_add(9999, &a.id).is_err());
    l.remove(&a.id).unwrap();
    assert_eq!(
        l.sets().unwrap()[0].items,
        vec![b.id.clone()],
        "удалённый пакет уходит из наборов"
    );
    l.delete_set(s.id).unwrap();
    assert!(l.sets().unwrap().is_empty());
    assert!(
        l.get(&b.id, None).is_ok(),
        "удаление набора не удаляет пакеты"
    );
}

#[test]
fn tampered_library_file_is_detected_before_use() {
    let (l, d) = lib("tamper");
    let it = l.import("a.sis", UNSIGNED, prov("")).unwrap();
    assert!(l.bytes(&it.id).is_ok());
    let p = d.join("files").join(&it.id);
    let mut data = std::fs::read(&p).unwrap();
    data[20] ^= 1;
    std::fs::write(&p, data).unwrap();
    assert_eq!(
        l.bytes(&it.id).unwrap_err().code(),
        ErrorCode::VerificationFailed
    );
    assert!(l.inspect(&it.id, Target::default(), AT).is_err());
    assert!(l.stage_for_transfer(&it.id, &tmp("stage-t")).is_err());
}

#[test]
fn inspect_saves_report_and_uses_library_as_dependency_source() {
    let (l, _d) = lib("report");
    let it = l.import("a.sis", SIGNED, prov("")).unwrap();
    let t = Target {
        model: "nokia-e52".into(),
        ..Target::default()
    };
    let r = l.inspect(&it.id, t, AT).unwrap();
    assert_eq!(r.sha256, it.id);
    let json = l.report_json(&it.id, "nokia-e52").unwrap().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["schema"], "yeskia.inspector/1");
    assert_eq!(v["compatibility"], "unknown");
    assert_eq!(v["installation"], "not-tested");
    assert!(l.report_json(&it.id, "nokia-n95").unwrap().is_none());
    // «предоставляет» UID пакета
    let provided = l.provided().unwrap();
    assert_eq!(provided.len(), 1);
    assert_eq!(provided[0].uid, 0xE000_0809);
}

#[test]
fn library_persists_across_reopen() {
    let d = tmp("reopen");
    {
        let l = Library::open(&d).unwrap();
        let it = l.import("a.sis", UNSIGNED, prov("где-то")).unwrap();
        let s = l.create_set("Набор").unwrap();
        l.set_add(s.id, &it.id).unwrap();
    }
    let l = Library::open(&d).unwrap();
    let items = l.list(&Query::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].provenance.source, "где-то");
    assert_eq!(l.sets().unwrap()[0].items.len(), 1);
}

// ---------- передача на телефон ----------

fn staged(l: &Library, tag: &str, name: &str, data: &[u8]) -> (PathBuf, String) {
    let it = l.import(name, data, prov("")).unwrap();
    let (path, item) = l.stage_for_transfer(&it.id, &tmp(tag)).unwrap();
    (path, item.id)
}

fn transfer(source: &std::path::Path, path: &str, sha: &str) -> OperationRequest {
    OperationRequest::PackageTransfer {
        source: source.display().to_string(),
        path: path.into(),
        sha256: sha.into(),
    }
}

#[test]
fn transfer_writes_the_package_and_reads_it_back_without_claiming_installation() {
    let (l, _d) = lib("xfer");
    let (e, mock, id) = engine();
    let (src, sha) = staged(&l, "xfer-s", "probe.sis", UNSIGNED);
    let r = e
        .run(&id, transfer(&src, "E:/Install/probe.sis", &sha))
        .unwrap();
    assert_eq!(r.operation, Operation::PackagesTransfer);
    assert_eq!(r.outcome, Outcome::Success);
    match r.data.unwrap() {
        OperationData::Written {
            verified,
            sha256,
            bytes,
            replaced,
        } => {
            assert!(verified && !replaced);
            assert_eq!(sha256, sha);
            assert_eq!(bytes, UNSIGNED.len() as u64);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(mock.file("E:/Install/probe.sis").unwrap(), UNSIGNED);
    // установка — отдельная операция и её не было
    let m = e.capability_matrix(&id).unwrap();
    let install = m
        .entries
        .iter()
        .find(|x| x.operation == Operation::PackagesInstall)
        .unwrap();
    assert_eq!(install.availability, Availability::Unsupported);
}

#[test]
fn transfer_never_overwrites_and_rejects_foreign_extensions() {
    let (l, _d) = lib("xfer2");
    let (e, mock, id) = engine();
    let (src, sha) = staged(&l, "xfer2-s", "probe.sis", UNSIGNED);
    mock.put_file("E:/Install/probe.sis", b"old");
    let r = e
        .run(&id, transfer(&src, "E:/Install/probe.sis", &sha))
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::AlreadyExists);
    assert_eq!(mock.file("E:/Install/probe.sis").unwrap(), b"old");
    let err = e
        .submit(&id, transfer(&src, "E:/Install/probe.exe", &sha))
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    let err = e
        .submit(&id, transfer(&src, "E:/Install/probe.sis", "xyz"))
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    let err = e
        .submit(
            &id,
            transfer(std::path::Path::new("rel.sis"), "E:/a.sis", &sha),
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::UnsafePath);
}

#[test]
fn transfer_refuses_a_file_that_changed_after_confirmation() {
    let (l, _d) = lib("xfer3");
    let (e, mock, id) = engine();
    let (src, sha) = staged(&l, "xfer3-s", "probe.sis", UNSIGNED);
    let mut changed = UNSIGNED.to_vec();
    changed[30] ^= 1;
    std::fs::write(&src, changed).unwrap();
    let r = e
        .run(&id, transfer(&src, "E:/Install/probe.sis", &sha))
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::InvalidArgument);
    assert!(
        mock.file("E:/Install/probe.sis").is_none(),
        "на устройство ничего не ушло"
    );
    assert_eq!(mock.started_writes(), 0);
}

#[test]
fn transfer_is_unavailable_without_a_proven_write_capability() {
    let (l, _d) = lib("xfer4");
    let mock = MockTransport::new();
    mock.set_maturity(Operation::FilesWrite, SupportStatus::Unsupported);
    let e = Engine::new(
        vec![Arc::new(mock.clone())],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    let id = DeviceId::new(MOCK_DEVICE_ID);
    e.connect(&id).unwrap();
    let m = e.capability_matrix(&id).unwrap();
    let t = m
        .entries
        .iter()
        .find(|x| x.operation == Operation::PackagesTransfer)
        .unwrap();
    assert_eq!(t.availability, Availability::Unsupported);
    let (src, sha) = staged(&l, "xfer4-s", "probe.sis", UNSIGNED);
    let err = e
        .submit(&id, transfer(&src, "E:/Install/probe.sis", &sha))
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
    assert_eq!(mock.started_writes(), 0);
}
