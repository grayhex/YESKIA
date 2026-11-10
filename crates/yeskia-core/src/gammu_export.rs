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
    let ext = match kind {
        "contacts" => "vcf",
        "calendar" => "ics",
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
    validate_export(&data, kind)?;
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

/// Validate component boundaries before publishing a Gammu export. Field values remain opaque:
/// vCard 2.1 may use legacy charsets and quoted-printable values.
fn validate_export(data: &[u8], kind: &str) -> Result<()> {
    let invalid =
        || Error::OperationFailed("Неполный, пустой или неподдерживаемый экспорт телефона".into());
    if data.is_empty() || data.len() as u64 > MAX_EXPORT || data.contains(&0) {
        return Err(invalid());
    }
    let root = match kind {
        "contacts" => "VCARD",
        "calendar" => "VCALENDAR",
        _ => return Err(invalid()),
    };
    let data = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(data);
    let mut stack: Vec<String> = Vec::new();
    let mut records = 0usize;
    let mut roots = 0usize;
    let mut property = false;
    let mut quoted_printable = false;
    let mut soft_break = false;
    let mut has_fields: Vec<bool> = Vec::new();
    for raw in data.split(|b| *b == b'\n') {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        if line.is_empty() {
            continue;
        }
        if line.len() > 64 * 1024 {
            return Err(invalid());
        }
        if quoted_printable && soft_break {
            soft_break = line.ends_with(b"=");
            continue;
        }
        if matches!(line.first(), Some(b' ' | b'\t')) {
            if stack.is_empty() || !property {
                return Err(invalid());
            }
            soft_break = line.ends_with(b"=");
            continue;
        }
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            return Err(invalid());
        };
        if colon == 0 {
            return Err(invalid());
        }
        let key = &line[..colon];
        if key.eq_ignore_ascii_case(b"BEGIN") {
            let name = std::str::from_utf8(&line[colon + 1..])
                .map_err(|_| invalid())?
                .to_ascii_uppercase();
            let allowed = match stack.last().map(String::as_str) {
                None => name == root && (kind == "contacts" || roots == 0),
                Some("VCALENDAR") => matches!(name.as_str(), "VEVENT" | "VTODO" | "VTIMEZONE"),
                Some("VEVENT" | "VTODO") => name == "VALARM",
                Some("VTIMEZONE") => matches!(name.as_str(), "STANDARD" | "DAYLIGHT"),
                _ => false,
            };
            if !allowed {
                return Err(invalid());
            }
            if stack.is_empty() {
                roots += 1;
            }
            stack.push(name);
            has_fields.push(false);
            property = false;
            quoted_printable = false;
        } else if key.eq_ignore_ascii_case(b"END") {
            let name = std::str::from_utf8(&line[colon + 1..])
                .map_err(|_| invalid())?
                .to_ascii_uppercase();
            if stack.pop().as_deref() != Some(name.as_str()) {
                return Err(invalid());
            }
            let populated = has_fields.pop().unwrap_or(false);
            if matches!(name.as_str(), "VCARD" | "VEVENT" | "VTODO") {
                if !populated {
                    return Err(invalid());
                }
                records += 1;
            }
            if records > 100_000 {
                return Err(invalid());
            }
            property = false;
            quoted_printable = false;
        } else {
            if stack.is_empty() {
                return Err(invalid());
            }
            if !key.eq_ignore_ascii_case(b"VERSION") && line.len() > colon + 1 {
                if let Some(populated) = has_fields.last_mut() {
                    *populated = true;
                }
            }
            quoted_printable = key
                .split(|b| *b == b';')
                .any(|p| p.eq_ignore_ascii_case(b"ENCODING=QUOTED-PRINTABLE"));
            soft_break = line.ends_with(b"=");
            property = true;
        }
    }
    if !stack.is_empty() || records == 0 {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod export_validation_tests {
    use super::validate_export;

    #[test]
    fn accepts_cards_with_folding_and_legacy_text() {
        let bytes = b"\xef\xbb\xbfBEGIN:VCARD\r\nVERSION:2.1\r\nFN;CHARSET=WINDOWS-1251:\xc0\r\nNOTE:long\r\n continuation\r\nEND:VCARD\r\nBEGIN:VCARD\nFN:Second\nEND:VCARD\n";
        assert!(validate_export(bytes, "contacts").is_ok());
    }
    #[test]
    fn handles_quoted_printable_soft_line_breaks() {
        assert!(validate_export(
            b"BEGIN:VCARD\nVERSION:2.1\nFN;ENCODING=QUOTED-PRINTABLE:=D0=90=\n=D0=91\nEND:VCARD",
            "contacts"
        )
        .is_ok());
        assert!(validate_export(b"BEGIN:VCARD\nEND:VCARD", "contacts").is_err());
    }

    #[test]
    fn accepts_calendar_events_tasks_and_nested_alarms() {
        let bytes = b"BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nSUMMARY:Test\nBEGIN:VALARM\nACTION:DISPLAY\nEND:VALARM\nEND:VEVENT\nBEGIN:VTODO\nSUMMARY:Task\nEND:VTODO\nEND:VCALENDAR\n";
        assert!(validate_export(bytes, "calendar").is_ok());
    }
    #[test]
    fn rejects_truncation_wrong_type_and_ambiguous_empty_calendar() {
        for bytes in [
            &b"BEGIN:VCALENDAR\nEND:VCALENDAR"[..],
            &b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Test\nEND:VEVENT"[..],
            &b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nEND:VTODO\nEND:VCALENDAR"[..],
            &b"log:BEGIN:VCALENDAR"[..],
            &b"BEGIN:VCARD\nFN:Test\nEND:VCARD"[..],
        ] {
            assert!(validate_export(bytes, "calendar").is_err());
        }
    }
    #[test]
    fn rejects_partial_card_after_complete_card_and_trailing_output() {
        assert!(validate_export(
            b"BEGIN:VCARD\nFN:A\nEND:VCARD\nBEGIN:VCARD\nFN:B",
            "contacts"
        )
        .is_err());
        assert!(validate_export(
            b"BEGIN:VCARD\nFN:A\nEND:VCARD\nERROR:phone disconnected",
            "contacts"
        )
        .is_err());
    }
}
