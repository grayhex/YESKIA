//! Канал «AT-команды поверх последовательного порта» — **открытый стандарт 3GPP** (TS 27.007 и
//! 27.005), а не протокол Nokia PC Suite (NPC). Модуль платформенно-независим: порт приходит через
//! трейт `SerialPort`, поэтому кадрирование и разбор ошибок проверяются без телефона.
//!
//! Принципы безопасности:
//! * набор команд **закрыт** (`Query`): текст команды берётся только из перечисления, пользовательских
//!   строк и записи нет; в перечислении нет `AT+CMGS` (отправка SMS), `AT+CPBW`/`AT+CMGD` (запись и
//!   удаление), `AT+CGSN` (IMEI), сброса (`ATZ`, `AT&F`) и смены режима (`ATE0`, `AT+CPBS="…"`);
//! * ответ ограничен по размеру и числу строк; режим данных (`CONNECT`) трактуется как нарушение
//!   протокола, обмен прерывается;
//! * данные пользователя (имена, номера, сообщения) не запрашиваются: читаются только идентификация и
//!   **счётчики** занятости хранилищ;
//! * ответы не пишутся в журнал.

use crate::error::{Error, Result};
use crate::operations::OperationContext;
use std::io;
use std::time::{Duration, Instant};

pub const MAX_REPLY_BYTES: usize = 4096;
pub const MAX_REPLY_LINES: usize = 64;
const POLL: Duration = Duration::from_millis(100);
/// Время ожидания ответа на одну команду.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Последовательный порт. Реализация на Windows — в `yeskia-discovery`; в тестах — поддельный модем.
pub trait SerialPort: Send {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()>;
    /// Читает не больше `buf.len()` байт, ожидая до `timeout`. `Ok(0)` — данных не пришло;
    /// `Err` — порт потерян (отключение), попытки продолжать не нужно.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize>;
}

/// Открывает порт по имени (`COM5`). Порт открывается на время операции и закрывается сразу после,
/// чтобы не занимать его для других программ.
pub trait SerialOpener: Send + Sync {
    fn open(&self, port_name: &str) -> Result<Box<dyn SerialPort>>;
}

/// Разрешённые команды (только чтение).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    /// `AT` — проверка связи.
    Attention,
    /// `AT+CGMI` — производитель.
    Manufacturer,
    /// `AT+CGMM` — модель.
    Model,
    /// `AT+CGMR` — версия прошивки.
    Revision,
    /// `AT+CPBS?` — текущее хранилище телефонной книги и **число** записей.
    PhonebookStatus,
    /// `AT+CPMS?` — хранилища сообщений и **число** сообщений.
    MessageStorageStatus,
}

impl Query {
    pub const ALL: [Query; 6] = [
        Query::Attention,
        Query::Manufacturer,
        Query::Model,
        Query::Revision,
        Query::PhonebookStatus,
        Query::MessageStorageStatus,
    ];

    pub fn text(self) -> &'static str {
        match self {
            Query::Attention => "AT",
            Query::Manufacturer => "AT+CGMI",
            Query::Model => "AT+CGMM",
            Query::Revision => "AT+CGMR",
            Query::PhonebookStatus => "AT+CPBS?",
            Query::MessageStorageStatus => "AT+CPMS?",
        }
    }

    /// Префикс информационной строки; `None` — текстовые строки без префикса.
    fn prefix(self) -> Option<&'static str> {
        match self {
            Query::PhonebookStatus => Some("+CPBS:"),
            Query::MessageStorageStatus => Some("+CPMS:"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtError {
    /// Финальный `ERROR` без кода.
    Error,
    /// `+CME ERROR: n` — ошибка оборудования.
    Cme(u32),
    /// `+CMS ERROR: n` — ошибка службы сообщений.
    Cms(u32),
    /// Нет ответа за отведённое время.
    Timeout,
    /// Порт потерян (телефон отключён или порт закрыт).
    Disconnected,
    /// Нарушение протокола: режим данных, мусор, слишком длинный ответ.
    Protocol(String),
}

impl AtError {
    /// Телефон ответил, что операцию не поддерживает/не разрешает (а не сломался канал).
    pub fn is_unsupported(&self) -> bool {
        match self {
            // 3GPP TS 27.007 9.2: 3 not allowed, 4 not supported, 100 unknown; 302/303 для CMS
            AtError::Error => true,
            AtError::Cme(c) => matches!(c, 3 | 4 | 100),
            AtError::Cms(c) => matches!(c, 300 | 301 | 302 | 303 | 304),
            _ => false,
        }
    }
}

impl From<AtError> for Error {
    fn from(e: AtError) -> Self {
        match e {
            AtError::Disconnected => Error::DeviceDisconnected,
            AtError::Timeout => Error::Timeout,
            other if other.is_unsupported() => {
                Error::CapabilityUnsupported("телефон отклонил AT-команду".into())
            }
            AtError::Cme(c) => Error::Transport(format!("AT: +CME ERROR {c}")),
            AtError::Cms(c) => Error::Transport(format!("AT: +CMS ERROR {c}")),
            AtError::Error => Error::Transport("AT: ERROR".into()),
            AtError::Protocol(m) => Error::Transport(format!("AT: нарушение протокола: {m}")),
        }
    }
}

/// Информационные строки ответа (без эха и финального кода).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reply {
    pub lines: Vec<String>,
}

fn final_code(line: &str) -> Option<std::result::Result<(), AtError>> {
    let up = line.to_ascii_uppercase();
    if up == "OK" {
        return Some(Ok(()));
    }
    if up == "ERROR" {
        return Some(Err(AtError::Error));
    }
    for (p, cme) in [("+CME ERROR:", true), ("+CMS ERROR:", false)] {
        if let Some(rest) = up.strip_prefix(p) {
            // код может быть числом или текстом (AT+CMEE=2) — текст считаем «неизвестной» ошибкой 100
            let n = rest.trim().parse::<u32>().unwrap_or(100);
            return Some(Err(if cme {
                AtError::Cme(n)
            } else {
                AtError::Cms(n)
            }));
        }
    }
    if matches!(
        up.as_str(),
        "NO CARRIER" | "BUSY" | "NO ANSWER" | "NO DIALTONE"
    ) {
        return Some(Err(AtError::Protocol(format!("неожиданный код {up}"))));
    }
    if up.starts_with("CONNECT") {
        return Some(Err(AtError::Protocol(
            "модем перешёл в режим данных".into(),
        )));
    }
    None
}

/// Накопитель байтов → строки (`\r`, `\n` и их сочетания разделяют строки, пустые отбрасываются).
#[derive(Default)]
struct Framer {
    buf: Vec<u8>,
    total: usize,
}

impl Framer {
    fn push(&mut self, data: &[u8]) -> std::result::Result<Vec<String>, AtError> {
        self.total += data.len();
        if self.total > MAX_REPLY_BYTES * 2 {
            return Err(AtError::Protocol("ответ слишком длинный".into()));
        }
        let mut lines = Vec::new();
        for &b in data {
            if b == b'\r' || b == b'\n' {
                if !self.buf.is_empty() {
                    lines
                        .push(String::from_utf8_lossy(&std::mem::take(&mut self.buf)).into_owned());
                }
            } else {
                self.buf.push(b);
                if self.buf.len() > 512 {
                    return Err(AtError::Protocol("строка ответа слишком длинная".into()));
                }
            }
        }
        Ok(lines)
    }
}

fn io_to_at(e: &io::Error) -> AtError {
    match e.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => AtError::Timeout,
        _ => AtError::Disconnected,
    }
}

/// Выбрасывает накопившиеся в приёмном буфере байты (остатки прошлой сессии, URC), ограниченно.
pub fn drain(port: &mut dyn SerialPort) {
    let mut buf = [0u8; 256];
    let mut left = 8;
    while left > 0 {
        match port.read(&mut buf, Duration::from_millis(30)) {
            Ok(0) | Err(_) => break,
            Ok(_) => left -= 1,
        }
    }
}

/// Выполняет одну разрешённую команду и возвращает информационные строки. Ошибки телефона
/// (`ERROR`, `+CME ERROR`) приходят как `Error::CapabilityUnsupported` либо `Error::Transport`;
/// отмена, таймаут операции и пропажа устройства — как свои коды.
pub fn exec(
    port: &mut dyn SerialPort,
    q: Query,
    timeout: Duration,
    ctx: &OperationContext,
) -> Result<Reply> {
    run(port, q, timeout, &|| ctx.checkpoint())
}

/// То же без контекста операции (тесты и разовые проверки).
pub fn exec_plain(port: &mut dyn SerialPort, q: Query, timeout: Duration) -> Result<Reply> {
    run(port, q, timeout, &|| Ok(()))
}

fn run(
    port: &mut dyn SerialPort,
    q: Query,
    timeout: Duration,
    check: &dyn Fn() -> Result<()>,
) -> Result<Reply> {
    let cmd = q.text();
    port.write_all(format!("{cmd}\r").as_bytes())
        .map_err(|e| Error::from(io_to_at(&e)))?;
    let started = Instant::now();
    let mut framer = Framer::default();
    let mut reply = Reply::default();
    let mut echo_seen = false;
    let mut chunk = [0u8; 256];
    loop {
        check()?;
        if started.elapsed() >= timeout {
            return Err(AtError::Timeout.into());
        }
        let n = port
            .read(&mut chunk, POLL)
            .map_err(|e| Error::from(io_to_at(&e)))?;
        if n == 0 {
            continue;
        }
        for line in framer.push(&chunk[..n]).map_err(Error::from)? {
            if !echo_seen && line.eq_ignore_ascii_case(cmd) {
                echo_seen = true;
                continue;
            }
            if let Some(done) = final_code(&line) {
                return done.map(|()| reply).map_err(Error::from);
            }
            // Неожиданные строки (URC вроде +CIEV/+CMTI) для запросов с префиксом отбрасываются.
            if let Some(p) = q.prefix() {
                if !line.to_ascii_uppercase().starts_with(p) {
                    continue;
                }
            }
            if reply.lines.len() >= MAX_REPLY_LINES
                || reply.lines.iter().map(String::len).sum::<usize>() + line.len() > MAX_REPLY_BYTES
            {
                return Err(AtError::Protocol("ответ слишком длинный".into()).into());
            }
            reply.lines.push(line);
        }
    }
}

// ---------- разбор ----------

/// Текст идентификации (`AT+CGMI/CGMM/CGMR`): печатные символы, не длиннее 64.
pub fn parse_info(reply: &Reply, prefix: &str) -> Result<String> {
    let text = reply
        .lines
        .iter()
        .map(|l| {
            let up = l.to_ascii_uppercase();
            let l = if up.starts_with(prefix) {
                &l[prefix.len()..]
            } else {
                l
            };
            l.trim().trim_matches('"').trim().to_string()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() || text.chars().count() > 64 || text.chars().any(char::is_control) {
        return Err(Error::Transport(
            "AT: непонятный ответ идентификации".into(),
        ));
    }
    Ok(text)
}

/// Занятость хранилища: имя, занято, всего.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageStatus {
    pub name: String,
    pub used: u64,
    pub total: u64,
}

fn unquote(s: &str) -> &str {
    s.trim().trim_matches('"')
}

/// `+CPBS: "ME",12,250` → занято 12 из 250. Ответ без счётчиков (`+CPBS: "SM"`) — ошибка: число
/// не выдумывается.
pub fn parse_cpbs(reply: &Reply) -> Result<StorageStatus> {
    let line = reply
        .lines
        .iter()
        .find(|l| l.to_ascii_uppercase().starts_with("+CPBS:"))
        .ok_or_else(|| Error::Transport("AT: нет строки +CPBS".into()))?;
    let parts: Vec<&str> = line["+CPBS:".len()..].split(',').collect();
    storage(&parts, "+CPBS")
}

/// `+CPMS: "ME",3,100,"ME",3,100,"ME",3,100` → первое хранилище (чтение/удаление).
pub fn parse_cpms(reply: &Reply) -> Result<StorageStatus> {
    let line = reply
        .lines
        .iter()
        .find(|l| l.to_ascii_uppercase().starts_with("+CPMS:"))
        .ok_or_else(|| Error::Transport("AT: нет строки +CPMS".into()))?;
    let parts: Vec<&str> = line["+CPMS:".len()..].split(',').collect();
    storage(&parts, "+CPMS")
}

fn storage(parts: &[&str], what: &str) -> Result<StorageStatus> {
    let bad = || Error::Transport(format!("AT: {what} без счётчиков"));
    let name = parts
        .first()
        .map(|s| unquote(s))
        .filter(|s| !s.is_empty() && s.len() <= 8)
        .ok_or_else(bad)?;
    let used: u64 = parts
        .get(1)
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(bad)?;
    let total: u64 = parts
        .get(2)
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(bad)?;
    if used > total || total > 1_000_000 {
        return Err(bad());
    }
    Ok(StorageStatus {
        name: name.to_string(),
        used,
        total,
    })
}

/// Результат идентификации по AT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtIdentity {
    pub manufacturer: String,
    pub model: String,
    pub revision: Option<String>,
}

/// `AT` → `CGMI` → `CGMM` → `CGMR`. Производитель обязан содержать «Nokia»: ответ чужого модема на
/// COM-порту не принимается за телефон.
pub fn identify(
    port: &mut dyn SerialPort,
    timeout: Duration,
    ctx: &OperationContext,
) -> Result<AtIdentity> {
    drain(port);
    exec(port, Query::Attention, timeout, ctx)?;
    let manufacturer = parse_info(&exec(port, Query::Manufacturer, timeout, ctx)?, "+CGMI:")?;
    if !manufacturer.to_ascii_lowercase().contains("nokia") {
        return Err(Error::Transport(
            "порт отвечает, но производитель не Nokia: это не телефон".into(),
        ));
    }
    let model = parse_info(&exec(port, Query::Model, timeout, ctx)?, "+CGMM:")?;
    let revision = exec(port, Query::Revision, timeout, ctx)
        .and_then(|r| parse_info(&r, "+CGMR:"))
        .ok();
    Ok(AtIdentity {
        manufacturer,
        model,
        revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_set_is_closed_and_read_only() {
        for q in Query::ALL {
            let t = q.text();
            assert!(t == "AT" || t.starts_with("AT+C"), "{t}");
            for forbidden in [
                "CMGS", "CMGW", "CMGD", "CPBW", "CGSN", "ATZ", "&F", "ATE", "CPBS=", "CPMS=",
                "ATD", "CFUN", "CKPD", "CLCK",
            ] {
                assert!(!t.contains(forbidden), "{t} содержит {forbidden}");
            }
            // запросы изменения (=) недопустимы; запрос чтения (?) — да
            assert!(!t.contains('='), "{t}");
        }
    }

    #[test]
    fn framer_splits_on_any_line_ending_and_limits_size() {
        let mut f = Framer::default();
        assert_eq!(f.push(b"AT\r\r\nOK").unwrap(), vec!["AT"]);
        assert_eq!(f.push(b"\r\n").unwrap(), vec!["OK"]);
        let mut f = Framer::default();
        assert!(f.push(&vec![b'x'; 600]).is_err());
        let mut f = Framer::default();
        assert!(f.push(&vec![b'a'; 9000]).is_err());
    }

    #[test]
    fn final_codes_are_classified() {
        assert_eq!(final_code("OK"), Some(Ok(())));
        assert_eq!(final_code("ok"), Some(Ok(())));
        assert_eq!(final_code("ERROR"), Some(Err(AtError::Error)));
        assert_eq!(final_code("+CME ERROR: 4"), Some(Err(AtError::Cme(4))));
        assert_eq!(
            final_code("+CME ERROR: operation not supported"),
            Some(Err(AtError::Cme(100)))
        );
        assert_eq!(final_code("+CMS ERROR: 303"), Some(Err(AtError::Cms(303))));
        assert!(matches!(
            final_code("CONNECT 9600"),
            Some(Err(AtError::Protocol(_)))
        ));
        assert!(matches!(
            final_code("NO CARRIER"),
            Some(Err(AtError::Protocol(_)))
        ));
        assert_eq!(final_code("Nokia"), None);
        assert_eq!(final_code("+CPBS: \"ME\",1,2"), None);
    }

    #[test]
    fn unsupported_is_separated_from_broken() {
        assert!(AtError::Cme(4).is_unsupported());
        assert!(AtError::Error.is_unsupported());
        assert!(
            !AtError::Cme(10).is_unsupported(),
            "SIM not inserted ≠ не поддерживается"
        );
        assert!(!AtError::Timeout.is_unsupported());
        assert!(!AtError::Disconnected.is_unsupported());
    }

    #[test]
    fn storage_parsers_never_invent_counts() {
        let r = |s: &str| Reply {
            lines: vec![s.into()],
        };
        assert_eq!(
            parse_cpbs(&r("+CPBS: \"ME\",12,250")).unwrap(),
            StorageStatus {
                name: "ME".into(),
                used: 12,
                total: 250
            }
        );
        assert!(parse_cpbs(&r("+CPBS: \"SM\"")).is_err(), "без счётчиков");
        assert!(
            parse_cpbs(&r("+CPBS: \"ME\",300,250")).is_err(),
            "занято > всего"
        );
        assert!(parse_cpbs(&r("+CPBS: \"ME\",a,b")).is_err());
        assert!(parse_cpbs(&Reply::default()).is_err());
        let m = parse_cpms(&r("+CPMS: \"ME\",3,100,\"ME\",3,100,\"ME\",3,100")).unwrap();
        assert_eq!((m.name.as_str(), m.used, m.total), ("ME", 3, 100));
        assert!(parse_cpms(&r("+CPMS: \"ME\"")).is_err());
    }

    #[test]
    fn info_parser_strips_prefix_and_rejects_garbage() {
        let r = |s: &str| Reply {
            lines: vec![s.into()],
        };
        assert_eq!(parse_info(&r("Nokia"), "+CGMI:").unwrap(), "Nokia");
        assert_eq!(
            parse_info(&r("+CGMM: \"Nokia E52-1\""), "+CGMM:").unwrap(),
            "Nokia E52-1"
        );
        assert!(parse_info(&Reply::default(), "+CGMI:").is_err());
        assert!(parse_info(&r(&"x".repeat(100)), "+CGMI:").is_err());
        assert!(parse_info(&r("a\u{7}b"), "+CGMI:").is_err());
    }
}
