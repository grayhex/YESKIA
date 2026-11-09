use yeskia_discovery::doctor::*;
use yeskia_discovery::fake::*;
use yeskia_discovery::model::*;

const SERIAL: &str = "SER0123456789AB";

fn wpd_ok(objects: u32) -> WpdListing {
    WpdListing::Ok {
        storages: vec![StorageSummary {
            object_count: objects,
        }],
    }
}

fn report(t: &FakeTopology) -> DoctorReport {
    analyze(&t.snap, 1)
}

#[test]
fn no_usb_is_reported_as_no_usb() {
    let t = FakeTopology::new();
    let r = report(&t);
    assert!(r.devices.is_empty());
    assert!(r.findings.contains(&FindingCode::NoUsbDevices));
}

#[test]
fn wpd_only_nokia_with_responding_mtp_is_not_pc_suite() {
    // кейс из issue: Nokia 0421:0223 как WPD/MTP, без COM и буквы диска
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    t.wpd(&usb, "Nokia E52", wpd_ok(7));
    let r = report(&t);
    let d = &r.devices[0];
    assert_eq!((d.vid.as_str(), d.pid.as_str()), ("0421", "0223"));
    assert_eq!(d.state, DoctorState::WpdOnly);
    assert_eq!(d.protocol, ProtocolStatus::MtpResponding);
    assert!(d.nokia_vendor_hint);
    assert!(!d.identity_confirmed, "VID/PID не подтверждают модель");
    assert_eq!(d.pc_suite, Proof::NotProven);
    assert_eq!(d.pim, Proof::NotProven);
    assert!(d.findings.contains(&FindingCode::MtpIsNotPcSuite));
    assert!(d.findings.contains(&FindingCode::VendorHintOnly));
    let w = d.wpd.as_ref().unwrap();
    assert!(w.associated_by_instance);
    assert_eq!((w.storages, w.top_level_objects), (Some(1), Some(7)));
}

#[test]
fn usb_visible_without_services_is_unknown_mode() {
    let mut t = FakeTopology::new();
    t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    let d = &report(&t).devices[0];
    assert_eq!(d.state, DoctorState::UnknownMode);
    assert_eq!(d.protocol, ProtocolStatus::NotProbed);
}

#[test]
fn com_present_and_com_busy_are_distinguished() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x01B5, SERIAL, "usbser");
    t.com(&usb, "COM5", Some(false));
    assert_eq!(report(&t).devices[0].state, DoctorState::ComPresent);

    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x01B5, SERIAL, "usbser");
    t.com(&usb, "COM5", Some(true));
    let d = &report(&t).devices[0];
    assert_eq!(d.state, DoctorState::ComBusy);
    assert_eq!((d.com_ports, d.com_busy), (1, 1));

    // занятость не проверялась → просто «есть COM», а не «занят»
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x01B5, SERIAL, "usbser");
    t.com(&usb, "COM5", None);
    assert_eq!(report(&t).devices[0].state, DoctorState::ComPresent);
}

#[test]
fn driver_missing_and_driver_error_are_distinguished() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "");
    t.problem(&usb, 28);
    assert_eq!(report(&t).devices[0].state, DoctorState::DriverMissing);

    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    t.problem(&usb, 10);
    let d = &report(&t).devices[0];
    assert_eq!(d.state, DoctorState::DriverError);
    assert_eq!(d.driver.problem_code, 10);
}

#[test]
fn driver_problem_on_child_is_reported() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    t.wpd(&usb, "Nokia", WpdListing::NotProbed);
    // проблема на WPD-узле (дочернем)
    for n in &mut t.snap.nodes {
        if n.class.as_deref() == Some("WPD") {
            n.problem_code = 28;
        }
    }
    assert_eq!(report(&t).devices[0].state, DoctorState::DriverMissing);
}

#[test]
fn mass_storage_device_is_recognised() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "USBSTOR");
    t.disk(&usb, 'F');
    let d = &report(&t).devices[0];
    assert_eq!(d.state, DoctorState::MassStorage);
    assert_eq!(d.volumes, 1);
}

#[test]
fn mtp_listing_failure_is_reported_as_failure_not_as_protocol_response() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    t.wpd(&usb, "Nokia", WpdListing::Failed { code: 0x8007_001F });
    let d = &report(&t).devices[0];
    assert_eq!(d.protocol, ProtocolStatus::MtpFailed);
    assert!(d.findings.contains(&FindingCode::MtpListingFailed));
    assert_eq!(d.wpd.as_ref().unwrap().error_code, Some(0x8007_001F));
}

#[test]
fn non_nokia_mtp_device_is_not_treated_as_nokia() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x04E8, 0x6860, SERIAL, "WinUSB");
    t.wpd(&usb, "Other Phone", wpd_ok(3));
    let d = &report(&t).devices[0];
    assert!(!d.nokia_vendor_hint);
    assert!(d.findings.contains(&FindingCode::NotNokiaVendor));
    assert!(!d.findings.contains(&FindingCode::VendorHintOnly));
    assert!(!d.identity_confirmed);
}

#[test]
fn multiple_devices_get_independent_wpd_association() {
    let mut t = FakeTopology::new();
    let a = t.usb(0x0421, 0x0223, "AAAA0000001", "WinUSB");
    let b = t.usb(0x0421, 0x0223, "BBBB0000002", "WinUSB");
    t.wpd(&a, "Nokia A", wpd_ok(1));
    // у b — только COM
    t.com(&b, "COM7", Some(false));
    let r = report(&t);
    assert_eq!(r.devices.len(), 2);
    assert!(r.findings.contains(&FindingCode::MultipleDevices));
    let states: Vec<_> = r.devices.iter().map(|d| d.state).collect();
    assert!(states.contains(&DoctorState::WpdOnly) && states.contains(&DoctorState::ComPresent));
    let wpd_dev = r
        .devices
        .iter()
        .find(|d| d.state == DoctorState::WpdOnly)
        .unwrap();
    assert_eq!(
        wpd_dev.wpd.as_ref().unwrap().model.as_deref(),
        Some("Nokia A")
    );
    let com_dev = r
        .devices
        .iter()
        .find(|d| d.state == DoctorState::ComPresent)
        .unwrap();
    assert!(
        com_dev.wpd.is_none(),
        "WPD не должен «перетекать» на соседнее устройство"
    );
}

#[test]
fn wpd_association_goes_through_composite_interface_to_parent() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "usbccgp");
    let mi = t.interface(&usb, 0);
    t.wpd(&mi, "Nokia", wpd_ok(2));
    let r = report(&t);
    assert_eq!(
        r.devices.len(),
        1,
        "интерфейс MI_ не считается отдельным устройством"
    );
    assert_eq!(r.devices[0].state, DoctorState::WpdOnly);
    assert!(r.devices[0].wpd.is_some());
}

#[test]
fn hubs_are_not_devices() {
    let mut t = FakeTopology::new();
    t.usb(0x8087, 0x0024, "HUB", "USBHUB3");
    assert!(report(&t).devices.is_empty());
}

#[test]
fn partial_collection_errors_are_flagged() {
    let mut t = FakeTopology::new();
    t.snap.errors.push(CollectError {
        stage: "wpd_enumerate".into(),
        code: Some(0x8007_0005),
    });
    let r = report(&t);
    assert!(r.findings.contains(&FindingCode::PartialData));
    assert_eq!(r.issues[0].stage, "wpd_enumerate");
}

#[test]
fn report_is_privacy_safe() {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    t.wpd(&usb, "Nokia E52", wpd_ok(7));
    t.com(&usb, "COM9", Some(false));
    // «чувствительные» значения, которые не должны попасть наружу
    t.snap.nodes[1].description = Some("Ivan's phone 356938035643809 AA:BB:CC:DD:EE:FF".into());
    let r = report(&t);
    let json = serde_json::to_string_pretty(&r).unwrap();
    for needle in [
        SERIAL,
        "356938035643809",
        "AA:BB:CC",
        "Ivan",
        "USB\\\\VID_",
        "usb#vid",
        "\\\\\\\\?\\\\",
        "WPDBUSENUM",
        "COM9",
    ] {
        assert!(!json.contains(needle), "в отчёт попало {needle:?}: {json}");
    }
    assert!(json.contains("0421") && json.contains("0223"));
}
