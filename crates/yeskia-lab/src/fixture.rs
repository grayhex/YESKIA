//! Журнал кадров (JSON Lines): первая строка — заголовок, далее кадры и маркеры UI-действий.
//! Репозиторий хранит только **синтетические** фикстуры (`synthetic: true`).

use serde::{Deserialize, Serialize};

pub const FIXTURE_SCHEMA: &str = "yeskia.lab.fixture/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub schema: String,
    pub synthetic: bool,
    pub channel: crate::report::Channel,
    pub description: String,
    /// Для локальных (несинтетических) журналов: к полезной нагрузке применена `mask_payload`.
    #[serde(default)]
    pub payload_masked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    /// Хост → устройство.
    Out,
    /// Устройство → хост.
    In,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Bulk,
    Control,
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Record {
    /// Метка действия пользователя в UI оригинального приложения (для сопоставления по времени).
    Marker { t_ms: u64, marker: String },
    /// Событие шины: `reset`, `resume`, `suspend`.
    Event { t_ms: u64, event: String },
    Frame {
        t_ms: u64,
        dir: Dir,
        ep: String,
        kind: Kind,
        hex: String,
    },
}

impl Record {
    pub fn t_ms(&self) -> u64 {
        match self {
            Record::Marker { t_ms, .. }
            | Record::Event { t_ms, .. }
            | Record::Frame { t_ms, .. } => *t_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    pub header: Header,
    pub records: Vec<Record>,
}

pub fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() % 2 != 0 {
        return Err("нечётная длина hex".into());
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse(text: &str) -> Result<Fixture, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().ok_or("пустой файл")?;
    let header: Header = serde_json::from_str(first).map_err(|e| format!("заголовок: {e}"))?;
    if header.schema != FIXTURE_SCHEMA {
        return Err(format!("ожидается схема {FIXTURE_SCHEMA}"));
    }
    let mut records = Vec::new();
    for (n, l) in lines.enumerate() {
        let r: Record = serde_json::from_str(l).map_err(|e| format!("запись {}: {e}", n + 1))?;
        if let Record::Frame { hex, .. } = &r {
            parse_hex(hex).map_err(|e| format!("запись {}: {e}", n + 1))?;
        }
        records.push(r);
    }
    Ok(Fixture { header, records })
}

pub fn serialize(f: &Fixture) -> String {
    let mut out = serde_json::to_string(&f.header).unwrap_or_default();
    out.push('\n');
    for r in &f.records {
        out.push_str(&serde_json::to_string(r).unwrap_or_default());
        out.push('\n');
    }
    out
}
