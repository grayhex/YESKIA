//! Mass Storage: обычная безопасная файловая система тома, **ограниченная выбранным томом**.
//! Путь устройства (`E:/Images/a.jpg` или `/Images/a.jpg`) разрешается строго внутри корня тома;
//! `..`, обратные слеши, симлинки наружу и зарезервированные имена Windows отвергаются.

use crate::device::DeviceId;
use crate::error::{Error, Result};
use crate::fsutil::{place_file, temp_path_for, validate_file_name, CHUNK};
use crate::operations::{FileEntry, OperationContext, OverwritePolicy};
use crate::paths::validate_device_path;
use crate::transport::FileProvider;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct VolumeFileProvider {
    root: PathBuf,
}

impl VolumeFileProvider {
    /// `root` — корень тома (`E:\`); канонизируется один раз.
    pub fn new(root: &Path) -> Result<Self> {
        let root = std::fs::canonicalize(root)
            .map_err(|_| Error::DeviceNotFound("том недоступен".into()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Путь устройства → путь внутри тома. Ничего за пределами корня.
    pub fn resolve(&self, device_path: &str) -> Result<PathBuf> {
        let norm = validate_device_path(device_path)?;
        let rest = match norm.as_bytes() {
            [_, b':', b'/', ..] => &norm[3..],
            [b'/', ..] => &norm[1..],
            _ => return Err(Error::UnsafePath("неверный путь".into())),
        };
        let mut target = self.root.clone();
        for seg in rest.split('/').filter(|s| !s.is_empty()) {
            validate_file_name(seg)?; // зарезервированные имена и т. п.
            target.push(seg);
        }
        self.jail(&target)?;
        Ok(target)
    }

    /// Самый глубокий существующий предок цели обязан лежать внутри тома (после раскрытия симлинков).
    fn jail(&self, target: &Path) -> Result<()> {
        let mut cur = target.to_path_buf();
        loop {
            match std::fs::canonicalize(&cur) {
                Ok(real) => {
                    return if real.starts_with(&self.root) {
                        Ok(())
                    } else {
                        Err(Error::UnsafePath("путь выходит за пределы тома".into()))
                    };
                }
                Err(_) => {
                    if !cur.pop() || cur.as_os_str().is_empty() {
                        return Err(Error::UnsafePath("путь вне тома".into()));
                    }
                }
            }
        }
    }
}

fn entry_of(path: &Path, name: String) -> Result<FileEntry> {
    let meta = std::fs::metadata(path)?;
    Ok(FileEntry {
        name,
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
    })
}

impl FileProvider for VolumeFileProvider {
    fn list(&self, _id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<FileEntry>> {
        ctx.checkpoint()?;
        let dir = self.resolve(path)?;
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if let Ok(entry) = entry_of(&e.path(), name) {
                out.push(entry);
            }
        }
        out.sort_by(|a, b| {
            (!a.is_dir, a.name.to_lowercase()).cmp(&(!b.is_dir, b.name.to_lowercase()))
        });
        Ok(out)
    }

    fn stat(
        &self,
        _id: &DeviceId,
        path: &str,
        ctx: &OperationContext,
    ) -> Result<Option<FileEntry>> {
        ctx.checkpoint()?;
        let p = self.resolve(path)?;
        if !p.exists() {
            return Ok(None);
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        entry_of(&p, name).map(Some)
    }

    fn create_dir(&self, _id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<()> {
        ctx.checkpoint()?;
        let dir = self.resolve(path)?;
        match std::fs::create_dir(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn read(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        self.read_to(id, path, &mut v, ctx)?;
        Ok(v)
    }

    fn write(
        &self,
        id: &DeviceId,
        path: &str,
        data: &[u8],
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        let mut r = std::io::Cursor::new(data);
        self.write_from(id, path, &mut r, data.len() as u64, overwrite, ctx)
    }

    fn read_to(
        &self,
        _id: &DeviceId,
        path: &str,
        sink: &mut dyn Write,
        ctx: &OperationContext,
    ) -> Result<u64> {
        ctx.checkpoint()?;
        let p = self.resolve(path)?;
        let mut f = std::fs::File::open(&p)?;
        let total = f.metadata()?.len();
        let mut buf = vec![0u8; CHUNK];
        let mut done = 0u64;
        loop {
            ctx.checkpoint()?;
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            sink.write_all(&buf[..n])?;
            done += n as u64;
            ctx.report_progress(done, total);
        }
        Ok(done)
    }

    fn write_from(
        &self,
        _id: &DeviceId,
        path: &str,
        source: &mut dyn Read,
        len: u64,
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        ctx.checkpoint()?;
        let dest = self.resolve(path)?;
        if overwrite == OverwritePolicy::Never && dest.exists() {
            return Err(Error::AlreadyExists);
        }
        let parent = dest
            .parent()
            .ok_or_else(|| Error::UnsafePath("нет родительской папки".into()))?;
        if !parent.is_dir() {
            return Err(Error::OperationFailed(
                "папка назначения не существует".into(),
            ));
        }
        // Пишем во временный файл рядом и размещаем атомарно: на устройстве не остаётся
        // «наполовину записанного» файла под настоящим именем.
        let tmp = temp_path_for(&dest);
        let result = (|| -> Result<()> {
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            let mut buf = vec![0u8; CHUNK];
            let mut done = 0u64;
            loop {
                ctx.checkpoint()?;
                let n = source.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                done += n as u64;
                ctx.report_progress(done, len.max(done));
            }
            out.flush()?;
            out.sync_all()?;
            ctx.checkpoint()?;
            place_file(&tmp, &dest, overwrite)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}
