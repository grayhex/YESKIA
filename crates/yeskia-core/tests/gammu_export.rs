#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use yeskia_core::{gammu_export::export, Error};

fn fixture(name: &str, script: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("yeskia-gammu-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("gammu");
    fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    let config = dir.join("config");
    fs::write(&config, "test").unwrap();
    (dir, exe, config)
}
fn count_scratch(dir: &Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".yeskia-export-")
        })
        .count()
}
#[test]
fn exports_contacts_and_refuses_overwrite() {
    let (dir, exe, config) = fixture(
        "contacts",
        "printf 'BEGIN:VCARD\\nFN:Test\\nEND:VCARD\\n' > \"$4\"",
    );
    let out = dir.join("contacts.vcf");
    export(&exe, &config, &out, "contacts", Duration::from_secs(2)).unwrap();
    assert!(fs::read_to_string(&out).unwrap().contains("FN:Test"));
    assert!(matches!(
        export(&exe, &config, &out, "contacts", Duration::from_secs(2)),
        Err(Error::AlreadyExists)
    ));
    assert_eq!(count_scratch(&dir), 0);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn invalid_output_and_failure_never_publish_a_file() {
    let (dir, exe, config) = fixture("invalid", "printf 'not a calendar' > \"$4\"");
    let out = dir.join("calendar.ics");
    assert!(export(&exe, &config, &out, "calendar", Duration::from_secs(2)).is_err());
    assert!(!out.exists());
    assert_eq!(count_scratch(&dir), 0);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn timeout_terminates_the_process_and_cleans_up() {
    let (dir, exe, config) = fixture("timeout", "exec sleep 10");
    let out = dir.join("calendar.ics");
    assert!(matches!(
        export(&exe, &config, &out, "calendar", Duration::from_millis(100)),
        Err(Error::Timeout)
    ));
    assert!(!out.exists());
    assert_eq!(count_scratch(&dir), 0);
    fs::remove_dir_all(dir).unwrap();
}
