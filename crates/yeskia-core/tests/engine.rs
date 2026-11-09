use std::sync::Arc;
use std::time::{Duration, Instant};
use yeskia_core::device::*;
use yeskia_core::diagnostics::Diagnostics;
use yeskia_core::engine::{Engine, EngineConfig};
use yeskia_core::error::{ErrorCode, UserAction};
use yeskia_core::mock::{MockTransport, WriteFault, MOCK_DEVICE_ID};
use yeskia_core::operations::*;
use yeskia_core::storage::{JournalState, Storage};

fn setup_with(config: EngineConfig) -> (Arc<Engine>, MockTransport) {
    let mock = MockTransport::new();
    let engine = Engine::with_config(
        vec![Arc::new(mock.clone())],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(200, None)),
        config,
    );
    (Arc::new(engine), mock)
}

fn setup() -> (Arc<Engine>, MockTransport) {
    setup_with(EngineConfig::default())
}

fn id() -> DeviceId {
    DeviceId::new(MOCK_DEVICE_ID)
}

fn connected(config: EngineConfig) -> (Arc<Engine>, MockTransport) {
    let (e, m) = setup_with(config);
    e.connect(&id()).unwrap();
    (e, m)
}

fn write_req(path: &str, data: &[u8], overwrite: OverwritePolicy) -> OperationRequest {
    OperationRequest::FilesWrite {
        path: path.into(),
        data: data.to_vec(),
        overwrite,
    }
}

fn sleep(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

#[test]
fn mock_device_is_marked_serial_masked_and_has_no_matrix_before_connect() {
    let (e, _) = setup();
    let d = &e.list_devices().unwrap()[0];
    assert_eq!(d.origin, DeviceOrigin::Mock);
    assert!(
        d.matrix.is_none(),
        "перечисление не даёт матрицы возможностей"
    );
    let json = serde_json::to_string(d).unwrap();
    assert!(!json.contains("356938035643809"), "serial leaked: {json}");
    assert!(!format!("{d:?}").contains("356938035643809"));
}

// ---------- матрица возможностей ----------

#[test]
fn matrix_is_built_from_probe_not_from_model_name() {
    let (e, mock) = setup();
    mock.set_model("Nokia E52 (supports everything)");
    let d = e.connect(&id()).unwrap();
    let m = d.matrix.unwrap();
    assert_eq!(
        m.availability(Operation::IdentityRead),
        Availability::Available
    );
    assert_eq!(
        m.availability(Operation::FilesList),
        Availability::Available
    );
    assert_eq!(
        m.availability(Operation::FilesRead),
        Availability::Available
    );
    // запись read-only probe не проверяет
    assert_eq!(
        m.availability(Operation::FilesWrite),
        Availability::NotTested
    );
    // адаптер не умеет — название модели ничего не меняет
    assert_eq!(
        m.availability(Operation::ContactsRead),
        Availability::Unsupported
    );
    assert_eq!(
        m.availability(Operation::SmsSend),
        Availability::Unsupported
    );
    assert_eq!(
        m.availability(Operation::PackagesInstall),
        Availability::Unsupported
    );
    // evidence probe содержит источник, время и fingerprint
    let ev = &m.entry(Operation::FilesList).unwrap().evidence[0];
    assert_eq!(ev.source, "probe");
    assert_eq!(
        ev.device_fingerprint.as_deref(),
        Some(m.device_fingerprint.as_str())
    );
    assert!(ev.checked_at_ms > 0);
}

#[test]
fn probe_never_writes_to_the_device() {
    let (e, mock) = setup();
    let before = mock.file("E:/readme.txt");
    e.connect(&id()).unwrap();
    assert_eq!(mock.started_writes(), 0);
    assert_eq!(mock.committed_writes(), 0);
    assert_eq!(mock.file("E:/readme.txt"), before);
}

#[test]
fn probe_error_is_reported_as_error_availability() {
    let (e, mock) = setup();
    mock.set_probe_error(Operation::FilesRead);
    let m = e.connect(&id()).unwrap().matrix.unwrap();
    assert_eq!(m.availability(Operation::FilesRead), Availability::Error);
    assert!(m.entry(Operation::FilesRead).unwrap().detail.is_some());
}

#[test]
fn matrix_is_per_connection_and_reset_on_reconnect() {
    let (e, _) = connected(EngineConfig::default());
    e.run(&id(), write_req("E:/a.txt", b"x", OverwritePolicy::Never))
        .unwrap();
    assert_eq!(
        e.capability_matrix(&id())
            .unwrap()
            .availability(Operation::FilesWrite),
        Availability::Available
    );
    e.disconnect(&id()).unwrap();
    assert!(e.capability_matrix(&id()).is_none());
    let m = e.connect(&id()).unwrap().matrix.unwrap();
    assert_eq!(
        m.availability(Operation::FilesWrite),
        Availability::NotTested
    );
}

#[test]
fn successful_operation_upgrades_matrix_with_evidence() {
    let (e, _) = connected(EngineConfig::default());
    let r = e
        .run(
            &id(),
            write_req("E:/new.txt", b"data", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    let entry = e
        .capability_matrix(&id())
        .unwrap()
        .entry(Operation::FilesWrite)
        .unwrap()
        .clone();
    assert_eq!(entry.availability, Availability::Available);
    assert_eq!(
        entry.maturity,
        SupportStatus::TestedOnMock,
        "mock не даёт device-verified"
    );
    assert!(entry
        .evidence
        .iter()
        .any(|ev| ev.source == "operation:files.write"
            && ev.kind == EvidenceKind::MockTest
            && ev.device_fingerprint.is_some()));
}

#[test]
fn unsupported_operation_is_rejected_by_matrix() {
    let (e, _) = connected(EngineConfig::default());
    let err = e
        .submit(&id(), OperationRequest::ContactsRead)
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
}

#[test]
fn sending_a_package_file_is_not_an_install() {
    let (e, mock) = connected(EngineConfig::default());
    let r = e
        .run(
            &id(),
            write_req("E:/app.sisx", b"SIS", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    assert_eq!(mock.file("E:/app.sisx").as_deref(), Some(&b"SIS"[..]));
    let m = e.capability_matrix(&id()).unwrap();
    assert_eq!(
        m.availability(Operation::PackagesInstall),
        Availability::Unsupported
    );
}

// ---------- запись: безопасность и подтверждение ----------

#[test]
fn write_is_verified_by_readback() {
    let (e, mock) = connected(EngineConfig::default());
    let r = e
        .run(
            &id(),
            write_req("E:/v.txt", b"hello", OverwritePolicy::Never),
        )
        .unwrap();
    match r.data.unwrap() {
        OperationData::Written {
            bytes,
            sha256,
            verified,
            replaced,
        } => {
            assert_eq!(bytes, 5);
            assert!(verified && !replaced);
            assert_eq!(sha256.len(), 64);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(mock.file("E:/v.txt").as_deref(), Some(&b"hello"[..]));
    assert!(!r.uncertain);
}

#[test]
fn silent_overwrite_is_refused_and_replace_is_explicit() {
    let (e, mock) = connected(EngineConfig::default());
    let r = e
        .run(
            &id(),
            write_req("E:/readme.txt", b"NEW", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    let err = r.error.unwrap();
    assert_eq!(err.code, ErrorCode::AlreadyExists);
    assert_eq!(err.action, UserAction::ChooseAnotherPath);
    assert!(!r.uncertain, "до записи содержимое не менялось");
    assert_eq!(mock.started_writes(), 0);
    assert_eq!(
        mock.file("E:/readme.txt").as_deref(),
        Some(&b"hello yeskia"[..])
    );

    let r = e
        .run(
            &id(),
            write_req("E:/readme.txt", b"NEW", OverwritePolicy::Replace),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    assert!(matches!(
        r.data,
        Some(OperationData::Written { replaced: true, .. })
    ));
    assert_eq!(mock.file("E:/readme.txt").as_deref(), Some(&b"NEW"[..]));
}

#[test]
fn unsafe_paths_are_rejected_before_queueing() {
    let (e, mock) = connected(EngineConfig::default());
    for p in ["E:/../secret", "relative.txt", "E:\\x", "E:/a?b"] {
        let err = e
            .submit(&id(), write_req(p, b"x", OverwritePolicy::Never))
            .err()
            .unwrap();
        assert_eq!(err.code(), ErrorCode::UnsafePath, "{p}");
    }
    assert_eq!(mock.started_writes(), 0);
    assert!(
        e.storage().journal_recent(10).unwrap().is_empty(),
        "ничего не поставлено в очередь"
    );
}

#[test]
fn readback_mismatch_marks_write_uncertain() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_fault(WriteFault::CorruptOnCommit);
    let r = e
        .run(
            &id(),
            write_req("E:/c.bin", b"payload", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(
        r.error.as_ref().unwrap().code,
        ErrorCode::VerificationFailed
    );
    assert!(r.uncertain);
    assert_eq!(r.error.unwrap().action, UserAction::VerifyDeviceContents);
}

#[test]
fn partial_write_is_uncertain_not_retried_and_manual_retry_needs_replace() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_fault(WriteFault::PartialThenFail);
    let r = e
        .run(
            &id(),
            write_req("E:/p.bin", b"0123456789", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert!(r.uncertain);
    assert_eq!(
        r.error.as_ref().unwrap().action,
        UserAction::VerifyDeviceContents
    );
    assert_eq!(mock.started_writes(), 1, "ядро не повторяет запись само");
    assert_eq!(
        mock.file("E:/p.bin").unwrap().len(),
        5,
        "на носителе осталась половина"
    );
    assert_eq!(
        e.capability_matrix(&id())
            .unwrap()
            .availability(Operation::FilesWrite),
        Availability::Error
    );
    let j = &e.storage().journal_recent(1).unwrap()[0];
    assert_eq!(j.state, JournalState::Failed);
    assert!(j.uncertain);

    // Ручной повтор без Replace упирается в уже существующий (неполный) файл.
    let r2 = e
        .run(
            &id(),
            write_req("E:/p.bin", b"0123456789", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r2.error.unwrap().code, ErrorCode::AlreadyExists);
    // По решению пользователя — Replace.
    let r3 = e
        .run(
            &id(),
            write_req("E:/p.bin", b"0123456789", OverwritePolicy::Replace),
        )
        .unwrap();
    assert_eq!(r3.outcome, Outcome::Success);
    assert_eq!(mock.file("E:/p.bin").as_deref(), Some(&b"0123456789"[..]));
    assert_eq!(
        e.capability_matrix(&id())
            .unwrap()
            .availability(Operation::FilesWrite),
        Availability::Available
    );
}

// ---------- hot unplug, отмена, таймауты ----------

#[test]
fn unplug_during_write_does_not_hang_or_commit() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(50, 20);
    let e2 = Arc::clone(&e);
    let h = std::thread::spawn(move || {
        e2.run(
            &id(),
            write_req("E:/big.bin", &[1; 64], OverwritePolicy::Never),
        )
    });
    sleep(100);
    mock.unplug();
    let r = h.join().unwrap().unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(
        r.error.as_ref().unwrap().code,
        ErrorCode::DeviceDisconnected
    );
    assert!(r.uncertain);
    assert_eq!(mock.committed_writes(), 0);
    assert_eq!(mock.started_writes(), 1);
    assert!(e.list_devices().unwrap().is_empty());
    assert!(e.capability_matrix(&id()).is_none(), "соединение закрыто");
    assert_eq!(
        e.submit(&id(), OperationRequest::IdentityRead)
            .err()
            .unwrap()
            .code(),
        ErrorCode::NotConnected
    );
}

#[test]
fn cancel_stops_write_without_commit_and_is_uncertain() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(50, 20);
    let h = e
        .submit(
            &id(),
            write_req("E:/big.bin", &[1; 8], OverwritePolicy::Never),
        )
        .unwrap();
    let op = h.operation_id().to_string();
    let t = std::thread::spawn(move || h.wait());
    sleep(100);
    assert!(e.cancel(&op));
    let r = t.join().unwrap();
    assert_eq!(r.outcome, Outcome::Cancelled);
    assert_eq!(mock.committed_writes(), 0);
    assert!(r.uncertain);
    assert!(!e.cancel(&op), "завершённой операции больше нет");
}

#[test]
fn write_timeout_is_reported_and_uncertain() {
    let (e, mock) = connected(EngineConfig {
        write_timeout: Duration::from_millis(150),
        ..EngineConfig::default()
    });
    mock.set_write_profile(100, 20);
    let r = e
        .run(
            &id(),
            write_req("E:/slow.bin", &[1; 8], OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::Timeout);
    assert!(r.uncertain);
    assert_eq!(mock.committed_writes(), 0);
}

#[test]
fn hung_driver_times_out_and_next_operation_waits_for_it() {
    let (e, mock) = connected(EngineConfig {
        identity_timeout: Duration::from_millis(100),
        cancel_grace: Duration::from_millis(100),
        ..EngineConfig::default()
    });
    mock.hang_next_op(700);
    let t0 = Instant::now();
    let r = e.run(&id(), OperationRequest::IdentityRead).unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::Timeout);
    assert!(
        t0.elapsed() < Duration::from_millis(600),
        "сторож не ждёт залипший драйвер"
    );
    // Слот занят воркером до его фактического выхода: следующая операция не перекрывается с ним.
    let r2 = e
        .run(&id(), OperationRequest::FilesList { path: "E:/".into() })
        .unwrap();
    assert_eq!(r2.outcome, Outcome::Success);
    assert!(
        t0.elapsed() >= Duration::from_millis(650),
        "операции не должны перекрываться"
    );
    assert!(!r.uncertain, "чтение не бывает неопределённой записью");
}

// ---------- очередь и прогресс ----------

#[test]
fn operations_on_a_device_run_fifo_one_at_a_time() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(10, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let b = e.submit(&id(), OperationRequest::IdentityRead).unwrap();
    let c = e
        .submit(&id(), OperationRequest::FilesList { path: "E:/".into() })
        .unwrap();
    assert_eq!(e.queued_operations(&id()), 3);
    let finished = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut ts = Vec::new();
    for (name, h) in [("a", a), ("b", b), ("c", c)] {
        let f = Arc::clone(&finished);
        ts.push(std::thread::spawn(move || {
            let r = h.wait();
            f.lock().unwrap().push(name);
            r
        }));
    }
    for t in ts {
        assert_eq!(t.join().unwrap().outcome, Outcome::Success);
    }
    assert_eq!(*finished.lock().unwrap(), vec!["a", "b", "c"]);
}

#[test]
fn queue_is_bounded() {
    let (e, mock) = connected(EngineConfig {
        max_queue: 2,
        ..EngineConfig::default()
    });
    mock.set_write_profile(50, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let b = e.submit(&id(), OperationRequest::IdentityRead).unwrap();
    let err = e
        .submit(&id(), OperationRequest::IdentityRead)
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::QueueFull);
    assert_eq!(err.to_info().action, UserAction::WaitAndRetry);
    e.cancel(a.operation_id());
    a.wait();
    b.wait();
}

#[test]
fn queued_operation_can_be_cancelled_without_running() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(10, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let b = e
        .submit(&id(), write_req("E:/b.bin", &[2], OverwritePolicy::Never))
        .unwrap();
    let b_id = b.operation_id().to_string();
    let ta = std::thread::spawn(move || a.wait());
    let tb = std::thread::spawn(move || b.wait());
    sleep(50);
    assert!(e.cancel(&b_id));
    let rb = tb.join().unwrap();
    assert_eq!(rb.outcome, Outcome::Cancelled);
    assert!(
        !rb.uncertain,
        "операция из очереди на устройство не попадала"
    );
    assert_eq!(ta.join().unwrap().outcome, Outcome::Success);
    assert_eq!(mock.started_writes(), 1);
    assert!(mock.file("E:/b.bin").is_none());
}

#[test]
fn progress_is_reported_while_running() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(20, 20);
    let h = e
        .submit(
            &id(),
            write_req("E:/p.bin", &[1; 4], OverwritePolicy::Never),
        )
        .unwrap();
    let op = h.operation_id().to_string();
    let t = std::thread::spawn(move || h.wait());
    sleep(150);
    let p = e.progress(&op).expect("операция выполняется");
    assert_eq!(p.total, 20);
    assert!(p.done > 0 && p.done < 20, "{p:?}");
    assert_eq!(t.join().unwrap().outcome, Outcome::Success);
    assert!(e.progress(&op).is_none());
}

// ---------- fingerprint, отключение ----------

#[test]
fn fingerprint_mismatch_rejects_operation_and_closes_connection() {
    let (e, mock) = connected(EngineConfig::default());
    let fp = e.capability_matrix(&id()).unwrap().device_fingerprint;
    mock.replace_device("Nokia E52", "999999999999999");
    let r = e
        .run(&id(), write_req("E:/x.txt", b"x", OverwritePolicy::Never))
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::FingerprintMismatch);
    assert!(!r.uncertain);
    assert_eq!(mock.started_writes(), 0);
    assert!(e.capability_matrix(&id()).is_none());
    // новое соединение — новая матрица и другой fingerprint
    let fp2 = e.connect(&id()).unwrap().matrix.unwrap().device_fingerprint;
    assert_ne!(fp, fp2);
    assert!(!fp.contains("356938035643809") && !fp2.contains("999999999999999"));
}

#[test]
fn explicit_disconnect_cancels_running_and_queued_operations() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(50, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let b = e.submit(&id(), OperationRequest::IdentityRead).unwrap();
    let ta = std::thread::spawn(move || a.wait());
    let tb = std::thread::spawn(move || b.wait());
    sleep(100);
    e.disconnect(&id()).unwrap();
    assert_eq!(ta.join().unwrap().outcome, Outcome::Cancelled);
    assert_eq!(tb.join().unwrap().outcome, Outcome::Cancelled);
    assert_eq!(
        e.disconnect(&id()).unwrap_err().code(),
        ErrorCode::NotConnected
    );
    assert_eq!(mock.committed_writes(), 0);
}

#[test]
fn list_devices_drops_connection_of_unplugged_device() {
    let (e, mock) = connected(EngineConfig::default());
    mock.unplug();
    assert!(e.list_devices().unwrap().is_empty());
    mock.plug();
    let d = &e.list_devices().unwrap()[0];
    assert_eq!(d.connection, ConnectionState::Disconnected);
    assert!(d.matrix.is_none());
}

// ---------- приватность и единый тип ответа ----------

#[test]
fn logs_and_journal_contain_no_paths_or_identifiers() {
    let (e, _) = connected(EngineConfig::default());
    e.run(
        &id(),
        write_req("E:/Private/secret-name.txt", b"x", OverwritePolicy::Never),
    )
    .unwrap();
    let logs = serde_json::to_string(&e.diagnostics().snapshot()).unwrap();
    let journal = serde_json::to_string(&e.storage().journal_recent(50).unwrap()).unwrap();
    for text in [&logs, &journal] {
        assert!(!text.contains("secret-name"), "{text}");
        assert!(!text.contains("Private"), "{text}");
        assert!(!text.contains("356938035643809"), "{text}");
        assert!(!text.contains(MOCK_DEVICE_ID), "{text}");
    }
}

/// «Реальный» адаптер для контрактного теста: делегирует mock, но сообщает `origin = Real`.
struct RealLike(MockTransport);

mod real_like {
    use super::*;
    use yeskia_core::error::Result;
    use yeskia_core::transport::*;

    impl DiscoveryProvider for RealLike {
        fn kind(&self) -> TransportKind {
            TransportKind::Mtp
        }
        fn discover(&self) -> Result<Vec<Device>> {
            let mut v = self.0.discover()?;
            for d in &mut v {
                d.origin = DeviceOrigin::Real;
                d.transport = TransportKind::Mtp;
            }
            Ok(v)
        }
    }
    impl DeviceTransport for RealLike {
        fn connect(&self, id: &DeviceId) -> Result<()> {
            self.0.connect(id)
        }
        fn disconnect(&self, id: &DeviceId) -> Result<()> {
            self.0.disconnect(id)
        }
        fn is_present(&self, id: &DeviceId) -> bool {
            self.0.is_present(id)
        }
        fn maturity(&self, op: Operation) -> SupportStatus {
            self.0.maturity(op)
        }
        fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
            self.0.probe(id, ctx)
        }
        fn identity(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Identity> {
            self.0.identity(id, ctx)
        }
        fn files(&self) -> Option<&dyn FileProvider> {
            self.0.files()
        }
    }
}

fn shape(v: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

#[test]
fn ui_gets_the_same_typed_response_from_mock_and_real_transport() {
    let storage = || Arc::new(Storage::open_in_memory().unwrap());
    let diag = || Arc::new(Diagnostics::new(50, None));
    let mock_engine = Engine::new(vec![Arc::new(MockTransport::new())], storage(), diag());
    let real_engine = Engine::new(
        vec![Arc::new(RealLike(MockTransport::new()))],
        storage(),
        diag(),
    );

    let mut shapes = Vec::new();
    for e in [&mock_engine, &real_engine] {
        let dev = e.connect(&id()).unwrap();
        let dev_json = serde_json::to_value(&dev).unwrap();
        let r = e
            .run(
                &id(),
                write_req("E:/same.txt", b"x", OverwritePolicy::Never),
            )
            .unwrap();
        let res_json = serde_json::to_value(&r).unwrap();
        shapes.push((
            shape(&dev_json),
            shape(&dev_json["matrix"]),
            shape(&dev_json["matrix"]["entries"][0]),
            shape(&res_json),
            shape(&res_json["data"]),
        ));
    }
    assert_eq!(shapes[0], shapes[1]);

    // Различается только origin и (честно) уровень подтверждения.
    let real = real_engine
        .run(&id(), OperationRequest::IdentityRead)
        .unwrap();
    assert_eq!(real.origin, DeviceOrigin::Real);
    let entry = real_engine
        .capability_matrix(&id())
        .unwrap()
        .entry(Operation::IdentityRead)
        .unwrap()
        .clone();
    assert_eq!(entry.maturity, SupportStatus::VerifiedOnDevice);
    assert!(entry
        .evidence
        .iter()
        .any(|ev| ev.kind == EvidenceKind::DeviceRun));
}

#[test]
fn operation_ids_are_unique() {
    let (e, _) = connected(EngineConfig::default());
    let a = e
        .run(&id(), OperationRequest::IdentityRead)
        .unwrap()
        .operation_id;
    let b = e
        .run(&id(), OperationRequest::IdentityRead)
        .unwrap()
        .operation_id;
    assert_ne!(a, b);
}

#[test]
fn fingerprint_change_during_write_keeps_uncertainty() {
    let (e, mock) = connected(EngineConfig::default());
    mock.swap_device_on_commit("Nokia E52", "999999999999999");
    let r = e
        .run(
            &id(),
            write_req("E:/swap.bin", b"data", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    let err = r.error.unwrap();
    assert_eq!(err.code, ErrorCode::FingerprintMismatch);
    assert!(
        r.uncertain,
        "запись уже произошла на неизвестно каком устройстве"
    );
    assert_eq!(err.action, UserAction::VerifyDeviceContents);
    assert!(e.storage().journal_recent(1).unwrap()[0].uncertain);
    assert!(e.capability_matrix(&id()).is_none());
}

#[test]
fn dropped_handle_does_not_block_the_queue() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(10, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let b = e.submit(&id(), OperationRequest::IdentityRead).unwrap();
    let b_id = b.operation_id().to_string();
    assert_eq!(e.queued_operations(&id()), 2);
    drop(b); // брошен, wait не вызывался
    assert_eq!(e.queued_operations(&id()), 1);
    let c = e
        .submit(&id(), OperationRequest::FilesList { path: "E:/".into() })
        .unwrap();
    let ta = std::thread::spawn(move || a.wait());
    assert_eq!(c.wait().outcome, Outcome::Success);
    assert_eq!(ta.join().unwrap().outcome, Outcome::Success);
    let j = e.storage().journal_recent(10).unwrap();
    let jb = j.iter().find(|x| x.operation_id == b_id).unwrap();
    assert_eq!(jb.state, JournalState::Cancelled);
}

#[test]
fn cancel_removes_queued_operation_without_its_waiter() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(10, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let b = e.submit(&id(), OperationRequest::IdentityRead).unwrap();
    assert!(e.cancel(b.operation_id()));
    assert_eq!(
        e.queued_operations(&id()),
        1,
        "очередь освобождена без вызова b.wait()"
    );
    let ta = std::thread::spawn(move || a.wait());
    // b ожидается позже — получает Cancelled без выполнения
    assert_eq!(b.wait().outcome, Outcome::Cancelled);
    assert_eq!(ta.join().unwrap().outcome, Outcome::Success);
}

#[test]
fn disconnect_clears_queue_even_with_unawaited_handles() {
    let (e, mock) = connected(EngineConfig::default());
    mock.set_write_profile(50, 20);
    let a = e
        .submit(&id(), write_req("E:/a.bin", &[1], OverwritePolicy::Never))
        .unwrap();
    let _b = e.submit(&id(), OperationRequest::IdentityRead).unwrap();
    let ta = std::thread::spawn(move || a.wait());
    sleep(60);
    e.disconnect(&id()).unwrap();
    assert_eq!(e.queued_operations(&id()), 0);
    assert_eq!(ta.join().unwrap().outcome, Outcome::Cancelled);
    e.connect(&id()).unwrap();
    assert_eq!(
        e.run(&id(), OperationRequest::IdentityRead)
            .unwrap()
            .outcome,
        Outcome::Success
    );
}

/// Транспорт, у которого перечисление может временно ломаться.
struct Flaky {
    inner: MockTransport,
    broken: std::sync::atomic::AtomicBool,
}

mod flaky_impl {
    use super::*;
    use std::sync::atomic::Ordering;
    use yeskia_core::error::{Error, Result};
    use yeskia_core::transport::*;

    impl DiscoveryProvider for Flaky {
        fn kind(&self) -> TransportKind {
            TransportKind::Usb
        }
        fn discover(&self) -> Result<Vec<Device>> {
            if self.broken.load(Ordering::SeqCst) {
                return Err(Error::Transport("перечисление не удалось".into()));
            }
            self.inner.discover()
        }
    }
    impl DeviceTransport for Flaky {
        fn connect(&self, id: &DeviceId) -> Result<()> {
            self.inner.connect(id)
        }
        fn disconnect(&self, id: &DeviceId) -> Result<()> {
            self.inner.disconnect(id)
        }
        fn is_present(&self, id: &DeviceId) -> bool {
            self.inner.is_present(id)
        }
        fn maturity(&self, op: Operation) -> SupportStatus {
            self.inner.maturity(op)
        }
        fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
            self.inner.probe(id, ctx)
        }
        fn identity(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Identity> {
            self.inner.identity(id, ctx)
        }
    }
}

#[test]
fn failing_discovery_neither_hides_other_transports_nor_drops_connections() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let flaky = Arc::new(Flaky {
        inner: MockTransport::new(),
        broken: AtomicBool::new(false),
    });
    let engine = Engine::new(
        vec![flaky.clone(), Arc::new(MockTransport::new())],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(50, None)),
    );
    engine.connect(&id()).unwrap();
    flaky.broken.store(true, Ordering::SeqCst);
    // перечисление первого транспорта упало: соединение не объявляется потерянным
    let _ = engine.list_devices().unwrap();
    assert!(
        engine.capability_matrix(&id()).is_some(),
        "соединение не должно закрываться из-за сбоя перечисления"
    );
    flaky.broken.store(false, Ordering::SeqCst);
    assert_eq!(
        engine
            .run(&id(), OperationRequest::IdentityRead)
            .unwrap()
            .outcome,
        Outcome::Success
    );
}
