//! Безопасные файловые примитивы: имена и пути хоста, временные файлы, размещение без
//! перезаписи, потоковое хеширование SHA-256.

use crate::error::{Error, Result};
use crate::operations::{OperationContext, OverwritePolicy};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// Размер блока потокового копирования; между блоками — отмена/таймаут/прогресс.
pub const CHUNK: usize = 256 * 1024;

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Допустимо ли имя одного файла/папки на Windows-хосте (и переносимо на другие ФС).
/// Кириллица и прочий Unicode разрешены.
pub fn validate_file_name(name: &str) -> Result<()> {
    let bad = |why: &str| Err(Error::UnsafePath(why.to_string()));
    if name.is_empty() || name == "." || name == ".." {
        return bad("недопустимое имя");
    }
    if name.chars().count() > 255 {
        return bad("слишком длинное имя");
    }
    if name
        .chars()
        .any(|c| c.is_control() || FORBIDDEN.contains(&c))
    {
        return bad("недопустимый символ в имени");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return bad("имя не может заканчиваться точкой или пробелом");
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return bad("зарезервированное имя Windows");
    }
    Ok(())
}

/// Путь на хосте для записи/чтения: абсолютный, без `..`, имя файла допустимо.
pub fn validate_host_path(p: &Path) -> Result<()> {
    if !p.is_absolute() {
        return Err(Error::UnsafePath(
            "путь на компьютере должен быть абсолютным".into(),
        ));
    }
    if p.components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(Error::UnsafePath("сегменты . и .. запрещены".into()));
    }
    if let Some(name) = p.file_name() {
        let name = name
            .to_str()
            .ok_or_else(|| Error::UnsafePath("имя файла не в Unicode".into()))?;
        validate_file_name(name)?;
    }
    Ok(())
}

/// Временный файл рядом с целью: `.<имя>.yeskia-part-<pid>-<nanos>`.
pub fn temp_path_for(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dest.with_file_name(format!(
        ".{name}.yeskia-part-{}-{nanos}",
        std::process::id()
    ))
}

/// Переносит готовый временный файл на место. `Never` — атомарный отказ, если цель есть
/// (жёсткая ссылка), `Replace` — явная замена. Временный файл удаляется в обоих случаях.
pub fn place_file(tmp: &Path, dest: &Path, overwrite: OverwritePolicy) -> Result<()> {
    match overwrite {
        OverwritePolicy::Replace => {
            std::fs::rename(tmp, dest)?;
        }
        OverwritePolicy::Never => match std::fs::hard_link(tmp, dest) {
            Ok(()) => {
                let _ = std::fs::remove_file(tmp);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(tmp);
                return Err(Error::AlreadyExists);
            }
            Err(_) => {
                // ФС без жёстких ссылок (FAT/exFAT): проверка + переименование.
                if dest.exists() {
                    let _ = std::fs::remove_file(tmp);
                    return Err(Error::AlreadyExists);
                }
                std::fs::rename(tmp, dest)?;
            }
        },
    }
    Ok(())
}

/// Писатель, который считает SHA-256 и объём «на лету» и сообщает прогресс.
pub struct HashWriter<'a, W: Write> {
    inner: W,
    hasher: Sha256,
    bytes: u64,
    total: u64,
    ctx: Option<&'a OperationContext>,
}

impl<'a, W: Write> HashWriter<'a, W> {
    pub fn new(inner: W, total: u64, ctx: Option<&'a OperationContext>) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            bytes: 0,
            total,
            ctx,
        }
    }
    pub fn finish(mut self) -> io::Result<(u64, String, W)> {
        self.inner.flush()?;
        Ok((self.bytes, hex(&self.hasher.finalize()), self.inner))
    }
}

impl<W: Write> Write for HashWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.bytes += n as u64;
        if let Some(c) = self.ctx {
            c.report_progress(self.bytes, self.total.max(self.bytes));
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Читатель, считающий SHA-256 прочитанного.
pub struct HashReader<R: Read> {
    inner: R,
    hasher: Sha256,
    bytes: u64,
}

impl<R: Read> HashReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            bytes: 0,
        }
    }
    pub fn finish(self) -> (u64, String) {
        (self.bytes, hex(&self.hasher.finalize()))
    }
}

impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
}

/// SHA-256 файла потоково, с проверкой отмены между блоками.
pub fn hash_file(path: &Path, ctx: Option<&OperationContext>) -> Result<(u64, String)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut total = 0u64;
    loop {
        if let Some(c) = ctx {
            c.checkpoint()?;
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    Ok((total, hex(&h.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_allow_unicode_and_reject_windows_traps() {
        for ok in ["фото 2024.jpg", "Документ (1).txt", "日本語.txt", "a.b.c"] {
            assert!(validate_file_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", ".", "..", "a/b", "a\\b", "a:b", "a?", "con", "NUL.txt", "COM1", "x.", "x ", "a\0b",
        ] {
            assert!(validate_file_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_file_name(&"я".repeat(256)).is_err());
    }

    #[test]
    fn host_paths_must_be_absolute_and_clean() {
        let base = std::env::temp_dir();
        assert!(validate_host_path(&base.join("файл.bin")).is_ok());
        assert!(validate_host_path(Path::new("relative/x")).is_err());
        assert!(validate_host_path(&base.join("..").join("x")).is_err());
        assert!(validate_host_path(&base.join("nul")).is_err());
    }

    #[test]
    fn place_never_refuses_to_overwrite_and_replace_does() {
        let dir = std::env::temp_dir().join(format!("yeskia-place-{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("a.txt");
        std::fs::write(&dest, b"old").unwrap();
        let tmp = temp_path_for(&dest);
        std::fs::write(&tmp, b"new").unwrap();
        assert!(matches!(
            place_file(&tmp, &dest, OverwritePolicy::Never),
            Err(Error::AlreadyExists)
        ));
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");
        assert!(!tmp.exists(), "временный файл убран");
        let tmp = temp_path_for(&dest);
        std::fs::write(&tmp, b"new").unwrap();
        place_file(&tmp, &dest, OverwritePolicy::Replace).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        let fresh = dir.join("b.txt");
        let tmp = temp_path_for(&fresh);
        std::fs::write(&tmp, b"x").unwrap();
        place_file(&tmp, &fresh, OverwritePolicy::Never).unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"x");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hash_writer_and_hash_file_agree() {
        let dir = std::env::temp_dir().join(format!("yeskia-hash-{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("h.bin");
        let data: Vec<u8> = (0..=255u8).cycle().take(700_000).collect();
        let f = std::fs::File::create(&p).unwrap();
        let mut hw = HashWriter::new(f, data.len() as u64, None);
        hw.write_all(&data).unwrap();
        let (n, sha, _) = hw.finish().unwrap();
        assert_eq!(n, data.len() as u64);
        assert_eq!(sha, sha256_hex(&data));
        assert_eq!(hash_file(&p, None).unwrap(), (n, sha));
        let _ = std::fs::remove_dir_all(dir);
    }
}
