//! Обезличивание: поиск и замена IMEI/идентификаторов, серийников, MAC, путей USB-устройств,
//! e-mail, телефонов, длинных токенов и имён пользователей Windows. Значения не логируются:
//! находки содержат только вид и позицию.

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Imei,
    Identifier,
    Mac,
    Serial,
    UsbPath,
    Email,
    Phone,
    Token,
    UserPath,
}

impl Kind {
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Imei => "IMEI",
            Kind::Identifier => "ID",
            Kind::Mac => "MAC",
            Kind::Serial => "SERIAL",
            Kind::UsbPath => "USB_PATH",
            Kind::Email => "EMAIL",
            Kind::Phone => "PHONE",
            Kind::Token => "TOKEN",
            Kind::UserPath => "USER",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub kind: Kind,
    /// 1-based строка и 1-based столбец (в символах).
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

struct Rule {
    kind: Kind,
    re: Regex,
}

fn rules() -> &'static Vec<Rule> {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |kind, pat: &str| Rule { kind, re: Regex::new(pat).expect("regex") };
        vec![
            r(Kind::Serial, r"(?i)\b(?:serial(?:\s*number)?|s/n|sn|imei|imsi|iccid|meid|esn|bdaddr|btaddr|серийн\w*(?:\s+номер)?|имей)\b\s*[:=]\s*[A-Za-z0-9._\-]{4,}"),
            r(Kind::UsbPath, r#"(?i)\b(?:USB|USBSTOR|WPDBUSENUM|BTHENUM|SWD)\\[A-Z0-9&_.\-]+\\[^\s"',;]+"#),
            r(Kind::UsbPath, r#"(?i)\\\\\?\\(?:usb|wpd)#[^\s"',;]+"#),
            r(Kind::Mac, r"(?i)\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b|\b(?:[0-9a-f]{2}-){5}[0-9a-f]{2}\b"),
            r(Kind::Email, r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"),
            r(Kind::Phone, r"(?:\+|\b00)\d[\d\s().\-]{6,17}\d"),
            r(Kind::Identifier, r"\b\d{14,22}\b"),
            r(Kind::Token, r"\b[0-9A-Fa-f]{32,}\b"),
            r(Kind::Token, r"\b[A-Za-z0-9_\-]{40,}\b"),
            r(Kind::UserPath, r#"(?i)\b[A-Z]:\\Users\\[^\\\s"']+"#),
        ]
    })
}

fn luhn(digits: &str) -> bool {
    let mut sum = 0;
    for (i, c) in digits.chars().rev().enumerate() {
        let Some(mut d) = c.to_digit(10) else {
            return false;
        };
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

/// Находки без перекрытий, по возрастанию позиции (байтовые смещения).
fn raw_matches(text: &str) -> Vec<(usize, usize, Kind)> {
    let mut all: Vec<(usize, usize, Kind)> = Vec::new();
    for rule in rules() {
        for m in rule.re.find_iter(text) {
            let s = m.as_str();
            let kind = match rule.kind {
                Kind::Identifier if s.len() == 15 && luhn(s) => Kind::Imei,
                Kind::Token
                    if !(s.chars().any(|c| c.is_ascii_digit())
                        && s.chars().any(|c| c.is_ascii_alphabetic())) =>
                {
                    continue;
                }
                Kind::UserPath => {
                    let name = s.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
                    if matches!(
                        name.as_str(),
                        "public" | "default" | "<user>" | "user" | "all users"
                    ) {
                        continue;
                    }
                    Kind::UserPath
                }
                k => k,
            };
            all.push((m.start(), m.end(), kind));
        }
    }
    all.sort_by_key(|(s, e, _)| (*s, std::cmp::Reverse(*e)));
    let mut out: Vec<(usize, usize, Kind)> = Vec::new();
    for m in all {
        if out.last().map_or(true, |l| m.0 >= l.1) {
            out.push(m);
        }
    }
    out
}

fn position(text: &str, byte: usize) -> (usize, usize) {
    let before = &text[..byte];
    let line = before.matches('\n').count() + 1;
    let col = before
        .rsplit('\n')
        .next()
        .map(|l| l.chars().count())
        .unwrap_or(0)
        + 1;
    (line, col)
}

/// Находки (без значений).
pub fn scan_text(text: &str) -> Vec<Finding> {
    raw_matches(text)
        .into_iter()
        .map(|(s, e, kind)| {
            let (line, col) = position(text, s);
            Finding {
                kind,
                line,
                col,
                len: text[s..e].chars().count(),
            }
        })
        .collect()
}

/// Заменяет найденное на `<REDACTED:KIND>`.
pub fn redact_text(text: &str) -> (String, Vec<Finding>) {
    let findings = scan_text(text);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (s, e, kind) in raw_matches(text) {
        out.push_str(&text[last..s]);
        out.push_str(&format!("<REDACTED:{}>", kind.tag()));
        last = e;
    }
    out.push_str(&text[last..]);
    (out, findings)
}

/// Маскирует полезную нагрузку кадра, сохраняя **длину и структуру**: первые `keep` байт
/// (заголовок) остаются, печатные ASCII-серии ≥4 символов и UTF-16LE-серии ≥4 символов
/// заменяются на `0x58`/`0x00 0x58`-подобные заполнители той же длины. Остальные байты
/// сохраняются, чтобы не терять данные для анализа framing, но ТЕКСТ (имена, номера, SMS) уходит.
pub fn mask_payload(data: &[u8], keep: usize) -> Vec<u8> {
    let mut out = data.to_vec();
    let printable = |b: u8| (0x20..=0x7E).contains(&b);
    // ASCII-серии
    let mut i = keep.min(data.len());
    while i < data.len() {
        if printable(data[i]) {
            let start = i;
            while i < data.len() && printable(data[i]) {
                i += 1;
            }
            if i - start >= 4 {
                out[start..i].fill(b'X');
            }
        } else {
            i += 1;
        }
    }
    // UTF-16LE-серии: пары (печатный, 0x00)
    let mut i = keep.min(data.len());
    while i + 1 < data.len() {
        if printable(data[i]) && data[i + 1] == 0 {
            let start = i;
            while i + 1 < data.len() && printable(data[i]) && data[i + 1] == 0 {
                i += 2;
            }
            if (i - start) / 2 >= 4 {
                let mut j = start;
                while j + 1 < i {
                    out[j] = b'X';
                    j += 2;
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<Kind> {
        scan_text(s).into_iter().map(|f| f.kind).collect()
    }

    #[test]
    fn finds_and_redacts_each_category_without_leaking_values() {
        let text = "imei 356938035643809; MAC aa:bb:cc:dd:ee:ff; SN: AB12CD34EF; mail ivan@example.com; \
                    tel +7 (916) 123-45-67; path USB\\VID_0421&PID_0223\\6&1A2B3C&0&2; C:\\Users\\ivan\\Desktop; \
                    token 0123456789abcdef0123456789abcdef0123";
        let (out, f) = redact_text(text);
        for secret in [
            "356938035643809",
            "aa:bb:cc",
            "AB12CD34EF",
            "ivan@example.com",
            "916",
            "6&1A2B3C",
            "ivan",
            "0123456789abcdef0123456789abcdef",
        ] {
            assert!(!out.contains(secret), "утечка {secret:?}: {out}");
        }
        let ks: Vec<_> = f.iter().map(|x| x.kind).collect();
        for k in [
            Kind::Imei,
            Kind::Mac,
            Kind::Serial,
            Kind::Email,
            Kind::Phone,
            Kind::UsbPath,
            Kind::UserPath,
            Kind::Token,
        ] {
            assert!(ks.contains(&k), "{k:?} не найден: {ks:?}");
        }
        assert!(out.contains("<REDACTED:IMEI>"));
    }

    #[test]
    fn keeps_harmless_technical_text() {
        let ok = "VID_0421 PID_0223, driver 10.0.26100.1, endpoint 0x81, 512 bytes, build 26100, ts 1700000000000";
        assert!(kinds(ok).is_empty(), "{:?}", scan_text(ok));
    }

    #[test]
    fn imei_needs_luhn_other_long_numbers_are_generic_ids() {
        assert_eq!(kinds("356938035643809"), vec![Kind::Imei]);
        assert_eq!(kinds("123456789012345"), vec![Kind::Identifier]);
    }

    #[test]
    fn reports_positions_but_not_values() {
        let f = scan_text("ok\nline imei: 4455667788");
        assert_eq!((f[0].line, f[0].kind), (2, Kind::Serial));
        assert!(!serde_json::to_string(&f).unwrap().contains("4455667788"));
    }

    #[test]
    fn payload_masking_keeps_length_and_header_but_drops_text() {
        let mut d = vec![0x7E, 0x20, 0x01];
        d.extend_from_slice(b"Ivan Petrov");
        d.extend_from_slice(&[0, 0xFF]);
        d.extend("Анна".encode_utf16().flat_map(|u| u.to_le_bytes()));
        d.extend("Anna".encode_utf16().flat_map(|u| u.to_le_bytes()));
        let m = mask_payload(&d, 3);
        assert_eq!(m.len(), d.len());
        assert_eq!(&m[..3], &d[..3]);
        assert!(!m.windows(4).any(|w| w == b"Ivan"));
        assert!(!m.windows(8).any(|w| w
            == "Anna"
                .encode_utf16()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<u8>>()
                .as_slice()));
    }
}
