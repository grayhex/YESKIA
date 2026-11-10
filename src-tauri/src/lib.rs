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
struct NativeState(Arc<yeskia_discovery::adapter::PnpTransport>);
static PIM_ACTIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
static PIM_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// Запускает Doctor и сохраняет отчёт по пути, выбранному пользователем.
#[tauri::command]
async fn export_doctor_report(options: DoctorOptions, path: String) -> Cmd<ExportedReport> {
    tauri::async_runtime::spawn_blocking(move || -> yeskia_core::Result<ExportedReport> {
        let report = yeskia_discovery::run_doctor(&doctor_options(&options));
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute() {
            return Err(Error::InvalidArgument("Выберите путь для отчёта".into()));
        }
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

// ---------- обновление из файла (приложение ничего не скачивает) ----------

#[tauri::command]
async fn update_check(
    state: tauri::State<'_, State>,
    installer: String,
    manifest: String,
    signature: Option<String>,
) -> Cmd<yeskia_core::update::UpdateCheck> {
    blocking(&state, move |c| {
        c.update_check(&installer, &manifest, signature.as_deref())
    })
    .await
}

#[tauri::command]
fn update_blockers(state: tauri::State<'_, State>) -> Vec<String> {
    state.0.update_blockers()
}

/// Запуск установщика: только с подтверждением, после повторной проверки и без незавершённых записей.
#[tauri::command]
async fn update_launch(
    state: tauri::State<'_, State>,
    installer: String,
    manifest: String,
    signature: Option<String>,
    confirm: bool,
    accept_unsigned: bool,
) -> Cmd<()> {
    blocking(&state, move |c| {
        c.update_launch(
            &installer,
            &manifest,
            signature.as_deref(),
            confirm,
            accept_unsigned,
        )
    })
    .await
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

#[tauri::command]
async fn inspect_certificate(path: String) -> Cmd<yeskia_core::pkg::cert::CertInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| Error::Internal("Проверьте дату компьютера".into()))?
            .as_secs() as i64;
        yeskia_core::pkg::cert::inspect_file(std::path::Path::new(&path), at)
    })
    .await
    .map_err(|_| Error::Internal("Ошибка проверки сертификата".into()).to_info())?
    .map_err(|e| e.to_info())
}

#[tauri::command]
async fn install_phone_certificate(
    state: tauri::State<'_, State>,
    native: tauri::State<'_, NativeState>,
    device_id: String,
    path: String,
    fingerprint: String,
    confirm: bool,
) -> Cmd<()> {
    if !confirm {
        return Err(Error::InvalidArgument("CERT_CONFIRM_REQUIRED".into()).to_info());
    }
    let transport = Arc::clone(&native.0);
    blocking(&state, move |core| {
        let id = DeviceId::new(device_id);
        if !core
            .engine
            .list_devices()?
            .iter()
            .any(|d| d.id == id && d.connection == yeskia_core::device::ConnectionState::Connected)
        {
            return Err(Error::NotConnected);
        }
        let der = yeskia_core::pkg::cert::checked_der(std::path::Path::new(&path), &fingerprint)?;
        let _active = PIM_ACTIVE
            .try_lock()
            .map_err(|_| Error::OperationFailed("PIM_BUSY".into()))?;
        PIM_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
        transport.send_certificate(&id, &der, &PIM_CANCEL)
    })
    .await
}

#[tauri::command]
fn cancel_personal_read() {
    PIM_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
async fn read_personal_data(
    state: tauri::State<'_, State>,
    native: tauri::State<'_, NativeState>,
    device_id: String,
    domain: String,
) -> Cmd<serde_json::Value> {
    let transport = Arc::clone(&native.0);
    blocking(&state, move |core| {
        let id = DeviceId::new(device_id);
        if !core
            .engine
            .list_devices()?
            .iter()
            .any(|d| d.id == id && d.connection == yeskia_core::device::ConnectionState::Connected)
        {
            return Err(Error::NotConnected);
        }
        let _active = PIM_ACTIVE
            .try_lock()
            .map_err(|_| Error::OperationFailed("PIM_BUSY".into()))?;
        PIM_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
        transport.read_personal(&id, &domain, &PIM_CANCEL)
    })
    .await
}

#[tauri::command]
fn save_personal_snapshot(path: String, data: String) -> Cmd<()> {
    use std::io::Write;
    if !std::path::Path::new(&path).is_absolute() || data.len() > 8 * 1024 * 1024 {
        return Err(Error::InvalidArgument("Некорректный путь или размер файла".into()).to_info());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| Error::HostAccess("Выберите новое имя файла".into()).to_info())?;
    if file
        .write_all(data.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(Error::HostAccess("Не удалось сохранить файл".into()).to_info());
    }
    Ok(())
}

#[tauri::command]
fn open_latest_release() -> Cmd<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg("https://github.com/grayhex/YESKIA/releases")
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

/// Нативное окно с сообщением (до создания окна приложения). Только Windows.
#[cfg(windows)]
fn show_error(title: &str, text: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let w = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (t, x) = (w(title), w(text));
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(x.as_ptr()),
            PCWSTR(t.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
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
    let native = Arc::new(transport);
    let pnp: Arc<dyn yeskia_core::transport::DeviceTransport> = native.clone();
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

    // WebView2 нужен для окна. Установщик его не скачивает и не ставит сам: при отсутствии — понятное
    // сообщение с инструкцией (Windows 11 включает Runtime).
    #[cfg(windows)]
    if let Err(e) = tauri::webview_version() {
        eprintln!("YESKIA: WebView2 недоступен: {e}");
        show_error(
            "YESKIA не может открыть окно",
            "Не найден Microsoft Edge WebView2 Runtime. В Windows 11 он входит в систему; если его нет, \
             установите его вручную с официальной страницы Microsoft \
             (developer.microsoft.com/microsoft-edge/webview2) и запустите YESKIA снова.\n\n\
             Приложение ничего не скачивает и не устанавливает само.",
        );
        std::process::exit(3);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(State(core))
        .manage(NativeState(native))
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
            open_latest_release,
            read_personal_data,
            cancel_personal_read,
            inspect_certificate,
            install_phone_certificate,
            save_personal_snapshot,
            backup_list,
            start_host_job,
            await_host_job,
            host_job_progress,
            cancel_host_job,
            save_report,
            update_check,
            update_blockers,
            update_launch,
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
