//! Файловый менеджер поверх обнаружения: Mass Storage (привязка тома) и read-only WPD/MTP.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
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

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-dfm-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn leftovers(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("yeskia-part")
        })
        .count()
}

fn engine_with(
    t: FakeTopology,
    volumes: Vec<(char, PathBuf)>,
) -> (Arc<Engine>, Arc<FakeSource>, DeviceId) {
    let src = Arc::new(FakeSource::new(t));
    let resolver: Arc<dyn Fn(char) -> PathBuf + Send + Sync> = Arc::new(move |c| {
        volumes
            .iter()
            .find(|(l, _)| *l == c)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| PathBuf::from("/nonexistent-yeskia-volume"))
    });
    let transport = PnpTransport::with_volume_resolver(src.clone(), resolver);
    let e = Engine::new(
        vec![Arc::new(transport)],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    let id = e.list_devices().unwrap()[0].id.clone();
    (Arc::new(e), src, id)
}

fn wpd_phone() -> (FakeTopology, String) {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "WinUSB");
    t.wpd(&usb, "Nokia E52", WpdListing::NotProbed);
    t.wpd_file(&usb, "Phone memory/DCIM/фото.jpg", &[9u8; 3000]);
    t.wpd_file(&usb, "Phone memory/Documents/a.txt", b"hello");
    t.wpd_file(&usb, "Memory card/Music/song.mp3", &[1u8; 100]);
    (t, usb)
}

fn list(e: &Engine, id: &DeviceId, path: &str) -> Vec<(String, bool)> {
    let r = e
        .run(id, OperationRequest::FilesList { path: path.into() })
        .unwrap();
    match r.data.unwrap_or_else(|| panic!("{:?}", r.error)) {
        OperationData::Entries { entries } => {
            entries.into_iter().map(|x| (x.name, x.is_dir)).collect()
        }
        other => panic!("{other:?}"),
    }
}

fn dl(path: &str, dest: &Path) -> OperationRequest {
    OperationRequest::FilesDownload {
        path: path.into(),
        dest: dest.display().to_string(),
        overwrite: OverwritePolicy::Never,
    }
}

// ---------- WPD/MTP: read-only ----------

#[test]
fn wpd_lists_storages_and_folders_read_only() {
    let (t, _) = wpd_phone();
    let (e, _, id) = engine_with(t, vec![]);
    let m = e.connect(&id).unwrap().matrix.unwrap();
    assert_eq!(
        m.availability(Operation::FilesList),
        Availability::Available
    );
    assert_eq!(
        m.availability(Operation::FilesWrite),
        Availability::Unsupported,
        "запись не заявляется"
    );
    assert_eq!(
        m.entry(Operation::FilesList).unwrap().maturity,
        SupportStatus::Implemented
    );

    let mut root = list(&e, &id, "/");
    root.sort();
    assert_eq!(
        root,
        vec![("Memory card".into(), true), ("Phone memory".into(), true)]
    );
    let mut dcim = list(&e, &id, "/Phone memory");
    dcim.sort();
    assert_eq!(
        dcim,
        vec![("DCIM".into(), true), ("Documents".into(), true)]
    );
    assert_eq!(
        list(&e, &id, "/Phone memory/DCIM"),
        vec![("фото.jpg".into(), false)]
    );
}

#[test]
fn wpd_download_verifies_size_and_hash_and_handles_cyrillic() {
    let (t, _) = wpd_phone();
    let (e, _, id) = engine_with(t, vec![]);
    e.connect(&id).unwrap();
    let dir = tmpdir("wpd-dl");
    let dest = dir.join("фото.jpg");
    let r = e
        .run(&id, dl("/Phone memory/DCIM/фото.jpg", &dest))
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert!(matches!(
        r.data,
        Some(OperationData::Downloaded {
            bytes: 3000,
            verified: true,
            ..
        })
    ));
    assert_eq!(std::fs::read(&dest).unwrap(), vec![9u8; 3000]);
    assert_eq!(leftovers(&dir), 0);
    // успешное скачивание подтверждает files.read на этом соединении
    assert_eq!(
        e.capability_matrix(&id)
            .unwrap()
            .availability(Operation::FilesRead),
        Availability::Available
    );
    // повтор без разрешения не перезаписывает
    let r = e
        .run(&id, dl("/Phone memory/DCIM/фото.jpg", &dest))
        .unwrap();
    assert_eq!(r.error.unwrap().code, ErrorCode::AlreadyExists);
}

#[test]
fn wpd_has_no_write_path_at_all() {
    let (t, _) = wpd_phone();
    let (e, _, id) = engine_with(t, vec![]);
    e.connect(&id).unwrap();
    let dir = tmpdir("wpd-ul");
    let src = dir.join("x.txt");
    std::fs::write(&src, b"x").unwrap();
    let err = e
        .submit(
            &id,
            OperationRequest::FilesUpload {
                source: src.display().to_string(),
                path: "/Phone memory/x.txt".into(),
                overwrite: OverwritePolicy::Never,
            },
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
}

#[test]
fn wpd_download_cancel_and_disconnect_leave_no_partial_file() {
    let (mut t, usb) = wpd_phone();
    t.read_delay_ms = 40;
    let (e, src, id) = engine_with(t, vec![]);
    e.connect(&id).unwrap();
    let dir = tmpdir("wpd-cancel");

    let dest = dir.join("c.jpg");
    let h = e
        .submit(&id, dl("/Phone memory/DCIM/фото.jpg", &dest))
        .unwrap();
    let op = h.operation_id().to_string();
    let t1 = std::thread::spawn(move || h.wait());
    std::thread::sleep(Duration::from_millis(120));
    e.cancel(&op);
    assert_eq!(t1.join().unwrap().outcome, Outcome::Cancelled);
    assert!(!dest.exists());
    assert_eq!(leftovers(&dir), 0);

    let dest2 = dir.join("d.jpg");
    let e2 = Arc::clone(&e);
    let (id2, d2) = (id.clone(), dest2.clone());
    let t2 = std::thread::spawn(move || e2.run(&id2, dl("/Phone memory/DCIM/фото.jpg", &d2)));
    std::thread::sleep(Duration::from_millis(120));
    src.with(|t| t.remove(&usb)); // кабель вытащили
    let r = t2.join().unwrap().unwrap();
    assert_eq!(
        r.error.as_ref().unwrap().code,
        ErrorCode::DeviceDisconnected
    );
    assert!(!dest2.exists());
    assert_eq!(leftovers(&dir), 0);
}

#[test]
fn wpd_missing_file_and_directory_are_structured_errors() {
    let (t, _) = wpd_phone();
    let (e, _, id) = engine_with(t, vec![]);
    e.connect(&id).unwrap();
    let dir = tmpdir("wpd-err");
    let r = e
        .run(&id, dl("/Phone memory/нет.txt", &dir.join("a")))
        .unwrap();
    assert_eq!(r.error.unwrap().code, ErrorCode::OperationFailed);
    let r = e
        .run(&id, dl("/Phone memory/DCIM", &dir.join("b")))
        .unwrap();
    assert_eq!(r.error.unwrap().code, ErrorCode::InvalidArgument);
    // WPD не принимает пути с буквой диска
    let r = e
        .run(&id, OperationRequest::FilesList { path: "E:/".into() })
        .unwrap();
    assert_eq!(r.error.unwrap().code, ErrorCode::UnsafePath);
}

// ---------- Mass Storage: привязка тома ----------

fn mass_phone(letters: &[char]) -> FakeTopology {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "USBSTOR");
    for l in letters {
        t.disk(&usb, *l);
    }
    t
}

#[test]
fn mass_storage_roundtrip_copy_with_sha_readback() {
    let card = tmpdir("ms-card");
    std::fs::create_dir_all(card.join("Музыка")).unwrap();
    std::fs::write(card.join("Музыка/трек.mp3"), vec![5u8; 70_000]).unwrap();
    let (e, _, id) = engine_with(mass_phone(&['F']), vec![('F', card.clone())]);
    let m = e.connect(&id).unwrap().matrix.unwrap();
    assert_eq!(
        m.availability(Operation::FilesList),
        Availability::Available
    );
    assert_eq!(
        m.availability(Operation::FilesWrite),
        Availability::NotTested,
        "запись — только по явной операции"
    );
    assert_eq!(
        m.entry(Operation::FilesWrite).unwrap().maturity,
        SupportStatus::TestedOnHost
    );
    assert_eq!(
        m.availability(Operation::IdentityRead),
        Availability::Unsupported
    );
    assert_eq!(
        e.list_devices().unwrap()[0].identified_model,
        None,
        "модель по Mass Storage не определяется"
    );

    assert_eq!(list(&e, &id, "/"), vec![("Музыка".into(), true)]);
    let host = tmpdir("ms-host");
    let dest = host.join("трек.mp3");
    let r = e.run(&id, dl("/Музыка/трек.mp3", &dest)).unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert_eq!(std::fs::read(&dest).unwrap().len(), 70_000);

    // загрузка на карту: только по явному запросу, с read-back
    let up = host.join("заметка.txt");
    std::fs::write(&up, "текст").unwrap();
    let r = e
        .run(
            &id,
            OperationRequest::FilesUpload {
                source: up.display().to_string(),
                path: "F:/Музыка/заметка.txt".into(),
                overwrite: OverwritePolicy::Never,
            },
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert!(matches!(
        r.data,
        Some(OperationData::Written { verified: true, .. })
    ));
    assert_eq!(
        std::fs::read_to_string(card.join("Музыка/заметка.txt")).unwrap(),
        "текст"
    );
    assert_eq!(
        e.capability_matrix(&id)
            .unwrap()
            .availability(Operation::FilesWrite),
        Availability::Available
    );
}

#[test]
fn mass_storage_cannot_reach_paths_outside_the_devices_volumes() {
    let card = tmpdir("ms-jail-card");
    let other = tmpdir("ms-jail-other");
    std::fs::write(other.join("secret.txt"), b"secret").unwrap();
    // у устройства только F:, диск G: принадлежит чужому устройству/системе
    let (e, _, id) = engine_with(mass_phone(&['F']), vec![('F', card), ('G', other)]);
    e.connect(&id).unwrap();
    let r = e
        .run(&id, OperationRequest::FilesList { path: "G:/".into() })
        .unwrap();
    assert_eq!(r.error.unwrap().code, ErrorCode::UnsafePath);
    let dir = tmpdir("ms-jail-host");
    let r = e.run(&id, dl("G:/secret.txt", &dir.join("s.txt"))).unwrap();
    assert_eq!(r.error.unwrap().code, ErrorCode::UnsafePath);
    assert!(!dir.join("s.txt").exists());
    for p in ["F:/../secret.txt", "/../x"] {
        let err = e.submit(&id, dl(p, &dir.join("t.txt"))).err().unwrap();
        assert_eq!(err.code(), ErrorCode::UnsafePath, "{p}");
    }
}

#[test]
fn mass_storage_with_several_volumes_requires_explicit_drive() {
    let a = tmpdir("ms-multi-a");
    let b = tmpdir("ms-multi-b");
    std::fs::write(a.join("a.txt"), b"A").unwrap();
    std::fs::write(b.join("b.txt"), b"B").unwrap();
    let (e, _, id) = engine_with(mass_phone(&['F', 'G']), vec![('F', a), ('G', b)]);
    e.connect(&id).unwrap();
    assert_eq!(
        list(&e, &id, "/"),
        vec![("F:".into(), true), ("G:".into(), true)]
    );
    assert_eq!(list(&e, &id, "G:/"), vec![("b.txt".into(), false)]);
    let r = e
        .run(
            &id,
            OperationRequest::FilesList {
                path: "/Музыка".into(),
            },
        )
        .unwrap();
    assert_eq!(
        r.error.unwrap().code,
        ErrorCode::UnsafePath,
        "без буквы диска путь неоднозначен"
    );
}

#[test]
fn volume_disappearing_with_the_device_closes_the_connection() {
    let card = tmpdir("ms-gone");
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, SERIAL, "USBSTOR");
    t.disk(&usb, 'F');
    let (e, src, id) = engine_with(t, vec![('F', card)]);
    e.connect(&id).unwrap();
    src.with(|t| t.remove(&usb));
    assert!(e.list_devices().unwrap().is_empty());
    assert!(e.capability_matrix(&id).is_none());
}

#[test]
fn no_pim_and_no_phone_c_drive_without_a_proven_transport() {
    let (t, _) = wpd_phone();
    let (e, _, id) = engine_with(t, vec![]);
    let m = e.connect(&id).unwrap().matrix.unwrap();
    for op in [
        Operation::ContactsRead,
        Operation::ContactsWrite,
        Operation::SmsRead,
        Operation::SmsSend,
    ] {
        assert_eq!(m.availability(op), Availability::Unsupported);
    }
    // «диск C:» телефона не существует в WPD-дереве
    let r = e
        .run(&id, OperationRequest::FilesList { path: "C:/".into() })
        .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
}
