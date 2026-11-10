//! Read-only python-gammu adapter ported from s60modern. No personal data is logged.
use crate::{Error, Result};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
static ACTIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
static CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn cancel() {
    CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}
const LIMIT: u64 = 8 * 1024 * 1024;

pub fn read(
    python: &Path,
    config: &Path,
    domain: &str,
    timeout: Duration,
) -> Result<serde_json::Value> {
    if !python.is_absolute()
        || !python.is_file()
        || !config.is_absolute()
        || !config.is_file()
        || !matches!(domain, "contacts" | "sms" | "calendar" | "tasks")
    {
        return Err(Error::InvalidArgument(
            "Выберите Python с python-gammu и профиль подключения".into(),
        ));
    }
    let _active = ACTIVE
        .try_lock()
        .map_err(|_| Error::OperationFailed("Дождитесь завершения чтения телефона".into()))?;
    CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let mut command = Command::new(python);
    command
        .args(["-I", "-c", include_str!("personal/reader.py")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| Error::OperationFailed("Не удалось запустить Python".into()))?;
    let input = serde_json::to_vec(&serde_json::json!({"config": config, "domain": domain}))
        .map_err(|_| Error::InvalidArgument("Профиль подключения недоступен".into()))?;
    let write_result = child.stdin.take().unwrap().write_all(&input);
    if write_result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::OperationFailed(
            "Не удалось передать профиль подключения".into(),
        ));
    }
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        stdout.take(LIMIT + 1).read_to_end(&mut data).map(|_| data)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(Error::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Error::Timeout);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Error::OperationFailed("Ошибка чтения телефона".into()));
            }
        }
    };
    let data = reader
        .join()
        .map_err(|_| Error::Internal("Ошибка чтения".into()))??;
    if !status?.success() || data.len() as u64 > LIMIT {
        return Err(Error::OperationFailed(
            "Не удалось прочитать данные. Проверьте python-gammu, профиль и соединение телефона."
                .into(),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&data)
        .map_err(|_| Error::OperationFailed("Некорректный ответ адаптера".into()))?;
    if !value
        .as_array()
        .is_some_and(|rows| rows.len() <= 5000 && rows.iter().all(|r| r.is_object()))
    {
        return Err(Error::OperationFailed("Некорректный список записей".into()));
    }
    Ok(value)
}
