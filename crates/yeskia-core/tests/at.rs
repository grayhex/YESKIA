//! Контрактные тесты AT-канала (issue #10): кадрирование, разбор ошибок, доступность возможностей.
//! Диалоги **синтетические** (по открытым стандартам 3GPP TS 27.007/27.005), не снятые с телефона:
//! они фиксируют контракт адаптера, но не доказывают работу на устройстве.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use yeskia_core::at::{self, AtError, Query, SerialPort};
use yeskia_core::error::{Error, ErrorCode};
use yeskia_core::operations::{CancellationToken, OperationContext, ProgressCell};

#[derive(Clone, Default)]
struct Modem {
    st: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    /// команда → ответ (сырые байты)
    script: HashMap<String, Vec<u8>>,
    out: VecDeque<u8>,
    /// размер порции чтения (имитация дробления)
    chunk: usize,
    /// эхо команд
    echo: bool,
    /// после этого числа записанных команд порт «отключается»
    unplug_after: Option<usize>,
    /// порт отключается после выдачи N байт ответа
    unplug_in_reply: Option<usize>,
    written: Vec<String>,
    served: usize,
    noise: Vec<u8>,
}

impl Modem {
    fn new() -> Self {
        let m = Modem::default();
        m.st.lock().unwrap().chunk = 64;
        m.st.lock().unwrap().echo = true;
        m
    }
    fn reply(&self, cmd: &str, bytes: &str) -> &Self {
        self.st.lock().unwrap().script.insert(cmd.into(), bytes.as_bytes().to_vec());
        self
    }
    fn with(&self, f: impl FnOnce(&mut State)) -> &Self {
        f(&mut self.st.lock().unwrap());
        self
    }
    fn written(&self) -> Vec<String> {
        self.st.lock().unwrap().written.clone()
    }
    /// Телефон Nokia с хранилищами (вымышленные значения).
    fn phone() -> Self {
        let m = Modem::new();
        m.reply("AT", "\r\nOK\r\n")
            .reply("AT+CGMI", "\r\nNokia\r\n\r\nOK\r\n")
            .reply("AT+CGMM", "\r\nNokia E52-1\r\n\r\nOK\r\n")
            .reply("AT+CGMR", "\r\nV 071.004\r\n01-01-10\r\nRM-469\r\n(c) Nokia\r\n\r\nOK\r\n")
            .reply("AT+CPBS?", "\r\n+CPBS: \"ME\",12,250\r\n\r\nOK\r\n")
            .reply("AT+CPMS?", "\r\n+CPMS: \"ME\",3,100,\"ME\",3,100,\"ME\",3,100\r\n\r\nOK\r\n");
        m
    }
}

impl SerialPort for Modem {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        let mut st = self.st.lock().unwrap();
        let text = String::from_utf8_lossy(data).trim_end_matches('\r').to_string();
        if let Some(n) = st.unplug_after {
            if st.written.len() >= n {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "порт потерян"));
            }
        }
        st.written.push(text.clone());
        if st.echo {
            let echo = format!("{text}\r");
            st.out.extend(echo.bytes());
        }
        let noise = std::mem::take(&mut st.noise);
        st.out.extend(noise);
        if let Some(r) = st.script.get(&text).cloned() {
            st.out.extend(r);
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], _timeout: Duration) -> io::Result<usize> {
        let mut st = self.st.lock().unwrap();
        if let Some(limit) = st.unplug_in_reply {
            if st.served >= limit {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "порт потерян"));
            }
        }
        let n = st.chunk.min(buf.len()).min(st.out.len());
        let n = match st.unplug_in_reply {
            Some(limit) => n.min(limit.saturating_sub(st.served)),
            None => n,
        };
        for b in buf.iter_mut().take(n) {
            *b = st.out.pop_front().unwrap();
        }
        st.served += n;
        if n == 0 {
            // имитация ожидания, чтобы тесты на таймаут не крутились вхолостую
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(n)
    }
}

fn ctx() -> OperationContext {
    OperationContext::new(
        "t".into(),
        CancellationToken::new(),
        std::time::Instant::now() + Duration::from_secs(30),
        Box::new(|| true),
        Arc::new(ProgressCell::default()),
    )
}

const T: Duration = Duration::from_millis(400);

#[test]
fn identify_reads_manufacturer_model_and_revision() {
    let mut m = Modem::phone();
    let id = at::identify(&mut m, T, &ctx()).unwrap();
    assert_eq!(id.manufacturer, "Nokia");
    assert_eq!(id.model, "Nokia E52-1");
    // многострочный ответ CGMR склеивается; персональных данных там нет
    assert!(id.revision.unwrap().starts_with("V 071.004"));
    // отправлены только запросы идентификации, и ничего больше
    assert_eq!(m.written(), vec!["AT", "AT+CGMI", "AT+CGMM", "AT+CGMR"]);
}

#[test]
fn framing_survives_byte_by_byte_delivery_missing_echo_and_urcs() {
    for (chunk, echo) in [(1, true), (1, false), (3, true), (500, false)] {
        let mut m = Modem::phone();
        m.with(|s| {
            s.chunk = chunk;
            s.echo = echo;
            // незапрошенный код между командой и ответом
            s.noise = b"\r\n+CIEV: 5,3\r\n".to_vec();
        });
        let id = at::identify(&mut m, T, &ctx()).unwrap();
        assert_eq!(id.model, "Nokia E52-1", "chunk={chunk} echo={echo}");
    }
    // URC в ответе на запрос с префиксом отбрасывается, а не ломает разбор
    let mut m = Modem::phone();
    m.reply("AT+CPBS?", "\r\n+CMTI: \"ME\",4\r\n+CPBS: \"ME\",7,50\r\n\r\nOK\r\n");
    let r = at::exec_plain(&mut m, Query::PhonebookStatus, T).unwrap();
    assert_eq!(at::parse_cpbs(&r).unwrap().used, 7);
}

#[test]
fn counts_come_from_storage_status_only() {
    let mut m = Modem::phone();
    let c = at::parse_cpbs(&at::exec(&mut m, Query::PhonebookStatus, T, &ctx()).unwrap()).unwrap();
    assert_eq!((c.name.as_str(), c.used, c.total), ("ME", 12, 250));
    let s = at::parse_cpms(&at::exec(&mut m, Query::MessageStorageStatus, T, &ctx()).unwrap()).unwrap();
    assert_eq!((s.used, s.total), (3, 100));
    assert_eq!(m.written(), vec!["AT+CPBS?", "AT+CPMS?"], "содержимое записей не запрашивалось");
}

#[test]
fn phone_errors_are_classified_not_swallowed() {
    let mut m = Modem::phone();
    m.reply("AT+CPBS?", "\r\n+CME ERROR: 4\r\n");
    let e = at::exec_plain(&mut m, Query::PhonebookStatus, T).unwrap_err();
    assert_eq!(e.code(), ErrorCode::CapabilityUnsupported, "операция не поддерживается телефоном");
    m.reply("AT+CPBS?", "\r\n+CME ERROR: 10\r\n");
    let e = at::exec_plain(&mut m, Query::PhonebookStatus, T).unwrap_err();
    assert_eq!(e.code(), ErrorCode::TransportError, "SIM отсутствует ≠ «не поддерживается»");
    m.reply("AT+CPMS?", "\r\n+CMS ERROR: 303\r\n");
    assert_eq!(at::exec_plain(&mut m, Query::MessageStorageStatus, T).unwrap_err().code(), ErrorCode::CapabilityUnsupported);
    m.reply("AT+CPMS?", "\r\nERROR\r\n");
    assert_eq!(at::exec_plain(&mut m, Query::MessageStorageStatus, T).unwrap_err().code(), ErrorCode::CapabilityUnsupported);
    assert!(AtError::Cme(4).is_unsupported());
}

#[test]
fn silent_port_times_out_and_does_not_hang() {
    let mut m = Modem::new(); // на команды не отвечает
    let t0 = std::time::Instant::now();
    let e = at::exec_plain(&mut m, Query::Attention, Duration::from_millis(150)).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Timeout);
    assert!(t0.elapsed() < Duration::from_secs(2));
}

#[test]
fn hot_unplug_is_reported_as_device_disconnected() {
    // отключение между командами
    let mut m = Modem::phone();
    m.with(|s| s.unplug_after = Some(2));
    let e = at::identify(&mut m, T, &ctx()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::DeviceDisconnected);
    // отключение посреди ответа
    let mut m = Modem::phone();
    m.with(|s| {
        s.echo = false;
        s.unplug_in_reply = Some(9);
    });
    let e = at::identify(&mut m, T, &ctx()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::DeviceDisconnected);
}

#[test]
fn cancellation_and_presence_loss_stop_the_exchange() {
    let token = CancellationToken::new();
    let present = Arc::new(Mutex::new(true));
    let p2 = Arc::clone(&present);
    let c = OperationContext::new(
        "t".into(),
        token.clone(),
        std::time::Instant::now() + Duration::from_secs(30),
        Box::new(move || *p2.lock().unwrap()),
        Arc::new(ProgressCell::default()),
    );
    let mut silent = Modem::new();
    token.cancel();
    assert_eq!(at::exec(&mut silent, Query::Attention, T, &c).unwrap_err().code(), ErrorCode::OperationCancelled);
    let token2 = CancellationToken::new();
    let p3 = Arc::clone(&present);
    let c2 = OperationContext::new(
        "t".into(),
        token2,
        std::time::Instant::now() + Duration::from_secs(30),
        Box::new(move || *p3.lock().unwrap()),
        Arc::new(ProgressCell::default()),
    );
    *present.lock().unwrap() = false;
    assert_eq!(at::exec(&mut silent, Query::Attention, T, &c2).unwrap_err().code(), ErrorCode::DeviceDisconnected);
}

#[test]
fn data_mode_and_hostile_replies_are_protocol_errors() {
    let mut m = Modem::phone();
    m.reply("AT", "\r\nCONNECT 115200\r\n");
    assert_eq!(at::exec_plain(&mut m, Query::Attention, T).unwrap_err().code(), ErrorCode::TransportError);
    // бесконечный поток без финального кода
    let mut flood = Modem::new();
    flood.reply("AT+CGMI", &"Nokia\r\n".repeat(2000));
    let e = at::exec_plain(&mut flood, Query::Manufacturer, T).unwrap_err();
    assert_eq!(e.code(), ErrorCode::TransportError);
    // огромная строка без перевода строки
    let mut long = Modem::new();
    long.reply("AT+CGMI", &"A".repeat(5000));
    assert!(at::exec_plain(&mut long, Query::Manufacturer, T).is_err());
    // NO CARRIER на запрос идентификации
    let mut nc = Modem::phone();
    nc.reply("AT+CGMI", "\r\nNO CARRIER\r\n");
    assert!(at::identify(&mut nc, T, &ctx()).is_err());
}

#[test]
fn a_non_nokia_modem_is_not_taken_for_a_phone() {
    let mut m = Modem::phone();
    m.reply("AT+CGMI", "\r\nQuectel\r\n\r\nOK\r\n");
    let e = at::identify(&mut m, T, &ctx()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::TransportError);
    assert!(e.to_string().contains("не Nokia"));
    assert_eq!(m.written(), vec!["AT", "AT+CGMI"], "после отказа модель не запрашивается");
}

#[test]
fn identity_without_firmware_is_still_identity() {
    let mut m = Modem::phone();
    m.reply("AT+CGMR", "\r\nERROR\r\n");
    let id = at::identify(&mut m, T, &ctx()).unwrap();
    assert_eq!(id.revision, None);
    assert_eq!(id.model, "Nokia E52-1");
    let _ = Error::Timeout;
}
