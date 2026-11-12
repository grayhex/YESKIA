//! Устойчивое локальное хранилище (SQLite) в пользовательском каталоге.

use crate::error::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 2;

/// Идентификатор приложения (совпадает с `identifier` в tauri.conf.json). Установщик Tauri по галочке
/// «удалить данные приложения» чистит именно `%APPDATA%\<id>` и `%LOCALAPPDATA%\<id>`.
pub const APP_ID: &str = "dev.yeskia.app";
/// Файл рядом с программой, включающий портативный режим (данные — в подпапке `data`).
pub const PORTABLE_MARKER: &str = "yeskia.portable";

/// Портативный режим: рядом с исполняемым файлом лежит `yeskia.portable` — данные в `<папка>\data`,
/// ничего не пишется в профиль пользователя и реестр.
pub fn portable_dir(exe_dir: &Path) -> Option<PathBuf> {
    exe_dir
        .join(PORTABLE_MARKER)
        .is_file()
        .then(|| exe_dir.join("data"))
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// `YESKIA_DATA_DIR` с непустым значением; пустая переменная считается незаданной.
fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os("YESKIA_DATA_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn is_portable() -> bool {
    data_dir_override().is_none() && exe_dir().is_some_and(|d| portable_dir(&d).is_some())
}

/// Каталог данных. Порядок: `YESKIA_DATA_DIR` → портативный режим → `%APPDATA%\dev.yeskia.app`.
pub fn data_dir() -> PathBuf {
    if let Some(p) = data_dir_override() {
        return p;
    }
    if let Some(p) = exe_dir().and_then(|d| portable_dir(&d)) {
        return p;
    }
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("APPDATA") {
            return PathBuf::from(p).join(APP_ID);
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

/// Хранилище резервных копий по умолчанию. На Windows (не портативный режим) — `Документы\YESKIA\Backups`
/// **вне** каталога данных: удаление данных приложения при деинсталляции копии не затрагивает. В портативном
/// режиме — рядом с программой (`data\backups`): пользователь переносит папку целиком.
pub fn default_backup_root() -> PathBuf {
    default_backup_root_for(
        data_dir_override().is_some(),
        exe_dir().and_then(|d| portable_dir(&d)),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        data_dir(),
    )
}

pub fn default_backup_root_for(
    data_dir_overridden: bool,
    portable: Option<PathBuf>,
    user_profile: Option<PathBuf>,
    data_dir: PathBuf,
) -> PathBuf {
    if data_dir_overridden || portable.is_some() || !cfg!(windows) {
        return data_dir.join("backups");
    }
    match user_profile {
        Some(home) => home.join("Documents").join("YESKIA").join("Backups"),
        None => data_dir.join("backups"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemePreference,
    pub language: String,
    /// Режим «Разработчик»: журнал операций, матрица возможностей, evidence.
    pub developer_mode: bool,
    pub use_mock_device: bool,
    pub log_to_file: bool,
    /// Хранилище резервных копий (папка Windows); `None` — `<каталог данных>\\backups`.
    pub backup_root: Option<String>,
    /// Папка с `ffmpeg` и `ffprobe`; `None` — искать рядом с приложением и в PATH.
    pub ffmpeg_dir: Option<String>,
    /// Первый запуск пройден (приветствие закрыто). Сбрасывается только вручную в настройках.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::Light,
            language: "ru".into(),
            developer_mode: false,
            use_mock_device: false,
            log_to_file: true,
            backup_root: None,
            ffmpeg_dir: None,
            onboarded: false,
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
    /// Для записей: исход на устройстве неизвестен.
    pub uncertain: bool,
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
        if version < 2 {
            conn.execute_batch(
                "ALTER TABLE operation_journal ADD COLUMN uncertain INTEGER NOT NULL DEFAULT 0;
                 PRAGMA user_version = 2;",
            )?;
        }
        // Операции, оставшиеся «running» после падения, помечаем interrupted и не повторяем.
        conn.execute(
            "UPDATE operation_journal SET state='interrupted', finished_at_ms=?1,
               uncertain = CASE WHEN operation IN
                 ('files.write','contacts.write','sms.send','packages.install') THEN 1 ELSE 0 END
             WHERE state='running'",
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

    /// Случайная соль установки для fingerprint устройств (создаётся при первом запуске).
    pub fn install_salt(&self) -> Result<String> {
        let existing: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key='install_salt'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(s) = existing {
            return Ok(s);
        }
        let mut buf = [0u8; 16];
        getrandom::getrandom(&mut buf).map_err(|e| Error::Storage(e.to_string()))?;
        let salt: String = buf.iter().map(|b| format!("{b:02x}")).collect();
        self.conn().execute(
            "INSERT INTO settings(key,value) VALUES('install_salt',?1)",
            params![salt],
        )?;
        Ok(salt)
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
        uncertain: bool,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE operation_journal SET state=?2, error_code=?3, finished_at_ms=?4, uncertain=?5
             WHERE operation_id=?1",
            params![
                op_id,
                state.as_str(),
                error_code,
                crate::now_ms() as i64,
                uncertain
            ],
        )?;
        Ok(())
    }

    pub fn journal_recent(&self, limit: u32) -> Result<Vec<JournalEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT operation_id,device_ref,operation,state,error_code,started_at_ms,finished_at_ms,uncertain
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
                uncertain: r.get::<_, i64>(7)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_mode_needs_the_marker_next_to_the_program() {
        let d = std::env::temp_dir().join(format!("yeskia-portable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(portable_dir(&d), None, "без маркера режим обычный");
        std::fs::write(d.join(PORTABLE_MARKER), b"").unwrap();
        assert_eq!(portable_dir(&d), Some(d.join("data")));
        // маркер-папка не считается маркером
        let e = d.join("sub");
        std::fs::create_dir_all(e.join(PORTABLE_MARKER)).unwrap();
        assert_eq!(portable_dir(&e), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn default_backups_live_outside_the_app_data_dir_on_windows() {
        let data = PathBuf::from("C:\\Users\\u\\AppData\\Roaming\\dev.yeskia.app");
        let home = PathBuf::from("C:\\Users\\u");
        let got = default_backup_root_for(false, None, Some(home.clone()), data.clone());
        if cfg!(windows) {
            assert_eq!(got, home.join("Documents").join("YESKIA").join("Backups"));
            assert!(
                !got.starts_with(&data),
                "деинсталляция данных не удаляет копии"
            );
        } else {
            assert_eq!(got, data.join("backups"));
        }
        // портативный режим и явный каталог данных — копии рядом с данными
        let p = PathBuf::from("D:\\Tools\\YESKIA\\data");
        assert_eq!(
            default_backup_root_for(false, Some(p.clone()), Some(home.clone()), p.clone()),
            p.join("backups")
        );
        assert_eq!(
            default_backup_root_for(true, None, Some(home), data.clone()),
            data.join("backups")
        );
        assert_eq!(
            default_backup_root_for(false, None, None, data.clone()),
            data.join("backups")
        );
    }

    #[test]
    fn settings_roundtrip_and_default() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.load_settings().unwrap(), Settings::default());
        let legacy: Settings =
            serde_json::from_str(r#"{"language":"ru","onboarded":true}"#).unwrap();
        assert_eq!(legacy.theme, ThemePreference::Light);
        let new = Settings {
            theme: ThemePreference::Dark,
            language: "en".into(),
            developer_mode: true,
            use_mock_device: true,
            log_to_file: false,
            backup_root: Some("D:\\Backups".into()),
            ffmpeg_dir: Some("D:\\Tools\\ffmpeg".into()),
            onboarded: true,
        };
        s.save_settings(&new).unwrap();
        assert_eq!(s.load_settings().unwrap(), new);
    }

    #[test]
    fn journal_lifecycle_and_no_raw_device_id() {
        let s = Storage::open_in_memory().unwrap();
        s.journal_start("op-1", "secret-device-id", "write_file")
            .unwrap();
        s.journal_finish("op-1", JournalState::Failed, Some("OPERATION_FAILED"), true)
            .unwrap();
        let e = &s.journal_recent(10).unwrap()[0];
        assert_eq!(e.state, JournalState::Failed);
        assert!(!e.device_ref.contains("secret"));
        assert!(e.uncertain);
    }

    #[test]
    fn install_salt_is_random_and_stable() {
        let a = Storage::open_in_memory().unwrap();
        let b = Storage::open_in_memory().unwrap();
        assert_eq!(a.install_salt().unwrap(), a.install_salt().unwrap());
        assert_ne!(a.install_salt().unwrap(), b.install_salt().unwrap());
        assert_eq!(a.install_salt().unwrap().len(), 32);
    }

    #[test]
    fn running_entries_become_interrupted_on_reopen() {
        let dir = std::env::temp_dir().join(format!("yeskia-test-{}", crate::now_ms()));
        let path = dir.join("db.sqlite");
        {
            let s = Storage::open(&path).unwrap();
            s.journal_start("op-9", "d", "files.write").unwrap();
        }
        let s = Storage::open(&path).unwrap();
        assert_eq!(
            s.journal_recent(1).unwrap()[0].state,
            JournalState::Interrupted
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
