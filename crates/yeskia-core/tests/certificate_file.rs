use yeskia_core::pkg::cert::inspect_file;
#[test]
fn reads_public_certificate_and_checks_dates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pkg/standalone-test.pem");
    let cert = inspect_file(&path, 0).unwrap();
    assert!(cert.subject.contains("YESKIA test certificate"));
    assert_eq!(cert.validity, "not-yet-valid");
    assert_eq!(cert.sha256.len(), 64);
    assert_eq!(
        inspect_file(&path, 4_102_444_800).unwrap().validity,
        "expired"
    );
    assert!(inspect_file(&path.with_file_name("sdk_probe_package.sis"), 0).is_err());
}

#[test]
fn certificate_transfer_is_bound_to_the_inspected_bytes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pkg/standalone-test.pem");
    let cert = inspect_file(&path, 0).unwrap();
    assert!(yeskia_core::pkg::cert::checked_der(&path, &cert.sha256).is_ok());
    assert!(matches!(
        yeskia_core::pkg::cert::checked_der(&path, &"0".repeat(64)),
        Err(yeskia_core::Error::FingerprintMismatch)
    ));
}
