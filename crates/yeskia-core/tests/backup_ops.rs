//! Резервные копии через AppCore: задания без устройства и операции устройства (mock).

use std::path::{Path, PathBuf};
use std::time::Duration;
use yeskia_core::app::{AppCore, HostJobData, HostJobRequest};
use yeskia_core::backup::PlanStatus;
use yeskia_core::device::DeviceId;
use yeskia_core::error::ErrorCode;
use yeskia_core::mock::MOCK_DEVICE_ID;
use yeskia_core::operations::*;
use yeskia_core::storage::Settings;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-bko-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn app(root: &Path) -> AppCore {
    let app = AppCore::in_memory();
    app.save_settings(&Settings {
        backup_root: Some(root.display().to_string()),
        use_mock_device: true,
        ..Settings::default()
    })
    .unwrap();
    app
}

fn put(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

fn run(app: &AppCore, req: HostJobRequest) -> yeskia_core::app::HostJobResult {
    let id = app.start_host_job(req).unwrap();
    app.await_host_job(&id).unwrap()
}

fn dev() -> DeviceId {
    DeviceId::new(MOCK_DEVICE_ID)
}

// ---------- задания на компьютере: create → verify → dry-run → apply ----------

#[test]
fn local_folder_create_verify_dry_run_apply_only_new() {
    let (root, src, target) = (tmp("root"), tmp("src"), tmp("target"));
    put(&src, "Документы/отчёт.txt", "данные".as_bytes());
    put(&src, "a.bin", &[1u8; 3000]);
    let app = app(&root);

    let r = run(
        &app,
        HostJobRequest::BackupCreateLocal {
            source: src.display().to_string(),
            label: "мой диск".into(),
            exclude: vec![],
        },
    );
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    let name = match r.data.unwrap() {
        HostJobData::BackupCreated {
            name,
            files,
            verified,
            complete,
            ..
        } => {
            assert_eq!((files, verified, complete), (2, true, true));
            name
        }
        other => panic!("{other:?}"),
    };
    assert!(root.join(&name).join("manifest.json").exists());
    assert_eq!(app.backup_list().unwrap()[0].name, name);

    let v = run(&app, HostJobRequest::BackupVerify { name: name.clone() });
    assert!(matches!(v.data, Some(HostJobData::BackupVerified { report }) if report.ok));

    // dry-run: новые файлы, ничего не пишется
    put(&target, "a.bin", &[9u8; 3000]); // конфликт
    let plan = run(
        &app,
        HostJobRequest::BackupRestoreLocal {
            name: name.clone(),
            target: target.display().to_string(),
            only: None,
            apply: false,
        },
    );
    match plan.data.unwrap() {
        HostJobData::BackupPlan {
            counts,
            items,
            truncated,
        } => {
            assert_eq!((counts.new, counts.conflict, truncated), (1, 1, false));
            assert!(items
                .iter()
                .any(|i| i.path == "a.bin" && i.status == PlanStatus::Conflict));
        }
        other => panic!("{other:?}"),
    }
    assert!(!target.join("Документы").exists());

    // apply: только новое
    let ap = run(
        &app,
        HostJobRequest::BackupRestoreLocal {
            name,
            target: target.display().to_string(),
            only: None,
            apply: true,
        },
    );
    match ap.data.unwrap() {
        HostJobData::BackupApplied { report } => {
            assert_eq!((report.written, report.conflicts), (1, 1))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        std::fs::read(target.join("a.bin")).unwrap(),
        vec![9u8; 3000]
    );
    assert_eq!(
        std::fs::read_to_string(target.join("Документы/отчёт.txt")).unwrap(),
        "данные"
    );
}

#[test]
fn backup_names_from_the_client_cannot_be_paths() {
    let root = tmp("names");
    let app = app(&root);
    for bad in ["../x", "a/b", "C:\\x", "..", ""] {
        let e = app
            .start_host_job(HostJobRequest::BackupVerify { name: bad.into() })
            .unwrap_err();
        assert_eq!(e.code(), ErrorCode::UnsafePath, "{bad:?}");
    }
}

#[test]
fn restore_of_a_corrupt_backup_fails_with_a_structured_error() {
    let (root, src, target) = (tmp("cor-root"), tmp("cor-src"), tmp("cor-target"));
    put(&src, "a.txt", b"hello");
    let app = app(&root);
    let name = match run(
        &app,
        HostJobRequest::BackupCreateLocal {
            source: src.display().to_string(),
            label: "c".into(),
            exclude: vec![],
        },
    )
    .data
    .unwrap()
    {
        HostJobData::BackupCreated { name, .. } => name,
        o => panic!("{o:?}"),
    };
    std::fs::write(root.join(&name).join("files/a.txt"), b"HELLO").unwrap();
    let r = run(
        &app,
        HostJobRequest::BackupRestoreLocal {
            name,
            target: target.display().to_string(),
            only: None,
            apply: true,
        },
    );
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.error.unwrap().code, ErrorCode::BackupInvalid);
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
}

// ---------- устройство (mock) ----------

fn connected(root: &Path) -> AppCore {
    let app = app(root);
    app.engine.connect(&dev()).unwrap();
    app
}

fn op(app: &AppCore, req: OperationRequest) -> OperationResult {
    let id = app.start_operation(&dev(), req).unwrap();
    app.await_operation(&id).unwrap()
}

fn create_on_device(app: &AppCore) -> OperationResult {
    // dest от клиента игнорируется: каталог копии назначает ядро
    op(
        app,
        OperationRequest::BackupCreate {
            path: "E:/".into(),
            dest: "C:/ignored".into(),
            label: "mock".into(),
            exclude: vec![],
        },
    )
}

#[test]
fn device_backup_is_read_only_verified_and_stored_in_the_core_chosen_folder() {
    let root = tmp("dev-root");
    let app = connected(&root);
    let r = create_on_device(&app);
    assert_eq!(r.outcome, Outcome::Success, "{:?}", r.error);
    assert!(!r.uncertain, "чтение устройства — не запись");
    match r.data.unwrap() {
        OperationData::BackupCreated {
            files,
            verified,
            complete,
            ..
        } => assert_eq!((files, verified, complete), (2, true, true)),
        other => panic!("{other:?}"),
    }
    let list = app.backup_list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].source_kind, "device");
    assert!(!Path::new("C:/ignored").exists());
}

#[test]
fn device_restore_plan_then_apply_writes_only_new_files() {
    let root = tmp("dev-restore");
    let app = connected(&root);
    create_on_device(&app);
    let name = app.backup_list().unwrap()[0].name.clone();

    // план в новую папку устройства: всё «новое»
    let plan = op(
        &app,
        OperationRequest::BackupRestore {
            backup: name.clone(),
            path: "E:/Restored".into(),
            only: None,
            apply: false,
        },
    );
    match plan.data.unwrap() {
        OperationData::BackupPlan { counts, .. } => {
            assert_eq!((counts.new, counts.identical), (2, 0))
        }
        other => panic!("{other:?}"),
    }
    // применение
    let ap = op(
        &app,
        OperationRequest::BackupRestore {
            backup: name.clone(),
            path: "E:/Restored".into(),
            only: None,
            apply: true,
        },
    );
    assert_eq!(ap.outcome, Outcome::Success, "{:?}", ap.error);
    match ap.data.unwrap() {
        OperationData::BackupApplied {
            written, conflicts, ..
        } => assert_eq!((written, conflicts), (2, 0)),
        other => panic!("{other:?}"),
    }
    // повтор: всё уже на месте, ничего не перезаписывается
    let again = op(
        &app,
        OperationRequest::BackupRestore {
            backup: name,
            path: "E:/Restored".into(),
            only: None,
            apply: true,
        },
    );
    match again.data.unwrap() {
        OperationData::BackupApplied {
            written, identical, ..
        } => assert_eq!((written, identical), (0, 2)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn device_restore_conflicts_are_never_overwritten() {
    let root = tmp("dev-conf");
    let app = connected(&root);
    create_on_device(&app);
    let name = app.backup_list().unwrap()[0].name.clone();
    // «Restored/readme.txt» уже есть и отличается
    let src = tmp("dev-conf-src");
    put(&src, "x.txt", b"DIFFERENT");
    let up = op(
        &app,
        OperationRequest::FilesUpload {
            source: src.join("x.txt").display().to_string(),
            path: "E:/Restored/readme.txt".into(),
            overwrite: OverwritePolicy::Never,
        },
    );
    assert_eq!(up.outcome, Outcome::Success);
    let ap = op(
        &app,
        OperationRequest::BackupRestore {
            backup: name,
            path: "E:/Restored".into(),
            only: None,
            apply: true,
        },
    );
    match ap.data.unwrap() {
        OperationData::BackupApplied {
            written, conflicts, ..
        } => assert_eq!((written, conflicts), (1, 1)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn restore_apply_is_blocked_when_the_connection_cannot_write_but_plan_still_works() {
    let root = tmp("dev-ro");
    let app = app(&root);
    app.engine.connect(&dev()).unwrap();
    create_on_device(&app);
    let name = app.backup_list().unwrap()[0].name.clone();
    app.engine.disconnect(&dev()).unwrap();
    // новое соединение, где запись заявлена как неподдерживаемая (как у WPD)
    let mock = yeskia_core::mock::MockTransport::new();
    let _ = mock; // mock внутри AppCore недоступен; проверяем через Engine с отдельным mock ниже
    drop(app);

    let mock = yeskia_core::mock::MockTransport::new();
    mock.set_maturity(
        Operation::FilesWrite,
        yeskia_core::device::SupportStatus::Unsupported,
    );
    let engine = yeskia_core::engine::Engine::new(
        vec![std::sync::Arc::new(mock)],
        std::sync::Arc::new(yeskia_core::storage::Storage::open_in_memory().unwrap()),
        std::sync::Arc::new(yeskia_core::diagnostics::Diagnostics::new(10, None)),
    );
    engine.connect(&dev()).unwrap();
    let dir = root.join(&name).display().to_string();
    let err = engine
        .submit(
            &dev(),
            OperationRequest::BackupRestore {
                backup: dir.clone(),
                path: "E:/R".into(),
                only: None,
                apply: true,
            },
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
    let plan = engine
        .run(
            &dev(),
            OperationRequest::BackupRestore {
                backup: dir,
                path: "E:/R".into(),
                only: None,
                apply: false,
            },
        )
        .unwrap();
    assert_eq!(plan.outcome, Outcome::Success, "{:?}", plan.error);
}

#[test]
fn unplug_during_device_backup_leaves_an_interrupted_backup_and_no_hang() {
    let root = tmp("dev-unplug");
    let app = std::sync::Arc::new(connected(&root));
    // медленное чтение: отключаем посреди копирования
    let mock = app_mock(&app);
    mock.set_read_delay(60);
    let id = app
        .start_operation(
            &dev(),
            OperationRequest::BackupCreate {
                path: "E:/".into(),
                dest: String::new(),
                label: "unplug".into(),
                exclude: vec![],
            },
        )
        .unwrap();
    let a2 = std::sync::Arc::clone(&app);
    let waiter = std::thread::spawn(move || a2.await_operation(&id).unwrap());
    std::thread::sleep(Duration::from_millis(150));
    mock.unplug();
    let r = waiter.join().unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(
        r.error.as_ref().unwrap().code,
        ErrorCode::DeviceDisconnected
    );
    assert!(!r.uncertain);
    let list = app.backup_list().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].interrupted && !list[0].complete, "{:?}", list[0]);
}

fn app_mock(app: &AppCore) -> yeskia_core::mock::MockTransport {
    app.mock_handle()
}
