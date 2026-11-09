//! Локальная файловая система Windows-хоста (левая панель файлового менеджера).
//! Только чтение и создание папок; удаления/переименования нет.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<u64>,
    pub hidden: bool,
    /// Имя не является корректным Unicode: показывается, но недоступно для операций.
    pub lossy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    Drive,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRoot {
    pub label: String,
    pub path: String,
    pub kind: RootKind,
}

/// Диски и «быстрые» папки пользователя.
pub fn roots() -> Vec<HostRoot> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for letter in 'A'..='Z' {
            let p = format!("{letter}:\\");
            if Path::new(&p).exists() {
                out.push(HostRoot {
                    label: format!("{letter}:"),
                    path: p,
                    kind: RootKind::Drive,
                });
            }
        }
        if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            for (label, sub) in [
                ("Загрузки", "Downloads"),
                ("Документы", "Documents"),
                ("Рабочий стол", "Desktop"),
            ] {
                let p = home.join(sub);
                if p.is_dir() {
                    out.push(HostRoot {
                        label: label.into(),
                        path: p.display().to_string(),
                        kind: RootKind::Folder,
                    });
                }
            }
        }
    } else {
        out.push(HostRoot {
            label: "/".into(),
            path: "/".into(),
            kind: RootKind::Drive,
        });
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            out.push(HostRoot {
                label: "~".into(),
                path: home.display().to_string(),
                kind: RootKind::Folder,
            });
        }
    }
    out
}

fn require_absolute(p: &Path) -> Result<()> {
    if !p.is_absolute() {
        return Err(Error::UnsafePath(
            "путь на компьютере должен быть абсолютным".into(),
        ));
    }
    Ok(())
}

fn is_hidden(name: &str, meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        return meta.file_attributes() & HIDDEN != 0 || name.starts_with('.');
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        name.starts_with('.')
    }
}

/// Содержимое папки. Ошибки доступа (permissions) возвращаются структурированно.
pub fn list_dir(path: &Path) -> Result<Vec<HostEntry>> {
    require_absolute(path)?;
    let rd = std::fs::read_dir(path).map_err(map_io)?;
    let mut out = Vec::new();
    for entry in rd {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let raw = entry.file_name();
        let (name, lossy) = match raw.to_str() {
            Some(s) => (s.to_string(), false),
            None => (raw.to_string_lossy().into_owned(), true),
        };
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        out.push(HostEntry {
            hidden: is_hidden(&name, &meta),
            name,
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified_ms,
            lossy,
        });
    }
    Ok(out)
}

/// Создаёт папку назначения (родитель должен существовать). Существующая папка — не ошибка
/// (слияние), существующий **файл** с таким именем — `AlreadyExists`.
pub fn create_dir(path: &Path) -> Result<()> {
    require_absolute(path)?;
    crate::fsutil::validate_host_path(path)?;
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if path.is_dir() {
                Ok(())
            } else {
                Err(Error::AlreadyExists)
            }
        }
        Err(e) => Err(map_io(e)),
    }
}

fn map_io(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            Error::HostAccess("нет доступа к папке (проверьте права)".into())
        }
        std::io::ErrorKind::NotFound => Error::HostAccess("папка не найдена".into()),
        _ => Error::Storage(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_unicode_names_and_sizes() {
        let dir = std::env::temp_dir().join(format!("yeskia-host-{}", crate::now_ms()));
        std::fs::create_dir_all(dir.join("Папка")).unwrap();
        std::fs::write(dir.join("фото.jpg"), [1, 2, 3]).unwrap();
        let mut e = list_dir(&dir).unwrap();
        e.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(e.len(), 2);
        let file = e.iter().find(|x| x.name == "фото.jpg").unwrap();
        assert_eq!((file.is_dir, file.size, file.lossy), (false, 3, false));
        assert!(e.iter().find(|x| x.name == "Папка").unwrap().is_dir);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn relative_and_missing_paths_are_structured_errors() {
        assert!(matches!(
            list_dir(Path::new("rel")),
            Err(Error::UnsafePath(_))
        ));
        let missing = std::env::temp_dir().join("yeskia-definitely-missing-dir");
        assert!(matches!(list_dir(&missing), Err(Error::HostAccess(_))));
    }

    #[test]
    fn create_dir_merges_existing_directory_but_not_a_file() {
        let dir = std::env::temp_dir().join(format!("yeskia-mk-{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        create_dir(&dir.join("Новая")).unwrap();
        create_dir(&dir.join("Новая")).unwrap(); // слияние
        std::fs::write(dir.join("file"), b"x").unwrap();
        assert!(matches!(
            create_dir(&dir.join("file")),
            Err(Error::AlreadyExists)
        ));
        assert!(create_dir(&dir.join("nul")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn roots_are_not_empty() {
        assert!(!roots().is_empty());
    }
}
