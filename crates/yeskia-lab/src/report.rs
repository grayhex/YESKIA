//! Версионированный формат отчёта эксперимента и его проверка.
//!
//! Правила (проверяются `validate`):
//! * фиксированная версия схемы; обязательные разделы «что известно / что неизвестно» и следующий эксперимент;
//! * сырые захваты не коммитятся (`capture.raw_committed = false`);
//! * у каждого вывода указан уровень доказательств; утверждение «поддерживается» допустимо
//!   только при `adapter-verified` со ссылкой на адаптер YESKIA и устройство;
//! * в любых строках нет IMEI/серийников/MAC/e-mail/телефонов/путей устройств/токенов;
//! * реальный (не синтетический) отчёт требует подтверждения, что человек просмотрел редакцию.

use crate::redact::{scan_text, Finding};
use serde::{Deserialize, Serialize};

pub const REPORT_SCHEMA: &str = "yeskia.lab.report/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// WPD/MTP — не смешивается с PC Suite.
    Wpd,
    /// Виртуальный COM-порт (PC Suite / AT / FBUS и т. п.).
    PcSuiteSerial,
    /// Сырой USB (bulk/control/interrupt).
    UsbRaw,
    Bluetooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Evidence {
    /// Увидено в захвате.
    Observed,
    /// Выведено из наблюдений (эвристика).
    Inferred,
    /// Гипотеза, требует эксперимента.
    Hypothesis,
    /// Подтверждено успешным общением отдельного адаптера YESKIA с телефоном.
    AdapterVerified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setup {
    pub os: String,
    pub phone_model: String,
    pub phone_rm: Option<String>,
    pub firmware: String,
    pub driver: String,
    pub usb_mode: String,
    pub npc_version: Option<String>,
    /// `0421:0223` — идентификатор модели USB, не персональные данные.
    pub usb_vid_pid: String,
    pub usb_interface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestData {
    pub separate_test_data: bool,
    pub user_data_backed_up: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub tool: String,
    pub filter: Option<String>,
    pub raw_stored_locally: bool,
    /// Всегда `false`: сырые захваты в репозиторий не попадают.
    pub raw_committed: bool,
    pub retention_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
    pub tool: String,
    pub categories: Vec<String>,
    pub reviewed_by_human: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingEntry {
    pub statement: String,
    pub evidence: Evidence,
    /// Для `adapter-verified`: адаптер YESKIA и устройство (fingerprint), на котором подтверждено.
    pub adapter_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentReport {
    pub schema: String,
    /// `EXP-0001`.
    pub id: String,
    pub title: String,
    /// `YYYY-MM-DD`.
    pub created: String,
    /// `true` — вымышленные данные для демонстрации формата/тестов.
    pub synthetic: bool,
    pub setup: Setup,
    pub test_data: TestData,
    /// Одно действие за эксперимент (enumerate / identity / list folders / read file / …).
    pub action: String,
    pub channel: Channel,
    pub capture: Capture,
    pub redaction: Redaction,
    pub findings: Vec<FindingEntry>,
    pub known: Vec<String>,
    pub unknown: Vec<String>,
    pub next_experiment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub path: String,
    pub rule: String,
}

fn v(path: &str, rule: &str) -> Violation {
    Violation {
        path: path.into(),
        rule: rule.into(),
    }
}

/// Все строки отчёта (для скана приватных данных).
fn strings(r: &ExperimentReport) -> Vec<(String, String)> {
    let mut out = vec![
        ("title".into(), r.title.clone()),
        ("action".into(), r.action.clone()),
        ("setup.os".into(), r.setup.os.clone()),
        ("setup.phone_model".into(), r.setup.phone_model.clone()),
        ("setup.firmware".into(), r.setup.firmware.clone()),
        ("setup.driver".into(), r.setup.driver.clone()),
        ("setup.usb_mode".into(), r.setup.usb_mode.clone()),
        ("setup.usb_vid_pid".into(), r.setup.usb_vid_pid.clone()),
        ("capture.tool".into(), r.capture.tool.clone()),
        ("redaction.tool".into(), r.redaction.tool.clone()),
        ("next_experiment".into(), r.next_experiment.clone()),
    ];
    for (name, o) in [
        ("setup.phone_rm", &r.setup.phone_rm),
        ("setup.npc_version", &r.setup.npc_version),
        ("setup.usb_interface", &r.setup.usb_interface),
        ("capture.filter", &r.capture.filter),
    ] {
        if let Some(s) = o {
            out.push((name.into(), s.clone()));
        }
    }
    for (i, f) in r.findings.iter().enumerate() {
        out.push((format!("findings[{i}].statement"), f.statement.clone()));
        if let Some(a) = &f.adapter_ref {
            out.push((format!("findings[{i}].adapter_ref"), a.clone()));
        }
    }
    for (i, s) in r.known.iter().enumerate() {
        out.push((format!("known[{i}]"), s.clone()));
    }
    for (i, s) in r.unknown.iter().enumerate() {
        out.push((format!("unknown[{i}]"), s.clone()));
    }
    out
}

/// Утверждает ли формулировка поддержку протокола/функции (без отрицания).
fn claims_support(s: &str) -> bool {
    let l = s.to_lowercase();
    let positive = l.contains("поддерж")
        || l.contains("support")
        || l.contains("works")
        || l.contains("работает");
    let negated = [
        "не поддерж",
        "не подтвержд",
        "unsupported",
        "not supported",
        "no support",
        "не работает",
        "not proven",
        "не доказан",
    ]
    .iter()
    .any(|n| l.contains(n));
    positive && !negated
}

pub fn validate(r: &ExperimentReport) -> Vec<Violation> {
    let mut out = Vec::new();
    if r.schema != REPORT_SCHEMA {
        out.push(v("schema", &format!("ожидается {REPORT_SCHEMA}")));
    }
    let id_ok = r.id.len() >= 5
        && r.id.starts_with("EXP-")
        && r.id[4..].chars().all(|c| c.is_ascii_digit());
    if !id_ok {
        out.push(v("id", "формат EXP-0001"));
    }
    let date_ok = r.created.len() == 10
        && r.created.chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !date_ok {
        out.push(v("created", "формат YYYY-MM-DD"));
    }
    for (name, val) in [
        ("title", &r.title),
        ("action", &r.action),
        ("setup.os", &r.setup.os),
        ("setup.phone_model", &r.setup.phone_model),
        ("setup.firmware", &r.setup.firmware),
        ("setup.driver", &r.setup.driver),
        ("setup.usb_mode", &r.setup.usb_mode),
        ("setup.usb_vid_pid", &r.setup.usb_vid_pid),
        ("capture.tool", &r.capture.tool),
        ("next_experiment", &r.next_experiment),
    ] {
        if val.trim().is_empty() {
            out.push(v(name, "обязательное поле пусто"));
        }
    }
    if r.capture.raw_committed {
        out.push(v("capture.raw_committed", "сырые захваты нельзя коммитить"));
    }
    if !r.capture.raw_stored_locally && !r.synthetic {
        out.push(v(
            "capture.raw_stored_locally",
            "сырой захват должен храниться локально",
        ));
    }
    if r.capture.retention_days == 0 || r.capture.retention_days > 90 {
        out.push(v(
            "capture.retention_days",
            "минимальный срок хранения: 1–90 дней",
        ));
    }
    if !r.test_data.separate_test_data || !r.test_data.user_data_backed_up {
        out.push(v(
            "test_data",
            "нужны отдельные тестовые данные и копия пользовательских",
        ));
    }
    if r.known.is_empty() {
        out.push(v("known", "раздел «что известно» обязателен"));
    }
    if r.unknown.is_empty() {
        out.push(v("unknown", "раздел «что неизвестно» обязателен"));
    }
    if r.findings.is_empty() {
        out.push(v(
            "findings",
            "нужен хотя бы один вывод с уровнем доказательств",
        ));
    }
    if !r.synthetic && !r.redaction.reviewed_by_human {
        out.push(v(
            "redaction.reviewed_by_human",
            "реальный отчёт требует ручной проверки редакции",
        ));
    }
    for (i, f) in r.findings.iter().enumerate() {
        let p = format!("findings[{i}]");
        if claims_support(&f.statement) && f.evidence != Evidence::AdapterVerified {
            out.push(v(
                &p,
                "утверждение о поддержке допустимо только при evidence = adapter-verified",
            ));
        }
        if f.evidence == Evidence::AdapterVerified
            && f.adapter_ref.as_deref().unwrap_or("").is_empty()
        {
            out.push(v(
                &p,
                "adapter-verified требует adapter_ref (адаптер и устройство)",
            ));
        }
        if f.evidence == Evidence::AdapterVerified && r.synthetic {
            out.push(v(&p, "синтетический отчёт не может быть adapter-verified"));
        }
    }
    for (path, text) in strings(r) {
        if text.contains("TODO") {
            out.push(v(&path, "заполните поле: остался плейсхолдер TODO"));
        }
        let findings: Vec<Finding> = scan_text(&text);
        if !findings.is_empty() {
            out.push(v(&path, "содержит приватные данные (IMEI/серийник/MAC/e-mail/телефон/путь устройства/токен)"));
        }
    }
    out
}

pub fn parse(json: &str) -> Result<ExperimentReport, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}
