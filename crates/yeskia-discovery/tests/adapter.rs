use std::sync::Arc;
use yeskia_core::device::*;
use yeskia_core::diagnostics::Diagnostics;
use yeskia_core::engine::Engine;
use yeskia_core::error::ErrorCode;
use yeskia_core::operations::*;
use yeskia_core::storage::Storage;
use yeskia_discovery::adapter::PnpTransport;
use yeskia_discovery::fake::*;
use yeskia_discovery::model::*;

const SERIAL: &str = "SER0123456789AB";

fn engine(src: Arc<FakeSource>) -> Engine {
    Engine::new(
        vec![Arc::new(PnpTransport::new(src))],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(50, None)),
    )
}

fn nokia_wpd(serial: &str) -> (FakeTopology, String) {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, serial, "WinUSB");
    t.wpd(
        &usb,
        "Nokia E52",
        WpdListing::Ok {
            storages: vec![StorageSummary { object_count: 3 }],
        },
    );
    (t, usb)
}

#[test]
fn nokia_device_is_listed_without_model_claims_and_without_leaking_ids() {
    let (t, _) = nokia_wpd(SERIAL);
    let e = engine(Arc::new(FakeSource::new(t)));
    let devs = e.list_devices().unwrap();
    assert_eq!(devs.len(), 1);
    let d = &devs[0];
    assert_eq!(d.origin, DeviceOrigin::Real);
    assert_eq!(d.transport, TransportKind::Mtp);
    assert!(d.display_name.contains("0421:0223"));
    assert!(
        !d.display_name.contains("E52"),
        "модель не выводится из VID/PID"
    );
    let json = serde_json::to_string(&devs).unwrap();
    assert!(
        !json.contains(SERIAL) && !json.contains("VID_0421"),
        "{json}"
    );
    assert!(d.matrix.is_none(), "перечисление USB не даёт матрицы");
}

#[test]
fn non_nokia_mtp_device_is_not_offered() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x04E8, 0x6860, SERIAL, "WinUSB");
    t.wpd(&usb, "Other", WpdListing::NotProbed);
    let e = engine(Arc::new(FakeSource::new(t)));
    assert!(e.list_devices().unwrap().is_empty());
}

#[test]
fn connect_builds_matrix_from_wpd_identity_probe_only() {
    let (t, _) = nokia_wpd(SERIAL);
    let e = engine(Arc::new(FakeSource::new(t)));
    let id = e.list_devices().unwrap()[0].id.clone();
    let m = e.connect(&id).unwrap().matrix.unwrap();
    assert_eq!(
        m.availability(Operation::IdentityRead),
        Availability::Available
    );
    assert_eq!(
        m.entry(Operation::IdentityRead).unwrap().maturity,
        SupportStatus::Implemented
    );
    // read-only прототип: файлы, PIM и запись не заявляются
    for op in [
        Operation::FilesList,
        Operation::FilesRead,
        Operation::FilesWrite,
        Operation::ContactsRead,
    ] {
        assert_eq!(m.availability(op), Availability::Unsupported, "{op:?}");
    }
    let r = e.run(&id, OperationRequest::IdentityRead).unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    match r.data.unwrap() {
        OperationData::Identity { model, .. } => assert_eq!(model, "Nokia E52"),
        other => panic!("{other:?}"),
    }
    let err = e
        .submit(&id, OperationRequest::FilesList { path: "E:/".into() })
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
}

#[test]
fn mode_without_wpd_has_no_identity_capability() {
    let mut t = FakeTopology::new();
    t.usb(0x0421, 0x0223, SERIAL, "WinUSB"); // видно USB, сервисов нет
    let e = engine(Arc::new(FakeSource::new(t)));
    let id = e.list_devices().unwrap()[0].id.clone();
    let m = e.connect(&id).unwrap().matrix.unwrap();
    assert_eq!(
        m.availability(Operation::IdentityRead),
        Availability::Unsupported
    );
    assert!(m
        .entry(Operation::IdentityRead)
        .unwrap()
        .detail
        .as_deref()
        .unwrap()
        .contains("WPD"));
}

#[test]
fn disconnect_removes_device_and_reconnect_does_not_inherit_matrix() {
    let (t, usb) = nokia_wpd(SERIAL);
    let src = Arc::new(FakeSource::new(t));
    let e = engine(Arc::clone(&src));
    let id = e.list_devices().unwrap()[0].id.clone();
    e.connect(&id).unwrap();
    e.run(&id, OperationRequest::IdentityRead).unwrap();
    assert!(e.capability_matrix(&id).is_some());

    // кабель вытащили
    src.with(|t| t.remove(&usb));
    assert!(
        e.list_devices().unwrap().is_empty(),
        "устройство пропало после disconnect"
    );
    assert!(e.capability_matrix(&id).is_none());
    assert_eq!(
        e.submit(&id, OperationRequest::IdentityRead)
            .err()
            .unwrap()
            .code(),
        ErrorCode::NotConnected
    );

    // тот же телефон вернулся: соединение новое, ничего не унаследовано
    src.with(|t| {
        let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
        t.wpd(&usb, "Nokia E52", WpdListing::NotProbed);
    });
    let d = &e.list_devices().unwrap()[0];
    assert_eq!(d.connection, ConnectionState::Disconnected);
    assert!(d.matrix.is_none());
}

#[test]
fn a_different_phone_gets_a_different_id_and_fingerprint() {
    let (t, usb) = nokia_wpd("AAAA0000001");
    let src = Arc::new(FakeSource::new(t));
    let e = engine(Arc::clone(&src));
    let id_a = e.list_devices().unwrap()[0].id.clone();
    let fp_a = e.connect(&id_a).unwrap().matrix.unwrap().device_fingerprint;
    e.disconnect(&id_a).unwrap();
    src.with(|t| {
        t.remove(&usb);
        let usb = t.usb(0x0421, 0x0223, "BBBB0000002", "WinUSB");
        t.wpd(&usb, "Nokia E52", WpdListing::NotProbed);
    });
    let id_b = e.list_devices().unwrap()[0].id.clone();
    assert_ne!(id_a, id_b);
    let fp_b = e.connect(&id_b).unwrap().matrix.unwrap().device_fingerprint;
    assert_ne!(fp_a, fp_b, "новый fingerprint не наследует старые права");
}

#[test]
fn multiple_nokia_devices_are_listed_independently() {
    let mut t = FakeTopology::new();
    let a = t.usb(0x0421, 0x0223, "AAAA0000001", "WinUSB");
    let b = t.usb(0x0421, 0x0223, "BBBB0000002", "WinUSB");
    t.wpd(&a, "Nokia A", WpdListing::NotProbed);
    t.com(&b, "COM4", None);
    let e = engine(Arc::new(FakeSource::new(t)));
    let devs = e.list_devices().unwrap();
    assert_eq!(devs.len(), 2);
    assert_ne!(devs[0].id, devs[1].id);
    let kinds: Vec<_> = devs.iter().map(|d| d.transport).collect();
    assert!(kinds.contains(&TransportKind::Mtp) && kinds.contains(&TransportKind::PcSuite));
}

#[test]
fn pnp_enumeration_failure_is_an_error_not_an_unplug() {
    let (t, _) = nokia_wpd(SERIAL);
    let src = Arc::new(FakeSource::new(t));
    let e = engine(Arc::clone(&src));
    let id = e.list_devices().unwrap()[0].id.clone();
    e.connect(&id).unwrap();

    // SetupAPI упал: пустой снимок + ошибка этапа (как делает WindowsSource)
    src.with(|t| {
        t.snap.nodes.clear();
        t.snap.wpd.clear();
        t.snap.errors.push(CollectError {
            stage: "setupapi_enum".into(),
            code: Some(5),
        });
    });
    let _ = e.list_devices().unwrap();
    assert!(
        e.capability_matrix(&id).is_some(),
        "соединение не должно закрываться из-за сбоя перечисления"
    );

    // частичный сбой WPD не считается сбоем перечисления
    let (t2, _) = nokia_wpd(SERIAL);
    let src2 = Arc::new(FakeSource::new(t2));
    src2.with(|t| {
        t.snap.errors.push(CollectError {
            stage: "wpd_enumerate".into(),
            code: Some(5),
        })
    });
    let e2 = engine(src2);
    assert_eq!(e2.list_devices().unwrap().len(), 1);
}

#[test]
fn identified_model_comes_from_wpd_properties_not_from_vid_pid() {
    let (t, _) = nokia_wpd(SERIAL);
    let e = engine(Arc::new(FakeSource::new(t)));
    let d = &e.list_devices().unwrap()[0];
    assert!(
        d.identified_model.is_none(),
        "по VID/PID модель не определяется"
    );
    let c = e.connect(&d.id).unwrap();
    assert_eq!(c.identified_model.as_deref(), Some("Nokia E52"));

    // режим без WPD: идентификации нет → модель не заявляется
    let mut t = FakeTopology::new();
    t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    let e = engine(Arc::new(FakeSource::new(t)));
    let id = e.list_devices().unwrap()[0].id.clone();
    assert!(e.connect(&id).unwrap().identified_model.is_none());
}
