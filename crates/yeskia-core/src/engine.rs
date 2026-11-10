//! `Engine` — единая точка входа для UI: устройства, подключение, матрица возможностей, операции.
//!
//! Гарантии:
//! * матрица возможностей строится на конкретном соединении (read-only probe + реальные операции);
//! * на устройство — строго последовательная FIFO-очередь (ограниченная), отмена в очереди и в работе;
//! * таймауты и hot-unplug обнаруживаются сторожем даже при «залипшем» драйвере;
//! * **никаких автоповторов**; неопределённый исход записи помечается `uncertain`;
//! * запись: безопасные пути, запрет silent overwrite, подтверждение read-back (SHA-256);
//! * при смене устройства на соединении (fingerprint) операции отклоняются;
//! * журнал и логи не содержат путей, контактов/SMS, IMEI и серийников.

use crate::device::*;
use crate::diagnostics::Diagnostics;
use crate::error::{Error, Result};
use crate::fsutil::{
    hash_file, place_file, sha256_hex, temp_path_for, validate_host_path, HashReader, HashWriter,
};
use crate::operations::*;
use crate::paths::validate_device_path;
use crate::storage::{JournalState, Storage};
use crate::transport::{DeviceTransport, ProbeOutcome};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Верхняя граница данных одной записи, передаваемых через ядро (защита от OOM).
pub const MAX_WRITE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub identity_timeout: Duration,
    pub list_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub probe_timeout: Duration,
    /// Сколько ждать завершения воркера после отмены/таймаута/отключения.
    pub cancel_grace: Duration,
    pub poll_interval: Duration,
    /// Максимум операций на устройство (выполняется + ожидают).
    pub max_queue: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            identity_timeout: Duration::from_secs(10),
            list_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(120),
            write_timeout: Duration::from_secs(300),
            probe_timeout: Duration::from_secs(10),
            cancel_grace: Duration::from_millis(500),
            poll_interval: Duration::from_millis(20),
            max_queue: 8,
        }
    }
}

impl EngineConfig {
    pub fn timeout_for(&self, op: Operation) -> Duration {
        match op {
            Operation::IdentityRead => self.identity_timeout,
            Operation::FilesList => self.list_timeout,
            Operation::FilesRead => self.read_timeout,
            _ => self.write_timeout,
        }
    }
}

struct Connection {
    transport: usize,
    fingerprint: String,
    matrix: CapabilityMatrix,
    /// Модель по результату `identity.read` при подключении.
    identified_model: Option<String>,
}

struct Pending {
    device: DeviceId,
    token: CancellationToken,
    progress: Arc<ProgressCell>,
}

#[derive(Default)]
struct DevQueue {
    running: Option<String>,
    waiting: VecDeque<String>,
}

#[derive(Default)]
struct State {
    connections: HashMap<DeviceId, Connection>,
    queues: HashMap<DeviceId, DevQueue>,
    pending: HashMap<String, Pending>,
}

struct Shared {
    transports: Vec<Arc<dyn DeviceTransport>>,
    state: Mutex<State>,
    cv: Condvar,
    storage: Arc<Storage>,
    diag: Arc<Diagnostics>,
    counter: AtomicU64,
    salt: String,
    config: EngineConfig,
}

pub struct Engine {
    shared: Arc<Shared>,
}

/// Операция, принятая в очередь. `wait` блокирует до результата.
pub struct OperationHandle {
    shared: Arc<Shared>,
    operation_id: String,
    device: DeviceId,
    request: OperationRequest,
    token: CancellationToken,
    progress: Arc<ProgressCell>,
    /// `wait` вызван: очисткой занимается он; иначе — `Drop` (брошенный handle).
    consumed: bool,
}

fn evidence(origin: DeviceOrigin, source: &str, summary: &str, fp: &str) -> Evidence {
    Evidence {
        kind: if origin == DeviceOrigin::Mock {
            EvidenceKind::MockTest
        } else {
            EvidenceKind::DeviceRun
        },
        source: source.to_string(),
        summary: summary.to_string(),
        checked_at_ms: crate::now_ms(),
        device_fingerprint: Some(fp.to_string()),
        reference: None,
    }
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn make_ctx(
        &self,
        transport: usize,
        device: &DeviceId,
        operation_id: String,
        token: CancellationToken,
        timeout: Duration,
        progress: Arc<ProgressCell>,
    ) -> OperationContext {
        let t = Arc::clone(&self.transports[transport]);
        let id = device.clone();
        OperationContext::new(
            operation_id,
            token,
            Instant::now() + timeout,
            Box::new(move || t.is_present(&id)),
            progress,
        )
    }

    fn find_device(&self, id: &DeviceId) -> Result<(usize, Device)> {
        for (i, t) in self.transports.iter().enumerate() {
            // Сбой одного транспорта не должен скрывать устройства других.
            match t.discover() {
                Ok(devs) => {
                    if let Some(d) = devs.into_iter().find(|d| &d.id == id) {
                        return Ok((i, d));
                    }
                }
                Err(e) => self.diag.warn(&format!("обнаружение: {e}")),
            }
        }
        Err(Error::DeviceNotFound(id.as_str().to_string()))
    }

    /// Снимает отметку «выполняется» и будит очередь.
    fn release_slot(&self, device: &DeviceId, op_id: &str) {
        let mut st = self.lock();
        if let Some(q) = st.queues.get_mut(device) {
            if q.running.as_deref() == Some(op_id) {
                q.running = None;
            }
        }
        drop(st);
        self.cv.notify_all();
    }

    /// Закрывает соединение и отменяет все операции устройства.
    fn drop_connection(&self, st: &mut State, device: &DeviceId) {
        st.connections.remove(device);
        for p in st.pending.values().filter(|p| &p.device == device) {
            p.token.cancel();
        }
        // Ожидающие операции больше не должны занимать очередь, даже если их waiter не вызван.
        if let Some(q) = st.queues.get_mut(device) {
            q.waiting.clear();
        }
    }

    /// Тот же ли телефон на соединении. `Err(FingerprintMismatch)` закрывает соединение.
    fn check_fingerprint(&self, device: &DeviceId) -> Result<()> {
        let (idx, expected) = {
            let st = self.lock();
            let c = st.connections.get(device).ok_or(Error::NotConnected)?;
            (c.transport, c.fingerprint.clone())
        };
        let found = self.transports[idx]
            .discover()?
            .into_iter()
            .find(|d| &d.id == device)
            .ok_or(Error::DeviceDisconnected)?;
        if fingerprint(&self.salt, &found) != expected {
            let mut st = self.lock();
            self.drop_connection(&mut st, device);
            drop(st);
            self.diag
                .warn("fingerprint устройства изменился; соединение закрыто");
            return Err(Error::FingerprintMismatch);
        }
        Ok(())
    }

    fn build_matrix(
        &self,
        transport: &dyn DeviceTransport,
        device: &Device,
        fp: &str,
        probe: Vec<ProbeOutcome>,
    ) -> CapabilityMatrix {
        let now = crate::now_ms();
        let entries = Operation::ALL
            .iter()
            .map(|&op| {
                // Передача пакета — это запись файла: готовность та же, что у `files.write`.
                let basis = if op == Operation::PackagesTransfer {
                    Operation::FilesWrite
                } else {
                    op
                };
                let maturity = transport.maturity_for(&device.id, basis);
                let outcome = probe.iter().find(|p| p.operation == basis);
                let (availability, detail) = match (maturity, outcome) {
                    // Причину «не поддерживается» берём у probe (режим USB и т. п.), если он её назвал.
                    (SupportStatus::Unsupported, o) => (
                        Availability::Unsupported,
                        o.and_then(|o| o.detail.clone())
                            .or(Some("адаптер не реализует операцию".to_string())),
                    ),
                    (_, Some(o)) => (o.availability, o.detail.clone()),
                    (_, None) => (Availability::NotTested, None),
                };
                let ev = outcome
                    .filter(|_| availability != Availability::Unsupported)
                    .map(|o| {
                        vec![evidence(
                            device.origin,
                            "probe",
                            &format!("read-only probe: {:?}", o.availability),
                            fp,
                        )]
                    })
                    .unwrap_or_default();
                CapabilityEntry {
                    operation: op,
                    availability,
                    maturity,
                    detail,
                    evidence: ev,
                }
            })
            .collect();
        CapabilityMatrix {
            device_fingerprint: fp.to_string(),
            probed_at_ms: now,
            entries,
        }
    }
}

/// Выполняет блокирующий вызов транспорта в воркере с ограничением по времени. По таймауту
/// отменяет токен и возвращает `Timeout`; «залипший» воркер завершится сам и результат отбросится.
fn run_bounded<T: Send + 'static>(
    timeout: Duration,
    token: &CancellationToken,
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
            .unwrap_or_else(|_| Err(Error::Internal("паника в транспорте".into())));
        let _ = tx.send(r);
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(RecvTimeoutError::Timeout) => {
            token.cancel();
            Err(Error::Timeout)
        }
        Err(RecvTimeoutError::Disconnected) => {
            Err(Error::Internal("воркер завершился без результата".into()))
        }
    }
}

fn files(t: &dyn DeviceTransport) -> Result<&dyn crate::transport::FileProvider> {
    t.files()
        .ok_or_else(|| Error::CapabilityUnsupported("files".into()))
}

/// Тело операции; выполняется в воркере.
fn execute_request(
    t: &dyn DeviceTransport,
    id: &DeviceId,
    req: &OperationRequest,
    ctx: &OperationContext,
) -> Result<OperationData> {
    ctx.checkpoint()?;
    match req {
        OperationRequest::IdentityRead => {
            let i = t.identity(id, ctx)?;
            Ok(OperationData::Identity {
                model: i.model,
                firmware: i.firmware,
            })
        }
        OperationRequest::FilesList { path } => Ok(OperationData::Entries {
            entries: files(t)?.list(id, path, ctx)?,
        }),
        OperationRequest::FilesRead { path } => {
            let data = files(t)?.read(id, path, ctx)?;
            Ok(OperationData::FileRead {
                bytes: data.len() as u64,
                sha256: sha256_hex(&data),
            })
        }
        OperationRequest::FilesWrite {
            path,
            data,
            overwrite,
        } => {
            let f = files(t)?;
            let existing = f.stat(id, path, ctx)?;
            if existing.is_some() && *overwrite == OverwritePolicy::Never {
                return Err(Error::AlreadyExists);
            }
            f.write(id, path, data, *overwrite, ctx)?;
            // Подтверждение записи: обратное чтение и сравнение контрольной суммы.
            ctx.checkpoint()?;
            let back = f.read(id, path, ctx)?;
            let want = sha256_hex(data);
            if sha256_hex(&back) != want {
                return Err(Error::VerificationFailed);
            }
            Ok(OperationData::Written {
                bytes: data.len() as u64,
                sha256: want,
                verified: true,
                replaced: existing.is_some(),
            })
        }
        OperationRequest::FilesDownload {
            path,
            dest,
            overwrite,
        } => download(t, id, path, std::path::Path::new(dest), *overwrite, ctx),
        OperationRequest::FilesUpload {
            source,
            path,
            overwrite,
        } => upload(t, id, std::path::Path::new(source), path, *overwrite, ctx),
        OperationRequest::PackageTransfer {
            source,
            path,
            sha256,
        } => package_transfer(t, id, std::path::Path::new(source), path, sha256, ctx),
        OperationRequest::BackupCreate {
            path,
            dest,
            label,
            exclude,
        } => backup_create(t, id, path, dest, label, exclude, ctx),
        OperationRequest::BackupRestore {
            backup,
            path,
            only,
            apply,
        } => backup_restore(t, id, backup, path, only.as_deref(), *apply, ctx),
        OperationRequest::ContactsRead => {
            let p = t
                .pim()
                .ok_or_else(|| Error::CapabilityUnsupported("pim".into()))?;
            Ok(OperationData::PimCount {
                count: p.contacts_count(id, ctx)?,
            })
        }
        OperationRequest::SmsRead => {
            let p = t
                .pim()
                .ok_or_else(|| Error::CapabilityUnsupported("pim".into()))?;
            Ok(OperationData::PimCount {
                count: p.sms_count(id, ctx)?,
            })
        }
    }
}

const PLAN_ITEMS_LIMIT: usize = 500;

fn backup_ctl<'a>(
    ctx: &'a OperationContext,
    check: &'a dyn Fn() -> Result<()>,
    progress: &'a dyn Fn(crate::backup::BackupProgress),
) -> crate::backup::Control<'a> {
    let _ = ctx;
    crate::backup::Control { check, progress }
}

/// Файловая копия папки устройства в каталог на компьютере (только чтение устройства).
fn backup_create(
    t: &dyn DeviceTransport,
    id: &DeviceId,
    path: &str,
    dest: &str,
    label: &str,
    exclude: &[String],
    ctx: &OperationContext,
) -> Result<OperationData> {
    let f = files(t)?;
    let store = crate::backup::DeviceStore {
        provider: f,
        id,
        ctx,
        root: path.to_string(),
    };
    let check = || ctx.checkpoint();
    let progress =
        |p: crate::backup::BackupProgress| ctx.report_progress(p.files_done, p.files_total);
    let ctl = backup_ctl(ctx, &check, &progress);
    let opts = crate::backup::Options {
        exclude: exclude.to_vec(),
        ..Default::default()
    };
    let s = crate::backup::create(&store, std::path::Path::new(dest), label, &opts, &ctl)?;
    Ok(OperationData::BackupCreated {
        files: s.files,
        bytes: s.bytes,
        skipped: s.skipped,
        complete: s.complete,
        verified: s.verified,
    })
}

/// План или применение восстановления на устройство (только новые файлы).
fn backup_restore(
    t: &dyn DeviceTransport,
    id: &DeviceId,
    backup: &str,
    path: &str,
    only: Option<&str>,
    apply: bool,
    ctx: &OperationContext,
) -> Result<OperationData> {
    let f = files(t)?;
    let store = crate::backup::DeviceStore {
        provider: f,
        id,
        ctx,
        root: path.to_string(),
    };
    let check = || ctx.checkpoint();
    let progress =
        |p: crate::backup::BackupProgress| ctx.report_progress(p.files_done, p.files_total);
    let ctl = backup_ctl(ctx, &check, &progress);
    let dir = std::path::Path::new(backup);
    if apply {
        let r = crate::backup::restore_apply(dir, &store, only, &ctl)?;
        Ok(OperationData::BackupApplied {
            written: r.written,
            identical: r.identical,
            conflicts: r.conflicts,
            blocked: r.blocked,
            raced: r.raced,
            bytes: r.bytes,
        })
    } else {
        let plan = crate::backup::restore_plan(dir, &store, only, &ctl)?;
        let truncated = plan.items.len() > PLAN_ITEMS_LIMIT;
        Ok(OperationData::BackupPlan {
            counts: plan.counts,
            items: plan.items.into_iter().take(PLAN_ITEMS_LIMIT).collect(),
            truncated,
        })
    }
}

/// Скачивание на компьютер: временный файл → проверка размера → атомарное размещение без
/// перезаписи → чтение с диска и сверка SHA-256. При любом сбое/отмене временный файл удаляется,
/// под настоящим именем частичного файла не остаётся.
fn download(
    t: &dyn DeviceTransport,
    id: &DeviceId,
    path: &str,
    dest: &std::path::Path,
    overwrite: OverwritePolicy,
    ctx: &OperationContext,
) -> Result<OperationData> {
    let f = files(t)?;
    let st = f
        .stat(id, path, ctx)?
        .ok_or_else(|| Error::OperationFailed("файл на устройстве не найден".into()))?;
    if st.is_dir {
        return Err(Error::InvalidArgument("это папка, а не файл".into()));
    }
    let existed = dest.exists();
    if existed && overwrite == OverwritePolicy::Never {
        return Err(Error::AlreadyExists);
    }
    let tmp = temp_path_for(dest);
    let cleanup = |e: Error| {
        let _ = std::fs::remove_file(&tmp);
        e
    };
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                Error::HostAccess("нет прав на запись в папку назначения".into())
            }
            std::io::ErrorKind::NotFound => Error::HostAccess("папка назначения не найдена".into()),
            _ => Error::from(e),
        })?;
    let mut hw = HashWriter::new(std::io::BufWriter::new(file), st.size, Some(ctx));
    let read = f.read_to(id, path, &mut hw, ctx).map_err(cleanup)?;
    let (bytes, sha, writer) = hw.finish().map_err(|e| cleanup(e.into()))?;
    let file = writer
        .into_inner()
        .map_err(|e| cleanup(e.into_error().into()))?;
    file.sync_all().map_err(|e| cleanup(e.into()))?;
    drop(file);
    if read != bytes || bytes != st.size {
        return Err(cleanup(Error::VerificationFailed));
    }
    ctx.checkpoint().map_err(cleanup)?;
    place_file(&tmp, dest, overwrite).map_err(cleanup)?;
    // Подтверждение: читаем результат с диска и сверяем с тем, что пришло с устройства.
    let (b2, sha2) = hash_file(dest, Some(ctx))?;
    if b2 != bytes || sha2 != sha {
        return Err(Error::VerificationFailed);
    }
    Ok(OperationData::Downloaded {
        bytes,
        sha256: sha,
        verified: true,
        replaced: existed,
    })
}

fn is_package_name(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    [".sis", ".sisx", ".jar", ".jad"]
        .iter()
        .any(|e| name.ends_with(e))
}

/// Передача пакета: хеш файла сверяется с подтверждённым до записи и после; перезаписи нет.
/// Размер ограничен как у пакетов в библиотеке. Это запись файла, а не установка.
fn package_transfer(
    t: &dyn DeviceTransport,
    id: &DeviceId,
    source: &std::path::Path,
    path: &str,
    sha256: &str,
    ctx: &OperationContext,
) -> Result<OperationData> {
    const MAX_PACKAGE: u64 = 32 * 1024 * 1024;
    let meta = std::fs::metadata(source).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::HostAccess("исходный файл не найден".into()),
        std::io::ErrorKind::PermissionDenied => {
            Error::HostAccess("нет прав на чтение файла".into())
        }
        _ => Error::from(e),
    })?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_PACKAGE {
        return Err(Error::InvalidArgument(
            "пакет должен быть файлом от 1 байта до 32 МиБ".into(),
        ));
    }
    let (_, actual) = crate::fsutil::hash_file(source, Some(ctx))?;
    if actual != sha256 {
        return Err(Error::InvalidArgument(
            "файл пакета изменился после проверки (SHA-256 не совпадает)".into(),
        ));
    }
    let data = upload(t, id, source, path, OverwritePolicy::Never, ctx)?;
    match &data {
        OperationData::Written { sha256: w, .. } if w == sha256 => Ok(data),
        _ => Err(Error::VerificationFailed),
    }
}

/// Загрузка файла с компьютера на устройство с подтверждением обратным чтением (SHA-256).
fn upload(
    t: &dyn DeviceTransport,
    id: &DeviceId,
    source: &std::path::Path,
    path: &str,
    overwrite: OverwritePolicy,
    ctx: &OperationContext,
) -> Result<OperationData> {
    let f = files(t)?;
    let meta = std::fs::metadata(source).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::HostAccess("исходный файл не найден".into()),
        std::io::ErrorKind::PermissionDenied => {
            Error::HostAccess("нет прав на чтение файла".into())
        }
        _ => Error::from(e),
    })?;
    if !meta.is_file() {
        return Err(Error::InvalidArgument(
            "загружать можно только файлы".into(),
        ));
    }
    let existing = f.stat(id, path, ctx)?;
    if existing.is_some() && overwrite == OverwritePolicy::Never {
        return Err(Error::AlreadyExists);
    }
    let mut reader = HashReader::new(std::fs::File::open(source)?);
    f.write_from(id, path, &mut reader, meta.len(), overwrite, ctx)?;
    let (bytes, want) = reader.finish();
    ctx.checkpoint()?;
    let mut hw = HashWriter::new(std::io::sink(), bytes, None);
    f.read_to(id, path, &mut hw, ctx)?;
    let (back_bytes, back_sha, _) = hw.finish()?;
    if back_bytes != bytes || back_sha != want {
        return Err(Error::VerificationFailed);
    }
    Ok(OperationData::Written {
        bytes,
        sha256: want,
        verified: true,
        replaced: existing.is_some(),
    })
}

impl Engine {
    pub fn new(
        transports: Vec<Arc<dyn DeviceTransport>>,
        storage: Arc<Storage>,
        diag: Arc<Diagnostics>,
    ) -> Self {
        Self::with_config(transports, storage, diag, EngineConfig::default())
    }

    pub fn with_config(
        transports: Vec<Arc<dyn DeviceTransport>>,
        storage: Arc<Storage>,
        diag: Arc<Diagnostics>,
        config: EngineConfig,
    ) -> Self {
        let salt = storage
            .install_salt()
            .unwrap_or_else(|_| format!("fallback-{}", crate::now_ms()));
        Self {
            shared: Arc::new(Shared {
                transports,
                state: Mutex::new(State::default()),
                cv: Condvar::new(),
                storage,
                diag,
                // Старт от текущего времени: id уникальны и между перезапусками (PRIMARY KEY журнала).
                counter: AtomicU64::new(crate::now_ms()),
                salt,
                config,
            }),
        }
    }

    pub fn storage(&self) -> &Arc<Storage> {
        &self.shared.storage
    }
    pub fn diagnostics(&self) -> &Arc<Diagnostics> {
        &self.shared.diag
    }

    /// Список устройств; подключённым добавляется матрица возможностей их соединения.
    /// Заодно обнаруживает пропавшие подключённые устройства (hot unplug).
    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let sh = &self.shared;
        let mut all = Vec::new();
        let mut failed = std::collections::HashSet::new();
        for (i, t) in sh.transports.iter().enumerate() {
            match t.discover() {
                Ok(devs) => all.extend(devs),
                Err(e) => {
                    failed.insert(i);
                    sh.diag.warn(&format!("обнаружение: {e}"));
                }
            }
        }
        let mut st = sh.lock();
        let lost: Vec<DeviceId> = st
            .connections
            .iter()
            // Если транспорт сейчас не смог перечислить устройства, их пропажу не утверждаем.
            .filter(|(id, c)| !failed.contains(&c.transport) && !all.iter().any(|d| &d.id == *id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in lost {
            sh.drop_connection(&mut st, &id);
            sh.diag
                .warn("подключённое устройство пропало; соединение закрыто");
        }
        for d in &mut all {
            match st.connections.get(&d.id) {
                Some(c) => {
                    d.connection = ConnectionState::Connected;
                    d.matrix = Some(c.matrix.clone());
                    d.identified_model = c.identified_model.clone();
                }
                None => {
                    d.connection = ConnectionState::Disconnected;
                    d.matrix = None;
                    d.identified_model = None;
                }
            }
        }
        Ok(all)
    }

    pub fn capability_matrix(&self, id: &DeviceId) -> Option<CapabilityMatrix> {
        self.shared
            .lock()
            .connections
            .get(id)
            .map(|c| c.matrix.clone())
    }

    /// Подключение: открытие транспорта + read-only probe → матрица соединения.
    pub fn connect(&self, id: &DeviceId) -> Result<Device> {
        let sh = &self.shared;
        if sh.lock().connections.contains_key(id) {
            return Err(Error::AlreadyConnected);
        }
        let (idx, mut device) = sh.find_device(id)?;
        let transport = &sh.transports[idx];
        transport.connect(id)?;

        // Probe и идентификация — блокирующие вызовы транспорта (например, COM/WPD), поэтому идут
        // через ограниченный по времени воркер: подключение не может зависнуть навсегда.
        let probe = {
            let token = CancellationToken::new();
            let ctx = sh.make_ctx(
                idx,
                id,
                "probe".into(),
                token.clone(),
                sh.config.probe_timeout,
                Arc::new(ProgressCell::default()),
            );
            let (t, did) = (Arc::clone(transport), id.clone());
            run_bounded(
                sh.config.probe_timeout + sh.config.cancel_grace,
                &token,
                move || t.probe(&did, &ctx),
            )
        };
        let probe = match probe {
            Ok(p) => p,
            Err(e) => {
                let _ = transport.disconnect(id);
                return Err(e);
            }
        };
        let fp = fingerprint(&sh.salt, &device);
        let matrix = sh.build_matrix(transport.as_ref(), &device, &fp, probe);
        // Модель берём только из идентификации по протоколу (read-only), не из VID/PID.
        // Сбой или таймаут идентификации не мешает подключению: просто без модели.
        let identified_model =
            if matrix.availability(Operation::IdentityRead) == Availability::Available {
                let token = CancellationToken::new();
                let ctx = sh.make_ctx(
                    idx,
                    id,
                    "identity".into(),
                    token.clone(),
                    sh.config.identity_timeout,
                    Arc::new(ProgressCell::default()),
                );
                let (t, did) = (Arc::clone(transport), id.clone());
                run_bounded(
                    sh.config.identity_timeout + sh.config.cancel_grace,
                    &token,
                    move || t.identity(&did, &ctx),
                )
                .ok()
                .map(|i| i.model)
            } else {
                None
            };
        sh.lock().connections.insert(
            id.clone(),
            Connection {
                transport: idx,
                fingerprint: fp,
                matrix: matrix.clone(),
                identified_model: identified_model.clone(),
            },
        );
        device.connection = ConnectionState::Connected;
        device.matrix = Some(matrix);
        device.identified_model = identified_model;
        sh.diag
            .info(&format!("подключено: {}", device.display_name));
        Ok(device)
    }

    pub fn disconnect(&self, id: &DeviceId) -> Result<()> {
        let sh = &self.shared;
        let idx = {
            let mut st = sh.lock();
            let idx = st.connections.get(id).ok_or(Error::NotConnected)?.transport;
            sh.drop_connection(&mut st, id);
            idx
        };
        sh.cv.notify_all();
        if let Err(e) = sh.transports[idx].disconnect(id) {
            sh.diag.warn(&format!("ошибка закрытия транспорта: {e}"));
        }
        sh.diag.info("устройство отключено");
        Ok(())
    }

    /// Отмена операции по id (в очереди или выполняемой). `false` — такой нет.
    pub fn cancel(&self, operation_id: &str) -> bool {
        let mut st = self.shared.lock();
        let Some(device) = st.pending.get(operation_id).map(|p| {
            p.token.cancel();
            p.device.clone()
        }) else {
            return false;
        };
        // Ожидающая операция покидает очередь сразу, независимо от того, вызван ли её `wait`.
        if let Some(q) = st.queues.get_mut(&device) {
            q.waiting.retain(|o| o != operation_id);
        }
        drop(st);
        self.shared.cv.notify_all();
        true
    }

    pub fn progress(&self, operation_id: &str) -> Option<Progress> {
        self.shared
            .lock()
            .pending
            .get(operation_id)
            .map(|p| p.progress.get())
    }

    /// id операции, выполняемой на устройстве сейчас.
    pub fn running_operation(&self, id: &DeviceId) -> Option<String> {
        self.shared
            .lock()
            .queues
            .get(id)
            .and_then(|q| q.running.clone())
    }

    /// Сколько операций ждёт в очереди устройства.
    pub fn queued_operations(&self, id: &DeviceId) -> usize {
        self.shared
            .lock()
            .queues
            .get(id)
            .map(|q| q.waiting.len())
            .unwrap_or(0)
    }

    /// Ставит операцию в очередь. `Err` — нарушенные предусловия (ничего не выполнялось).
    pub fn submit(&self, id: &DeviceId, mut request: OperationRequest) -> Result<OperationHandle> {
        let sh = &self.shared;
        let op = request.operation();

        // Безопасные пути и размеры — до очереди.
        match &mut request {
            OperationRequest::FilesList { path }
            | OperationRequest::FilesRead { path }
            | OperationRequest::FilesWrite { path, .. }
            | OperationRequest::FilesDownload { path, .. }
            | OperationRequest::FilesUpload { path, .. }
            | OperationRequest::PackageTransfer { path, .. }
            | OperationRequest::BackupCreate { path, .. }
            | OperationRequest::BackupRestore { path, .. } => *path = validate_device_path(path)?,
            _ => {}
        }
        match &request {
            OperationRequest::BackupCreate { dest, .. } => {
                let p = std::path::Path::new(dest);
                if !p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Error::UnsafePath(
                        "каталог копии должен быть абсолютным путём".into(),
                    ));
                }
            }
            OperationRequest::BackupRestore { backup, .. } => {
                let p = std::path::Path::new(backup);
                if !p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Error::UnsafePath(
                        "каталог копии должен быть абсолютным путём".into(),
                    ));
                }
            }
            OperationRequest::FilesDownload { dest, .. } => {
                validate_host_path(std::path::Path::new(dest))?
            }
            OperationRequest::PackageTransfer {
                source,
                path,
                sha256,
            } => {
                let p = std::path::Path::new(source);
                if !p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Error::UnsafePath(
                        "исходный путь должен быть абсолютным".into(),
                    ));
                }
                if sha256.len() != 64
                    || !sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(Error::InvalidArgument(
                        "нужен SHA-256 пакета (64 hex-символа)".into(),
                    ));
                }
                if !is_package_name(path) {
                    return Err(Error::InvalidArgument(
                        "передавать можно только файлы .sis, .sisx, .jar, .jad".into(),
                    ));
                }
            }
            OperationRequest::FilesUpload { source, .. } => {
                let p = std::path::Path::new(source);
                if !p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Error::UnsafePath(
                        "исходный путь должен быть абсолютным".into(),
                    ));
                }
            }
            _ => {}
        }
        if let OperationRequest::FilesWrite { data, .. } = &request {
            if data.len() > MAX_WRITE_BYTES {
                return Err(Error::InvalidArgument(
                    "слишком большой объём записи".into(),
                ));
            }
        }

        let operation_id = format!("op-{}", sh.counter.fetch_add(1, Ordering::SeqCst));
        let token = CancellationToken::new();
        let progress = Arc::new(ProgressCell::default());
        {
            let mut st = sh.lock();
            let conn = st.connections.get(id).ok_or(Error::NotConnected)?;
            // Недоступность определяется матрицей соединения, а не названием модели.
            if conn.matrix.availability(op) == Availability::Unsupported {
                return Err(Error::CapabilityUnsupported(op.id().to_string()));
            }
            let q = st.queues.entry(id.clone()).or_default();
            if q.waiting.len() + usize::from(q.running.is_some()) >= sh.config.max_queue {
                return Err(Error::QueueFull);
            }
            q.waiting.push_back(operation_id.clone());
            st.pending.insert(
                operation_id.clone(),
                Pending {
                    device: id.clone(),
                    token: token.clone(),
                    progress: Arc::clone(&progress),
                },
            );
        }
        if let Err(e) = sh
            .storage
            .journal_start(&operation_id, id.as_str(), op.id())
        {
            let mut st = sh.lock();
            st.pending.remove(&operation_id);
            if let Some(q) = st.queues.get_mut(id) {
                q.waiting.retain(|o| o != &operation_id);
            }
            return Err(e);
        }
        sh.diag
            .info(&format!("{operation_id}: {} в очереди", op.id()));
        Ok(OperationHandle {
            shared: Arc::clone(sh),
            operation_id,
            device: id.clone(),
            request,
            token,
            progress,
            consumed: false,
        })
    }

    /// Удобная обёртка: `submit` + `wait`.
    pub fn run(&self, id: &DeviceId, request: OperationRequest) -> Result<OperationResult> {
        Ok(self.submit(id, request)?.wait())
    }
}

impl Drop for OperationHandle {
    /// Брошенный (не ожидаемый) handle не должен навсегда занимать очередь устройства.
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        let sh = &self.shared;
        self.token.cancel();
        {
            let mut st = sh.lock();
            if let Some(q) = st.queues.get_mut(&self.device) {
                q.waiting.retain(|o| o != &self.operation_id);
            }
            st.pending.remove(&self.operation_id);
        }
        let _ = sh.storage.journal_finish(
            &self.operation_id,
            JournalState::Cancelled,
            Some("OPERATION_CANCELLED"),
            false,
        );
        sh.diag.info(&format!(
            "{}: операция отброшена без ожидания",
            self.operation_id
        ));
        sh.cv.notify_all();
    }
}

impl OperationHandle {
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn device_id(&self) -> &DeviceId {
        &self.device
    }

    /// Ждёт своей очереди, выполняет операцию, возвращает типизированный результат.
    pub fn wait(mut self) -> OperationResult {
        self.consumed = true;
        let started = Instant::now();
        let op = self.request.operation();
        let sh = Arc::clone(&self.shared);

        // 1) Очередь: FIFO на устройство.
        let queue_err = self.wait_turn();
        let origin_and_fp = {
            let st = sh.lock();
            st.connections.get(&self.device).map(|c| {
                let o = sh.transports[c.transport]
                    .discover()
                    .ok()
                    .and_then(|d| d.into_iter().find(|d| d.id == self.device))
                    .map(|d| d.origin);
                (c.transport, c.fingerprint.clone(), o)
            })
        };
        let origin = origin_and_fp
            .as_ref()
            .and_then(|(_, _, o)| *o)
            .unwrap_or(DeviceOrigin::Real);

        if let Some(e) = queue_err {
            return self.finish(started, origin, Err(e), false);
        }
        let Some((transport_idx, _, _)) = origin_and_fp else {
            sh.release_slot(&self.device, &self.operation_id);
            return self.finish(started, origin, Err(Error::DeviceDisconnected), false);
        };

        // 2) Тот же ли телефон.
        if let Err(e) = sh.check_fingerprint(&self.device) {
            sh.release_slot(&self.device, &self.operation_id);
            return self.finish(started, origin, Err(e), false);
        }

        // 3) Выполнение в воркере; сторож следит за отменой, таймаутом и отключением.
        let transport = Arc::clone(&sh.transports[transport_idx]);
        let timeout = sh.config.timeout_for(op);
        let deadline = Instant::now() + timeout;
        let ctx = sh.make_ctx(
            transport_idx,
            &self.device,
            self.operation_id.clone(),
            self.token.clone(),
            timeout,
            Arc::clone(&self.progress),
        );
        sh.diag
            .info(&format!("{}: {} начата", self.operation_id, op.id()));

        let (tx, rx) = mpsc::channel();
        {
            let sh2 = Arc::clone(&sh);
            let transport = Arc::clone(&transport);
            let device = self.device.clone();
            let op_id = self.operation_id.clone();
            let request = self.request.clone();
            std::thread::spawn(move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute_request(transport.as_ref(), &device, &request, &ctx)
                }))
                .unwrap_or_else(|_| Err(Error::Internal("паника в транспорте".into())));
                // Слот освобождает воркер: следующая операция не начнётся, пока предыдущая
                // реально не завершилась, даже если сторож уже вернул результат.
                sh2.release_slot(&device, &op_id);
                let _ = tx.send(r);
            });
        }

        let poll = sh.config.poll_interval;
        let mut forced: Option<Error> = None;
        let mut grace_end: Option<Instant> = None;
        let worker: Option<Result<OperationData>> = loop {
            match rx.recv_timeout(poll) {
                Ok(r) => break Some(r),
                Err(RecvTimeoutError::Disconnected) => {
                    break Some(Err(Error::Internal(
                        "воркер завершился без результата".into(),
                    )))
                }
                Err(RecvTimeoutError::Timeout) => {
                    let now = Instant::now();
                    if forced.is_none() {
                        if self.token.is_cancelled() {
                            forced = Some(Error::Cancelled);
                        } else if now >= deadline {
                            forced = Some(Error::Timeout);
                            self.token.cancel();
                        } else if !transport.is_present(&self.device) {
                            forced = Some(Error::DeviceDisconnected);
                            self.token.cancel();
                        }
                        if forced.is_some() {
                            grace_end = Some(now + sh.config.cancel_grace);
                        }
                    } else if grace_end.is_some_and(|g| now >= g) {
                        break None; // воркер завис; слот останется занятым до его выхода
                    }
                }
            }
        };

        let mut result = match (worker, forced) {
            (Some(Ok(d)), _) => Ok(d),
            (Some(Err(Error::Cancelled)), Some(f)) => Err(f),
            (Some(Err(e)), _) => Err(e),
            (None, Some(f)) => Err(f),
            (None, None) => Err(Error::Internal("нет результата".into())),
        };

        // Для записи после успеха — ещё раз убеждаемся, что это тот же телефон.
        if result.is_ok() && op.is_write() {
            if let Err(e) = sh.check_fingerprint(&self.device) {
                result = Err(e);
            }
        }
        if matches!(
            result,
            Err(Error::DeviceDisconnected) | Err(Error::FingerprintMismatch)
        ) {
            let mut st = sh.lock();
            sh.drop_connection(&mut st, &self.device);
        }
        self.finish(started, origin, result, true)
    }

    /// Ждёт очереди. `Some(err)` — операция не была запущена.
    fn wait_turn(&self) -> Option<Error> {
        let sh = &self.shared;
        let mut st = sh.lock();
        loop {
            let cancelled = self.token.is_cancelled();
            let connected = st.connections.contains_key(&self.device);
            let q = st.queues.entry(self.device.clone()).or_default();
            if cancelled || !connected {
                q.waiting.retain(|o| o != &self.operation_id);
                return Some(if cancelled {
                    Error::Cancelled
                } else {
                    Error::DeviceDisconnected
                });
            }
            if q.running.is_none() && q.waiting.front() == Some(&self.operation_id) {
                q.waiting.pop_front();
                q.running = Some(self.operation_id.clone());
                return None;
            }
            st = sh
                .cv
                .wait_timeout(st, sh.config.poll_interval)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Формирует результат, обновляет матрицу и журнал.
    fn finish(
        self,
        started: Instant,
        origin: DeviceOrigin,
        result: Result<OperationData>,
        executed: bool,
    ) -> OperationResult {
        let sh = &self.shared;
        let op = self.request.operation();
        let fp = sh
            .lock()
            .connections
            .get(&self.device)
            .map(|c| c.fingerprint.clone())
            .unwrap_or_default();

        let (outcome, data, mut error, jstate) = match result {
            Ok(d) => (Outcome::Success, Some(d), None, JournalState::Success),
            Err(Error::Cancelled) => (
                Outcome::Cancelled,
                None,
                Some(Error::Cancelled),
                JournalState::Cancelled,
            ),
            Err(e) => (Outcome::Failed, None, Some(e), JournalState::Failed),
        };
        let uncertain =
            op.is_write() && executed && error.as_ref().is_some_and(|e| !e.is_precondition());

        let mut info = error.as_ref().map(|e| e.to_info());
        if let (Some(i), true) = (info.as_mut(), uncertain) {
            i.action = crate::error::UserAction::VerifyDeviceContents;
        }

        // Обновление матрицы по факту операции.
        {
            let mut st = sh.lock();
            if let Some(conn) = st.connections.get_mut(&self.device) {
                if let Some(entry) = conn.matrix.entries.iter_mut().find(|e| e.operation == op) {
                    match (&outcome, &error) {
                        (Outcome::Success, _) => {
                            entry.availability = Availability::Available;
                            entry.detail = None;
                            let proven = if origin == DeviceOrigin::Mock {
                                SupportStatus::TestedOnMock
                            } else {
                                SupportStatus::VerifiedOnDevice
                            };
                            entry.maturity = entry.maturity.max(proven);
                            entry.evidence.push(evidence(
                                origin,
                                &format!("operation:{}", op.id()),
                                "операция выполнена на этом соединении",
                                &fp,
                            ));
                            if entry.evidence.len() > 5 {
                                entry.evidence.remove(0);
                            }
                        }
                        (_, Some(Error::Transport(_)))
                        | (_, Some(Error::OperationFailed(_)))
                        | (_, Some(Error::VerificationFailed)) => {
                            entry.availability = Availability::Error;
                            entry.detail = info.as_ref().map(|i| i.message.clone());
                        }
                        _ => {}
                    }
                }
            }
            st.pending.remove(&self.operation_id);
        }
        error.take();

        let code = info.as_ref().map(|i| format!("{:?}", i.code));
        if let Err(e) =
            sh.storage
                .journal_finish(&self.operation_id, jstate, code.as_deref(), uncertain)
        {
            sh.diag.error(&format!("журнал: {e}"));
        }
        // В лог — только имя операции и исход: без путей, данных и идентификаторов.
        sh.diag.info(&format!(
            "{}: {} → {:?}{}",
            self.operation_id,
            op.id(),
            outcome,
            if uncertain {
                " (исход записи неизвестен)"
            } else {
                ""
            }
        ));
        sh.cv.notify_all();

        let ev = if origin == DeviceOrigin::Mock || outcome == Outcome::Success {
            vec![evidence(
                origin,
                &format!("operation:{}", op.id()),
                if origin == DeviceOrigin::Mock {
                    "Выполнено на mock-устройстве; не подтверждает работу на реальном телефоне"
                } else {
                    "Выполнено на реальном устройстве на этом соединении"
                },
                &fp,
            )]
        } else {
            vec![]
        };

        OperationResult {
            operation_id: self.operation_id.clone(),
            device_id: self.device.clone(),
            operation: op,
            origin,
            outcome,
            data,
            error: info,
            uncertain,
            duration_ms: started.elapsed().as_millis() as u64,
            evidence: ev,
        }
    }
}
