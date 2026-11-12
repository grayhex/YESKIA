struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "yeskia-revival-{}",
            yeskia_core::workbench::local_id().unwrap()
        )))
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
use std::io::{Cursor, Write};
use yeskia_core::{
    preparation as p,
    revive::{self, Component, Target},
    workbench::Workbench,
};
fn pinned(raw: &[u8]) -> Component {
    let mut c = revive::spec("ssl-patch").unwrap();
    c.size = raw.len() as u64;
    c.sha256 = yeskia_core::fsutil::sha256_hex(raw);
    c
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}
#[test]
fn fixed_author_manifest_is_versioned_and_unknown_or_changed_downloads_are_blocked() {
    let m = revive::manifest().unwrap();
    assert_eq!(m.components.len(), 2);
    let mb = &m.components["mbedtls-lib"];
    assert_eq!((mb.size, mb.version.as_str()), (926820, "1.4.4"));
    assert_eq!(m.components["ssl-patch"].version, "21");
    assert_eq!(revive::manifest_version().len(), 64);
    assert!(revive::spec("future").is_err());
    assert!(revive::validate("ssl-patch", b"unreviewed new release").is_err());
    let raw = zip(&[("sys/bin/ssl.dll", b"dll")]);
    let c = pinned(&raw);
    assert!(revive::validate_against("ssl-patch", &c, &raw).is_ok());
    let mut other = c.clone();
    other.size += 1;
    assert!(revive::validate_against("ssl-patch", &other, &raw).is_err());
    let mut other = c;
    other.sha256 = "a".repeat(64);
    assert!(revive::validate_against("ssl-patch", &other, &raw).is_err());
}
#[test]
fn zip_validation_refuses_traversal_absolute_unexpected_missing_and_bombs_even_with_matching_pin() {
    for name in [
        "../ssl.dll",
        "sys/bin/../../ssl.dll",
        "/sys/bin/ssl.dll",
        "C:/sys/bin/ssl.dll",
        "sys\\bin\\ssl.dll",
        "sys/bin/other.dll",
        "sys/bin/./ssl.dll",
    ] {
        let raw = zip(&[(name, b"x")]);
        assert!(
            revive::validate_against("ssl-patch", &pinned(&raw), &raw).is_err(),
            "{name}"
        );
    }
    let raw = zip(&[("sys/bin/ssl.dll", b"dll"), ("readme.txt", b"extra")]);
    assert!(revive::validate_against("ssl-patch", &pinned(&raw), &raw).is_err());
    let huge = vec![0; revive::MAX_ZIP as usize + 1];
    let raw = zip(&[("sys/bin/ssl.dll", &huge)]);
    assert!(revive::validate_against("ssl-patch", &pinned(&raw), &raw).is_err());
    let raw = b"not zip";
    assert!(revive::validate_against("ssl-patch", &pinned(raw), raw).is_err());
}
#[test]
fn sis_identity_platform_version_and_crc_are_checked_after_pin() {
    let raw = include_bytes!("fixtures/pkg/sdk_probe_package.sis");
    let info = yeskia_core::pkg::sis::inspect(raw).unwrap();
    let mut c = revive::spec("mbedtls-lib").unwrap();
    c.size = raw.len() as u64;
    c.sha256 = yeskia_core::fsutil::sha256_hex(raw);
    c.version = info
        .package
        .version
        .unwrap()
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".");
    let package = c.package.as_mut().unwrap();
    package.uid = format!("0x{:08x}", info.package.uid);
    package.signed = info.signed;
    package.targets = info
        .dependencies
        .iter()
        .filter_map(|d| d.uid)
        .map(|v| format!("0x{v:08x}"))
        .collect();
    assert!(revive::validate_against("mbedtls-lib", &c, raw).is_ok());
    let mut bad = c.clone();
    bad.package.as_mut().unwrap().uid = "0xffffffff".into();
    assert!(revive::validate_against("mbedtls-lib", &bad, raw).is_err());
    let mut bad = c.clone();
    bad.version = "99.0.0".into();
    assert!(revive::validate_against("mbedtls-lib", &bad, raw).is_err());
    let mut bad = c.clone();
    bad.package
        .as_mut()
        .unwrap()
        .targets
        .push("0xffffffff".into());
    assert!(revive::validate_against("mbedtls-lib", &bad, raw).is_err());
    let mut broken = raw.to_vec();
    broken[12] ^= 1;
    let mut bad = c;
    bad.sha256 = yeskia_core::fsutil::sha256_hex(&broken);
    assert!(revive::validate_against("mbedtls-lib", &bad, &broken).is_err());
}
#[test]
fn platform_is_not_inferred_from_name_and_qt_eka1_or_conflicts_are_explicit() {
    let mut target = Target {
        model: "nokia-e72".into(),
        rm: "RM-530".into(),
        firmware: "054.005".into(),
        os: "9.3".into(),
        identity_confirmed: false,
    };
    let a = revive::applicability(&target, "system-native");
    assert_eq!(a.status, "manual");
    assert!(!a.device_verified && !a.automatic_apply && !a.firmware_verified);
    target.identity_confirmed = true;
    assert_eq!(revive::applicability(&target, "j2me").status, "documented");
    assert_eq!(
        revive::applicability(&target, "mbedtls-native").components,
        ["mbedtls-lib"]
    );
    assert_eq!(revive::applicability(&target, "qt").status, "unsupported");
    target.rm = "RM-999".into();
    assert_eq!(
        revive::applicability(&target, "system-native").status,
        "blocked"
    );
    target.model = "nokia-7610".into();
    target.os = "eka1".into();
    assert_eq!(
        revive::applicability(&target, "system-native").status,
        "unsupported"
    );
    for model in ["nokia-n8", "nokia-e7"] {
        target.model = model.into();
        target.os = "symbian3".into();
        let a = revive::applicability(&target, "system-native");
        assert_eq!(a.status, "manual");
        assert!(!a.device_verified);
    }
}
#[test]
fn tls_manual_reports_cannot_claim_proof_or_rollback_from_cache() {
    let tmp = Temp::new();
    let store = Workbench::open(tmp.path()).unwrap();
    let key = revive::progress_key("a", "054.005");
    assert!(revive::record_manual(&store, &key, 0, "ssl-patch", "applied-reported", true).is_err());
    assert!(
        revive::record_manual(&store, &key, 0, "mbedtls-lib", "device-verified", true).is_err()
    );
    assert!(
        revive::record_manual(&store, &key, 0, "snapshot", "installed-reported", true).is_err()
    );
    assert!(
        revive::record_manual(&store, &key, 0, "snapshot", "snapshot-reported", false).is_err()
    );
    let p = revive::record_manual(&store, &key, 0, "snapshot", "snapshot-reported", true).unwrap();
    let p = revive::record_manual(
        &store,
        &key,
        p.revision,
        "ssl-patch",
        "restored-reported",
        true,
    )
    .unwrap();
    assert!(p.value.transferred.is_empty());
    assert!(p.value.components["ssl-patch"]
        .source
        .contains("not-device-verified"));
    assert_ne!(key, revive::progress_key("b", "054.005"));
    assert_ne!(key, revive::progress_key("a", "091.004"));
    assert!(revive::clock_check("2026-02-29", "2026-11-11").is_err());
    assert!(revive::clock_check("2024-02-29", "2026-11-11")
        .unwrap()
        .contains("differs"));
}
#[test]
fn prep_context_changes_invalidate_proof_and_reports_exclude_private_inputs() {
    let tmp = Temp::new();
    let store = Workbench::open(tmp.path()).unwrap();
    let mut context = p::Context {
        fingerprint: "salted-private-id".into(),
        transport: "bluetooth".into(),
        driver_signature: "driver-v1".into(),
        mode: "standard".into(),
        profile: p::Profile::default(),
    };
    assert!(p::record_manual(&store, &context, 0, "network", p::Status::Verified).is_err());
    let first = p::record_manual(&store, &context, 0, "network", p::Status::Manual).unwrap();
    assert_eq!(first.revision, 1);
    p::record_device_proof(
        &store,
        &context,
        "files-backup",
        "completed-file-provider-read",
    )
    .unwrap();
    let old = context.key();
    context.profile.firmware = "091.004".into();
    assert_ne!(old, context.key());
    assert!(p::load(&store, &context).unwrap().value.steps.is_empty());
    context.profile.firmware = "054.005".into();
    let mut other = context.clone();
    other.driver_signature.push('2');
    assert_ne!(old, other.key());
    let mut other = context.clone();
    other.mode = "developer".into();
    assert_ne!(old, other.key());
    let mut other = context.clone();
    other.fingerprint = "second-phone".into();
    assert_ne!(old, other.key());
    context.transport = "usb".into();
    assert_ne!(old, context.key());
    context.transport = "bluetooth".into();
    context.profile.rm = "RM-531".into();
    assert_ne!(old, context.key());
    context.profile.rm = "RM-530".into();
    context.profile.os = "9.4".into();
    assert_ne!(old, context.key());
    let report = serde_json::to_string(&p::report(&context, &first.value)).unwrap();
    assert!(!report.contains("salted-private-id") && !report.contains("driver-v1"));
    assert!(!p::report(&context, &first.value).ready);
    let invalid: Result<p::Profile, _> = serde_json::from_str(
        r#"{"model":"nokia-e72","rm":"RM-530","firmware":"054.005","os":"9.3","APN":"private"}"#,
    );
    assert!(invalid.is_err());
}
#[test]
fn exported_tls_report_keeps_pins_and_manual_clock_without_identifiers_or_device_proof() {
    let tmp = Temp::new();
    let store = Workbench::open(tmp.path()).unwrap();
    let profile = p::Profile::default();
    let key = revive::progress_key(
        "private-device-fingerprint",
        &revive::profile_scope(&profile),
    );
    let clock = revive::record_clock(&store, &key, 0, "2026-11-12").unwrap();
    assert!(revive::record_clock(&store, &key, clock.revision, "2026-02-30").is_err());
    let report = revive::report(
        tmp.path(),
        &store,
        "private-device-fingerprint",
        profile.clone(),
        "system-native",
    )
    .unwrap();
    assert_eq!(report.progress.phone_clock.as_deref(), Some("2026-11-12"));
    assert!(!report.applicability.device_verified);
    let raw = serde_json::to_string(&report).unwrap();
    assert!(
        !raw.contains("private-device-fingerprint")
            && !raw.contains(&tmp.path().to_string_lossy().to_string())
    );
    assert!(raw.contains("d20d6d41c796d53f"));
    assert!(report.hardware_status.starts_with("not-tested"));
    assert!(revive::report(
        tmp.path(),
        &store,
        "other-device",
        p::Profile {
            firmware: "personal-secret".into(),
            ..profile
        },
        "system-native"
    )
    .is_err());
}
