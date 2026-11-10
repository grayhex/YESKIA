#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
use yeskia_core::{personal, Error};
#[test]
fn reader_returns_private_rows_rejects_bad_output_and_times_out() {
    let dir = std::env::temp_dir().join(format!("yeskia-pim-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("worker");
    let config = dir.join("profile");
    fs::write(&config, "test").unwrap();
    // Fixed fake worker stands in for the selected Python process; no phone required.
    let script = |body: &str| {
        fs::write(&exe, format!("#!/bin/sh\ncat >/dev/null\n{body}\n")).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    };
    script("printf '[{\"name\":\"Test\",\"phones\":[\"123\"]}]'");
    let rows = personal::read(&exe, &config, "contacts", Duration::from_secs(2)).unwrap();
    assert_eq!(rows[0]["name"], "Test");
    script("printf 'private error text'; exit 1");
    let error = personal::read(&exe, &config, "sms", Duration::from_secs(2)).unwrap_err();
    assert!(!error.to_string().contains("private error text"));
    script("printf '{}'");
    assert!(personal::read(&exe, &config, "calendar", Duration::from_secs(2)).is_err());
    script("exec sleep 10");
    assert!(matches!(
        personal::read(&exe, &config, "tasks", Duration::from_millis(100)),
        Err(Error::Timeout)
    ));
    // Cancellation uses a worker-created readiness marker, avoiding a timing guess.
    // Use the fixture directory explicitly for the marker instead of writing into the repository.
    let body = format!(
        "printf ready > '{}'; exec sleep 10",
        dir.join("started").display()
    );
    script(&body);
    let marker = dir.join("started");
    let canceller = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        personal::cancel();
    });
    assert!(matches!(
        personal::read(&exe, &config, "sms", Duration::from_secs(3)),
        Err(Error::Cancelled)
    ));
    canceller.join().unwrap();
    assert!(personal::read(&exe, &config, "send", Duration::from_secs(1)).is_err());
    fs::remove_dir_all(dir).unwrap();
}
