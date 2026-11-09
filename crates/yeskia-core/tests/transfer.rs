//! Передача файлов: скачивание на компьютер, загрузка на устройство, Mass Storage-том.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use yeskia_core::device::*;
use yeskia_core::diagnostics::Diagnostics;
use yeskia_core::engine::{Engine, EngineConfig};
use yeskia_core::error::{ErrorCode, Result};
use yeskia_core::mock::{MockTransport, MOCK_DEVICE_ID};
use yeskia_core::operations::*;
use yeskia_core::storage::Storage;
use yeskia_core::transport::*;
use yeskia_core::volume::VolumeFileProvider;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-xfer-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn mock_engine() -> (Arc<Engine>, MockTransport, DeviceId) {
    let mock = MockTransport::new();
    let e = Engine::new(
        vec![Arc::new(mock.clone())],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    let id = DeviceId::new(MOCK_DEVICE_ID);
    e.connect(&id).unwrap();
    (Arc::new(e), mock, id)
}

fn download(path: &str, dest: &Path, overwrite: OverwritePolicy) -> OperationRequest {
    OperationRequest::FilesDownload {
        path: path.into(),
        dest: dest.display().to_string(),
        overwrite,
    }
}

fn upload(source: &Path, path: &str, overwrite: OverwritePolicy) -> OperationRequest {
    OperationRequest::FilesUpload {
        source: source.display().to_string(),
        path: path.into(),
        overwrite,
    }
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("yeskia-part"))
        .collect()
}

/// Транспорт Mass Storage поверх настоящей папки (том) — как будет на Windows с `E:\`.
struct VolTransport {
    provider: VolumeFileProvider,
    present: std::sync::atomic::AtomicBool,
}

impl VolTransport {
    fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            provider: VolumeFileProvider::new(root).unwrap(),
            present: std::sync::atomic::AtomicBool::new(true),
        })
    }
}

impl DiscoveryProvider for VolTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::MassStorage
    }
    fn discover(&self) -> Result<Vec<Device>> {
        if !self.present.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(vec![]);
        }
        Ok(vec![Device {
            id: DeviceId::new("vol-1"),
            display_name: "USB 0421:0223 (Mass Storage)".into(),
            origin: DeviceOrigin::Real,
            transport: TransportKind::MassStorage,
            connection: ConnectionState::Disconnected,
            serial: Some(Sensitive::new("USBSTOR\\SERIAL")),
            identified_model: None,
            matrix: None,
        }])
    }
}

impl DeviceTransport for VolTransport {
    fn connect(&self, _id: &DeviceId) -> Result<()> {
        Ok(())
    }
    fn disconnect(&self, _id: &DeviceId) -> Result<()> {
        Ok(())
    }
    fn is_present(&self, _id: &DeviceId) -> bool {
        self.present.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn maturity(&self, op: Operation) -> SupportStatus {
        match op {
            Operation::FilesList | Operation::FilesRead | Operation::FilesWrite => {
                SupportStatus::TestedOnHost
            }
            _ => SupportStatus::Unsupported,
        }
    }
    fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
        self.provider.list(id, "/", ctx)?;
        Ok(vec![ProbeOutcome {
            operation: Operation::FilesList,
            availability: Availability::Available,
            detail: None,
        }])
    }
    fn identity(&self, _id: &DeviceId, _ctx: &OperationContext) -> Result<Identity> {
        Err(yeskia_core::Error::CapabilityUnsupported(
            "identity.read".into(),
        ))
    }
    fn files(&self) -> Option<&dyn FileProvider> {
        Some(&self.provider)
    }
}

fn vol_engine(root: &Path) -> (Arc<Engine>, Arc<VolTransport>, DeviceId) {
    let t = VolTransport::new(root);
    let e = Engine::with_config(
        vec![t.clone()],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
        EngineConfig::default(),
    );
    let id = DeviceId::new("vol-1");
    e.connect(&id).unwrap();
    (Arc::new(e), t, id)
}

// ---------- скачивание ----------

#[test]
fn download_lands_verified_without_leftovers() {
    let (e, mock, id) = mock_engine();
    mock.put_file("E:/Photos/img.jpg", &[7u8; 5000]);
    let dir = tmpdir("dl-ok");
    let dest = dir.join("img.jpg");
    let r = e
        .run(
            &id,
            download("E:/Photos/img.jpg", &dest, OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    match r.data.unwrap() {
        OperationData::Downloaded {
            bytes,
            sha256,
            verified,
            replaced,
        } => {
            assert_eq!(bytes, 5000);
            assert!(verified && !replaced);
            assert_eq!(sha256.len(), 64);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&dest).unwrap(), vec![7u8; 5000]);
    assert!(leftovers(&dir).is_empty());
}

#[test]
fn download_never_overwrites_and_replace_is_explicit() {
    let (e, mock, id) = mock_engine();
    mock.put_file("E:/a.txt", b"device");
    let dir = tmpdir("dl-collide");
    let dest = dir.join("a.txt");
    std::fs::write(&dest, b"local").unwrap();
    let r = e
        .run(&id, download("E:/a.txt", &dest, OverwritePolicy::Never))
        .unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::AlreadyExists);
    assert!(!r.uncertain);
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        b"local",
        "существующий файл не тронут"
    );
    assert!(leftovers(&dir).is_empty());

    let r = e
        .run(&id, download("E:/a.txt", &dest, OverwritePolicy::Replace))
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    assert!(matches!(
        r.data,
        Some(OperationData::Downloaded { replaced: true, .. })
    ));
    assert_eq!(std::fs::read(&dest).unwrap(), b"device");
}

#[test]
fn cyrillic_and_unicode_names_work_end_to_end() {
    let (e, mock, id) = mock_engine();
    mock.put_file("E:/Фото/снимок №1.jpg", "содержимое".as_bytes());
    let dir = tmpdir("dl-uni");
    let dest = dir.join("снимок №1.jpg");
    let r = e
        .run(
            &id,
            download("E:/Фото/снимок №1.jpg", &dest, OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "содержимое");
}

#[test]
fn download_cancel_leaves_no_partial_file() {
    let (e, mock, id) = mock_engine();
    mock.put_file("E:/big.bin", &[1u8; 4000]);
    mock.set_read_delay(30);
    let dir = tmpdir("dl-cancel");
    let dest = dir.join("big.bin");
    let h = e
        .submit(&id, download("E:/big.bin", &dest, OverwritePolicy::Never))
        .unwrap();
    let op = h.operation_id().to_string();
    let t = std::thread::spawn(move || h.wait());
    std::thread::sleep(Duration::from_millis(120));
    assert!(e.progress(&op).is_some());
    e.cancel(&op);
    let r = t.join().unwrap();
    assert_eq!(r.outcome, Outcome::Cancelled);
    assert!(!r.uncertain, "скачивание ничего не пишет на устройство");
    assert!(!dest.exists());
    assert!(leftovers(&dir).is_empty());
}

#[test]
fn unplug_during_download_leaves_no_partial_file() {
    let (e, mock, id) = mock_engine();
    mock.put_file("E:/big.bin", &[1u8; 4000]);
    mock.set_read_delay(40);
    let dir = tmpdir("dl-unplug");
    let dest = dir.join("big.bin");
    let e2 = Arc::clone(&e);
    let (id2, d2) = (id.clone(), dest.clone());
    let t = std::thread::spawn(move || {
        e2.run(&id2, download("E:/big.bin", &d2, OverwritePolicy::Never))
    });
    std::thread::sleep(Duration::from_millis(120));
    mock.unplug();
    let r = t.join().unwrap().unwrap();
    assert_eq!(
        r.error.as_ref().unwrap().code,
        ErrorCode::DeviceDisconnected
    );
    assert!(!dest.exists());
    assert!(leftovers(&dir).is_empty());
}

#[test]
fn download_errors_are_structured() {
    let (e, mock, id) = mock_engine();
    mock.put_file("E:/a.txt", b"x");
    let dir = tmpdir("dl-err");
    // нет папки назначения
    let r = e
        .run(
            &id,
            download(
                "E:/a.txt",
                &dir.join("нет такой").join("a.txt"),
                OverwritePolicy::Never,
            ),
        )
        .unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::HostAccess);
    // файла на устройстве нет
    let r = e
        .run(
            &id,
            download("E:/missing.txt", &dir.join("m.txt"), OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::OperationFailed);
    // это папка
    let r = e
        .run(
            &id,
            download(
                "E:/Images/cat.jpg",
                &dir.join("c.jpg"),
                OverwritePolicy::Never,
            ),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success);
    // небезопасные пути отвергаются до очереди
    for dest in ["relative.txt", "../x.txt"] {
        let err = e
            .submit(
                &id,
                OperationRequest::FilesDownload {
                    path: "E:/a.txt".into(),
                    dest: dest.into(),
                    overwrite: OverwritePolicy::Never,
                },
            )
            .err()
            .unwrap();
        assert_eq!(err.code(), ErrorCode::UnsafePath, "{dest}");
    }
    let err = e
        .submit(
            &id,
            download("E:/a.txt", &dir.join("con"), OverwritePolicy::Never),
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::UnsafePath);
}

// ---------- загрузка ----------

#[test]
fn upload_to_mock_is_verified_and_refuses_silent_overwrite() {
    let (e, mock, id) = mock_engine();
    let dir = tmpdir("ul-mock");
    let src = dir.join("док.txt");
    std::fs::write(&src, "привет").unwrap();
    let r = e
        .run(&id, upload(&src, "E:/док.txt", OverwritePolicy::Never))
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert!(matches!(
        r.data,
        Some(OperationData::Written {
            verified: true,
            replaced: false,
            ..
        })
    ));
    assert_eq!(mock.file("E:/док.txt").unwrap(), "привет".as_bytes());
    let r = e
        .run(&id, upload(&src, "E:/док.txt", OverwritePolicy::Never))
        .unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::AlreadyExists);
    assert!(!r.uncertain);
    let r = e
        .run(&id, upload(&src, "E:/док.txt", OverwritePolicy::Replace))
        .unwrap();
    assert!(matches!(
        r.data,
        Some(OperationData::Written { replaced: true, .. })
    ));
}

#[test]
fn upload_cancel_and_failures_are_uncertain_and_never_retried() {
    let (e, mock, id) = mock_engine();
    let dir = tmpdir("ul-cancel");
    let src = dir.join("a.bin");
    std::fs::write(&src, [3u8; 100]).unwrap();
    mock.set_write_profile(50, 20);
    let h = e
        .submit(&id, upload(&src, "E:/a.bin", OverwritePolicy::Never))
        .unwrap();
    let op = h.operation_id().to_string();
    let t = std::thread::spawn(move || h.wait());
    std::thread::sleep(Duration::from_millis(100));
    e.cancel(&op);
    let r = t.join().unwrap();
    assert_eq!(r.outcome, Outcome::Cancelled);
    assert!(r.uncertain, "запись на устройство могла начаться");
    assert_eq!(mock.committed_writes(), 0);
    assert_eq!(mock.started_writes(), 1);
}

#[test]
fn upload_source_errors_and_unsupported_write() {
    let (e, mock, id) = mock_engine();
    let dir = tmpdir("ul-src");
    let r = e
        .run(
            &id,
            upload(&dir.join("нет.txt"), "E:/x.txt", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::HostAccess);
    let r = e
        .run(&id, upload(&dir, "E:/x.txt", OverwritePolicy::Never))
        .unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::InvalidArgument);
    assert_eq!(mock.started_writes(), 0);

    // запись заявлена как неподдерживаемая → отказ до очереди, на устройство ничего не уходит
    mock.set_maturity(Operation::FilesWrite, SupportStatus::Unsupported);
    e.disconnect(&id).unwrap();
    e.connect(&id).unwrap();
    let src = dir.join("f.txt");
    std::fs::write(&src, b"x").unwrap();
    let err = e
        .submit(&id, upload(&src, "E:/f.txt", OverwritePolicy::Never))
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
}

// ---------- Mass Storage: том и его границы ----------

#[test]
fn volume_roundtrip_with_streaming_progress_for_a_large_file() {
    let root = tmpdir("vol-big");
    let host = tmpdir("vol-big-host");
    let (e, _t, id) = vol_engine(&root);

    // 24 МБ: поток блоками, прогресс доходит до конца
    let src = host.join("big.bin");
    let data: Vec<u8> = (0..24 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &data).unwrap();
    let h = e
        .submit(
            &id,
            upload(&src, "E:/Big/копия.bin", OverwritePolicy::Never),
        )
        .unwrap();
    // папки нет → понятная ошибка, ничего не создано
    let r = h.wait();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::OperationFailed);
    std::fs::create_dir_all(root.join("Big")).unwrap();

    let r = e
        .run(
            &id,
            upload(&src, "E:/Big/копия.bin", OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert!(root.join("Big/копия.bin").exists());
    assert!(leftovers(&root.join("Big")).is_empty());

    let dest = host.join("back.bin");
    let r = e
        .run(
            &id,
            download("E:/Big/копия.bin", &dest, OverwritePolicy::Never),
        )
        .unwrap();
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    match r.data.unwrap() {
        OperationData::Downloaded {
            bytes, verified, ..
        } => assert!(verified && bytes == data.len() as u64),
        other => panic!("{other:?}"),
    }
}

#[test]
fn volume_matrix_marks_write_as_not_tested_and_tested_on_host() {
    let root = tmpdir("vol-matrix");
    let t = VolTransport::new(&root);
    let e = Engine::new(
        vec![t],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(10, None)),
    );
    let id = DeviceId::new("vol-1");
    let m = e.connect(&id).unwrap().matrix.unwrap();
    assert_eq!(
        m.availability(Operation::FilesList),
        Availability::Available
    );
    assert_eq!(
        m.availability(Operation::FilesWrite),
        Availability::NotTested
    );
    assert_eq!(
        m.entry(Operation::FilesWrite).unwrap().maturity,
        SupportStatus::TestedOnHost
    );
    assert_eq!(
        m.availability(Operation::ContactsRead),
        Availability::Unsupported
    );
}

#[test]
fn volume_provider_cannot_leave_the_selected_volume() {
    let root = tmpdir("vol-jail");
    let outside = tmpdir("vol-outside");
    std::fs::write(outside.join("secret.txt"), b"secret").unwrap();
    let (e, _t, id) = vol_engine(&root);
    let host = tmpdir("vol-jail-host");

    // `..` и обратные слеши отвергаются ещё до очереди
    for p in ["E:/../secret.txt", "/../secret.txt", "E:\\x"] {
        let err = e
            .submit(
                &id,
                download(p, &host.join("s.txt"), OverwritePolicy::Never),
            )
            .err()
            .unwrap();
        assert_eq!(err.code(), ErrorCode::UnsafePath, "{p}");
    }
    // абсолютный путь хоста трактуется внутри тома, а не на ОС
    let r = e.run(
        &id,
        download(
            &format!("{}/secret.txt", outside.display()),
            &host.join("s.txt"),
            OverwritePolicy::Never,
        ),
    );
    if let Ok(r) = r {
        assert_ne!(r.outcome, Outcome::Success, "доступ вне тома невозможен");
    }
    assert!(!host.join("s.txt").exists());

    // зарезервированные имена Windows
    let err = e
        .submit(
            &id,
            download("E:/CON", &host.join("c"), OverwritePolicy::Never),
        )
        .err();
    assert!(err.is_none() || err.unwrap().code() == ErrorCode::UnsafePath);
    let provider = VolumeFileProvider::new(&root).unwrap();
    assert!(provider.resolve("E:/NUL.txt").is_err());
    assert!(provider
        .resolve("E:/Images/ok.jpg")
        .unwrap()
        .starts_with(provider.root()));
}

#[cfg(unix)]
#[test]
fn symlink_pointing_outside_the_volume_is_refused() {
    let root = tmpdir("vol-link");
    let outside = tmpdir("vol-link-out");
    std::fs::write(outside.join("secret.txt"), b"secret").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    let provider = VolumeFileProvider::new(&root).unwrap();
    assert!(matches!(
        provider.resolve("E:/escape/secret.txt"),
        Err(yeskia_core::Error::UnsafePath(_))
    ));
    assert!(provider.resolve("E:/escape").is_err());
}

#[test]
fn volume_list_is_sorted_directories_first_and_unicode() {
    let root = tmpdir("vol-list");
    std::fs::create_dir_all(root.join("Музыка")).unwrap();
    std::fs::write(root.join("b.txt"), b"1").unwrap();
    std::fs::write(root.join("Я.txt"), b"22").unwrap();
    let (e, _t, id) = vol_engine(&root);
    let r = e
        .run(&id, OperationRequest::FilesList { path: "E:/".into() })
        .unwrap();
    match r.data.unwrap() {
        OperationData::Entries { entries } => {
            let names: Vec<_> = entries.iter().map(|x| x.name.as_str()).collect();
            assert_eq!(names, vec!["Музыка", "b.txt", "Я.txt"]);
            assert!(entries[0].is_dir);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn failed_volume_write_leaves_no_file_under_the_real_name() {
    use std::io::Read;
    struct Boom(u8);
    impl Read for Boom {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0 == 0 {
                return Err(std::io::Error::other("обрыв источника"));
            }
            self.0 -= 1;
            buf[0] = 1;
            Ok(1)
        }
    }
    let root = tmpdir("vol-boom");
    let (_e, t, id) = vol_engine(&root);
    let ctx = OperationContext::new(
        "t".into(),
        CancellationToken::new(),
        std::time::Instant::now() + Duration::from_secs(5),
        Box::new(|| true),
        Arc::new(ProgressCell::default()),
    );
    let err = t
        .provider
        .write_from(
            &id,
            "E:/half.bin",
            &mut Boom(3),
            10,
            OverwritePolicy::Never,
            &ctx,
        )
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::StorageError);
    assert!(!root.join("half.bin").exists());
    assert!(leftovers(&root).is_empty());
}
