//! Тонкая Tauri-обёртка над `yeskia_core::app::AppCore`.
//! Команды строго типизированы: ошибки — `ErrorInfo` с кодом.

use std::sync::Arc;
use yeskia_core::app::{AppCore, SmokeReport};
use yeskia_core::device::{Device, DeviceId};
use yeskia_core::diagnostics::LogEntry;
use yeskia_core::error::{Error, ErrorInfo};
use yeskia_core::operations::{OperationRequest, OperationResult, Progress};
use yeskia_core::storage::{data_dir, JournalEntry, Settings};
use yeskia_core::system::SystemInfo;

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
    state.0.engine.cancel(&operation_id)
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

fn smoke_test(core: &AppCore) -> yeskia_core::Result<SmokeReport> {
    core.smoke()
}

pub fn run() {
    let core = match AppCore::init(&data_dir()) {
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
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска YESKIA");
}
