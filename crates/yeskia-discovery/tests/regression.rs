//! Регрессионные сценарии Windows-распространения (issue #11): чистая система без телефона, WPD/MTP-only,
//! Mass Storage, занятый COM, отключение USB, чужое MTP, два телефона, отсутствие сети.
//! Топология поддельная (без телефона и без Windows API): это не заменяет ручную проверку на устройстве.

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
use yeskia_discovery::{analyze, DoctorReport};

fn engine(src: Arc<FakeSource>) -> Engine {
    Engine::new(
        vec![Arc::new(PnpTransport::new(src))],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(50, None)),
    )
}

fn report(t: &FakeTopology) -> DoctorReport {
    analyze(&t.snap, 0)
}

#[test]
fn clean_windows_without_a_phone() {
    let src = Arc::new(FakeSource::new(FakeTopology::new()));
    let e = engine(src);
    assert!(
        e.list_devices().unwrap().is_empty(),
        "список пуст, без ошибок"
    );
    let r = report(&FakeTopology::new());
    assert!(r.devices.is_empty());
    // предписывающих действий над драйверами нет: отчёт ничего не устанавливает
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains("VID_"), "{json}");
}

#[test]
fn wpd_mtp_only_is_read_only() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, "SERWPD0001", "WinUSB");
    t.wpd(
        &usb,
        "Nokia E52",
        WpdListing::Ok {
            storages: vec![StorageSummary { object_count: 2 }],
        },
    );
    let e = engine(Arc::new(FakeSource::new(t)));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    assert_eq!(d.transport, TransportKind::Mtp);
    let m = d.matrix.unwrap();
    assert_eq!(
        m.availability(Operation::FilesWrite),
        Availability::Unsupported
    );
    assert_eq!(
        m.availability(Operation::ContactsRead),
        Availability::Unsupported,
        "MTP ≠ PC Suite"
    );
}

#[test]
fn mass_storage_is_limited_to_its_volume() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, "SERMSC0001", "USBSTOR");
    t.disk(&usb, 'F');
    // том F: подменён временной папкой (на Windows это был бы F:\\)
    let root = std::env::temp_dir().join(format!("yeskia-reg-vol-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let r2 = root.clone();
    let e = Engine::new(
        vec![Arc::new(PnpTransport::with_volume_resolver(
            Arc::new(FakeSource::new(t)),
            Arc::new(move |_| r2.clone()),
        ))],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(50, None)),
    );
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    assert_eq!(d.transport, TransportKind::MassStorage);
    let m = d.matrix.unwrap();
    assert_eq!(
        m.availability(Operation::FilesList),
        Availability::Available
    );
    assert_eq!(
        m.availability(Operation::ContactsRead),
        Availability::Unsupported
    );
}

#[test]
fn busy_com_port_is_diagnosed_and_not_faked() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, "SERCOM0001", "usbser");
    t.com(&usb, "COM9", Some(true));
    let r = report(&t);
    assert_eq!(r.devices.len(), 1);
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains("com_busy"), "{json}");
    assert!(
        !json.contains("COM9") || !json.contains("SERCOM0001"),
        "серийник не раскрывается"
    );
    // в списке приложения нет «доступных» возможностей: ни AT-канала, ни файлов
    let e = engine(Arc::new(FakeSource::new(t)));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    let m = d.matrix.unwrap();
    for op in [
        Operation::FilesList,
        Operation::ContactsRead,
        Operation::SmsRead,
        Operation::IdentityRead,
    ] {
        assert_ne!(m.availability(op), Availability::Available, "{op:?}");
    }
}

#[test]
fn usb_unplug_closes_the_connection_and_stops_operations() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, "SERUNPLUG1", "WinUSB");
    t.wpd(
        &usb,
        "Nokia E52",
        WpdListing::Ok {
            storages: vec![StorageSummary { object_count: 1 }],
        },
    );
    let src = Arc::new(FakeSource::new(t));
    let e = engine(Arc::clone(&src));
    let id = e.connect(&e.list_devices().unwrap()[0].id).unwrap().id;
    src.with(|t| t.remove(&usb));
    assert!(e.list_devices().unwrap().is_empty());
    let err = e.submit(&id, OperationRequest::IdentityRead).err().unwrap();
    assert!(
        matches!(
            err.code(),
            ErrorCode::NotConnected | ErrorCode::DeviceDisconnected | ErrorCode::DeviceNotFound
        ),
        "{err}"
    );
}

#[test]
fn foreign_mtp_device_is_not_offered_but_the_nokia_beside_it_is() {
    let mut t = FakeTopology::new();
    let other = t.usb(0x04E8, 0x6860, "SERFOREIGN", "WinUSB");
    t.wpd(&other, "Phone X", WpdListing::NotProbed);
    let nokia = t.usb(0x0421, 0x0223, "SERNOKIA01", "WinUSB");
    t.wpd(&nokia, "Nokia E72", WpdListing::NotProbed);
    let e = engine(Arc::new(FakeSource::new(t)));
    let devs = e.list_devices().unwrap();
    assert_eq!(devs.len(), 1);
    assert!(devs[0].display_name.contains("0421"));
}

#[test]
fn two_phones_are_independent() {
    let mut t = FakeTopology::new();
    let a = t.usb(0x0421, 0x0223, "SERPHONE_A", "WinUSB");
    t.wpd(
        &a,
        "Nokia E52",
        WpdListing::Ok {
            storages: vec![StorageSummary { object_count: 1 }],
        },
    );
    let b = t.usb(0x0421, 0x0223, "SERPHONE_B", "WinUSB");
    t.wpd(
        &b,
        "Nokia E72",
        WpdListing::Ok {
            storages: vec![StorageSummary { object_count: 2 }],
        },
    );
    let src = Arc::new(FakeSource::new(t));
    let e = engine(Arc::clone(&src));
    let devs = e.list_devices().unwrap();
    assert_eq!(devs.len(), 2);
    assert_ne!(
        devs[0].id, devs[1].id,
        "у одинаковых моделей разные идентификаторы"
    );
    let da = e.connect(&devs[0].id).unwrap();
    let db = e.connect(&devs[1].id).unwrap();
    assert_ne!(da.identified_model, db.identified_model);
    let models: std::collections::HashSet<_> =
        [da.identified_model.clone(), db.identified_model.clone()]
            .into_iter()
            .flatten()
            .collect();
    assert_eq!(
        models.len(),
        2,
        "каждый телефон опознан по своему протоколу: {models:?}"
    );
    // отключаем один — второй продолжает работать
    let gone = if da.identified_model.as_deref() == Some("Nokia E52") {
        &a
    } else {
        &b
    };
    let keep_id = if da.identified_model.as_deref() == Some("Nokia E52") {
        db.id.clone()
    } else {
        da.id.clone()
    };
    src.with(|t| t.remove(gone));
    let r = e.run(&keep_id, OperationRequest::IdentityRead).unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    assert_eq!(e.list_devices().unwrap().len(), 1);
}

#[test]
fn the_core_never_requires_a_network() {
    let s = yeskia_core::system::SystemInfo::collect();
    assert!(!s.network_required);
}
