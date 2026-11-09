//! Файловые резервные копии: create → verify → restore-plan → apply (только новые).

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::{Path, PathBuf};
use yeskia_core::backup::*;
use yeskia_core::error::{Error, ErrorCode, Result};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "yeskia-bk-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok_check() -> Result<()> {
    Ok(())
}
fn no_progress(_: BackupProgress) {}

fn ctl() -> Control<'static> {
    Control {
        check: &ok_check,
        progress: &no_progress,
    }
}

fn put(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

fn make_source() -> PathBuf {
    let s = tmp("src");
    put(&s, "readme.txt", b"hello");
    put(&s, "Фото/снимок 1.jpg", &[7u8; 5000]);
    put(&s, "Музыка/Альбом/трек.mp3", &[3u8; 70_000]);
    put(&s, "empty.bin", b"");
    s
}

fn run_create(src: &Path, dest: &Path) -> BackupSummary {
    create_from_folder(src, dest, "тест", &Options::default(), &ctl()).unwrap()
}

// ---------- create / verify ----------

#[test]
fn create_verify_roundtrip_with_unicode_and_empty_files() {
    let (src, root) = (make_source(), tmp("dst"));
    let dest = root.join("копия-1");
    let sum = run_create(&src, &dest);
    assert_eq!(
        (sum.files, sum.skipped, sum.complete, sum.verified),
        (4, 0, true, true)
    );
    assert_eq!(sum.bytes, 5 + 5000 + 70_000);
    let m = load_manifest(&dest).unwrap();
    assert_eq!(m.schema, MANIFEST_SCHEMA);
    assert_eq!(m.source_kind, "folder");
    assert!(m
        .files
        .iter()
        .any(|f| f.path == "Фото/снимок 1.jpg" && f.size == 5000 && f.sha256.len() == 64));
    assert!(m.files.iter().any(|f| f.path == "empty.bin" && f.size == 0));
    // абсолютный путь источника в манифест не попадает
    let json = std::fs::read_to_string(dest.join("manifest.json")).unwrap();
    assert!(!json.contains(src.to_string_lossy().as_ref()));
    let v = verify(&dest, &ctl()).unwrap();
    assert!(v.ok && v.manifest_ok && v.files_checked == 4, "{v:?}");
}

#[test]
fn modification_time_is_preserved() {
    let (src, root) = (make_source(), tmp("mtime"));
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
    std::fs::File::options()
        .write(true)
        .open(src.join("readme.txt"))
        .unwrap()
        .set_modified(t)
        .unwrap();
    let dest = root.join("b");
    run_create(&src, &dest);
    let got = std::fs::metadata(dest.join("files/readme.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(got, t);
}

#[test]
fn large_file_is_streamed_and_verified() {
    let (src, root) = (tmp("big-src"), tmp("big-dst"));
    let data: Vec<u8> = (0..24 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    put(&src, "big.bin", &data);
    let sum = run_create(&src, &root.join("b"));
    assert!(sum.verified && sum.bytes == data.len() as u64);
}

#[test]
fn very_long_paths_are_supported() {
    let (src, root) = (tmp("long-src"), tmp("long-dst"));
    // ~320 символов относительного пути (сегменты ≤ 255), больше MAX_PATH на Windows
    let seg = "д".repeat(60);
    let rel = format!("{seg}/{seg}/{seg}/{seg}/{seg}/файл.txt");
    let p = yeskia_core::fsutil::long_path(&src.join(&rel));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, b"long").unwrap();
    let dest = root.join("b");
    let sum = run_create(&src, &dest);
    assert_eq!((sum.files, sum.verified), (1, true), "{sum:?}");
    assert!(verify(&dest, &ctl()).unwrap().ok);
}

#[test]
fn excluded_paths_and_unsafe_names_make_the_backup_incomplete() {
    let (src, root) = (make_source(), tmp("excl"));
    put(&src, "bad:name.txt", b"x"); // двоеточие допустимо на Linux, но не на Windows-хосте
    let opts = Options {
        exclude: vec!["Музыка".into()],
        ..Options::default()
    };
    let sum = create_from_folder(&src, &root.join("b"), "e", &opts, &ctl()).unwrap();
    assert!(!sum.complete && sum.skipped >= 2);
    let m = load_manifest(&root.join("b")).unwrap();
    assert!(!m.complete);
    assert!(m
        .skipped
        .iter()
        .any(|s| s.path == "Музыка" && s.reason == SkipReason::Excluded));
    assert!(m
        .skipped
        .iter()
        .any(|s| s.path == "bad:name.txt" && s.reason == SkipReason::UnsafeName));
    assert!(m.files.iter().all(|f| !f.path.starts_with("Музыка")));
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_followed_and_are_recorded() {
    let (src, root) = (make_source(), tmp("sym"));
    let outside = tmp("sym-out");
    put(&outside, "secret.txt", b"secret");
    std::os::unix::fs::symlink(&outside, src.join("escape")).unwrap();
    std::os::unix::fs::symlink(src.join("readme.txt"), src.join("link.txt")).unwrap();
    let sum = run_create(&src, &root.join("b"));
    assert!(!sum.complete);
    let m = load_manifest(&root.join("b")).unwrap();
    assert!(
        m.skipped
            .iter()
            .filter(|s| s.reason == SkipReason::Symlink)
            .count()
            == 2
    );
    assert!(m
        .files
        .iter()
        .all(|f| !f.path.contains("secret") && f.path != "link.txt"));
}

// ---------- отказы до записи ----------

#[test]
fn refuses_non_empty_destination_destination_inside_source_and_source_inside_destination() {
    let (src, root) = (make_source(), tmp("refuse"));
    std::fs::create_dir_all(root.join("busy")).unwrap();
    std::fs::write(root.join("busy/x"), b"x").unwrap();
    let e =
        create_from_folder(&src, &root.join("busy"), "x", &Options::default(), &ctl()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::AlreadyExists);
    let e = create_from_folder(&src, &src.join("backup"), "x", &Options::default(), &ctl())
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::UnsafePath);
    assert!(
        !src.join("backup").exists(),
        "внутри источника ничего не создано"
    );
    let e = create_from_folder(
        &src,
        src.parent().unwrap(),
        "x",
        &Options::default(),
        &ctl(),
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::UnsafePath);
    let e = create_from_folder(
        &src,
        Path::new("relative"),
        "x",
        &Options::default(),
        &ctl(),
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::UnsafePath);
}

#[test]
fn refuses_when_limits_or_free_space_are_exceeded() {
    let (src, root) = (make_source(), tmp("limits"));
    let e = create_from_folder(
        &src,
        &root.join("a"),
        "x",
        &Options {
            max_files: 2,
            ..Options::default()
        },
        &ctl(),
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    let e = create_from_folder(
        &src,
        &root.join("b"),
        "x",
        &Options {
            max_bytes: 100,
            ..Options::default()
        },
        &ctl(),
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    // запас 5 %: ровно по размеру места недостаточно
    let total = 5 + 5000 + 70_000;
    let e = create_from_folder(
        &src,
        &root.join("c"),
        "x",
        &Options {
            available_space: Some(total),
            ..Options::default()
        },
        &ctl(),
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::InsufficientSpace);
    create_from_folder(
        &src,
        &root.join("d"),
        "x",
        &Options {
            available_space: Some(total + total / 20),
            ..Options::default()
        },
        &ctl(),
    )
    .unwrap();
}

// ---------- verify ловит порчу ----------

fn fresh() -> (PathBuf, PathBuf, PathBuf) {
    let (src, root) = (make_source(), tmp("vfy"));
    let dest = root.join("b");
    run_create(&src, &dest);
    (src, root, dest)
}

#[test]
fn verify_detects_corruption_missing_extra_and_tampered_manifest() {
    let (_s, _r, dest) = fresh();
    // повреждён файл
    std::fs::write(dest.join("files/readme.txt"), b"HELLO").unwrap();
    // пропал файл
    std::fs::remove_file(dest.join("files/Фото/снимок 1.jpg")).unwrap();
    // лишний файл
    put(&dest.join("files"), "лишний.txt", b"extra");
    let v = verify(&dest, &ctl()).unwrap();
    assert!(!v.ok && v.manifest_ok);
    assert_eq!(v.corrupt, vec!["readme.txt"]);
    assert_eq!(v.missing, vec!["Фото/снимок 1.jpg"]);
    assert_eq!(v.extra, vec!["лишний.txt"]);

    // подмена манифеста (хеш не совпадает)
    let m = std::fs::read_to_string(dest.join("manifest.json"))
        .unwrap()
        .replace("тест", "подмена");
    std::fs::write(dest.join("manifest.json"), m).unwrap();
    let v = verify(&dest, &ctl()).unwrap();
    assert!(!v.ok && !v.manifest_ok);
    assert_eq!(
        load_manifest(&dest).unwrap_err().code(),
        ErrorCode::BackupInvalid
    );
}

#[test]
fn tampered_manifest_with_recomputed_hash_cannot_escape_the_target() {
    let (_s, _r, dest) = fresh();
    // злоумышленник правит манифест и пересчитывает manifest.sha256, подставляя `..`
    let json = std::fs::read_to_string(dest.join("manifest.json"))
        .unwrap()
        .replace("readme.txt", "../escape.txt");
    std::fs::write(dest.join("manifest.json"), &json).unwrap();
    std::fs::write(
        dest.join("manifest.sha256"),
        yeskia_core::fsutil::sha256_hex(json.as_bytes()),
    )
    .unwrap();
    let v = verify(&dest, &ctl()).unwrap();
    assert_eq!(v.unsafe_paths, vec!["../escape.txt"]);
    assert!(!v.ok);
    let target = tmp("esc-target");
    let store = FolderStore::new(&target).unwrap();
    let plan = restore_plan(&dest, &store, None, &ctl()).unwrap();
    assert_eq!(plan.counts.blocked, 1);
    let e = restore_apply(&dest, &store, None, &ctl()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::BackupInvalid);
    assert!(!target.parent().unwrap().join("escape.txt").exists());
}

// ---------- restore ----------

#[test]
fn restore_plan_classifies_new_identical_conflict_and_blocked() {
    let (_s, _r, dest) = fresh();
    let target = tmp("rst");
    put(&target, "readme.txt", b"hello"); // идентичен
    put(&target, "Фото/снимок 1.jpg", &[9u8; 5000]); // конфликт
    std::fs::create_dir_all(target.join("empty.bin")).unwrap(); // на месте файла — папка → blocked
    let store = FolderStore::new(&target).unwrap();
    let plan = restore_plan(&dest, &store, None, &ctl()).unwrap();
    let c = &plan.counts;
    assert_eq!(
        (c.new, c.identical, c.conflict, c.blocked),
        (1, 1, 1, 1),
        "{plan:?}"
    );
    let st = |p: &str| plan.items.iter().find(|i| i.path == p).unwrap().status;
    assert_eq!(st("Музыка/Альбом/трек.mp3"), PlanStatus::New);
    assert_eq!(st("readme.txt"), PlanStatus::Identical);
    assert_eq!(st("Фото/снимок 1.jpg"), PlanStatus::Conflict);
    assert_eq!(st("empty.bin"), PlanStatus::Blocked);
    // план ничего не пишет
    assert!(!target.join("Музыка").exists());
}

#[test]
fn restore_apply_writes_only_new_files_never_overwrites_and_never_deletes() {
    let (_s, _r, dest) = fresh();
    let target = tmp("apply");
    put(&target, "Фото/снимок 1.jpg", &[9u8; 5000]); // конфликт — должен остаться как есть
    put(&target, "мой-файл.txt", b"mine"); // лишний — не удаляется
    let store = FolderStore::new(&target).unwrap();
    let r = restore_apply(&dest, &store, None, &ctl()).unwrap();
    assert_eq!(
        (r.written, r.conflicts, r.identical, r.blocked),
        (3, 1, 0, 0),
        "{r:?}"
    );
    assert_eq!(
        std::fs::read(target.join("Фото/снимок 1.jpg")).unwrap(),
        vec![9u8; 5000]
    );
    assert_eq!(std::fs::read(target.join("мой-файл.txt")).unwrap(), b"mine");
    assert_eq!(
        std::fs::read(target.join("Музыка/Альбом/трек.mp3")).unwrap(),
        vec![3u8; 70_000]
    );
    assert_eq!(std::fs::read(target.join("readme.txt")).unwrap(), b"hello");
    assert!(std::fs::read_dir(&target).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("yeskia-part")));
    // повтор: всё уже совпадает/конфликтует, новых нет
    let again = restore_apply(&dest, &store, None, &ctl()).unwrap();
    assert_eq!((again.written, again.identical, again.conflicts), (0, 3, 1));
}

#[test]
fn restore_only_selected_path() {
    let (_s, _r, dest) = fresh();
    let target = tmp("only");
    let store = FolderStore::new(&target).unwrap();
    let r = restore_apply(&dest, &store, Some("Фото"), &ctl()).unwrap();
    assert_eq!(r.written, 1);
    assert!(target.join("Фото/снимок 1.jpg").exists());
    assert!(!target.join("readme.txt").exists());
}

#[test]
fn restore_refuses_a_corrupt_backup() {
    let (_s, _r, dest) = fresh();
    std::fs::write(dest.join("files/readme.txt"), b"HELLO").unwrap();
    let target = tmp("corrupt");
    let store = FolderStore::new(&target).unwrap();
    let e = restore_apply(&dest, &store, None, &ctl()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::BackupInvalid);
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "ничего не записано"
    );
    // но выбор неповреждённой части допустим
    restore_apply(&dest, &store, Some("Фото"), &ctl()).unwrap();
}

#[cfg(unix)]
#[test]
fn restore_target_symlink_pointing_outside_is_refused() {
    let (_s, _r, dest) = fresh();
    let target = tmp("tsym");
    let outside = tmp("tsym-out");
    std::os::unix::fs::symlink(&outside, target.join("Фото")).unwrap();
    let store = FolderStore::new(&target).unwrap();
    let plan = restore_plan(&dest, &store, None, &ctl()).unwrap();
    assert!(plan.counts.blocked >= 1, "{plan:?}");
    let _ = restore_apply(&dest, &store, None, &ctl());
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "наружу ничего не записано"
    );
}

// ---------- отмена, отключение, изменение во время копирования ----------

#[test]
fn cancellation_marks_the_backup_interrupted_and_incomplete() {
    let (src, root) = (make_source(), tmp("cancel"));
    let calls = Cell::new(0);
    let check = || {
        calls.set(calls.get() + 1);
        if calls.get() > 8 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    };
    let ctl = Control {
        check: &check,
        progress: &no_progress,
    };
    let dest = root.join("b");
    let e = create_from_folder(&src, &dest, "x", &Options::default(), &ctl).unwrap_err();
    assert_eq!(e.code(), ErrorCode::OperationCancelled);
    let m = load_manifest(&dest).unwrap();
    assert!(m.interrupted && !m.complete, "{m:?}");
    assert!(m.files.len() < 4);
    // скопированное цело и проверяется; временных файлов не осталось
    assert!(verify(&dest, &self::ctl()).unwrap().ok);
    let info = list_backups(&root);
    assert!(info[0].interrupted && !info[0].complete);
}

/// Источник, у которого один файл «меняется» при копировании, а устройство может «отключиться».
struct Flaky {
    inner: FolderStore,
    change: &'static str,
    disconnect_after_reads: Cell<i32>,
    stats: RefCell<std::collections::HashMap<String, u32>>,
}

impl BackupSource for Flaky {
    fn kind(&self) -> &'static str {
        "device"
    }
    fn list(&self, dir: &str) -> Result<Vec<SrcEntry>> {
        self.inner.list(dir)
    }
    fn stat(&self, rel: &str) -> Result<Option<SrcEntry>> {
        let mut e = self.inner.stat(rel)?;
        if rel == self.change {
            let n = {
                let mut m = self.stats.borrow_mut();
                let c = m.entry(rel.into()).or_insert(0);
                *c += 1;
                *c
            };
            if n >= 2 {
                if let Some(x) = e.as_mut() {
                    x.size += 1;
                } // размер изменился после чтения
            }
        }
        Ok(e)
    }
    fn read_to(&self, rel: &str, sink: &mut dyn Write, ctl: &Control) -> Result<u64> {
        if self.disconnect_after_reads.get() == 0 {
            return Err(Error::DeviceDisconnected);
        }
        self.disconnect_after_reads
            .set(self.disconnect_after_reads.get() - 1);
        self.inner.read_to(rel, sink, ctl)
    }
}

#[test]
fn file_changed_during_copy_is_skipped_and_not_in_the_manifest() {
    let (src, root) = (make_source(), tmp("chg"));
    let flaky = Flaky {
        inner: FolderStore::new(&src).unwrap(),
        change: "readme.txt",
        disconnect_after_reads: Cell::new(i32::MAX),
        stats: RefCell::new(Default::default()),
    };
    let dest = root.join("b");
    let sum = create(&flaky, &dest, "x", &Options::default(), &ctl()).unwrap();
    assert!(!sum.complete);
    let m = load_manifest(&dest).unwrap();
    assert!(m
        .skipped
        .iter()
        .any(|s| s.path == "readme.txt" && s.reason == SkipReason::ChangedDuringCopy));
    assert!(m.files.iter().all(|f| f.path != "readme.txt"));
    assert!(
        !dest.join("files/readme.txt").exists(),
        "изменившийся файл не остаётся в копии"
    );
    assert!(verify(&dest, &ctl()).unwrap().ok);
}

#[test]
fn unplug_during_backup_aborts_with_an_interrupted_manifest() {
    let (src, root) = (make_source(), tmp("unplug"));
    let flaky = Flaky {
        inner: FolderStore::new(&src).unwrap(),
        change: "",
        disconnect_after_reads: Cell::new(2),
        stats: RefCell::new(Default::default()),
    };
    let dest = root.join("b");
    let e = create(&flaky, &dest, "x", &Options::default(), &ctl()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::DeviceDisconnected);
    let m = load_manifest(&dest).unwrap();
    assert!(m.interrupted && !m.complete);
    assert_eq!(m.files.len(), 2);
    assert!(
        verify(&dest, &ctl()).unwrap().ok,
        "то, что успели скопировать, цело"
    );
}

#[test]
fn restore_cancellation_leaves_no_partial_files() {
    let (_s, _r, dest) = fresh();
    let target = tmp("rcancel");
    let store = FolderStore::new(&target).unwrap();
    let calls = Cell::new(0);
    let check = || {
        calls.set(calls.get() + 1);
        if calls.get() > 14 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    };
    let ctl = Control {
        check: &check,
        progress: &no_progress,
    };
    let e = restore_apply(&dest, &store, None, &ctl).unwrap_err();
    assert_eq!(e.code(), ErrorCode::OperationCancelled);
    fn all_files(d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                all_files(&p, out)
            } else {
                out.push(p.file_name().unwrap().to_string_lossy().into_owned())
            }
        }
    }
    let mut names = Vec::new();
    all_files(&target, &mut names);
    assert!(
        names.iter().all(|n| !n.contains("yeskia-part")),
        "{names:?}"
    );
}

// ---------- список и имена ----------

#[test]
fn list_backups_shows_newest_first_and_flags_a_broken_manifest() {
    let (src, root) = (make_source(), tmp("list"));
    run_create(&src, &root.join("a"));
    std::thread::sleep(std::time::Duration::from_millis(5));
    run_create(&src, &root.join("b"));
    std::fs::create_dir_all(root.join("не-копия")).unwrap();
    let list = list_backups(&root);
    assert_eq!(list.len(), 2);
    assert!(list[0].created_at_ms >= list[1].created_at_ms);
    std::fs::write(root.join("a/manifest.json"), b"{}").unwrap();
    let list = list_backups(&root);
    assert!(list.iter().any(|b| b.name == "a" && !b.manifest_ok));
}

#[test]
fn backup_dir_names_are_safe_and_sorted_by_time() {
    let n = backup_dir_name(1_700_000_000_000, "e52: карта/памяти?");
    assert_eq!(n, "20231114-221320-e52--карта-памяти");
    yeskia_core::fsutil::validate_file_name(&n).unwrap();
    assert_eq!(backup_dir_name(1_700_000_000_000, "  "), "20231114-221320");
}
