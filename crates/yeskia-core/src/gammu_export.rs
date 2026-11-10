//! Read-only Gammu export. The user selects both the executable and its connection profile.
//! Output and error streams are discarded because they may contain private phone data.
use crate::{Error, Result};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_EXPORT: u64 = 8 * 1024 * 1024;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Read one store to a new file. No phone write commands are issued.
pub fn export(
    executable: &Path,
    config: &Path,
    destination: &Path,
    kind: &str,
    timeout: Duration,
) -> Result<()> {
    let (ext, marker): (&str, &[u8]) = match kind {
        "contacts" => ("vcf", b"BEGIN:VCARD"),
        "calendar" => ("ics", b"BEGIN:VCALENDAR"),
        _ => return Err(Error::InvalidArgument("неизвестный тип экспорта".into())),
    };
    if !executable.is_absolute()
        || !executable.is_file()
        || !config.is_absolute()
        || !config.is_file()
        || !destination.is_absolute()
        || destination
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
            != Some(ext)
    {
        return Err(Error::InvalidArgument(
            "Проверьте программу Gammu, её настройки и имя файла".into(),
        ));
    }
    if destination.exists() {
        return Err(Error::AlreadyExists);
    }
    let parent = destination
        .parent()
        .ok_or_else(|| Error::HostAccess("папка назначения недоступна".into()))?;
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce)
        .map_err(|_| Error::Internal("не удалось создать временную папку".into()))?;
    let suffix: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
    let scratch = Scratch(parent.join(format!(".yeskia-export-{suffix}")));
    fs::create_dir(&scratch.0)?;
    let output = scratch.0.join(format!("phone.{ext}"));
    let mut command = Command::new(executable);
    command
        .arg("--config")
        .arg(config)
        .arg("backup")
        .arg(&output)
        .arg("-yes")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| Error::OperationFailed("Не удалось запустить Gammu".into()))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => {
                return Err(Error::OperationFailed(
                    "Gammu не смог прочитать данные телефона. Проверьте соединение и настройки."
                        .into(),
                ))
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Timeout);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::OperationFailed("Ошибка ожидания Gammu".into()));
            }
        }
    }
    let file = fs::File::open(&output)
        .map_err(|_| Error::OperationFailed("Gammu не создал файл".into()))?;
    let mut data = Vec::new();
    file.take(MAX_EXPORT + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_EXPORT
        || !data
            .windows(marker.len())
            .any(|w| w.eq_ignore_ascii_case(marker))
    {
        return Err(Error::OperationFailed(
            "Пустой или неподдерживаемый результат: невозможно подтвердить чтение".into(),
        ));
    }
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                Error::AlreadyExists
            } else {
                e.into()
            }
        })?;
    if let Err(e) = out.write_all(&data).and_then(|_| out.sync_all()) {
        drop(out);
        let _ = fs::remove_file(destination);
        return Err(e.into());
    }
    Ok(())
}
