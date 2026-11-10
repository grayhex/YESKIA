//! Пакеты (issue #8): golden-тесты на фикстурах SDK и сверка с поведением SymPort (Python),
//! враждебные входы, JAR/JAD, E32.

use serde_json::Value;
use std::io::Write;
use yeskia_core::inspector::InspectionVerdict;
use yeskia_core::pkg::inspect::{self, Format, Options, Provided, Target};
use yeskia_core::pkg::{cert, e32, jar, sis};

const UNSIGNED: &[u8] = include_bytes!("fixtures/pkg/sdk_probe_package.sis");
const SIGNED: &[u8] = include_bytes!("fixtures/pkg/sdk_probe_package_signed.sis");
const EXE: &[u8] = include_bytes!("fixtures/pkg/e32_probe_armv5t.exe");
const REFERENCE: &str = include_str!("golden/symport_python_reference.json");
/// 2026-11-10T00:00:00Z — момент, на который снят эталон Python.
const AT: i64 = 1_794_268_800;

fn reference() -> Value {
    serde_json::from_str(REFERENCE).unwrap()
}

fn opts(model: &str) -> Options {
    Options {
        target: Target {
            model: model.into(),
            ..Target::default()
        },
        at: AT,
        provided: vec![],
        jar: None,
    }
}

// ---------- сверка со SymPort ----------

#[test]
fn sis_structure_matches_symport_python_reference() {
    let r = reference();
    for (name, data) in [
        ("sdk_probe_package", UNSIGNED),
        ("sdk_probe_package_signed", SIGNED),
    ] {
        let want = &r[name]["sis"];
        let got = serde_json::to_value(sis::inspect(data).unwrap()).unwrap();
        for k in [
            "uid1",
            "uid2",
            "uid3",
            "size",
            "uid_checksum_ok",
            "controller_crc_ok",
            "data_crc_ok",
            "signed",
            "signature_count",
            "data_units",
        ] {
            assert_eq!(got[k], want[k], "{name}: {k}");
        }
        for k in [
            "uid",
            "uid_matches_header",
            "names",
            "vendor_unique_name",
            "vendor_names",
            "version",
        ] {
            assert_eq!(got["package"][k], want["package"][k], "{name}: package.{k}");
        }
        let (g, w) = (&got["files"][0], &want["files"][0]);
        for k in [
            "target",
            "mime_type",
            "hash_algorithm",
            "sha1",
            "operation",
            "length",
            "uncompressed_length",
            "file_index",
            "actual_sha1",
            "hash_ok",
        ] {
            assert_eq!(g[k], w[k], "{name}: files[0].{k}");
        }
        assert_eq!(
            got["files"].as_array().unwrap().len(),
            want["files"].as_array().unwrap().len()
        );
        assert_eq!(
            got["dependencies"].as_array().unwrap().len() as u64,
            want["dependencies"]["declared"].as_u64().unwrap()
        );
    }
}

#[test]
fn signatures_match_symport_python_reference() {
    let r = reference();
    for name in ["sdk_probe_package", "sdk_probe_package_signed"] {
        let data = if name.ends_with("signed") {
            SIGNED
        } else {
            UNSIGNED
        };
        let want = &r[name]["signatures"];
        let got = serde_json::to_value(cert::inspect_signatures(data, AT).unwrap()).unwrap();
        for k in ["status", "trust", "revocation", "phone_constraints", "at"] {
            assert_eq!(got[k], want[k], "{name}: {k}");
        }
        let (gc, wc) = (&got["chains"], &want["chains"]);
        assert_eq!(
            gc.as_array().unwrap().len(),
            wc.as_array().unwrap().len(),
            "{name}"
        );
        for (g, w) in gc.as_array().unwrap().iter().zip(wc.as_array().unwrap()) {
            for k in ["status", "phone_trust", "root_signature", "chain_links"] {
                assert_eq!(g[k], w[k], "{name}: chain.{k}");
            }
            for k in [
                "oid",
                "status",
                "legacy_sha1",
                "signed_bytes",
                "signed_sha256",
            ] {
                assert_eq!(
                    g["signatures"][0][k], w["signatures"][0][k],
                    "{name}: signature.{k}"
                );
            }
            for (gc, wc) in g["certificates"]
                .as_array()
                .unwrap()
                .iter()
                .zip(w["certificates"].as_array().unwrap())
            {
                for k in [
                    "subject",
                    "issuer",
                    "serial",
                    "sha256",
                    "not_before",
                    "not_after",
                    "validity",
                    "self_issued",
                    "extensions",
                ] {
                    assert_eq!(gc[k], wc[k], "{name}: cert.{k}");
                }
                // имя типа ключа отличается по форме записи: RSAPublicKey (Python) / RSA (YESKIA)
                assert_eq!(gc["key_type"], "RSA");
            }
        }
    }
}

#[test]
fn e32_header_matches_sdk_and_symport() {
    let e = e32::analyze(EXE).unwrap();
    assert_eq!(e.uid3, 3_758_098_440);
    assert_eq!(
        (e.kernel.as_str(), e.cpu.as_str(), e.abi.as_str()),
        ("eka2", "armv5", "eabi")
    );
    assert_eq!(
        (e.header_format.as_str(), e.compression.as_str()),
        ("V", "none")
    );
    assert_eq!(e.code_size, 168);
    assert_eq!(e.header_crc_ok, Some(true));
    assert!(e.uid_checksum_ok && !e.is_dll);
    assert_eq!(e.secure_id.as_deref(), Some("0xe0000808"));
    assert_eq!(e.capabilities, Some(vec![]));
    assert_eq!(e.imports, Some(vec![]));
}

// ---------- отчёт инспектора ----------

#[test]
fn unsigned_package_is_reported_unsigned_and_never_compatible() {
    let r = inspect::inspect_package("probe.sis", UNSIGNED, &opts("nokia-e52")).unwrap();
    assert_eq!(r.signatures.as_ref().unwrap().status, "unsigned");
    assert!(r.issues.contains(&"unsigned-package".to_string()));
    assert_eq!(r.compatibility, InspectionVerdict::Unknown);
    assert_eq!(
        (r.emulator, r.installation, r.physical_device),
        ("not-tested", "not-tested", "not-tested")
    );
    assert_eq!(r.binaries.len(), 1);
    assert_eq!(r.binaries[0].target, "!:\\sys\\bin\\probe.exe");
    // подписи нет — в отчёте нет «доверия» и «установлено»
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["schema"], "yeskia.inspector/1");
    assert_eq!(json["format"], "sis");
}

#[test]
fn signed_package_valid_signature_does_not_mean_trusted_or_compatible() {
    let r = inspect::inspect_package("probe.sisx", SIGNED, &opts("nokia-e52")).unwrap();
    let s = r.signatures.as_ref().unwrap();
    assert_eq!(s.status, "valid");
    assert_eq!(s.trust, "unknown");
    assert_eq!(s.chains[0].phone_trust, "unknown");
    assert_eq!(s.revocation, "not-checked");
    assert!(!r.issues.contains(&"invalid-certificate".to_string()));
    assert_eq!(
        r.compatibility,
        InspectionVerdict::Unknown,
        "математически верная подпись ≠ совместимо"
    );
    assert!(r.issues.contains(&"platform-undeclared".to_string()));
    assert_eq!(r.format, Format::Sisx);
}

#[test]
fn certificate_validity_is_checked_against_the_given_date_not_the_clock() {
    let before = 1_700_000_000; // 2023: сертификат (2026–2036) ещё не действует
    let after = 2_300_000_000; // 2042: уже истёк
    let r = inspect::inspect_package(
        "p.sis",
        SIGNED,
        &Options {
            at: before,
            ..opts("")
        },
    )
    .unwrap();
    assert!(r.issues.contains(&"certificate-not-yet-valid".to_string()));
    assert_eq!(
        r.signatures.as_ref().unwrap().status,
        "valid",
        "даты ≠ математика подписи"
    );
    let r = inspect::inspect_package(
        "p.sis",
        SIGNED,
        &Options {
            at: after,
            ..opts("")
        },
    )
    .unwrap();
    assert!(r.issues.contains(&"expired-certificate".to_string()));
    assert_eq!(r.signatures.as_ref().unwrap().status, "valid");
}

/// Смещение среза внутри буфера.
fn off(data: &[u8], part: &[u8]) -> usize {
    part.as_ptr() as usize - data.as_ptr() as usize
}

/// Пересчитывает CRC контроллера/данных так, чтобы подделка выглядела «целой».
fn fix_crcs(data: &mut [u8]) {
    let (cs, comp_range, ds, data_range) = {
        let mut root = sis::Cursor::at(data, 16);
        let contents = root.field(Some(sis::T_CONTENTS)).unwrap();
        let mut c = sis::Cursor::new(contents.payload);
        let cc = c.field(Some(sis::T_CONTROLLER_CHECKSUM)).unwrap();
        let dc = c.field(Some(sis::T_DATA_CHECKSUM)).unwrap();
        let comp = c.field(Some(sis::T_COMPRESSED)).unwrap();
        let d = c.field(Some(sis::T_DATA)).unwrap();
        (
            off(data, cc.payload),
            (off(data, comp.raw), comp.raw.len()),
            off(data, dc.payload),
            (off(data, d.raw), d.raw.len()),
        )
    };
    let crc_c = sis::crc16(&data[comp_range.0..comp_range.0 + comp_range.1]);
    let crc_d = sis::crc16(&data[data_range.0..data_range.0 + data_range.1]);
    data[cs..cs + 2].copy_from_slice(&crc_c.to_le_bytes());
    data[ds..ds + 2].copy_from_slice(&crc_d.to_le_bytes());
}

fn find(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).position(|w| w == needle).unwrap()
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

#[test]
fn tampering_with_recomputed_crc_is_caught_by_the_signature() {
    let mut d = SIGNED.to_vec();
    let at = find(&d, &utf16("Symbian research"));
    d[at] = b'T'; // «Tymbian research»
                  // без пересчёта — CRC контроллера ловит правку
    let r = inspect::inspect_package("t.sis", &d, &opts("")).unwrap();
    assert_eq!(r.integrity.as_ref().unwrap().controller_crc_ok, Some(false));
    assert!(r.issues.contains(&"corrupt-package".to_string()));
    // с пересчитанными CRC — целостность «верна», но подпись нет
    fix_crcs(&mut d);
    let r = inspect::inspect_package("t.sis", &d, &opts("")).unwrap();
    assert_eq!(r.integrity.as_ref().unwrap().controller_crc_ok, Some(true));
    assert_eq!(r.signatures.as_ref().unwrap().status, "invalid");
    assert!(r.issues.contains(&"invalid-certificate".to_string()));
    assert_eq!(r.compatibility, InspectionVerdict::Incompatible);
}

#[test]
fn changed_payload_with_valid_crc_is_caught_by_the_file_hash() {
    let mut d = UNSIGNED.to_vec();
    // начало E32-образа внутри данных: заголовок «EPOC» — правим байт после него
    let at = find(&d, b"EPOC") + 40;
    d[at] ^= 0xFF;
    fix_crcs(&mut d);
    let info = sis::inspect(&d).unwrap();
    assert_eq!(info.data_crc_ok, Some(true));
    assert_eq!(info.files[0].hash_ok, Some(false));
    let r = inspect::inspect_package("t.sis", &d, &opts("")).unwrap();
    assert!(r.issues.contains(&"corrupt-package".to_string()));
    assert_eq!(r.compatibility, InspectionVerdict::Incompatible);
}

#[test]
fn omitted_crc_fields_are_reported_as_not_checked_never_true() {
    // убираем CRC-поля: контейнер Contents = [checksum][checksum][compressed][data]
    let mut root = sis::Cursor::at(SIGNED, 16);
    let contents = root.field(Some(sis::T_CONTENTS)).unwrap();
    let mut c = sis::Cursor::new(contents.payload);
    c.field(None).unwrap();
    c.field(None).unwrap();
    let rest = &contents.payload[c.pos..];
    let mut out = SIGNED[..16].to_vec();
    out.extend_from_slice(&sis::T_CONTENTS.to_le_bytes());
    out.extend_from_slice(&(rest.len() as u32).to_le_bytes());
    out.extend_from_slice(rest);
    let info = sis::inspect(&out).unwrap();
    assert_eq!((info.controller_crc_ok, info.data_crc_ok), (None, None));
}

#[test]
fn unknown_uid_dependencies_stay_unresolved_never_satisfied() {
    // собираем пакет с зависимостью: используем независимо построенный писатель из тестового модуля
    let pkg = common::package_with_dependency(0xDEAD_BEEF, "Mystery Runtime", [1, 2, 3]);
    let r = inspect::inspect_package("dep.sis", &pkg, &opts("nokia-e52")).unwrap();
    let dep = r.dependencies.as_ref().unwrap();
    assert_eq!(dep.packages.len(), 1);
    assert_eq!(dep.packages[0].status, "required-unverified");
    assert_eq!(dep.packages[0].uid.as_deref(), Some("0xdeadbeef"));
    assert_eq!(dep.verdict, "unresolved");
    assert_eq!(dep.imports, "not-decoded");
    assert_eq!(r.compatibility, InspectionVerdict::Unknown);
    assert!(r.issues.contains(&"dependency-unverified".to_string()));

    // зависимость, которую даёт пакет библиотеки, — «в библиотеке», но не «установлено»
    let mut o = opts("nokia-e52");
    o.provided = vec![Provided {
        uid: 0xDEAD_BEEF,
        name: "rt".into(),
        version: Some([1, 2, 3]),
    }];
    let dep = inspect::inspect_package("dep.sis", &pkg, &o)
        .unwrap()
        .dependencies
        .unwrap();
    assert_eq!(dep.packages[0].status, "provided-in-library");
    assert_eq!(dep.verdict, "provided-in-library");

    o.provided = vec![Provided {
        uid: 0xDEAD_BEEF,
        name: "rt".into(),
        version: Some([1, 0, 0]),
    }];
    let r = inspect::inspect_package("dep.sis", &pkg, &o).unwrap();
    assert_eq!(r.dependencies.unwrap().verdict, "blocked");
    assert_eq!(r.compatibility, InspectionVerdict::Incompatible);
}

#[test]
fn platform_dependency_is_checked_against_the_selected_model() {
    let pkg = common::package_with_platform(0x1028_315F); // S60 5th
    let e52 = inspect::inspect_package("p.sis", &pkg, &opts("nokia-e52")).unwrap();
    assert_eq!(
        e52.dependencies.as_ref().unwrap().platform_verdict,
        "unsupported"
    );
    assert_eq!(e52.compatibility, InspectionVerdict::Incompatible);
    let n97 = inspect::inspect_package("p.sis", &pkg, &opts("nokia-n97-mini")).unwrap();
    assert_eq!(
        n97.dependencies.as_ref().unwrap().platform_verdict,
        "supported-static"
    );
    assert_eq!(
        n97.compatibility,
        InspectionVerdict::Unknown,
        "supported-static ≠ совместимо"
    );
    let none = inspect::inspect_package("p.sis", &pkg, &opts("")).unwrap();
    assert_eq!(
        none.dependencies.as_ref().unwrap().platform_verdict,
        "unknown"
    );
    let legacy = inspect::inspect_package("p.sis", &pkg, &opts("nokia-7610")).unwrap();
    assert_eq!(
        legacy.compatibility,
        InspectionVerdict::Unknown,
        "S60 2nd — правил нет"
    );
    let os = inspect::inspect_package(
        "p.sis",
        &pkg,
        &Options {
            target: Target {
                model: "nokia-n97-mini".into(),
                os: "9.2".into(),
                ..Target::default()
            },
            ..opts("")
        },
    )
    .unwrap();
    assert!(os.issues.contains(&"target-os-mismatch".to_string()));
}

// ---------- враждебные входы ----------

#[test]
fn truncated_and_bit_flipped_packages_never_panic() {
    for data in [UNSIGNED, SIGNED] {
        for n in 0..data.len() {
            let _ = inspect::inspect_package("t.sis", &data[..n], &opts("nokia-e52"));
        }
        let mut d = data.to_vec();
        for i in 0..d.len() {
            let keep = d[i];
            d[i] ^= 0x5A;
            let _ = inspect::inspect_package("t.sis", &d, &opts("nokia-e52"));
            d[i] = keep;
        }
    }
}

#[test]
fn garbage_and_wrong_formats_are_errors() {
    assert!(inspect::inspect_package("a.sis", b"", &opts("")).is_err());
    assert!(inspect::inspect_package("a.sis", &[0u8; 64], &opts("")).is_err());
    assert!(inspect::inspect_package("a.sis", b"PK\x03\x04 not sis", &opts("")).is_err());
    assert!(inspect::inspect_package("a.txt", SIGNED, &opts("")).is_err());
    assert!(inspect::inspect_package("a.jar", SIGNED, &opts("")).is_err());
}

#[test]
fn oversized_declared_lengths_are_rejected_without_allocation() {
    // Contents с длиной 0x7FFFFFFF
    let mut d = SIGNED[..16].to_vec();
    d.extend_from_slice(&sis::T_CONTENTS.to_le_bytes());
    d.extend_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
    assert!(sis::inspect(&d).is_err());
    // 64-битные длины не поддерживаются
    let mut d = SIGNED[..16].to_vec();
    d.extend_from_slice(&sis::T_CONTENTS.to_le_bytes());
    d.extend_from_slice(&0x8000_0010u32.to_le_bytes());
    assert!(sis::inspect(&d).is_err());
}

#[test]
fn decompression_limits_hold() {
    // заявленный размер выше предела
    let mut p = 1u32.to_le_bytes().to_vec();
    p.extend_from_slice(&(sis::MAX_UNCOMPRESSED + 1).to_le_bytes());
    assert!(sis::decompress(&p).is_err());
    // zlib-«бомба»: заявлено 10 байт, фактически — мегабайты нулей
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(&vec![0u8; 8 * 1024 * 1024]).unwrap();
    let body = enc.finish().unwrap();
    let mut p = 1u32.to_le_bytes().to_vec();
    p.extend_from_slice(&10u64.to_le_bytes());
    p.extend_from_slice(&body);
    assert!(sis::decompress(&p).is_err());
    // неизвестный алгоритм и «хранится» с неверной длиной
    let mut p = 7u32.to_le_bytes().to_vec();
    p.extend_from_slice(&0u64.to_le_bytes());
    assert!(sis::decompress(&p).is_err());
    let mut p = 0u32.to_le_bytes().to_vec();
    p.extend_from_slice(&5u64.to_le_bytes());
    p.extend_from_slice(b"abc");
    assert!(sis::decompress(&p).is_err());
}

#[test]
fn diagnosis_is_heuristic_and_bounded() {
    assert_eq!(
        inspect::diagnose("Certificate has expired").unwrap().issue,
        "expired-certificate"
    );
    assert_eq!(
        inspect::diagnose("Сертификат недействителен")
            .unwrap()
            .issue,
        "invalid-certificate"
    );
    assert_eq!(inspect::diagnose("???").unwrap().issue, "unknown");
    assert!(inspect::diagnose(&"x".repeat(2001)).is_err());
}

// ---------- JAR / JAD ----------

fn make_jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default();
    for (n, b) in entries {
        w.start_file(*n, o).unwrap();
        w.write_all(b).unwrap();
    }
    w.finish().unwrap().into_inner()
}

const MANIFEST_STR: &str = "Manifest-Version: 1.0\r\nMIDlet-Name: Демо\r\nMIDlet-Version: 1.0\r\nMIDlet-Vendor: Автор\r\nMIDlet-1: Демо,,Main\r\nMicroEdition-Profile: MIDP-2.0\r\nMicroEdition-Configuration: CLDC-1.1\r\n";
const MANIFEST: &[u8] = MANIFEST_STR.as_bytes();

#[test]
fn jar_manifest_and_jad_pairing() {
    let jar_bytes = make_jar(&[
        ("META-INF/MANIFEST.MF", MANIFEST),
        ("Main.class", b"\xCA\xFE\xBA\xBE"),
    ]);
    let info = jar::inspect_jar(&jar_bytes).unwrap();
    assert_eq!(
        (info.midp.as_str(), info.cldc.as_str()),
        ("MIDP-2.0", "CLDC-1.1")
    );
    assert_eq!(info.manifest["MIDlet-Name"], "Демо");
    let jad = format!(
        "MIDlet-Name: Демо\r\nMIDlet-Version: 1.0\r\nMIDlet-Vendor: Автор\r\nMIDlet-Jar-URL: http://x/app.jar?v=1\r\nMIDlet-Jar-Size: {}\r\n",
        jar_bytes.len()
    );
    let ok = jar::inspect_jad(jad.as_bytes(), Some(("app.jar", &jar_bytes))).unwrap();
    assert!(ok.paired);
    assert!(jar::inspect_jad(jad.as_bytes(), Some(("other.jar", &jar_bytes))).is_err());
    assert!(jar::inspect_jad(
        jad.as_bytes(),
        Some(("app.jar", &jar_bytes[..jar_bytes.len() - 1]))
    )
    .is_err());
    assert!(!jar::inspect_jad(jad.as_bytes(), None).unwrap().paired);
    let wrong = jad.replace("Автор", "Другой");
    assert!(jar::inspect_jad(wrong.as_bytes(), Some(("app.jar", &jar_bytes))).is_err());
    let r = inspect::inspect_package("app.jar", &jar_bytes, &opts("nokia-e52")).unwrap();
    assert_eq!(r.compatibility, InspectionVerdict::Unknown);
    assert!(r.midlet.is_some());
}

#[test]
fn bad_jars_are_rejected() {
    let m = ("META-INF/MANIFEST.MF", MANIFEST);
    assert!(jar::inspect_jar(b"not a zip").is_err());
    assert!(jar::inspect_jar(&make_jar(&[m])).is_err(), "нет классов");
    assert!(
        jar::inspect_jar(&make_jar(&[("A.class", b"x")])).is_err(),
        "нет манифеста"
    );
    assert!(jar::inspect_jar(&make_jar(&[m, ("../evil.class", b"x")])).is_err());
    assert!(jar::inspect_jar(&make_jar(&[m, ("/abs.class", b"x")])).is_err());
    let dup = b"MIDlet-Name: A\r\nMIDlet-Name: B\r\nMIDlet-Version: 1\r\nMIDlet-Vendor: V\r\nMIDlet-1: A,,M\r\n";
    assert!(jar::inspect_jar(&make_jar(&[
        ("META-INF/MANIFEST.MF", dup),
        ("a.class", b"x")
    ]))
    .is_err());
    let missing = b"MIDlet-Name: A\r\n";
    assert!(jar::inspect_jar(&make_jar(&[
        ("META-INF/MANIFEST.MF", missing),
        ("a.class", b"x")
    ]))
    .is_err());
}

#[test]
fn corrupted_jar_entry_is_detected() {
    let mut j = make_jar(&[
        ("META-INF/MANIFEST.MF", MANIFEST),
        ("Main.class", &vec![7u8; 4000]),
    ]);
    // портим данные записи (stored/deflate — CRC не сойдётся)
    let at = j.windows(4).position(|w| w == b"Main").unwrap() + 20;
    j[at] ^= 0xFF;
    assert!(jar::inspect_jar(&j).is_err());
}

mod common {
    //! Небольшой писатель SIS для тестов: контейнер с зависимостью/платформой, без подписи.
    use yeskia_core::pkg::sis::{self, crc16, uid_checksum};

    fn u32b(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }
    fn field(t: u32, payload: &[u8]) -> Vec<u8> {
        let mut o = u32b(t);
        o.extend(item(payload));
        o
    }
    fn item(payload: &[u8]) -> Vec<u8> {
        let mut o = u32b(payload.len() as u32);
        o.extend_from_slice(payload);
        o.extend(std::iter::repeat(0u8).take((4 - payload.len() % 4) % 4));
        o
    }
    fn array(t: u32, elems: &[Vec<u8>]) -> Vec<u8> {
        let mut p = u32b(t);
        for e in elems {
            p.extend(item(e));
        }
        field(sis::T_ARRAY, &p)
    }
    fn u16s(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }
    fn version(v: [i32; 3]) -> Vec<u8> {
        let mut o = Vec::new();
        for x in v {
            o.extend(x.to_le_bytes());
        }
        o
    }
    fn dependency(uid: u32, name: &str, from: [i32; 3]) -> Vec<u8> {
        let mut d = field(sis::T_UID, &u32b(uid));
        let mut range = field(sis::T_VERSION, &version(from));
        if from != [0, 0, 0] {
            range.extend(field(sis::T_VERSION, &version([i32::MAX, 0, 0])));
        }
        d.extend(field(sis::T_VERSION_RANGE, &range));
        d.extend(array(sis::T_STRING, &[u16s(name)]));
        d
    }

    fn build(prereq_target: Option<Vec<Vec<u8>>>, prereq_dep: Option<Vec<Vec<u8>>>) -> Vec<u8> {
        let uid3 = 0xE000_0A01u32;
        let mut info = field(sis::T_UID, &u32b(uid3));
        info.extend(field(sis::T_STRING, &u16s("Vendor")));
        info.extend(array(sis::T_STRING, &[u16s("Test")]));
        info.extend(array(sis::T_STRING, &[u16s("Vendor")]));
        info.extend(field(sis::T_VERSION, &{
            let mut v = Vec::new();
            for x in [1u32, 0, 0] {
                v.extend(x.to_le_bytes());
            }
            v
        }));
        let mut ctrl = field(sis::T_INFO, &info);
        if prereq_target.is_some() || prereq_dep.is_some() {
            let mut pre = array(sis::T_DEPENDENCY, &prereq_target.unwrap_or_default());
            pre.extend(array(sis::T_DEPENDENCY, &prereq_dep.unwrap_or_default()));
            ctrl.extend(field(sis::T_PREREQUISITES, &pre));
        }
        ctrl.extend(field(
            sis::T_INSTALL_BLOCK,
            &array(sis::T_FILE_DESCRIPTION, &[]),
        ));
        let controller = field(sis::T_CONTROLLER, &ctrl);
        let mut comp = u32b(0);
        comp.extend((controller.len() as u64).to_le_bytes());
        comp.extend(&controller);
        let comp_f = field(sis::T_COMPRESSED, &comp);
        let data_f = field(
            sis::T_DATA,
            &array(sis::T_DATA_UNIT, &[array(sis::T_FILE_DATA, &[])]),
        );
        let mut contents = field(sis::T_CONTROLLER_CHECKSUM, &crc16(&comp_f).to_le_bytes());
        contents.extend(field(sis::T_DATA_CHECKSUM, &crc16(&data_f).to_le_bytes()));
        contents.extend(&comp_f);
        contents.extend(&data_f);
        let mut out = Vec::new();
        out.extend(u32b(sis::SIS_UID1));
        out.extend(u32b(0));
        out.extend(u32b(uid3));
        let head = out.clone();
        out.extend(u32b(uid_checksum(&head).unwrap()));
        out.extend(field(sis::T_CONTENTS, &contents));
        out
    }

    pub fn package_with_dependency(uid: u32, name: &str, from: [i32; 3]) -> Vec<u8> {
        build(None, Some(vec![dependency(uid, name, from)]))
    }
    pub fn package_with_platform(uid: u32) -> Vec<u8> {
        build(Some(vec![dependency(uid, "Platform", [0, 0, 0])]), None)
    }
}
