//! Устойчивое локальное хранилище (SQLite) в пользовательском каталоге.

use crate::error::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;

/// Каталог данных: `%APPDATA%\YESKIA` на Windows. `YESKIA_DATA_DIR` переопределяет.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("YESKIA_DATA_DIR") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("APPDATA") {
            return PathBuf::from(p).join("YESKIA");
        }
    }
    if let Some(p) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(p).join("yeskia");
    }
    if let Some(p) = std::env::var_os("HOME") {
        return PathBuf::from(p).join(".local/share/yeskia");
    }
    std::env::temp_dir().join("yeskia")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: String,
    /// Режим «Разработчик»: журнал операций, матрица возможностей, evidence.
    pub developer_mode: bool,
    pub use_mock_device: bool,
    pub log_to_file: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "ru".into(),
            developer_mode: false,
            use_mock_device: false,
            log_to_file: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    Running,
    Success,
    Failed,
    Cancelled,
    /// Приложение завершилось посреди операции; автоматически не возобновляется.
    Interrupted,
}

impl JournalState {
    fn as_str(self) -> &'static str {
        match self {
            JournalState::Running => "running",
            JournalState::Success => "success",
            JournalState::Failed => "failed",
            JournalState::Cancelled => "cancelled",
            JournalState::Interrupted => "interrupted",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "success" => JournalState::Success,
            "failed" => JournalState::Failed,
            "cancelled" => JournalState::Cancelled,
            "interrupted" => JournalState::Interrupted,
            _ => JournalState::Running,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub operation_id: String,
    /// Необратимый короткий отпечаток id устройства, а не сам id.
    pub device_ref: String,
    pub operation: String,
    pub state: JournalState,
    pub error_code: Option<String>,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
}

pub struct Storage {
    conn: Mutex<Connection>,
}

/// FNV-1a: короткий отпечаток для журнала (не криптография, лишь не хранит исходный id).
pub fn device_ref(id: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("dev-{h:016x}")
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::Storage(format!(
                "база создана более новой версией (schema {version})"
            )));
        }
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE operation_journal (
                    operation_id TEXT PRIMARY KEY,
                    device_ref TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    state TEXT NOT NULL,
                    error_code TEXT,
                    started_at_ms INTEGER NOT NULL,
                    finished_at_ms INTEGER
                 );
                 PRAGMA user_version = 1;",
            )?;
        }
        // Операции, оставшиеся «running» после падения, помечаем interrupted и не повторяем.
        conn.execute(
            "UPDATE operation_journal SET state='interrupted', finished_at_ms=?1 WHERE state='running'",
            params![crate::now_ms() as i64],
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let raw: Option<String> = self
            .conn()
            .query_row("SELECT value FROM settings WHERE key='app'", [], |r| {
                r.get(0)
            })
            .optional()?;
        match raw {
            Some(s) => serde_json::from_str(&s).map_err(|e| Error::Storage(e.to_string())),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(s).map_err(|e| Error::Storage(e.to_string()))?;
        self.conn().execute(
            "INSERT INTO settings(key,value) VALUES('app',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn journal_start(&self, op_id: &str, device_id: &str, operation: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO operation_journal(operation_id,device_ref,operation,state,started_at_ms)
             VALUES(?1,?2,?3,'running',?4)",
            params![
                op_id,
                device_ref(device_id),
                operation,
                crate::now_ms() as i64
            ],
        )?;
        Ok(())
    }

    pub fn journal_finish(
        &self,
        op_id: &str,
        state: JournalState,
        error_code: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE operation_journal SET state=?2, error_code=?3, finished_at_ms=?4 WHERE operation_id=?1",
            params![op_id, state.as_str(), error_code, crate::now_ms() as i64],
        )?;
        Ok(())
    }

    pub fn journal_recent(&self, limit: u32) -> Result<Vec<JournalEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT operation_id,device_ref,operation,state,error_code,started_at_ms,finished_at_ms
             FROM operation_journal ORDER BY started_at_ms DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(JournalEntry {
                operation_id: r.get(0)?,
                device_ref: r.get(1)?,
                operation: r.get(2)?,
                state: JournalState::parse(&r.get::<_, String>(3)?),
                error_code: r.get(4)?,
                started_at_ms: r.get::<_, i64>(5)? as u64,
                finished_at_ms: r.get::<_, Option<i64>>(6)?.map(|v| v as u64),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_default() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.load_settings().unwrap(), Settings::default());
        let new = Settings {
            language: "en".into(),
            developer_mode: true,
            use_mock_device: true,
            log_to_file: false,
        };
        s.save_settings(&new).unwrap();
        assert_eq!(s.load_settings().unwrap(), new);
    }

    #[test]
    fn journal_lifecycle_and_no_raw_device_id() {
        let s = Storage::open_in_memory().unwrap();
        s.journal_start("op-1", "secret-device-id", "write_file")
            .unwrap();
        s.journal_finish("op-1", JournalState::Failed, Some("OPERATION_FAILED"))
            .unwrap();
        let e = &s.journal_recent(10).unwrap()[0];
        assert_eq!(e.state, JournalState::Failed);
        assert!(!e.device_ref.contains("secret"));
    }

    #[test]
    fn running_entries_become_interrupted_on_reopen() {
        let dir = std::env::temp_dir().join(format!("yeskia-test-{}", crate::now_ms()));
        let path = dir.join("db.sqlite");
        {
            let s = Storage::open(&path).unwrap();
            s.journal_start("op-9", "d", "write_file").unwrap();
        }
        let s = Storage::open(&path).unwrap();
        assert_eq!(
            s.journal_recent(1).unwrap()[0].state,
            JournalState::Interrupted
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
