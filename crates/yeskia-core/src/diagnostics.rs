//! Диагностика без персональных данных: редактор строк и кольцевой журнал.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Маска идентификатора: остаются последние 2 символа.
pub fn mask_identifier(v: &str) -> String {
    let chars: Vec<char> = v.chars().collect();
    if chars.len() <= 2 {
        return "***".to_string();
    }
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("***{tail}")
}

const SENSITIVE_KEYS: &[&str] = &["serial", "sn", "imei", "imsi", "meid", "esn"];

/// Скрывает IMEI/IMSI-подобные числа (14–16 цифр подряд) и значения
/// `serial=…`, `imei: …` и т. п. Применяется ко всем сообщениям журнала и ошибок.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_sensitive_key = false;
    let mut token = String::new();

    fn flush(token: &mut String, out: &mut String, pending: &mut bool) {
        if token.is_empty() {
            return;
        }
        let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
        let all_digits = digits == token.chars().count();
        if *pending && token.chars().any(|c| c.is_ascii_alphanumeric()) {
            out.push_str(&mask_identifier(token));
            *pending = false;
        } else if all_digits && (14..=16).contains(&digits) {
            out.push_str(&mask_identifier(token));
        } else {
            let lower = token.to_ascii_lowercase();
            *pending = SENSITIVE_KEYS.contains(&lower.as_str());
            out.push_str(token);
        }
        token.clear();
    }

    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            token.push(ch);
        } else {
            flush(&mut token, &mut out, &mut pending_sensitive_key);
            // разделители `=`, `:` и пробел не сбрасывают ожидание значения
            if !(ch == '=' || ch == ':' || ch == ' ') {
                pending_sensitive_key = false;
            }
            out.push(ch);
        }
    }
    flush(&mut token, &mut out, &mut pending_sensitive_key);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp_ms: u64,
    pub level: LogLevel,
    pub message: String,
}

/// Кольцевой журнал в памяти + необязательная запись в файл. Всё редактируется.
pub struct Diagnostics {
    entries: Mutex<VecDeque<LogEntry>>,
    capacity: usize,
    file: Option<PathBuf>,
}

impl Diagnostics {
    pub fn new(capacity: usize, file: Option<PathBuf>) -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            capacity: capacity.max(1),
            file,
        }
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        let entry = LogEntry {
            timestamp_ms: crate::now_ms(),
            level,
            message: redact(message),
        };
        if let Some(path) = &self.file {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(
                    f,
                    "{} {:?} {}",
                    entry.timestamp_ms, entry.level, entry.message
                );
            }
        }
        let mut q = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() == self.capacity {
            q.pop_front();
        }
        q.push_back(entry);
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }
    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }
    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_imei_like_numbers() {
        let s = redact("found phone imei 356938035643809 on usb");
        assert!(!s.contains("356938035643809"));
        assert!(s.contains("***09"));
    }

    #[test]
    fn masks_key_value_serials() {
        assert!(!redact("serial=ABC123456789").contains("ABC123456789"));
        assert!(!redact("SN: 0123abcd").contains("0123abcd"));
    }

    #[test]
    fn keeps_ordinary_text_and_short_numbers() {
        assert_eq!(
            redact("copied 12345 bytes to E:/Images"),
            "copied 12345 bytes to E:/Images"
        );
    }

    #[test]
    fn log_is_redacted_and_bounded() {
        let d = Diagnostics::new(2, None);
        d.info("imei 356938035643809");
        d.info("b");
        d.info("c");
        let snap = d.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].message, "b");
        let d2 = Diagnostics::new(5, None);
        d2.info("imei 356938035643809");
        assert!(!d2.snapshot()[0].message.contains("356938035643809"));
    }
}
