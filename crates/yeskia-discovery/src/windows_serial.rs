//! COM-порт Windows для AT-канала (`yeskia_core::at`). Порт открывается на время одной операции и
//! закрывается в `Drop`; параметры линии (скорость, чётность) не меняются — для USB CDC они не
//! используются, а чужие настройки порта не нужно трогать. Записывается только то, что передал
//! `at::exec` (разрешённые запросы только на чтение).
//!
//! Статус: `implemented` — компиляция и запуск на Windows-раннере CI без телефона; на реальном
//! телефоне `not-tested`.

use std::io;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Devices::Communication::*;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::*;
use yeskia_core::at::{SerialOpener, SerialPort};
use yeskia_core::error::{Error, Result};

pub struct WindowsSerialOpener;

struct Com {
    h: HANDLE,
}

// HANDLE файла порта используется только из потока операции.
unsafe impl Send for Com {}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `COM7` и только он: произвольные пути устройств (`\\.\PhysicalDrive0`, `NUL`) отвергаются.
fn valid_port(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    up.strip_prefix("COM").is_some_and(|n| {
        !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()) && n != "0"
    })
}

impl SerialOpener for WindowsSerialOpener {
    fn open(&self, port_name: &str) -> Result<Box<dyn SerialPort>> {
        if !valid_port(port_name) {
            return Err(Error::UnsafePath("недопустимое имя COM-порта".into()));
        }
        let path = wide(&format!("\\\\.\\{port_name}"));
        let h = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(|e| {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                Error::HostAccess(
                    "COM-порт занят другой программой (например, Nokia PC Suite): закройте её и повторите"
                        .into(),
                )
            } else if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() {
                Error::DeviceDisconnected
            } else {
                Error::Transport(format!("не удалось открыть COM-порт: код {:#x}", e.code().0 as u32))
            }
        })?;
        let mut com = Com { h };
        unsafe {
            // Состояние линии: терминал готов (DTR) — некоторые модемы молчат без него.
            let _ = EscapeCommFunction(com.h, SETDTR);
            let _ = PurgeComm(com.h, PURGE_RXCLEAR | PURGE_TXCLEAR);
        }
        com.set_timeout(Duration::from_millis(100))
            .map_err(|e| Error::Transport(format!("настройка таймаутов COM: {e}")))?;
        Ok(Box::new(com))
    }
}

impl Com {
    fn set_timeout(&mut self, t: Duration) -> io::Result<()> {
        // ReadIntervalTimeout = MAXDWORD и множитель MAXDWORD: вернуть сразу, если байты есть,
        // иначе ждать первый байт не дольше константы.
        let ct = COMMTIMEOUTS {
            ReadIntervalTimeout: u32::MAX,
            ReadTotalTimeoutMultiplier: u32::MAX,
            ReadTotalTimeoutConstant: t.as_millis().clamp(1, 60_000) as u32,
            WriteTotalTimeoutMultiplier: 0,
            WriteTotalTimeoutConstant: 2000,
        };
        unsafe { SetCommTimeouts(self.h, &ct) }.map_err(|e| io::Error::other(e.to_string()))
    }
}

impl SerialPort for Com {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        let mut done = 0usize;
        while done < data.len() {
            let mut n = 0u32;
            unsafe { WriteFile(self.h, Some(&data[done..]), Some(&mut n), None) }
                .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "запись не принята"));
            }
            done += n as usize;
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        self.set_timeout(timeout)?;
        let mut n = 0u32;
        unsafe { ReadFile(self.h, Some(buf), Some(&mut n), None) }
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?;
        Ok(n as usize)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_port;

    #[test]
    fn only_com_ports_are_accepted() {
        assert!(valid_port("COM7") && valid_port("com12") && valid_port("COM255"));
        for bad in [
            "",
            "COM",
            "COM0",
            "COM1234",
            "NUL",
            "C:\\x",
            "\\\\.\\PhysicalDrive0",
            "COM1 ",
            "COM-1",
            "LPT1",
        ] {
            assert!(!valid_port(bad), "{bad:?}");
        }
    }
}
