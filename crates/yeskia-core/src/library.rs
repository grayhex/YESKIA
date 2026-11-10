//! Локальная библиотека пакетов (issue #8): импорт файлов, SHA-256, метаданные из статического
//! разбора, поиск и фильтры, наборы приложений, отчёты инспектора. Всё офлайн, в SQLite
//! (`library.sqlite`) и папке `library/files/<sha256>`. Файлы хранятся без расширения и **никогда
//! не исполняются**; исходное имя — только метаданные. Облака и скачивания нет.
//!
//! Происхождение (provenance) и права вводит пользователь: приложение их не проверяет и не
//! выводит; по умолчанию права `unknown`.

use crate::error::{Error, Result};
use crate::pkg::inspect::{self, Format, Options, Target};
use crate::pkg::{jar, profile};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;
/// Пределы библиотеки: защита диска и интерфейса.
pub const MAX_ITEMS: i64 = 5000;
pub const MAX_SETS: i64 = 200;
pub const MAX_SET_ITEMS: i64 = 200;
const MAX_TEXT: usize = 300;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Откуда файл (свободный текст пользователя: «сайт автора», «карта памяти»…).
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub license: String,
    /// `unknown` | `user-confirmed`: пользователь подтвердил, что вправе хранить/передавать файл.
    #[serde(default = "unknown")]
    pub rights: String,
    #[serde(default)]
    pub note: String,
}

fn unknown() -> String {
    "unknown".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// SHA-256 содержимого (hex): идентификатор и ключ дедупликации.
    pub id: String,
    pub name: String,
    pub format: String,
    pub size: u64,
    pub added_ms: u64,
    /// `ok` | `corrupt` (контрольные суммы не сошлись) | `unreadable` (не разобрать как формат).
    pub status: String,
    pub error: Option<String>,
    pub uid: Option<String>,
    pub title: Option<String>,
    pub version: Option<String>,
    pub vendor: Option<String>,
    pub signed: Option<bool>,
    /// UID платформ из prerequisites (`0x101f7961`…).
    pub platform_uids: Vec<String>,
    pub provenance: Provenance,
    /// Для запроса с моделью: `declared` (платформа пакета принимается моделью) | `not-declared`
    /// (платформа пакета не принимается) | `unknown` (пакет не объявляет платформу или модель не выбрана).
    /// Это заявленные данные пакета, а не проверка установки.
    pub fit: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub format: Option<String>,
    /// Идентификатор профиля модели (`nokia-e52`).
    #[serde(default)]
    pub model: Option<String>,
    /// Показать только `declared` для выбранной модели.
    #[serde(default)]
    pub only_fitting: bool,
    #[serde(default)]
    pub set: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSet {
    pub id: i64,
    pub name: String,
    pub items: Vec<String>,
}

pub struct Library {
    dir: PathBuf,
    conn: Mutex<Connection>,
}

fn now() -> u64 {
    crate::now_ms()
}

fn clip(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect()
}

fn db(e: rusqlite::Error) -> Error {
    Error::Storage(e.to_string())
}

impl Library {
    /// Открывает (создаёт) библиотеку в `dir` (`library.sqlite` и `files/`).
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir.join("files"))?;
        let conn = Connection::open(dir.join("library.sqlite")).map_err(db)?;
        Self::init(dir, conn)
    }

    pub fn open_in_memory(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir.join("files"))?;
        Self::init(dir, Connection::open_in_memory().map_err(db)?)
    }

    fn init(dir: &Path, conn: Connection) -> Result<Self> {
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db)?;
        if version > SCHEMA_VERSION {
            return Err(Error::Storage(
                "библиотека создана более новой версией приложения".into(),
            ));
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(db)?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE packages (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    format TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    added_ms INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    error TEXT,
                    uid TEXT,
                    title TEXT,
                    version TEXT,
                    vendor TEXT,
                    signed INTEGER,
                    platform_uids TEXT NOT NULL DEFAULT '[]',
                    provenance TEXT NOT NULL DEFAULT '{}'
                 );
                 CREATE TABLE sets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE
                 );
                 CREATE TABLE set_items (
                    set_id INTEGER NOT NULL REFERENCES sets(id) ON DELETE CASCADE,
                    package_id TEXT NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
                    position INTEGER NOT NULL,
                    PRIMARY KEY (set_id, package_id)
                 );
                 CREATE TABLE reports (
                    package_id TEXT NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
                    model TEXT NOT NULL,
                    created_ms INTEGER NOT NULL,
                    json TEXT NOT NULL,
                    PRIMARY KEY (package_id, model)
                 );
                 PRAGMA user_version = 1;",
            )
            .map_err(db)?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn file_path(&self, id: &str) -> Result<PathBuf> {
        // id — всегда hex SHA-256: никаких путей из внешнего ввода.
        if id.len() != 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::InvalidArgument(
                "неверный идентификатор пакета".into(),
            ));
        }
        Ok(self.dir.join("files").join(id))
    }

    /// Импорт файла. Повторный импорт тех же байтов обновляет только происхождение.
    /// Нечитаемые и повреждённые пакеты сохраняются со статусом (чтобы пользователь видел причину).
    pub fn import(&self, name: &str, data: &[u8], provenance: Provenance) -> Result<Item> {
        let clean: String = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(name)
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        crate::fsutil::validate_file_name(&clean)?;
        let format = Format::from_name(&clean).ok_or_else(|| {
            Error::InvalidArgument("поддерживаются файлы .sis, .sisx, .jar, .jad".into())
        })?;
        if data.is_empty() || data.len() > jar::MAX_PACKAGE {
            return Err(Error::InvalidArgument(
                "размер пакета должен быть от 1 байта до 32 МиБ".into(),
            ));
        }
        let id = crate::fsutil::sha256_hex(data);
        let prov = Provenance {
            source: clip(&provenance.source),
            url: clip(&provenance.url),
            license: clip(&provenance.license),
            rights: if provenance.rights == "user-confirmed" {
                "user-confirmed".into()
            } else {
                "unknown".into()
            },
            note: clip(&provenance.note),
        };
        let fields = analyse(&clean, data);

        {
            let c = self.conn();
            let count: i64 = c
                .query_row("SELECT COUNT(*) FROM packages", [], |r| r.get(0))
                .map_err(db)?;
            let exists: bool = c
                .query_row("SELECT 1 FROM packages WHERE id = ?1", [&id], |_| Ok(true))
                .optional()
                .map_err(db)?
                .unwrap_or(false);
            if !exists && count >= MAX_ITEMS {
                return Err(Error::InvalidArgument("библиотека переполнена".into()));
            }
        }
        // Файл на диск — временный + переименование, затем проверка по хешу.
        let dest = self.file_path(&id)?;
        if !dest.exists() {
            let tmp = dest.with_extension(format!("part-{}", now()));
            std::fs::write(&tmp, data)?;
            if let Err(e) = std::fs::rename(&tmp, &dest) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
        let c = self.conn();
        c.execute(
            "INSERT INTO packages (id, name, format, size, added_ms, status, error, uid, title, version, vendor, signed, platform_uids, provenance)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET provenance = excluded.provenance, name = excluded.name",
            params![
                id,
                clean,
                format_str(format),
                data.len() as i64,
                now() as i64,
                fields.status,
                fields.error,
                fields.uid,
                fields.title,
                fields.version,
                fields.vendor,
                fields.signed,
                serde_json::to_string(&fields.platform_uids).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&prov).unwrap_or_else(|_| "{}".into()),
            ],
        )
        .map_err(db)?;
        drop(c);
        self.get(&id, None)
    }

    pub fn get(&self, id: &str, model: Option<&str>) -> Result<Item> {
        self.file_path(id)?;
        let c = self.conn();
        let item = c
            .query_row(
                "SELECT id, name, format, size, added_ms, status, error, uid, title, version, vendor, signed, platform_uids, provenance FROM packages WHERE id = ?1",
                [id],
                row_to_item,
            )
            .optional()
            .map_err(db)?
            .ok_or_else(|| Error::InvalidArgument("пакета нет в библиотеке".into()))?;
        Ok(with_fit(item, model))
    }

    /// Поиск и фильтры. Сортировка: новые сверху.
    pub fn list(&self, q: &Query) -> Result<Vec<Item>> {
        let c = self.conn();
        let mut stmt = c
            .prepare(
                "SELECT id, name, format, size, added_ms, status, error, uid, title, version, vendor, signed, platform_uids, provenance FROM packages ORDER BY added_ms DESC, name ASC",
            )
            .map_err(db)?;
        let rows = stmt.query_map([], row_to_item).map_err(db)?;
        let needle = q.text.trim().to_lowercase();
        let in_set: Option<Vec<String>> = match q.set {
            Some(sid) => {
                let mut s = c
                    .prepare("SELECT package_id FROM set_items WHERE set_id = ?1")
                    .map_err(db)?;
                let ids = s
                    .query_map([sid], |r| r.get::<_, String>(0))
                    .map_err(db)?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(db)?;
                Some(ids)
            }
            None => None,
        };
        let mut out = Vec::new();
        for r in rows {
            let it = with_fit(r.map_err(db)?, q.model.as_deref());
            if let Some(f) = &q.format {
                if &it.format != f {
                    continue;
                }
            }
            if let Some(ids) = &in_set {
                if !ids.contains(&it.id) {
                    continue;
                }
            }
            if q.only_fitting && it.fit.as_deref() != Some("declared") {
                continue;
            }
            if !needle.is_empty() {
                let hay = format!(
                    "{} {} {} {} {}",
                    it.name,
                    it.title.clone().unwrap_or_default(),
                    it.vendor.clone().unwrap_or_default(),
                    it.uid.clone().unwrap_or_default(),
                    it.id
                )
                .to_lowercase();
                if !hay.contains(&needle) {
                    continue;
                }
            }
            out.push(it);
        }
        Ok(out)
    }

    /// Байты пакета с проверкой целостности хранилища (SHA-256 должен совпасть с id).
    pub fn bytes(&self, id: &str) -> Result<(Item, Vec<u8>)> {
        let item = self.get(id, None)?;
        let data = std::fs::read(self.file_path(id)?)
            .map_err(|_| Error::HostAccess("файл пакета в библиотеке недоступен".into()))?;
        if crate::fsutil::sha256_hex(&data) != item.id {
            return Err(Error::VerificationFailed);
        }
        Ok((item, data))
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let path = self.file_path(id)?;
        let n = self
            .conn()
            .execute("DELETE FROM packages WHERE id = ?1", [id])
            .map_err(db)?;
        if n > 0 {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    /// Пакеты, которые могут предоставить зависимости (для инспектора).
    pub fn provided(&self) -> Result<Vec<inspect::Provided>> {
        let c = self.conn();
        let mut stmt = c
            .prepare(
                "SELECT uid, title, version FROM packages WHERE status = 'ok' AND uid IS NOT NULL",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(db)?;
        let mut out = Vec::new();
        for r in rows {
            let (uid, title, version) = r.map_err(db)?;
            let Some(uid) = u32::from_str_radix(uid.trim_start_matches("0x"), 16).ok() else {
                continue;
            };
            let version = version.and_then(|v| {
                let p: Vec<u32> = v.split('.').filter_map(|x| x.parse().ok()).collect();
                (p.len() == 3).then(|| [p[0], p[1], p[2]])
            });
            out.push(inspect::Provided {
                uid,
                name: title.unwrap_or_default(),
                version,
            });
        }
        Ok(out)
    }

    /// Анализ сохранённых байтов (целостность хранилища проверяется) и сохранение отчёта.
    pub fn inspect(&self, id: &str, target: Target, at: i64) -> Result<inspect::Report> {
        let (item, data) = self.bytes(id)?;
        let report = inspect::inspect_package(
            &item.name,
            &data,
            &Options {
                target: target.clone(),
                at,
                provided: self.provided()?,
                jar: None,
            },
        )
        .map_err(|e| Error::InvalidArgument(e.0))?;
        let json = serde_json::to_string(&report).map_err(|e| Error::Internal(e.to_string()))?;
        self.conn()
            .execute(
                "INSERT INTO reports (package_id, model, created_ms, json) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(package_id, model) DO UPDATE SET created_ms = excluded.created_ms, json = excluded.json",
                params![id, target.model, now() as i64, json],
            )
            .map_err(db)?;
        Ok(report)
    }

    /// Последний сохранённый отчёт (JSON) для пакета и модели.
    pub fn report_json(&self, id: &str, model: &str) -> Result<Option<String>> {
        self.file_path(id)?;
        self.conn()
            .query_row(
                "SELECT json FROM reports WHERE package_id = ?1 AND model = ?2",
                params![id, model],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)
    }

    // ---- наборы приложений ----

    pub fn create_set(&self, name: &str) -> Result<PackageSet> {
        let name = clip(name).trim().to_string();
        if name.is_empty() {
            return Err(Error::InvalidArgument(
                "название набора не может быть пустым".into(),
            ));
        }
        let c = self.conn();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM sets", [], |r| r.get(0))
            .map_err(db)?;
        if n >= MAX_SETS {
            return Err(Error::InvalidArgument("слишком много наборов".into()));
        }
        c.execute("INSERT INTO sets (name) VALUES (?1)", [&name])
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    Error::AlreadyExists
                }
                e => db(e),
            })?;
        Ok(PackageSet {
            id: c.last_insert_rowid(),
            name,
            items: vec![],
        })
    }

    pub fn delete_set(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM sets WHERE id = ?1", [id])
            .map_err(db)?;
        Ok(())
    }

    pub fn set_add(&self, set: i64, package: &str) -> Result<()> {
        self.file_path(package)?;
        let c = self.conn();
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM set_items WHERE set_id = ?1",
                [set],
                |r| r.get(0),
            )
            .map_err(db)?;
        if n >= MAX_SET_ITEMS {
            return Err(Error::InvalidArgument("набор переполнен".into()));
        }
        c.execute(
            "INSERT OR IGNORE INTO set_items (set_id, package_id, position) VALUES (?1, ?2, ?3)",
            params![set, package, n],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Error::InvalidArgument("набора или пакета не существует".into())
            }
            e => db(e),
        })?;
        Ok(())
    }

    pub fn set_remove(&self, set: i64, package: &str) -> Result<()> {
        self.conn()
            .execute(
                "DELETE FROM set_items WHERE set_id = ?1 AND package_id = ?2",
                params![set, package],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn sets(&self) -> Result<Vec<PackageSet>> {
        let c = self.conn();
        let mut stmt = c
            .prepare("SELECT id, name FROM sets ORDER BY name")
            .map_err(db)?;
        let sets: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db)?;
        let mut out = Vec::new();
        for (id, name) in sets {
            let mut s = c
                .prepare("SELECT package_id FROM set_items WHERE set_id = ?1 ORDER BY position")
                .map_err(db)?;
            let items = s
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(db)?
                .collect::<std::result::Result<_, _>>()
                .map_err(db)?;
            out.push(PackageSet { id, name, items });
        }
        Ok(out)
    }

    /// Копия файла из библиотеки во временную папку под исходным именем — для передачи на телефон.
    /// Возвращает путь и SHA-256, которые затем сверяет Engine. Хранилище не изменяется.
    pub fn stage_for_transfer(&self, id: &str, staging_dir: &Path) -> Result<(PathBuf, Item)> {
        let (item, data) = self.bytes(id)?;
        if item.status != "ok" {
            return Err(Error::InvalidArgument(
                "передавать можно только пакеты, прошедшие проверку целостности".into(),
            ));
        }
        crate::fsutil::validate_file_name(&item.name)?;
        std::fs::create_dir_all(staging_dir)?;
        let path = staging_dir.join(&item.name);
        std::fs::write(&path, &data)?;
        Ok((path, item))
    }
}

fn format_str(f: Format) -> &'static str {
    match f {
        Format::Sis => "sis",
        Format::Sisx => "sisx",
        Format::Jar => "jar",
        Format::Jad => "jad",
    }
}

struct Fields {
    status: String,
    error: Option<String>,
    uid: Option<String>,
    title: Option<String>,
    version: Option<String>,
    vendor: Option<String>,
    signed: Option<bool>,
    platform_uids: Vec<String>,
}

fn analyse(name: &str, data: &[u8]) -> Fields {
    let mut f = Fields {
        status: "ok".into(),
        error: None,
        uid: None,
        title: None,
        version: None,
        vendor: None,
        signed: None,
        platform_uids: vec![],
    };
    let opts = Options {
        target: Target::default(),
        at: (crate::now_ms() / 1000) as i64,
        provided: vec![],
        jar: None,
    };
    match inspect::inspect_package(name, data, &opts) {
        Err(e) => {
            f.status = "unreadable".into();
            f.error = Some(clip(&e.0));
        }
        Ok(r) => {
            if r.issues.iter().any(|i| i == "corrupt-package") {
                f.status = "corrupt".into();
                f.error = Some("контрольные суммы или хеши файлов не сходятся".into());
            }
            if let Some(s) = &r.sis {
                f.uid = Some(format!("{:#010x}", s.package.uid));
                f.title = s.package.names.first().map(|n| clip(n));
                f.vendor = Some(clip(&s.package.vendor_unique_name)).filter(|v| !v.is_empty());
                f.version = s
                    .package
                    .version
                    .map(|v| format!("{}.{}.{}", v[0], v[1], v[2]));
                f.signed = Some(s.signed);
            }
            if let Some(d) = &r.dependencies {
                f.platform_uids = d.platforms.iter().map(|p| p.uid.clone()).collect();
            }
            if let Some(m) = &r.midlet {
                f.title = m.manifest.get("MIDlet-Name").map(|v| clip(v));
                f.vendor = m.manifest.get("MIDlet-Vendor").map(|v| clip(v));
                f.version = m.manifest.get("MIDlet-Version").map(|v| clip(v));
            }
            if let Some(d) = &r.descriptor {
                f.title = d.descriptor.get("MIDlet-Name").map(|v| clip(v));
                f.vendor = d.descriptor.get("MIDlet-Vendor").map(|v| clip(v));
                f.version = d.descriptor.get("MIDlet-Version").map(|v| clip(v));
            }
        }
    }
    f
}

fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<Item> {
    let platform: String = r.get(12)?;
    let prov: String = r.get(13)?;
    Ok(Item {
        id: r.get(0)?,
        name: r.get(1)?,
        format: r.get(2)?,
        size: r.get::<_, i64>(3)? as u64,
        added_ms: r.get::<_, i64>(4)? as u64,
        status: r.get(5)?,
        error: r.get(6)?,
        uid: r.get(7)?,
        title: r.get(8)?,
        version: r.get(9)?,
        vendor: r.get(10)?,
        signed: r.get::<_, Option<i64>>(11)?.map(|v| v != 0),
        platform_uids: serde_json::from_str(&platform).unwrap_or_default(),
        provenance: serde_json::from_str(&prov).unwrap_or_default(),
        fit: None,
    })
}

fn with_fit(mut it: Item, model: Option<&str>) -> Item {
    let Some(m) = model.and_then(profile::model) else {
        return it;
    };
    let uids: Vec<u32> = it
        .platform_uids
        .iter()
        .filter_map(|u| u32::from_str_radix(u.trim_start_matches("0x"), 16).ok())
        .collect();
    it.fit = Some(
        if m.family == "s60v2" || uids.is_empty() {
            "unknown"
        } else if uids.iter().any(|u| m.platform_uids.contains(u)) {
            "declared"
        } else {
            "not-declared"
        }
        .into(),
    );
    it
}
