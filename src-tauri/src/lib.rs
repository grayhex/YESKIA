//! Тонкая Tauri-обёртка над `yeskia_core::app::AppCore`.
//! Команды строго типизированы: ошибки — `ErrorInfo` с кодом.

use std::sync::Arc;
use yeskia_core::app::{AppCore, SmokeReport};
use yeskia_core::app::{AudioPreview, HostJobRequest, HostJobResult, ThemeSource};
use yeskia_core::backup::BackupInfo;
use yeskia_core::device::{Device, DeviceId};
use yeskia_core::diagnostics::LogEntry;
use yeskia_core::error::{Error, ErrorInfo};
use yeskia_core::host::{HostEntry, HostRoot};
use yeskia_core::library::{Item, PackageSet, Provenance, Query};
use yeskia_core::media::{MediaInfo, ToolInfo};
use yeskia_core::operations::{OperationRequest, OperationResult, Progress};
use yeskia_core::pkg::inspect::{Recommendation, Target};
use yeskia_core::storage::{data_dir, JournalEntry, Settings};
use yeskia_core::system::SystemInfo;
use yeskia_discovery::{CollectOptions, DoctorReport};

type Cmd<T> = Result<T, ErrorInfo>;

struct State(Arc<AppCore>);

/// Выполняет блокирующую работу ядра вне потока UI.
async fn blocking<T, F>(core: &State, f: F) -> Cmd<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<AppCore>) -> yeskia_core::Result<T> + Send + 'static,
{
    let core = Arc::clone(&core.0);
    tauri::async_runtime::spawn_blocking(move || f(core))
        .await
        .map_err(|e| Error::Internal(e.to_string()).to_info())?
        .map_err(|e| e.to_info())
}

#[tauri::command]
fn get_system_info(state: tauri::State<'_, State>) -> SystemInfo {
    state.0.system_info()
}

#[tauri::command]
async fn list_devices(state: tauri::State<'_, State>) -> Cmd<Vec<Device>> {
    blocking(&state, |c| c.engine.list_devices()).await
}

#[tauri::command]
async fn connect_device(state: tauri::State<'_, State>, id: String) -> Cmd<Device> {
    blocking(&state, move |c| c.engine.connect(&DeviceId::new(id))).await
}

#[tauri::command]
async fn disconnect_device(state: tauri::State<'_, State>, id: String) -> Cmd<()> {
    blocking(&state, move |c| c.engine.disconnect(&DeviceId::new(id))).await
}

/// Ставит операцию в очередь устройства и возвращает её id (для отмены и прогресса).
#[tauri::command]
fn start_operation(
    state: tauri::State<'_, State>,
    id: String,
    request: OperationRequest,
) -> Cmd<String> {
    state
        .0
        .start_operation(&DeviceId::new(id), request)
        .map_err(|e| e.to_info())
}

/// Ждёт результата ранее начатой операции (очередь, выполнение, таймаут).
#[tauri::command]
async fn await_operation(
    state: tauri::State<'_, State>,
    operation_id: String,
) -> Cmd<OperationResult> {
    blocking(&state, move |c| c.await_operation(&operation_id)).await
}

#[tauri::command]
fn operation_progress(state: tauri::State<'_, State>, operation_id: String) -> Option<Progress> {
    state.0.operation_progress(&operation_id)
}

#[tauri::command]
fn cancel_operation(state: tauri::State<'_, State>, operation_id: String) -> bool {
    state.0.cancel_operation(&operation_id)
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, State>) -> Cmd<Settings> {
    state.0.get_settings().map_err(|e| e.to_info())
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, State>, settings: Settings) -> Cmd<Settings> {
    state.0.save_settings(&settings).map_err(|e| e.to_info())
}

#[tauri::command]
fn recent_operations(state: tauri::State<'_, State>, limit: u32) -> Cmd<Vec<JournalEntry>> {
    state
        .0
        .engine
        .storage()
        .journal_recent(limit.min(200))
        .map_err(|e| e.to_info())
}

#[tauri::command]
fn get_diagnostics(state: tauri::State<'_, State>) -> Vec<LogEntry> {
    state.0.engine.diagnostics().snapshot()
}

#[derive(serde::Deserialize, Default)]
struct DoctorOptions {
    /// Кратковременно открывать COM-порты, чтобы отличить «занят» от «свободен».
    probe_com_busy: bool,
}

fn doctor_options(o: &DoctorOptions) -> CollectOptions {
    CollectOptions {
        wpd_listing: true,
        probe_com_busy: o.probe_com_busy,
    }
}

/// Doctor: read-only диагностика USB/PnP/WPD/COM, privacy-safe отчёт.
#[tauri::command]
async fn run_doctor(options: DoctorOptions) -> Cmd<DoctorReport> {
    tauri::async_runtime::spawn_blocking(move || {
        yeskia_discovery::run_doctor(&doctor_options(&options))
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()).to_info())
}

#[derive(serde::Serialize)]
struct ExportedReport {
    path: String,
    report: DoctorReport,
}

/// Запускает Doctor и сохраняет отчёт (без IMEI/серийников/instance id) в каталог данных.
#[tauri::command]
async fn export_doctor_report(options: DoctorOptions) -> Cmd<ExportedReport> {
    tauri::async_runtime::spawn_blocking(move || -> yeskia_core::Result<ExportedReport> {
        let report = yeskia_discovery::run_doctor(&doctor_options(&options));
        let dir = data_dir().join("reports");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("doctor-{}.json", report.generated_at_ms));
        let json =
            serde_json::to_string_pretty(&report).map_err(|e| Error::Internal(e.to_string()))?;
        std::fs::write(&path, json)?;
        Ok(ExportedReport {
            path: path.display().to_string(),
            report,
        })
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()).to_info())?
    .map_err(|e| e.to_info())
}

// ---------- медиа и темы (локально) ----------

/// FFmpeg: версия, лицензия, путь. Нет инструмента — ошибка `TOOL_MISSING`.
#[tauri::command]
async fn media_tool_info(state: tauri::State<'_, State>) -> Cmd<ToolInfo> {
    blocking(&state, |c| c.media_tool_info()).await
}

#[tauri::command]
async fn media_probe(state: tauri::State<'_, State>, path: String) -> Cmd<MediaInfo> {
    blocking(&state, move |c| c.media_probe(&path)).await
}

#[tauri::command]
async fn media_waveform(
    state: tauri::State<'_, State>,
    path: String,
    buckets: usize,
) -> Cmd<Vec<f32>> {
    blocking(&state, move |c| c.media_waveform(&path, buckets)).await
}

#[tauri::command]
async fn media_preview(state: tauri::State<'_, State>, path: String) -> Cmd<AudioPreview> {
    blocking(&state, move |c| c.media_preview(&path)).await
}

#[tauri::command]
async fn theme_inspect(
    state: tauri::State<'_, State>,
    source: ThemeSource,
    model: Option<String>,
) -> Cmd<yeskia_core::theme::ThemeReport> {
    blocking(&state, move |c| c.theme_inspect(&source, model.as_deref())).await
}

#[tauri::command]
async fn theme_preview(
    state: tauri::State<'_, State>,
    source: ThemeSource,
    limit: usize,
) -> Cmd<Vec<yeskia_core::theme::BitmapPreview>> {
    blocking(&state, move |c| c.theme_preview(&source, limit)).await
}

// ---------- библиотека пакетов (офлайн) ----------

#[tauri::command]
async fn library_list(state: tauri::State<'_, State>, query: Query) -> Cmd<Vec<Item>> {
    blocking(&state, move |c| c.library_list(&query)).await
}

/// Импорт выбранных файлов (пути из нативного диалога). Каждый файл — отдельный результат.
#[tauri::command]
async fn library_import(
    state: tauri::State<'_, State>,
    paths: Vec<String>,
    provenance: Provenance,
) -> Cmd<Vec<Item>> {
    blocking(&state, move |c| {
        if paths.len() > 100 {
            return Err(Error::InvalidArgument("не больше 100 файлов за раз".into()));
        }
        paths
            .iter()
            .map(|p| c.library_import(p, provenance.clone()))
            .collect()
    })
    .await
}

#[tauri::command]
async fn library_remove(state: tauri::State<'_, State>, id: String) -> Cmd<()> {
    blocking(&state, move |c| c.library().remove(&id)).await
}

#[tauri::command]
async fn library_inspect(
    state: tauri::State<'_, State>,
    id: String,
    target: Target,
    at: Option<i64>,
) -> Cmd<serde_json::Value> {
    blocking(&state, move |c| c.library_inspect(&id, target, at)).await
}

#[tauri::command]
async fn library_sets(state: tauri::State<'_, State>) -> Cmd<Vec<PackageSet>> {
    blocking(&state, |c| c.library_sets()).await
}

#[tauri::command]
async fn library_create_set(state: tauri::State<'_, State>, name: String) -> Cmd<PackageSet> {
    blocking(&state, move |c| c.library().create_set(&name)).await
}

#[tauri::command]
async fn library_delete_set(state: tauri::State<'_, State>, id: i64) -> Cmd<()> {
    blocking(&state, move |c| c.library().delete_set(id)).await
}

#[tauri::command]
async fn library_set_add(state: tauri::State<'_, State>, set: i64, package: String) -> Cmd<()> {
    blocking(&state, move |c| c.library().set_add(set, &package)).await
}

#[tauri::command]
async fn library_set_remove(state: tauri::State<'_, State>, set: i64, package: String) -> Cmd<()> {
    blocking(&state, move |c| c.library().set_remove(set, &package)).await
}

#[tauri::command]
fn package_diagnose(state: tauri::State<'_, State>, message: String) -> Cmd<Recommendation> {
    state.0.package_diagnose(&message).map_err(|e| e.to_info())
}

/// «Передать на телефон»: запись файла пакета (не установка). Возвращает id операции.
#[tauri::command]
async fn start_package_transfer(
    state: tauri::State<'_, State>,
    id: String,
    package: String,
    dir: String,
) -> Cmd<String> {
    blocking(&state, move |c| {
        c.start_package_transfer(&DeviceId::new(id), &package, &dir)
    })
    .await
}

/// Резервные копии в хранилище (папка пользователя или `<данные>/backups`).
#[tauri::command]
async fn backup_list(state: tauri::State<'_, State>) -> Cmd<Vec<BackupInfo>> {
    blocking(&state, |c| c.backup_list()).await
}

/// Запускает задание без устройства (копия папки компьютера, проверка, восстановление в папку).
#[tauri::command]
fn start_host_job(state: tauri::State<'_, State>, request: HostJobRequest) -> Cmd<String> {
    state.0.start_host_job(request).map_err(|e| e.to_info())
}

#[tauri::command]
async fn await_host_job(state: tauri::State<'_, State>, job_id: String) -> Cmd<HostJobResult> {
    blocking(&state, move |c| c.await_host_job(&job_id)).await
}

#[tauri::command]
fn host_job_progress(state: tauri::State<'_, State>, job_id: String) -> Option<Progress> {
    state.0.host_job_progress(&job_id)
}

#[tauri::command]
fn cancel_host_job(state: tauri::State<'_, State>, job_id: String) -> bool {
    state.0.cancel_host_job(&job_id)
}

/// Сохраняет отчёт (JSON) в `<данные>/reports/<имя>.json`; имя — только безопасное имя файла.
#[tauri::command]
fn save_report(name: String, json: String) -> Cmd<String> {
    let go = || -> yeskia_core::Result<String> {
        yeskia_core::fsutil::validate_file_name(&name)?;
        let dir = data_dir().join("reports");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, json)?;
        Ok(path.display().to_string())
    };
    go().map_err(|e| e.to_info())
}

/// Диски и быстрые папки пользователя (левая панель файлового менеджера).
#[tauri::command]
fn host_roots() -> Vec<HostRoot> {
    yeskia_core::host::roots()
}

/// Содержимое папки на компьютере (только чтение).
#[tauri::command]
async fn host_list_dir(path: String) -> Cmd<Vec<HostEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        yeskia_core::host::list_dir(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()).to_info())?
    .map_err(|e| e.to_info())
}

/// Создаёт папку назначения (существующая папка — не ошибка, существующий файл — `ALREADY_EXISTS`).
#[tauri::command]
async fn host_create_dir(path: String) -> Cmd<()> {
    tauri::async_runtime::spawn_blocking(move || {
        yeskia_core::host::create_dir(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()).to_info())?
    .map_err(|e| e.to_info())
}

/// «Показать в проводнике»: выделяет файл/папку в Explorer. Ничего не открывает и не запускает.
#[tauri::command]
fn reveal_in_explorer(path: String) -> Cmd<()> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_absolute() || path.chars().any(|c| c.is_control()) || !p.exists() {
        return Err(Error::HostAccess("путь не найден".into()).to_info());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", p.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| Error::Internal(e.to_string()).to_info())
    }
    #[cfg(not(windows))]
    {
        Err(Error::HostAccess("доступно только в Windows".into()).to_info())
    }
}

fn smoke_test(core: &AppCore) -> yeskia_core::Result<SmokeReport> {
    core.smoke()
}

pub fn run() {
    // `yeskia-app --doctor [--com-busy]`: печатает privacy-safe отчёт JSON и завершается
    // (без окна; для чистого Windows 11 и для CI). Ничего не устанавливает и не пишет на устройство.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--doctor") {
        let options = CollectOptions {
            wpd_listing: true,
            probe_com_busy: args.iter().any(|a| a == "--com-busy"),
        };
        let report = yeskia_discovery::run_doctor(&options);
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        return;
    }

    let source: std::sync::Arc<dyn yeskia_discovery::SnapshotSource> =
        std::sync::Arc::from(yeskia_discovery::system_source());
    #[allow(unused_mut)]
    let mut transport = yeskia_discovery::adapter::PnpTransport::new(source);
    // AT-канал по виртуальному COM-порту (открытый стандарт 3GPP, только чтение). Не протокол NPC;
    // на реальном телефоне не проверялся.
    #[cfg(windows)]
    {
        transport = transport.with_serial(std::sync::Arc::new(
            yeskia_discovery::windows_serial::WindowsSerialOpener,
        ));
    }
    let pnp: std::sync::Arc<dyn yeskia_core::transport::DeviceTransport> =
        std::sync::Arc::new(transport);
    let core = match AppCore::init_with(&data_dir(), vec![pnp]) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!(
                "YESKIA: не удалось инициализировать ядро: {}",
                e.to_info().message
            );
            std::process::exit(1);
        }
    };

    // `yeskia-app --smoke-test`: старт без окна, печатает JSON и завершается.
    if std::env::args().any(|a| a == "--smoke-test") {
        match smoke_test(&core) {
            Ok(report) => {
                println!("{}", serde_json::to_string(&report).unwrap_or_default());
                return;
            }
            Err(e) => {
                eprintln!("smoke-test failed: {}", e.to_info().message);
                std::process::exit(2);
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(State(core))
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            list_devices,
            connect_device,
            disconnect_device,
            start_operation,
            await_operation,
            operation_progress,
            cancel_operation,
            get_settings,
            save_settings,
            recent_operations,
            get_diagnostics,
            run_doctor,
            export_doctor_report,
            backup_list,
            start_host_job,
            await_host_job,
            host_job_progress,
            cancel_host_job,
            save_report,
            media_tool_info,
            media_probe,
            media_waveform,
            media_preview,
            theme_inspect,
            theme_preview,
            library_list,
            library_import,
            library_remove,
            library_inspect,
            library_sets,
            library_create_set,
            library_delete_set,
            library_set_add,
            library_set_remove,
            package_diagnose,
            start_package_transfer,
            host_roots,
            host_list_dir,
            host_create_dir,
            reveal_in_explorer,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска YESKIA");
}
