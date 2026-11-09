use std::sync::Arc;
use std::time::Duration;
use yeskia_core::device::*;
use yeskia_core::diagnostics::Diagnostics;
use yeskia_core::error::ErrorCode;
use yeskia_core::mock::{MockTransport, MOCK_DEVICE_ID};
use yeskia_core::operations::*;
use yeskia_core::storage::{JournalState, Storage};
use yeskia_core::Engine;

fn setup() -> (Arc<Engine>, MockTransport) {
    let mock = MockTransport::new();
    let engine = Engine::new(
        vec![Arc::new(mock.clone())],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    (Arc::new(engine), mock)
}

fn id() -> DeviceId {
    DeviceId::new(MOCK_DEVICE_ID)
}

#[test]
fn mock_device_is_marked_and_serial_masked() {
    let (e, _) = setup();
    let d = &e.list_devices().unwrap()[0];
    assert_eq!(d.origin, DeviceOrigin::Mock);
    let json = serde_json::to_string(d).unwrap();
    assert!(!json.contains("356938035643809"), "serial leaked: {json}");
    assert!(!format!("{d:?}").contains("356938035643809"));
}

#[test]
fn run_requires_connection_and_succeeds_after() {
    let (e, _) = setup();
    assert_eq!(
        e.run(&id(), OperationRequest::Identify).unwrap_err().code(),
        ErrorCode::NotConnected
    );
    e.connect(&id()).unwrap();
    assert_eq!(
        e.connect(&id()).unwrap_err().code(),
        ErrorCode::AlreadyConnected
    );
    let r = e.run(&id(), OperationRequest::Identify).unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    assert_eq!(r.origin, DeviceOrigin::Mock);
    assert!(!r.evidence.is_empty());
}

#[test]
fn unsupported_capability_is_rejected() {
    let (e, _) = setup();
    let d = &e.list_devices().unwrap()[0];
    assert_eq!(
        d.capability(CapabilityId::PimRead).unwrap().status,
        SupportStatus::Unsupported
    );
}

#[test]
fn failed_write_is_not_retried() {
    let (e, mock) = setup();
    e.connect(&id()).unwrap();
    mock.fail_next_write();
    let r = e
        .run(
            &id(),
            OperationRequest::WriteFile {
                path: "E:/a.txt".into(),
                size: 10,
            },
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::OperationFailed);
    assert_eq!(mock.started_writes(), 1, "запись не должна повторяться");
    assert_eq!(mock.committed_writes(), 0);
    // устройство остаётся подключённым, ручной повтор работает
    let r2 = e
        .run(
            &id(),
            OperationRequest::WriteFile {
                path: "E:/a.txt".into(),
                size: 10,
            },
        )
        .unwrap();
    assert_eq!(r2.outcome, Outcome::Success);
    assert_eq!(mock.committed_writes(), 1);
}

#[test]
fn unplug_during_write_does_not_hang_or_commit() {
    let (e, mock) = setup();
    e.connect(&id()).unwrap();
    mock.set_write_profile(50, 20);
    let e2 = Arc::clone(&e);
    let h = std::thread::spawn(move || {
        e2.run(
            &id(),
            OperationRequest::WriteFile {
                path: "E:/big.bin".into(),
                size: 1 << 20,
            },
        )
    });
    std::thread::sleep(Duration::from_millis(100));
    mock.unplug();
    let r = h.join().unwrap().unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::DeviceDisconnected);
    assert_eq!(mock.committed_writes(), 0);
    assert_eq!(mock.started_writes(), 1);
    // соединение закрыто, повторная операция — NotConnected, не зависание
    assert!(e.list_devices().unwrap().is_empty());
    assert_eq!(
        e.run(&id(), OperationRequest::Identify).unwrap_err().code(),
        ErrorCode::DeviceNotFound
    );
}

#[test]
fn cancel_stops_write_without_commit() {
    let (e, mock) = setup();
    e.connect(&id()).unwrap();
    mock.set_write_profile(50, 20);
    let e2 = Arc::clone(&e);
    let h = std::thread::spawn(move || {
        e2.run(
            &id(),
            OperationRequest::WriteFile {
                path: "E:/big.bin".into(),
                size: 1,
            },
        )
    });
    std::thread::sleep(Duration::from_millis(100));
    let op = e.running_operation(&id()).expect("операция выполняется");
    assert!(e.cancel(&op));
    let r = h.join().unwrap().unwrap();
    assert_eq!(r.outcome, Outcome::Cancelled);
    assert_eq!(mock.committed_writes(), 0);
}

#[test]
fn second_operation_on_busy_device_is_rejected() {
    let (e, mock) = setup();
    e.connect(&id()).unwrap();
    mock.set_write_profile(50, 20);
    let e2 = Arc::clone(&e);
    let h = std::thread::spawn(move || {
        e2.run(
            &id(),
            OperationRequest::WriteFile {
                path: "E:/x".into(),
                size: 1,
            },
        )
    });
    std::thread::sleep(Duration::from_millis(100));
    let err = e.run(&id(), OperationRequest::Identify).unwrap_err();
    assert_eq!(err.code(), ErrorCode::OperationInProgress);
    let op = e.running_operation(&id()).unwrap();
    e.cancel(&op);
    h.join().unwrap().unwrap();
}

#[test]
fn explicit_disconnect_cancels_and_journal_records_outcome() {
    let (e, mock) = setup();
    e.connect(&id()).unwrap();
    mock.set_write_profile(50, 20);
    let e2 = Arc::clone(&e);
    let h = std::thread::spawn(move || {
        e2.run(
            &id(),
            OperationRequest::WriteFile {
                path: "E:/x".into(),
                size: 1,
            },
        )
    });
    std::thread::sleep(Duration::from_millis(100));
    e.disconnect(&id()).unwrap();
    let r = h.join().unwrap().unwrap();
    assert_eq!(r.outcome, Outcome::Cancelled);
    let j = e.storage().journal_recent(5).unwrap();
    assert_eq!(j[0].state, JournalState::Cancelled);
    assert_eq!(
        e.disconnect(&id()).unwrap_err().code(),
        ErrorCode::NotConnected
    );
}

#[test]
fn invalid_path_is_structured_error() {
    let (e, _) = setup();
    e.connect(&id()).unwrap();
    let r = e
        .run(
            &id(),
            OperationRequest::ListDirectory {
                path: "../x".into(),
            },
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::InvalidArgument);
}
