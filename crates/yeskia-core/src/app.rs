//! `AppCore` — собранное приложение без UI: хранилище, журнал, транспорты, движок.
//! Tauri-команды — тонкая обёртка над этим типом, поэтому всё тестируется без WebView.

use crate::device::DeviceId;
use crate::diagnostics::Diagnostics;
use crate::engine::{Engine, OperationHandle};
use crate::error::Result;
use crate::mock::{MockTransport, MOCK_DEVICE_ID};
use crate::operations::{OperationRequest, OperationResult, Outcome, Progress};
use crate::storage::{Settings, Storage};
use crate::system::SystemInfo;
use crate::transport::DeviceTransport;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct AppCore {
    pub engine: Arc<Engine>,
    /// Каталог данных (None — режим в памяти): нужен для переключения файлового лога.
    data_dir: Option<std::path::PathBuf>,
    mock: MockTransport,
    /// Принятые, но ещё не ожидаемые операции (для пары start/await из UI).
    handles: Mutex<HashMap<String, OperationHandle>>,
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
        let mut core = Self::assemble(storage, diag, settings.use_mock_device, extra);
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
        )
    }

    fn assemble(
        storage: Arc<Storage>,
        diag: Arc<Diagnostics>,
        mock_enabled: bool,
        extra: Vec<Arc<dyn DeviceTransport>>,
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
        }
    }

    /// Ставит операцию в очередь и возвращает её id (для отмены/прогресса до завершения).
    pub fn start_operation(&self, id: &DeviceId, request: OperationRequest) -> Result<String> {
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
        Ok(handle.wait())
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
        drop(orphan);
        cancelled
    }

    pub fn operation_progress(&self, operation_id: &str) -> Option<Progress> {
        self.engine.progress(operation_id)
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
