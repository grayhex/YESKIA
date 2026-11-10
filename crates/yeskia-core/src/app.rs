//! `AppCore` — собранное приложение без UI: хранилище, журнал, транспорты, движок.
//! Tauri-команды — тонкая обёртка над этим типом, поэтому всё тестируется без WebView.

use crate::backup::{self, BackupInfo};
use crate::device::DeviceId;
use crate::diagnostics::Diagnostics;
use crate::engine::{Engine, OperationHandle};
use crate::error::Result;
use crate::error::{Error, ErrorInfo};
use crate::library::{Item, Library, PackageSet, Provenance, Query};
use crate::media::{self, MediaInfo, Toolchain};
use crate::mock::{MockTransport, MOCK_DEVICE_ID};
use crate::operations::{
    CancellationToken, OperationRequest, OperationResult, Outcome, Progress, ProgressCell,
};
use crate::pkg::inspect::{self, Recommendation, Target};
use crate::storage::{Settings, Storage};
use crate::system::SystemInfo;
use crate::transport::DeviceTransport;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPreview {
    pub mime: String,
    pub base64: String,
}

/// Откуда взять тему для анализа.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThemeSource {
    /// Папка с файлами темы, один из файлов `.skn/.mbm/.mif/.pkg` или SIS на компьютере.
    Path { path: String },
    /// SIS из библиотеки пакетов (по SHA-256).
    Library { id: String },
}

/// Задание без устройства (копии папок компьютера, проверка, восстановление в папку).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HostJobRequest {
    /// Копия папки компьютера в хранилище копий (каталог создаётся ядром).
    #[serde(rename = "backup.create_local")]
    BackupCreateLocal {
        source: String,
        label: String,
        #[serde(default)]
        exclude: Vec<String>,
    },
    /// `name` — имя копии в хранилище (не путь).
    #[serde(rename = "backup.verify")]
    BackupVerify { name: String },
    #[serde(rename = "backup.restore_local")]
    BackupRestoreLocal {
        name: String,
        target: String,
        #[serde(default)]
        only: Option<String>,
        #[serde(default)]
        apply: bool,
    },
    /// Рингтон (MP3) из фрагмента аудио/видео; результат пишется без перезаписи.
    #[serde(rename = "media.ringtone")]
    MediaRingtone {
        source: String,
        dest: String,
        options: media::RingtoneOptions,
    },
    /// Подготовка файла по пресету (экран модели для картинок).
    #[serde(rename = "media.prepare")]
    MediaPrepare {
        source: String,
        dest: String,
        preset: String,
        #[serde(default)]
        model: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostJobData {
    BackupCreated {
        name: String,
        files: u64,
        bytes: u64,
        skipped: u64,
        complete: bool,
        verified: bool,
    },
    BackupVerified {
        report: backup::VerifyReport,
    },
    BackupPlan {
        counts: backup::PlanCounts,
        items: Vec<backup::PlanItem>,
        truncated: bool,
    },
    BackupApplied {
        report: backup::ApplyReport,
    },
    /// Файл создан на компьютере и перечитан (`generated`); передача и установка — отдельные шаги.
    MediaGenerated {
        result: media::Generated,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostJobResult {
    pub job_id: String,
    pub outcome: Outcome,
    pub data: Option<HostJobData>,
    pub error: Option<ErrorInfo>,
}

struct HostJob {
    token: CancellationToken,
    progress: Arc<ProgressCell>,
    rx: Mutex<Option<Receiver<HostJobResult>>>,
}

pub struct AppCore {
    pub engine: Arc<Engine>,
    /// Каталог данных (None — режим в памяти): нужен для переключения файлового лога.
    data_dir: Option<std::path::PathBuf>,
    mock: MockTransport,
    /// Принятые, но ещё не ожидаемые операции (для пары start/await из UI).
    handles: Mutex<HashMap<String, OperationHandle>>,
    host_jobs: Mutex<HashMap<String, Arc<HostJob>>>,
    library: Library,
    /// Временные копии пакетов для передачи: операция → папка (удаляется по завершении).
    staging: Mutex<HashMap<String, PathBuf>>,
    /// Найденный FFmpeg: ключ — настроенная папка.
    tools: Mutex<Option<(Option<String>, Arc<Toolchain>)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmokeReport {
    pub system: SystemInfo,
    pub storage_ok: bool,
    pub mock_identify: String,
}

impl AppCore {
    /// Открывает (создаёт) хранилище в `data_dir` и собирает движок.
    pub fn init(data_dir: &Path) -> Result<Self> {
        Self::init_with(data_dir, Vec::new())
    }

    /// То же, но с дополнительными (реальными) транспортами: Windows PnP/WPD и др.
    pub fn init_with(data_dir: &Path, extra: Vec<Arc<dyn DeviceTransport>>) -> Result<Self> {
        let storage = Arc::new(Storage::open(&data_dir.join("yeskia.sqlite"))?);
        let settings = storage.load_settings()?;
        let log_file = settings
            .log_to_file
            .then(|| data_dir.join("logs").join("yeskia.log"));
        let diag = Arc::new(Diagnostics::new(500, log_file));
        let library = Library::open(&data_dir.join("library"))?;
        // Остатки временных копий после аварийного выхода.
        let _ = std::fs::remove_dir_all(data_dir.join("staging"));
        let mut core = Self::assemble(storage, diag, settings.use_mock_device, extra, library);
        core.data_dir = Some(data_dir.to_path_buf());
        Ok(core)
    }

    /// Целиком в памяти (тесты).
    pub fn in_memory() -> Self {
        let storage = Arc::new(Storage::open_in_memory().expect("in-memory sqlite"));
        Self::assemble(
            storage,
            Arc::new(Diagnostics::new(500, None)),
            true,
            Vec::new(),
            Library::open(&{
                static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                std::env::temp_dir().join(format!(
                    "yeskia-lib-{}-{}-{}",
                    std::process::id(),
                    crate::now_ms(),
                    SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ))
            })
            .expect("библиотека во временной папке"),
        )
    }

    fn assemble(
        storage: Arc<Storage>,
        diag: Arc<Diagnostics>,
        mock_enabled: bool,
        extra: Vec<Arc<dyn DeviceTransport>>,
        library: Library,
    ) -> Self {
        let mock = MockTransport::new();
        if !mock_enabled {
            mock.unplug();
        }
        // Реальные транспорты (WPD/MTP, Mass Storage, PC Suite) добавляются здесь
        // в issue #4–#6; mock явно помечен в модели устройства.
        let mut transports: Vec<Arc<dyn DeviceTransport>> = vec![Arc::new(mock.clone())];
        transports.extend(extra);
        let engine = Arc::new(Engine::new(transports, storage, diag));
        Self {
            engine,
            data_dir: None,
            mock,
            handles: Mutex::new(HashMap::new()),
            host_jobs: Mutex::new(HashMap::new()),
            library,
            staging: Mutex::new(HashMap::new()),
            tools: Mutex::new(None),
        }
    }

    /// Ставит операцию в очередь и возвращает её id (для отмены/прогресса до завершения).
    pub fn start_operation(&self, id: &DeviceId, request: OperationRequest) -> Result<String> {
        let request = self.resolve_backup_paths(request)?;
        let handle = self.engine.submit(id, request)?;
        let op_id = handle.operation_id().to_string();
        self.handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(op_id.clone(), handle);
        Ok(op_id)
    }

    /// Блокирует до результата ранее начатой операции (вызывать из блокирующего потока).
    pub fn await_operation(&self, operation_id: &str) -> Result<OperationResult> {
        let handle = self
            .handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(operation_id)
            .ok_or_else(|| crate::Error::InvalidArgument("неизвестная операция".into()))?;
        let result = handle.wait();
        self.drop_staging(operation_id);
        Ok(result)
    }

    /// Отмена по id. Если операция ещё не ожидается (`await_operation` не вызван), её handle
    /// освобождается сразу и не держит очередь.
    pub fn cancel_operation(&self, operation_id: &str) -> bool {
        let cancelled = self.engine.cancel(operation_id);
        let orphan = self
            .handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(operation_id);
        if orphan.is_some() {
            self.drop_staging(operation_id);
        }
        drop(orphan);
        cancelled
    }

    fn drop_staging(&self, operation_id: &str) {
        let dir = self
            .staging
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(operation_id);
        if let Some(d) = dir {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    // ---------- медиа и темы ----------

    /// FFmpeg: настроенная папка → рядом с приложением → PATH. Кэшируется по настройке.
    pub fn toolchain(&self) -> Result<Arc<Toolchain>> {
        let dir = self
            .get_settings()?
            .ffmpeg_dir
            .filter(|d| !d.trim().is_empty());
        let mut cache = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((key, t)) = cache.as_ref() {
            if *key == dir {
                return Ok(Arc::clone(t));
            }
        }
        let t = Arc::new(Toolchain::locate(dir.as_deref().map(std::path::Path::new))?);
        *cache = Some((dir, Arc::clone(&t)));
        Ok(t)
    }

    pub fn media_tool_info(&self) -> Result<media::ToolInfo> {
        Ok(self.toolchain()?.info.clone())
    }

    pub fn media_probe(&self, path: &str) -> Result<MediaInfo> {
        let p = media::check_input(path)?;
        self.toolchain()?.probe(&p, &|| Ok(()))
    }

    pub fn media_waveform(&self, path: &str, buckets: usize) -> Result<Vec<f32>> {
        let p = media::check_input(path)?;
        self.toolchain()?.waveform(&p, buckets, &|| Ok(()))
    }

    /// Небольшой аудиофайл для предпросмотра в окне (до 8 МиБ; mp3, m4a, aac, wav) как base64.
    pub fn media_preview(&self, path: &str) -> Result<AudioPreview> {
        let p = std::path::Path::new(path);
        crate::fsutil::validate_host_path(p)?;
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mime = match ext.as_str() {
            "mp3" => "audio/mpeg",
            "m4a" | "aac" => "audio/mp4",
            "wav" => "audio/wav",
            _ => {
                return Err(Error::InvalidArgument(
                    "предпросмотр: mp3, m4a, aac или wav".into(),
                ))
            }
        };
        let lp = crate::fsutil::long_path(p);
        let meta =
            std::fs::metadata(&lp).map_err(|_| Error::HostAccess("файл недоступен".into()))?;
        if !meta.is_file() || meta.len() == 0 || meta.len() > 8 * 1024 * 1024 {
            return Err(Error::LimitExceeded(
                "для предпросмотра нужен файл до 8 МиБ".into(),
            ));
        }
        Ok(AudioPreview {
            mime: mime.into(),
            base64: crate::theme::base64(&std::fs::read(lp)?),
        })
    }

    /// Анализ темы: файл/папка на компьютере или SIS из библиотеки пакетов.
    pub fn theme_load(&self, source: &ThemeSource) -> Result<crate::theme::Theme> {
        match source {
            ThemeSource::Path { path } => {
                let lower = path.to_ascii_lowercase();
                if lower.ends_with(".sis") || lower.ends_with(".sisx") {
                    let p = std::path::Path::new(path);
                    crate::fsutil::validate_host_path(p)?;
                    let meta = std::fs::metadata(crate::fsutil::long_path(p))
                        .map_err(|_| Error::HostAccess("файл недоступен".into()))?;
                    if !meta.is_file()
                        || meta.len() == 0
                        || meta.len() > crate::pkg::jar::MAX_PACKAGE as u64
                    {
                        return Err(Error::InvalidArgument(
                            "пакет должен быть файлом до 32 МиБ".into(),
                        ));
                    }
                    crate::theme::load_sis(&std::fs::read(crate::fsutil::long_path(p))?)
                } else {
                    crate::theme::load_folder(path)
                }
            }
            ThemeSource::Library { id } => {
                let (_, data) = self.library.bytes(id)?;
                crate::theme::load_sis(&data)
            }
        }
    }

    pub fn theme_inspect(
        &self,
        source: &ThemeSource,
        model: Option<&str>,
    ) -> Result<crate::theme::ThemeReport> {
        Ok(crate::theme::inspect(&self.theme_load(source)?, model))
    }

    pub fn theme_preview(
        &self,
        source: &ThemeSource,
        limit: usize,
    ) -> Result<Vec<crate::theme::BitmapPreview>> {
        crate::theme::preview(&self.theme_load(source)?, limit.clamp(1, 24))
    }

    // ---------- библиотека пакетов ----------

    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Импорт файла с компьютера. Размер проверяется до чтения.
    pub fn library_import(&self, path: &str, provenance: Provenance) -> Result<Item> {
        let p = std::path::Path::new(path);
        crate::fsutil::validate_host_path(p)?;
        let meta = std::fs::metadata(crate::fsutil::long_path(p))
            .map_err(|_| Error::HostAccess("файл недоступен".into()))?;
        if !meta.is_file() {
            return Err(Error::InvalidArgument("нужен файл, а не папка".into()));
        }
        if meta.len() == 0 || meta.len() > crate::pkg::jar::MAX_PACKAGE as u64 {
            return Err(Error::InvalidArgument(
                "размер пакета должен быть от 1 байта до 32 МиБ".into(),
            ));
        }
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::UnsafePath("имя файла не в Unicode".into()))?;
        let data = std::fs::read(crate::fsutil::long_path(p))?;
        self.library.import(name, &data, provenance)
    }

    pub fn library_list(&self, q: &Query) -> Result<Vec<Item>> {
        self.library.list(q)
    }

    pub fn library_sets(&self) -> Result<Vec<PackageSet>> {
        self.library.sets()
    }

    /// Анализ пакета из библиотеки на момент `at` (секунды Unix; `None` — сейчас).
    pub fn library_inspect(
        &self,
        id: &str,
        target: Target,
        at: Option<i64>,
    ) -> Result<serde_json::Value> {
        let at = at.unwrap_or((crate::now_ms() / 1000) as i64);
        let report = self.library.inspect(id, target, at)?;
        serde_json::to_value(report).map_err(|e| Error::Internal(e.to_string()))
    }

    pub fn package_diagnose(&self, message: &str) -> Result<Recommendation> {
        inspect::diagnose(message).map_err(|e| Error::InvalidArgument(e.0))
    }

    /// «Передать на телефон»: копия пакета из библиотеки (с проверкой целостности) во временную
    /// папку и операция `packages.transfer`. Это запись файла, не установка. Права доступа к
    /// устройству проверяет Engine по матрице возможностей.
    pub fn start_package_transfer(
        &self,
        device: &DeviceId,
        package_id: &str,
        device_dir: &str,
    ) -> Result<String> {
        let staging_root = match &self.data_dir {
            Some(d) => d.join("staging"),
            None => std::env::temp_dir().join(format!("yeskia-staging-{}", std::process::id())),
        };
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stage = staging_root.join(format!(
            "{}-{}",
            crate::now_ms(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let (path, item) = self.library.stage_for_transfer(package_id, &stage)?;
        let dir = device_dir.trim_end_matches('/');
        let request = OperationRequest::PackageTransfer {
            source: path.display().to_string(),
            path: format!("{dir}/{}", item.name),
            sha256: item.id.clone(),
        };
        match self.start_operation(device, request) {
            Ok(op_id) => {
                self.staging
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(op_id.clone(), stage);
                Ok(op_id)
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(stage);
                Err(e)
            }
        }
    }

    pub fn operation_progress(&self, operation_id: &str) -> Option<Progress> {
        self.engine.progress(operation_id)
    }

    // ---------- резервные копии ----------

    /// Хранилище копий: настройка пользователя или `<каталог данных>/backups`.
    pub fn backup_root(&self) -> Result<PathBuf> {
        let settings = self.get_settings()?;
        Ok(
            match settings.backup_root.filter(|r| !r.trim().is_empty()) {
                Some(r) => PathBuf::from(r),
                None => crate::storage::data_dir().join("backups"),
            },
        )
    }

    pub fn backup_list(&self) -> Result<Vec<BackupInfo>> {
        Ok(backup::list_backups(&self.backup_root()?))
    }

    /// Каталог копии по **имени в хранилище**. Пути от клиента не принимаются.
    fn backup_dir(&self, name: &str) -> Result<PathBuf> {
        crate::fsutil::validate_file_name(name)
            .map_err(|_| Error::UnsafePath("недопустимое имя копии".into()))?;
        Ok(self.backup_root()?.join(name))
    }

    /// Для операций устройства: каталог новой копии назначает ядро, существующая копия — по имени.
    fn resolve_backup_paths(&self, request: OperationRequest) -> Result<OperationRequest> {
        Ok(match request {
            OperationRequest::BackupCreate {
                path,
                label,
                exclude,
                ..
            } => {
                let name = backup::backup_dir_name(crate::now_ms(), &label);
                OperationRequest::BackupCreate {
                    path,
                    dest: self.backup_root()?.join(name).display().to_string(),
                    label,
                    exclude,
                }
            }
            OperationRequest::BackupRestore {
                backup,
                path,
                only,
                apply,
            } => OperationRequest::BackupRestore {
                backup: self.backup_dir(&backup)?.display().to_string(),
                path,
                only,
                apply,
            },
            other => other,
        })
    }

    /// Запускает задание без устройства в отдельном потоке (одно за раз).
    pub fn start_host_job(&self, request: HostJobRequest) -> Result<String> {
        let mut jobs = self.host_jobs.lock().unwrap_or_else(|e| e.into_inner());
        if jobs
            .values()
            .any(|j| j.rx.lock().map(|r| r.is_some()).unwrap_or(false))
        {
            return Err(Error::QueueFull);
        }
        let job_id = format!("job-{}", crate::now_ms());
        let (tx, rx) = mpsc::channel();
        let job = Arc::new(HostJob {
            token: CancellationToken::new(),
            progress: Arc::new(ProgressCell::default()),
            rx: Mutex::new(Some(rx)),
        });
        // внутри потока нужны путь хранилища и каталоги — разрешаем их заранее
        let root = self.backup_root()?;
        let plan = match &request {
            HostJobRequest::BackupCreateLocal { label, .. } => {
                Some(root.join(backup::backup_dir_name(crate::now_ms(), label)))
            }
            HostJobRequest::BackupVerify { name }
            | HostJobRequest::BackupRestoreLocal { name, .. } => Some(self.backup_dir(name)?),
            HostJobRequest::MediaRingtone { .. } | HostJobRequest::MediaPrepare { .. } => None,
        };
        let tools = match &request {
            HostJobRequest::MediaRingtone { .. } | HostJobRequest::MediaPrepare { .. } => {
                Some(self.toolchain()?)
            }
            _ => None,
        };
        jobs.insert(job_id.clone(), Arc::clone(&job));
        let id2 = job_id.clone();
        std::thread::spawn(move || {
            let token = job.token.clone();
            let check = || {
                if token.is_cancelled() {
                    Err(Error::Cancelled)
                } else {
                    Ok(())
                }
            };
            let progress =
                |p: backup::BackupProgress| job.progress.set(p.files_done, p.files_total);
            let ctl = backup::Control {
                check: &check,
                progress: &progress,
            };
            let r = run_host_job(request, plan, &root, &ctl, tools);
            let (outcome, data, error) = match r {
                Ok(d) => (Outcome::Success, Some(d), None),
                Err(Error::Cancelled) => {
                    (Outcome::Cancelled, None, Some(Error::Cancelled.to_info()))
                }
                Err(e) => (Outcome::Failed, None, Some(e.to_info())),
            };
            let _ = tx.send(HostJobResult {
                job_id: id2,
                outcome,
                data,
                error,
            });
        });
        Ok(job_id)
    }

    /// Блокирует до результата задания.
    pub fn await_host_job(&self, job_id: &str) -> Result<HostJobResult> {
        let job = self
            .host_jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(job_id)
            .cloned()
            .ok_or_else(|| Error::InvalidArgument("неизвестное задание".into()))?;
        let rx = job
            .rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or_else(|| Error::InvalidArgument("результат задания уже получен".into()))?;
        let r = rx
            .recv()
            .map_err(|_| Error::Internal("задание завершилось без результата".into()))?;
        self.host_jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(job_id);
        Ok(r)
    }

    pub fn host_job_progress(&self, job_id: &str) -> Option<Progress> {
        self.host_jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(job_id)
            .map(|j| j.progress.get())
    }

    pub fn cancel_host_job(&self, job_id: &str) -> bool {
        match self
            .host_jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(job_id)
        {
            Some(j) => {
                j.token.cancel();
                true
            }
            None => false,
        }
    }

    /// Mock-транспорт для тестов (управление отключением, паузами и сбоями).
    pub fn mock_handle(&self) -> MockTransport {
        self.mock.clone()
    }

    pub fn system_info(&self) -> SystemInfo {
        SystemInfo::collect()
    }

    pub fn get_settings(&self) -> Result<Settings> {
        self.engine.storage().load_settings()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<Settings> {
        self.engine.storage().save_settings(s)?;
        if s.use_mock_device {
            self.mock.plug();
        } else {
            self.mock.unplug();
        }
        if let Some(dir) = &self.data_dir {
            let file = s.log_to_file.then(|| dir.join("logs").join("yeskia.log"));
            self.engine.diagnostics().set_file(file);
        }
        Ok(s.clone())
    }

    /// Старт без окна: проверяет хранилище, системные данные и mock-цепочку.
    /// Используется как smoke-test сборки (`--smoke-test`).
    pub fn smoke(&self) -> Result<SmokeReport> {
        let storage_ok = self.engine.storage().load_settings().is_ok();
        let was_enabled = self.get_settings()?.use_mock_device;
        self.mock.plug();
        let id = DeviceId::new(MOCK_DEVICE_ID);
        self.engine.connect(&id)?;
        let r = self.engine.run(&id, OperationRequest::IdentityRead)?;
        self.engine.disconnect(&id)?;
        if !was_enabled {
            self.mock.unplug();
        }
        Ok(SmokeReport {
            system: self.system_info(),
            storage_ok,
            mock_identify: if r.outcome == Outcome::Success {
                "ok".into()
            } else {
                "failed".into()
            },
        })
    }
}

fn run_host_job(
    request: HostJobRequest,
    dir: Option<PathBuf>,
    root: &std::path::Path,
    ctl: &backup::Control,
    tools: Option<Arc<Toolchain>>,
) -> Result<HostJobData> {
    const PLAN_ITEMS_LIMIT: usize = 500;
    let stage = |done: u64, total: u64| {
        (ctl.progress)(backup::BackupProgress {
            files_done: done,
            files_total: total,
            bytes_done: 0,
            bytes_total: 0,
        })
    };
    match request {
        HostJobRequest::MediaRingtone {
            source,
            dest,
            options,
        } => {
            let t = tools.ok_or_else(|| Error::Internal("нет FFmpeg".into()))?;
            Ok(HostJobData::MediaGenerated {
                result: t.ringtone(&source, &dest, &options, ctl.check, &stage)?,
            })
        }
        HostJobRequest::MediaPrepare {
            source,
            dest,
            preset,
            model,
        } => {
            let t = tools.ok_or_else(|| Error::Internal("нет FFmpeg".into()))?;
            Ok(HostJobData::MediaGenerated {
                result: t.prepare(&source, &dest, &preset, &model, ctl.check, &stage)?,
            })
        }
        HostJobRequest::BackupCreateLocal {
            source,
            label,
            exclude,
        } => {
            std::fs::create_dir_all(crate::fsutil::long_path(root))?;
            let dest = dir.ok_or_else(|| Error::Internal("нет каталога копии".into()))?;
            let opts = backup::Options {
                exclude,
                ..Default::default()
            };
            let s = backup::create_from_folder(
                std::path::Path::new(&source),
                &dest,
                &label,
                &opts,
                ctl,
            )?;
            Ok(HostJobData::BackupCreated {
                name: dest
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                files: s.files,
                bytes: s.bytes,
                skipped: s.skipped,
                complete: s.complete,
                verified: s.verified,
            })
        }
        HostJobRequest::BackupVerify { .. } => {
            let dir = dir.ok_or_else(|| Error::Internal("нет каталога копии".into()))?;
            Ok(HostJobData::BackupVerified {
                report: backup::verify(&dir, ctl)?,
            })
        }
        HostJobRequest::BackupRestoreLocal {
            target,
            only,
            apply,
            ..
        } => {
            let dir = dir.ok_or_else(|| Error::Internal("нет каталога копии".into()))?;
            let store = backup::FolderStore::new(std::path::Path::new(&target))?;
            if apply {
                Ok(HostJobData::BackupApplied {
                    report: backup::restore_apply(&dir, &store, only.as_deref(), ctl)?,
                })
            } else {
                let plan = backup::restore_plan(&dir, &store, only.as_deref(), ctl)?;
                let truncated = plan.items.len() > PLAN_ITEMS_LIMIT;
                Ok(HostJobData::BackupPlan {
                    counts: plan.counts,
                    items: plan.items.into_iter().take(PLAN_ITEMS_LIMIT).collect(),
                    truncated,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_runs_in_memory() {
        let app = AppCore::in_memory();
        let r = app.smoke().unwrap();
        assert!(r.storage_ok);
        assert_eq!(r.mock_identify, "ok");
        assert!(!r.system.network_required);
    }

    #[test]
    fn mock_toggle_hides_device() {
        let app = AppCore::in_memory();
        assert_eq!(app.engine.list_devices().unwrap().len(), 1);
        app.save_settings(&Settings {
            use_mock_device: false,
            ..Settings::default()
        })
        .unwrap();
        assert!(app.engine.list_devices().unwrap().is_empty());
    }

    #[test]
    fn log_to_file_setting_applies_immediately() {
        let dir = std::env::temp_dir().join(format!("yeskia-logset-{}", crate::now_ms()));
        let app = AppCore::init(&dir).unwrap();
        let log = dir.join("logs").join("yeskia.log");
        app.save_settings(&Settings {
            log_to_file: false,
            ..Settings::default()
        })
        .unwrap();
        app.engine.diagnostics().info("silent");
        assert!(!log.exists() || !std::fs::read_to_string(&log).unwrap().contains("silent"));
        app.save_settings(&Settings {
            log_to_file: true,
            ..Settings::default()
        })
        .unwrap();
        app.engine.diagnostics().info("loud");
        assert!(std::fs::read_to_string(&log).unwrap().contains("loud"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn operation_ids_do_not_collide_after_restart() {
        let dir = std::env::temp_dir().join(format!("yeskia-restart-{}", crate::now_ms()));
        for _ in 0..2 {
            let app = AppCore::init(&dir).unwrap();
            assert_eq!(app.smoke().unwrap().mock_identify, "ok");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn init_creates_files_in_data_dir() {
        let dir = std::env::temp_dir().join(format!("yeskia-init-{}", crate::now_ms()));
        let app = AppCore::init(&dir).unwrap();
        assert!(dir.join("yeskia.sqlite").exists());
        assert!(app.engine.list_devices().unwrap().is_empty()); // по умолчанию mock выключен
        let _ = std::fs::remove_dir_all(dir);
    }
}
