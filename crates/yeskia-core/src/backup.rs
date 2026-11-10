//! Локальные файловые резервные копии: create → verify → restore-plan → apply (только новые).
//!
//! **Файловая копия ≠ полный backup телефона.** SMS, контакты, календарь, реестр приложений,
//! `C:\` и прошивка сюда не входят, пока отдельный транспорт этого не подтвердил.
//!
//! Алгоритмы общие для источников «папка на компьютере» и «подключённое устройство» (через
//! `FileProvider`): копия потоковая, с SHA-256 каждого файла, манифестом `yeskia.backup/1` и защитой
//! манифеста (`manifest.sha256`). Restore по умолчанию — план; запись только новых файлов, без
//! перезаписи и удаления, с обратным чтением. Абсолютный путь источника в манифест не пишется.

use crate::device::DeviceId;
use crate::error::{Error, Result};
use crate::fsutil::{
    hash_file, long_path, place_file, sha256_hex, temp_path_for, validate_file_name, HashWriter,
    CHUNK,
};
use crate::operations::{OperationContext, OverwritePolicy};
use crate::transport::FileProvider;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MANIFEST_SCHEMA: &str = "yeskia.backup/1";
const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_HASH_FILE: &str = "manifest.sha256";
const FILES_DIR: &str = "files";
const MAX_DEPTH: usize = 64;
const MAX_REL_LEN: usize = 1024;

/// Лимиты по умолчанию: 200 000 файлов / 64 ГиБ, запас места 5 %.
#[derive(Debug, Clone)]
pub struct Options {
    pub max_files: u64,
    pub max_bytes: u64,
    /// Пути (относительно источника), которые пропускаются по просьбе пользователя.
    pub exclude: Vec<String>,
    /// Только для тестов: подмена свободного места.
    pub available_space: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_files: 200_000,
            max_bytes: 64 * 1024 * 1024 * 1024,
            exclude: Vec::new(),
            available_space: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Symlink,
    NotRegular,
    Excluded,
    Unreadable,
    ChangedDuringCopy,
    UnsafeName,
    PathTooLong,
    TooDeep,
    Disappeared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// Относительный путь с `/`.
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub mtime_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub label: String,
    pub created_at_ms: u64,
    pub tool_version: String,
    /// `folder` или `device`: откуда сделана копия (без путей и идентификаторов).
    pub source_kind: String,
    /// `false`, если что-то пропущено или копия прервана.
    pub complete: bool,
    /// Копия прервана (отмена, отключение, ошибка): содержит только уже скопированное.
    pub interrupted: bool,
    pub total_files: u64,
    pub total_bytes: u64,
    pub files: Vec<FileRecord>,
    pub skipped: Vec<Skipped>,
    pub exclude: Vec<String>,
}

// ---------- управление выполнением ----------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BackupProgress {
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Проверка отмены/таймаута/отключения и приём прогресса (без имён файлов).
pub struct Control<'a> {
    pub check: &'a dyn Fn() -> Result<()>,
    pub progress: &'a dyn Fn(BackupProgress),
}

impl Control<'_> {
    fn ok(&self) -> Result<()> {
        (self.check)()
    }
}

// ---------- источник и цель ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrcEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub mtime_ms: Option<u64>,
}

pub trait BackupSource {
    fn kind(&self) -> &'static str;
    /// Содержимое папки (`""` — корень).
    fn list(&self, dir: &str) -> Result<Vec<SrcEntry>>;
    fn stat(&self, rel: &str) -> Result<Option<SrcEntry>>;
    fn read_to(&self, rel: &str, sink: &mut dyn Write, ctl: &Control) -> Result<u64>;
}

pub trait RestoreTarget: BackupSource {
    fn mkdir_all(&self, rel_dir: &str, ctl: &Control) -> Result<()>;
    /// Создаёт файл **только если его нет**; перезапись запрещена.
    fn write_new(&self, rel: &str, src: &mut dyn Read, len: u64, ctl: &Control) -> Result<()>;
}

fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Проверка относительного пути из манифеста/источника: без `..`, абсолютных путей, `:`, `\`,
/// зарезервированных имён Windows.
pub fn validate_rel_path(rel: &str) -> Result<()> {
    if rel.is_empty() || rel.len() > MAX_REL_LEN || rel.starts_with('/') || rel.contains('\\') {
        return Err(Error::UnsafePath("недопустимый относительный путь".into()));
    }
    for seg in rel.split('/') {
        validate_file_name(seg)?;
    }
    Ok(())
}

fn mtime_ms(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

// ---------- папка на компьютере ----------

/// Папка на компьютере как источник и цель. Корень канонизируется; пути не выходят за него
/// (симлинки наружу отвергаются), симлинки внутри не копируются.
pub struct FolderStore {
    root: PathBuf,
}

impl FolderStore {
    pub fn new(root: &Path) -> Result<Self> {
        let root = std::fs::canonicalize(long_path(root))
            .map_err(|_| Error::HostAccess("папка не найдена или недоступна".into()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let mut p = self.root.clone();
        if !rel.is_empty() {
            validate_rel_path(rel)?;
            for seg in rel.split('/') {
                p.push(seg);
            }
        }
        // самый глубокий существующий предок обязан лежать внутри корня
        let mut cur = p.clone();
        loop {
            match std::fs::canonicalize(&cur) {
                Ok(real) => {
                    if !real.starts_with(&self.root) {
                        return Err(Error::UnsafePath("путь выходит за пределы папки".into()));
                    }
                    break;
                }
                Err(_) => {
                    if !cur.pop() {
                        return Err(Error::UnsafePath("путь вне папки".into()));
                    }
                }
            }
        }
        Ok(p)
    }
}

impl BackupSource for FolderStore {
    fn kind(&self) -> &'static str {
        "folder"
    }

    fn list(&self, dir: &str) -> Result<Vec<SrcEntry>> {
        let p = self.resolve(dir)?;
        let mut out = Vec::new();
        for e in std::fs::read_dir(&p)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            // symlink_metadata: ссылки не разыменовываются
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            let ft = meta.file_type();
            let kind = if ft.is_symlink() {
                EntryKind::Symlink
            } else if ft.is_dir() {
                EntryKind::Dir
            } else if ft.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            out.push(SrcEntry {
                name,
                kind,
                size: if ft.is_file() { meta.len() } else { 0 },
                mtime_ms: mtime_ms(meta.modified()),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn stat(&self, rel: &str) -> Result<Option<SrcEntry>> {
        let p = self.resolve(rel)?;
        match std::fs::symlink_metadata(&p) {
            Ok(meta) => {
                let ft = meta.file_type();
                let kind = if ft.is_symlink() {
                    EntryKind::Symlink
                } else if ft.is_dir() {
                    EntryKind::Dir
                } else if ft.is_file() {
                    EntryKind::File
                } else {
                    EntryKind::Other
                };
                Ok(Some(SrcEntry {
                    name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
                    kind,
                    size: if ft.is_file() { meta.len() } else { 0 },
                    mtime_ms: mtime_ms(meta.modified()),
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn read_to(&self, rel: &str, sink: &mut dyn Write, ctl: &Control) -> Result<u64> {
        let p = self.resolve(rel)?;
        let mut f = std::fs::File::open(&p)?;
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            ctl.ok()?;
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            sink.write_all(&buf[..n])?;
            total += n as u64;
        }
        Ok(total)
    }
}

impl RestoreTarget for FolderStore {
    fn mkdir_all(&self, rel_dir: &str, _ctl: &Control) -> Result<()> {
        let p = self.resolve(rel_dir)?;
        std::fs::create_dir_all(&p)?;
        Ok(())
    }

    fn write_new(&self, rel: &str, src: &mut dyn Read, len: u64, ctl: &Control) -> Result<()> {
        let dest = self.resolve(rel)?;
        if dest.exists() {
            return Err(Error::AlreadyExists);
        }
        let tmp = temp_path_for(&dest);
        let result = (|| -> Result<()> {
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            let mut buf = vec![0u8; CHUNK];
            let mut done = 0u64;
            loop {
                ctl.ok()?;
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                done += n as u64;
            }
            if done != len {
                return Err(Error::VerificationFailed);
            }
            out.flush()?;
            out.sync_all()?;
            place_file(&tmp, &dest, OverwritePolicy::Never)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

// ---------- устройство (через FileProvider) ----------

/// Устройство как источник/цель: пути корня — `root` (`E:/`, `/` …), все операции идут через
/// `FileProvider` текущей операции Engine (отмена, таймаут, прогресс, fingerprint).
pub struct DeviceStore<'a> {
    pub provider: &'a dyn FileProvider,
    pub id: &'a DeviceId,
    pub ctx: &'a OperationContext,
    pub root: String,
}

impl DeviceStore<'_> {
    fn path(&self, rel: &str) -> String {
        if rel.is_empty() {
            self.root.clone()
        } else if self.root.ends_with('/') {
            format!("{}{rel}", self.root)
        } else {
            format!("{}/{rel}", self.root)
        }
    }
}

impl BackupSource for DeviceStore<'_> {
    fn kind(&self) -> &'static str {
        "device"
    }

    fn list(&self, dir: &str) -> Result<Vec<SrcEntry>> {
        Ok(self
            .provider
            .list(self.id, &self.path(dir), self.ctx)?
            .into_iter()
            .map(|e| SrcEntry {
                name: e.name,
                kind: if e.is_dir {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                size: e.size,
                mtime_ms: None,
            })
            .collect())
    }

    fn stat(&self, rel: &str) -> Result<Option<SrcEntry>> {
        Ok(self
            .provider
            .stat(self.id, &self.path(rel), self.ctx)?
            .map(|e| SrcEntry {
                name: e.name,
                kind: if e.is_dir {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                size: e.size,
                mtime_ms: None,
            }))
    }

    fn read_to(&self, rel: &str, sink: &mut dyn Write, _ctl: &Control) -> Result<u64> {
        self.provider
            .read_to(self.id, &self.path(rel), sink, self.ctx)
    }
}

impl RestoreTarget for DeviceStore<'_> {
    fn mkdir_all(&self, rel_dir: &str, _ctl: &Control) -> Result<()> {
        let mut cur = String::new();
        for seg in rel_dir.split('/').filter(|s| !s.is_empty()) {
            cur = join_rel(&cur, seg);
            self.provider
                .create_dir(self.id, &self.path(&cur), self.ctx)?;
        }
        Ok(())
    }

    fn write_new(&self, rel: &str, src: &mut dyn Read, len: u64, _ctl: &Control) -> Result<()> {
        self.provider.write_from(
            self.id,
            &self.path(rel),
            src,
            len,
            OverwritePolicy::Never,
            self.ctx,
        )
    }
}

// ---------- создание ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSummary {
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub complete: bool,
    pub interrupted: bool,
    /// Копия сразу перечитана и сверена по хешам.
    pub verified: bool,
}

struct Planned {
    rel: String,
    size: u64,
    mtime_ms: Option<u64>,
}

fn is_excluded(rel: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|e| {
        let e = e.trim_matches('/');
        !e.is_empty() && (rel == e || rel.starts_with(&format!("{e}/")))
    })
}

fn scan(
    src: &dyn BackupSource,
    dir: &str,
    depth: usize,
    opts: &Options,
    ctl: &Control,
    planned: &mut Vec<Planned>,
    skipped: &mut Vec<Skipped>,
) -> Result<()> {
    ctl.ok()?;
    let mut entries = src.list(dir)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    for e in entries {
        let rel = join_rel(dir, &e.name);
        let skip = |skipped: &mut Vec<Skipped>, reason| {
            skipped.push(Skipped {
                path: rel.clone(),
                reason,
            })
        };
        if is_excluded(&rel, &opts.exclude) {
            skip(skipped, SkipReason::Excluded);
            continue;
        }
        if validate_file_name(&e.name).is_err() {
            skip(skipped, SkipReason::UnsafeName);
            continue;
        }
        if rel.len() > MAX_REL_LEN {
            skip(skipped, SkipReason::PathTooLong);
            continue;
        }
        match e.kind {
            EntryKind::Symlink => skip(skipped, SkipReason::Symlink),
            EntryKind::Other => skip(skipped, SkipReason::NotRegular),
            EntryKind::Dir => {
                if depth >= MAX_DEPTH {
                    skip(skipped, SkipReason::TooDeep);
                } else {
                    scan(src, &rel, depth + 1, opts, ctl, planned, skipped)?;
                }
            }
            EntryKind::File => {
                planned.push(Planned {
                    rel,
                    size: e.size,
                    mtime_ms: e.mtime_ms,
                });
                if planned.len() as u64 > opts.max_files {
                    return Err(Error::LimitExceeded(format!(
                        "больше {} файлов",
                        opts.max_files
                    )));
                }
            }
        }
    }
    Ok(())
}

fn write_manifest(dest: &Path, m: &Manifest) -> Result<()> {
    let json = serde_json::to_vec_pretty(m).map_err(|e| Error::Internal(e.to_string()))?;
    std::fs::write(long_path(&dest.join(MANIFEST_FILE)), &json)?;
    std::fs::write(long_path(&dest.join(MANIFEST_HASH_FILE)), sha256_hex(&json))?;
    Ok(())
}

/// Создаёт копию `src` в **новом или пустом** каталоге `dest`. При отмене/отключении/ошибке пишет
/// манифест `interrupted: true` с уже скопированным и возвращает ошибку.
pub fn create(
    src: &dyn BackupSource,
    dest: &Path,
    label: &str,
    opts: &Options,
    ctl: &Control,
) -> Result<BackupSummary> {
    if !dest.is_absolute() {
        return Err(Error::UnsafePath(
            "каталог копии должен быть абсолютным путём".into(),
        ));
    }
    let dest_l = long_path(dest);
    match std::fs::read_dir(&dest_l) {
        Ok(mut rd) => {
            if rd.next().is_some() {
                return Err(Error::AlreadyExists); // непустой каталог не трогаем
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&dest_l)?;
        }
        Err(e) => return Err(e.into()),
    }
    let mut planned = Vec::new();
    let mut skipped = Vec::new();
    ctl.ok()?;
    scan(src, "", 0, opts, ctl, &mut planned, &mut skipped)?;

    let total_bytes: u64 = planned.iter().map(|p| p.size).sum();
    if total_bytes > opts.max_bytes {
        return Err(Error::LimitExceeded(format!(
            "больше {} байт",
            opts.max_bytes
        )));
    }
    let available = match opts.available_space {
        Some(a) => a,
        None => fs2::available_space(&dest_l)
            .map_err(|e| Error::HostAccess(format!("свободное место: {e}")))?,
    };
    if total_bytes + total_bytes / 20 > available {
        return Err(Error::InsufficientSpace); // нужен запас 5 %
    }

    let files_dir = dest.join(FILES_DIR);
    std::fs::create_dir_all(long_path(&files_dir))?;

    let mut manifest = Manifest {
        schema: MANIFEST_SCHEMA.into(),
        label: label.to_string(),
        created_at_ms: crate::now_ms(),
        tool_version: env!("CARGO_PKG_VERSION").into(),
        source_kind: src.kind().into(),
        complete: false,
        interrupted: false,
        total_files: 0,
        total_bytes: 0,
        files: Vec::new(),
        skipped,
        exclude: opts.exclude.clone(),
    };
    let files_total = planned.len() as u64;
    let mut bytes_done = 0u64;

    let outcome = (|| -> Result<()> {
        for (i, p) in planned.iter().enumerate() {
            ctl.ok()?;
            match copy_one(src, &files_dir, p, ctl) {
                Ok(Some(rec)) => {
                    bytes_done += rec.size;
                    manifest.files.push(rec);
                }
                Ok(None) => manifest.skipped.push(Skipped {
                    path: p.rel.clone(),
                    reason: SkipReason::ChangedDuringCopy,
                }),
                Err(Error::NotFoundOnSource) => manifest.skipped.push(Skipped {
                    path: p.rel.clone(),
                    reason: SkipReason::Disappeared,
                }),
                Err(
                    e @ (Error::Cancelled
                    | Error::DeviceDisconnected
                    | Error::Timeout
                    | Error::FingerprintMismatch),
                ) => return Err(e),
                Err(Error::Storage(_)) | Err(Error::HostAccess(_)) => {
                    // не читается этот файл — фиксируем и идём дальше
                    manifest.skipped.push(Skipped {
                        path: p.rel.clone(),
                        reason: SkipReason::Unreadable,
                    })
                }
                Err(e) => return Err(e),
            }
            (ctl.progress)(BackupProgress {
                files_done: i as u64 + 1,
                files_total,
                bytes_done,
                bytes_total: total_bytes,
            });
        }
        Ok(())
    })();

    manifest.total_files = manifest.files.len() as u64;
    manifest.total_bytes = manifest.files.iter().map(|f| f.size).sum();
    manifest.interrupted = outcome.is_err();
    manifest.complete = outcome.is_ok() && manifest.skipped.is_empty();
    write_manifest(dest, &manifest)?;
    outcome?;

    // сразу перечитываем и сверяем
    let verified = verify(dest, ctl).map(|r| r.ok).unwrap_or(false);
    Ok(BackupSummary {
        files: manifest.total_files,
        bytes: manifest.total_bytes,
        skipped: manifest.skipped.len() as u64,
        complete: manifest.complete,
        interrupted: false,
        verified,
    })
}

/// Копирует один файл потоком; `Ok(None)` — изменился во время копирования (в манифест не попадает).
fn copy_one(
    src: &dyn BackupSource,
    files_dir: &Path,
    p: &Planned,
    ctl: &Control,
) -> Result<Option<FileRecord>> {
    let before = src.stat(&p.rel)?.ok_or(Error::NotFoundOnSource)?;
    let mut dest = files_dir.to_path_buf();
    let mut segs: Vec<&str> = p.rel.split('/').collect();
    let file_name = segs.pop().unwrap_or("");
    for s in &segs {
        dest.push(s);
    }
    std::fs::create_dir_all(long_path(&dest))?;
    dest.push(file_name);
    let dest_l = long_path(&dest);
    let tmp = temp_path_for(&dest_l);
    let result = (|| -> Result<(u64, String)> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        let mut hw = HashWriter::new(std::io::BufWriter::new(file), p.size, None);
        let n = src.read_to(&p.rel, &mut hw, ctl)?;
        let (bytes, sha, writer) = hw.finish()?;
        let file = writer
            .into_inner()
            .map_err(|e| Error::from(e.into_error()))?;
        if let Some(ms) = p.mtime_ms {
            let _ = file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms));
        }
        file.sync_all()?;
        if n != bytes {
            return Err(Error::VerificationFailed);
        }
        Ok((bytes, sha))
    })();
    let (bytes, sha) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    // изменился ли файл, пока копировали
    let after = src.stat(&p.rel)?;
    let changed = match after {
        None => true,
        Some(a) => {
            a.size != before.size
                || a.size != bytes
                || (before.mtime_ms.is_some() && a.mtime_ms != before.mtime_ms)
        }
    };
    if changed {
        let _ = std::fs::remove_file(&tmp);
        return Ok(None);
    }
    std::fs::rename(&tmp, &dest_l)?;
    Ok(Some(FileRecord {
        path: p.rel.clone(),
        size: bytes,
        sha256: sha,
        mtime_ms: p.mtime_ms,
    }))
}

/// Каноническая форма пути, который может ещё не существовать: канонизируется ближайший существующий
/// предок, остаток добавляется как есть.
fn resolve_future(p: &Path) -> PathBuf {
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(real) = std::fs::canonicalize(&cur) {
            return rest.iter().rev().fold(real, |acc, c| acc.join(c));
        }
        match cur.file_name().map(|n| n.to_os_string()) {
            Some(name) => {
                rest.push(name);
                cur.pop();
            }
            None => return p.to_path_buf(),
        }
    }
}

/// Создание копии из папки на компьютере: копия не должна лежать внутри источника и не должна
/// содержать его (иначе копия скопирует саму себя).
pub fn create_from_folder(
    source: &Path,
    dest: &Path,
    label: &str,
    opts: &Options,
    ctl: &Control,
) -> Result<BackupSummary> {
    let store = FolderStore::new(source)?;
    let dest_real = resolve_future(&long_path(dest));
    if dest_real.starts_with(store.root()) || store.root().starts_with(&dest_real) {
        return Err(Error::UnsafePath(
            "копия не должна находиться внутри источника или содержать его".into(),
        ));
    }
    create(&store, dest, label, opts, ctl)
}

// ---------- verify ----------

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub manifest_ok: bool,
    pub files_checked: u64,
    pub missing: Vec<String>,
    pub corrupt: Vec<String>,
    pub extra: Vec<String>,
    /// Пути из манифеста, которые небезопасны (traversal и т. п.).
    pub unsafe_paths: Vec<String>,
}

/// Читает и проверяет манифест: хеш `manifest.sha256`, схему, безопасность путей.
pub fn load_manifest(dir: &Path) -> Result<Manifest> {
    let bytes = std::fs::read(long_path(&dir.join(MANIFEST_FILE)))
        .map_err(|_| Error::BackupInvalid("манифест не найден".into()))?;
    let want = std::fs::read_to_string(long_path(&dir.join(MANIFEST_HASH_FILE)))
        .map_err(|_| Error::BackupInvalid("нет manifest.sha256".into()))?;
    if sha256_hex(&bytes) != want.trim() {
        return Err(Error::BackupInvalid(
            "манифест изменён или повреждён".into(),
        ));
    }
    let m: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| Error::BackupInvalid(format!("манифест: {e}")))?;
    if m.schema != MANIFEST_SCHEMA {
        return Err(Error::BackupInvalid(format!(
            "неподдерживаемая схема {}",
            m.schema
        )));
    }
    Ok(m)
}

/// Перечитывает файлы и хеши; ловит повреждение, пропажу, лишнее и подмену манифеста.
pub fn verify(dir: &Path, ctl: &Control) -> Result<VerifyReport> {
    let mut rep = VerifyReport::default();
    let m = match load_manifest(dir) {
        Ok(m) => m,
        Err(Error::BackupInvalid(_)) => return Ok(rep), // manifest_ok = false, ok = false
        Err(e) => return Err(e),
    };
    rep.manifest_ok = true;
    let files_dir = dir.join(FILES_DIR);
    let mut known = std::collections::HashSet::new();
    let total = m.files.len() as u64;
    for (i, f) in m.files.iter().enumerate() {
        ctl.ok()?;
        if validate_rel_path(&f.path).is_err() {
            rep.unsafe_paths.push(f.path.clone());
            continue;
        }
        known.insert(f.path.clone());
        let mut p = files_dir.clone();
        for s in f.path.split('/') {
            p.push(s);
        }
        let p = long_path(&p);
        match std::fs::symlink_metadata(&p) {
            Err(_) => rep.missing.push(f.path.clone()),
            Ok(meta) if !meta.is_file() => rep.corrupt.push(f.path.clone()),
            Ok(_) => match hash_file(&p, None) {
                Ok((size, sha)) if size == f.size && sha == f.sha256 => {}
                _ => rep.corrupt.push(f.path.clone()),
            },
        }
        rep.files_checked += 1;
        (ctl.progress)(BackupProgress {
            files_done: i as u64 + 1,
            files_total: total,
            bytes_done: 0,
            bytes_total: 0,
        });
    }
    // лишнее: файлы в files/, которых нет в манифесте
    // Относительный путь накапливается по именам, а не берётся из `strip_prefix`: на Windows
    // `read_dir(\\?\…)` возвращает пути с префиксом, который не совпадает с базой.
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(rd) = std::fs::read_dir(long_path(dir)) else {
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel_child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            match e.file_type() {
                Ok(ft) if ft.is_dir() => walk(&e.path(), &rel_child, out, depth + 1),
                _ => out.push(rel_child),
            }
        }
    }
    let mut on_disk = Vec::new();
    walk(&files_dir, "", &mut on_disk, 0);
    rep.extra = on_disk
        .into_iter()
        .filter(|r| !known.contains(r) && !r.contains("yeskia-part"))
        .collect();
    rep.ok = rep.missing.is_empty()
        && rep.corrupt.is_empty()
        && rep.extra.is_empty()
        && rep.unsafe_paths.is_empty();
    Ok(rep)
}

// ---------- restore ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Файла в цели нет — будет записан после подтверждения.
    New,
    /// Уже есть с тем же содержимым.
    Identical,
    /// Есть и отличается — не перезаписывается.
    Conflict,
    /// Нельзя восстановить (небезопасный путь, цель — папка и т. п.).
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub path: String,
    pub size: u64,
    pub status: PlanStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlanCounts {
    pub new: u64,
    pub identical: u64,
    pub conflict: u64,
    pub blocked: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub items: Vec<PlanItem>,
    pub counts: PlanCounts,
}

fn selected(path: &str, only: Option<&str>) -> bool {
    match only.map(|o| o.trim_matches('/')).filter(|o| !o.is_empty()) {
        None => true,
        Some(o) => path == o || path.starts_with(&format!("{o}/")),
    }
}

/// План восстановления (dry-run): ничего не пишет.
pub fn restore_plan(
    dir: &Path,
    target: &dyn RestoreTarget,
    only: Option<&str>,
    ctl: &Control,
) -> Result<RestorePlan> {
    let m = load_manifest(dir)?;
    let mut items = Vec::new();
    let mut counts = PlanCounts::default();
    for f in m.files.iter().filter(|f| selected(&f.path, only)) {
        ctl.ok()?;
        let mut push = |status: PlanStatus, reason: Option<&str>| {
            match status {
                PlanStatus::New => counts.new += 1,
                PlanStatus::Identical => counts.identical += 1,
                PlanStatus::Conflict => counts.conflict += 1,
                PlanStatus::Blocked => counts.blocked += 1,
            }
            items.push(PlanItem {
                path: f.path.clone(),
                size: f.size,
                status,
                reason: reason.map(String::from),
            });
        };
        if validate_rel_path(&f.path).is_err() {
            push(PlanStatus::Blocked, Some("небезопасный путь в манифесте"));
            continue;
        }
        match target.stat(&f.path) {
            Ok(None) => push(PlanStatus::New, None),
            Ok(Some(e)) if e.kind == EntryKind::File => {
                if e.size != f.size {
                    push(PlanStatus::Conflict, Some("размер отличается"));
                } else {
                    let mut hw = HashWriter::new(std::io::sink(), f.size, None);
                    target.read_to(&f.path, &mut hw, ctl)?;
                    let (_, sha, _) = hw.finish()?;
                    if sha == f.sha256 {
                        push(PlanStatus::Identical, None);
                    } else {
                        push(PlanStatus::Conflict, Some("содержимое отличается"));
                    }
                }
            }
            Ok(Some(_)) => push(PlanStatus::Blocked, Some("в цели на этом месте не файл")),
            Err(
                e @ (Error::Cancelled
                | Error::DeviceDisconnected
                | Error::Timeout
                | Error::FingerprintMismatch),
            ) => return Err(e),
            Err(Error::UnsafePath(_)) => {
                push(PlanStatus::Blocked, Some("путь недопустим для цели"))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(RestorePlan { items, counts })
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApplyReport {
    pub written: u64,
    pub identical: u64,
    pub conflicts: u64,
    pub blocked: u64,
    /// Файл появился в цели между планом и записью — не перезаписан.
    pub raced: u64,
    pub bytes: u64,
}

/// Записывает **только новые** файлы: сначала проверяет саму копию, потом пишет, затем читает
/// записанное обратно и сверяет SHA-256. Существующее не перезаписывается, ничего не удаляется.
pub fn restore_apply(
    dir: &Path,
    target: &dyn RestoreTarget,
    only: Option<&str>,
    ctl: &Control,
) -> Result<ApplyReport> {
    let m = load_manifest(dir)?;
    // копия обязана быть целой хотя бы в выбранной части
    let rep = verify(dir, ctl)?;
    let bad = |p: &String| selected(p, only);
    if !rep.manifest_ok
        || rep.corrupt.iter().any(bad)
        || rep.missing.iter().any(bad)
        || rep.unsafe_paths.iter().any(bad)
    {
        return Err(Error::BackupInvalid(
            "копия не прошла проверку; восстановление отменено".into(),
        ));
    }
    let plan = restore_plan(dir, target, only, ctl)?;
    let mut out = ApplyReport {
        identical: plan.counts.identical,
        conflicts: plan.counts.conflict,
        blocked: plan.counts.blocked,
        ..Default::default()
    };
    let todo: Vec<&PlanItem> = plan
        .items
        .iter()
        .filter(|i| i.status == PlanStatus::New)
        .collect();
    let total = todo.len() as u64;
    for (n, item) in todo.iter().enumerate() {
        ctl.ok()?;
        let rec = m
            .files
            .iter()
            .find(|f| f.path == item.path)
            .ok_or_else(|| Error::BackupInvalid("нет записи в манифесте".into()))?;
        if let Some((parent, _)) = item.path.rsplit_once('/') {
            target.mkdir_all(parent, ctl)?;
        }
        let mut p = dir.join(FILES_DIR);
        for s in item.path.split('/') {
            p.push(s);
        }
        let mut f = std::fs::File::open(long_path(&p))?;
        match target.write_new(&item.path, &mut f, rec.size, ctl) {
            Ok(()) => {}
            Err(Error::AlreadyExists) => {
                out.raced += 1;
                continue;
            }
            Err(e) => return Err(e),
        }
        // read-back
        let mut hw = HashWriter::new(std::io::sink(), rec.size, None);
        target.read_to(&item.path, &mut hw, ctl)?;
        let (bytes, sha, _) = hw.finish()?;
        if bytes != rec.size || sha != rec.sha256 {
            return Err(Error::VerificationFailed);
        }
        out.written += 1;
        out.bytes += bytes;
        (ctl.progress)(BackupProgress {
            files_done: n as u64 + 1,
            files_total: total,
            bytes_done: out.bytes,
            bytes_total: 0,
        });
    }
    Ok(out)
}

// ---------- список копий ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Имя каталога внутри хранилища.
    pub name: String,
    pub label: String,
    pub created_at_ms: u64,
    pub total_files: u64,
    pub total_bytes: u64,
    pub complete: bool,
    pub interrupted: bool,
    pub source_kind: String,
    /// Манифест цел (хеш совпал); полный verify — отдельной командой.
    pub manifest_ok: bool,
}

/// Копии в хранилище `root` (подкаталоги с манифестом), новые сверху.
pub fn list_backups(root: &Path) -> Vec<BackupInfo> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(long_path(root)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        match load_manifest(&p) {
            Ok(m) => out.push(BackupInfo {
                name,
                label: m.label,
                created_at_ms: m.created_at_ms,
                total_files: m.total_files,
                total_bytes: m.total_bytes,
                complete: m.complete,
                interrupted: m.interrupted,
                source_kind: m.source_kind,
                manifest_ok: true,
            }),
            Err(_) if p.join(MANIFEST_FILE).exists() => out.push(BackupInfo {
                name,
                label: String::new(),
                created_at_ms: 0,
                total_files: 0,
                total_bytes: 0,
                complete: false,
                interrupted: false,
                source_kind: String::new(),
                manifest_ok: false,
            }),
            Err(_) => {}
        }
    }
    out.sort_by(|a, b| {
        b.created_at_ms
            .cmp(&a.created_at_ms)
            .then(a.name.cmp(&b.name))
    });
    out
}

/// Имя каталога копии: `20261109-153000-метка` (метка очищена до допустимых символов).
pub fn backup_dir_name(created_at_ms: u64, label: &str) -> String {
    let secs = created_at_ms / 1000;
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, mi, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let clean: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(40)
        .collect();
    if clean.is_empty() {
        format!("{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}")
    } else {
        format!("{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}-{clean}")
    }
}
