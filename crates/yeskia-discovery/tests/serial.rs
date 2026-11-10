//! AT-канал в адаптере Windows-обнаружения (issue #10): доступность возможностей по результатам
//! реального обмена, а не по VID/PID; ошибки порта; отключение. Модем — синтетический.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use yeskia_core::at::{SerialOpener, SerialPort};
use yeskia_core::device::*;
use yeskia_core::diagnostics::Diagnostics;
use yeskia_core::engine::Engine;
use yeskia_core::error::{Error, ErrorCode};
use yeskia_core::operations::*;
use yeskia_core::storage::Storage;
use yeskia_discovery::adapter::PnpTransport;
use yeskia_discovery::fake::*;

#[derive(Clone)]
struct Behaviour {
    script: Arc<Mutex<HashMap<String, String>>>,
    written: Arc<Mutex<Vec<String>>>,
    fail_open: Arc<Mutex<Option<String>>>,
}

impl Behaviour {
    fn phone() -> Self {
        let mut s = HashMap::new();
        for (c, r) in [
            ("AT", "OK"),
            ("AT+CGMI", "Nokia\r\n\r\nOK"),
            ("AT+CGMM", "Nokia E52-1\r\n\r\nOK"),
            ("AT+CGMR", "V 071.004\r\nRM-469\r\n\r\nOK"),
            ("AT+CPBS?", "+CPBS: \"ME\",12,250\r\n\r\nOK"),
            (
                "AT+CPMS?",
                "+CPMS: \"ME\",3,100,\"ME\",3,100,\"ME\",3,100\r\n\r\nOK",
            ),
        ] {
            s.insert(c.to_string(), format!("\r\n{r}\r\n"));
        }
        Behaviour {
            script: Arc::new(Mutex::new(s)),
            written: Arc::default(),
            fail_open: Arc::default(),
        }
    }
    fn set(&self, cmd: &str, reply: &str) {
        self.script
            .lock()
            .unwrap()
            .insert(cmd.into(), format!("\r\n{reply}\r\n"));
    }
}

struct Port {
    b: Behaviour,
    out: VecDeque<u8>,
}

impl SerialPort for Port {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        let cmd = String::from_utf8_lossy(data)
            .trim_end_matches('\r')
            .to_string();
        self.b.written.lock().unwrap().push(cmd.clone());
        self.out.extend(format!("{cmd}\r").bytes());
        if let Some(r) = self.b.script.lock().unwrap().get(&cmd) {
            self.out.extend(r.bytes());
        }
        Ok(())
    }
    fn read(&mut self, buf: &mut [u8], _t: Duration) -> io::Result<usize> {
        let n = buf.len().min(self.out.len());
        for b in buf.iter_mut().take(n) {
            *b = self.out.pop_front().unwrap();
        }
        if n == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(n)
    }
}

impl SerialOpener for Behaviour {
    fn open(&self, _name: &str) -> yeskia_core::error::Result<Box<dyn SerialPort>> {
        if let Some(m) = self.fail_open.lock().unwrap().clone() {
            return Err(Error::HostAccess(m));
        }
        Ok(Box::new(Port {
            b: self.clone(),
            out: VecDeque::new(),
        }))
    }
}

fn com_phone(serial: &str) -> (FakeTopology, String) {
    let mut t = FakeTopology::new();
    let usb = t.usb(0x0421, 0x0223, serial, "usbser");
    t.com(&usb, "COM7", None);
    (t, usb)
}

fn engine(src: Arc<FakeSource>, b: Option<&Behaviour>) -> Engine {
    let mut t = PnpTransport::new(src);
    if let Some(b) = b {
        t = t.with_serial(Arc::new(b.clone()));
    }
    Engine::new(
        vec![Arc::new(t)],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(50, None)),
    )
}

fn entry(d: &Device, op: Operation) -> &CapabilityEntry {
    d.matrix
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .find(|e| e.operation == op)
        .unwrap()
}

#[test]
fn identity_and_counts_are_available_only_after_a_real_exchange() {
    let (t, _) = com_phone("SER0001");
    let b = Behaviour::phone();
    let e = engine(Arc::new(FakeSource::new(t)), Some(&b));
    let devs = e.list_devices().unwrap();
    assert_eq!(devs[0].transport, TransportKind::PcSuite);
    assert_eq!(
        devs[0].identified_model, None,
        "до подключения модель не заявляется"
    );
    let d = e.connect(&devs[0].id).unwrap();
    assert_eq!(
        d.identified_model.as_deref(),
        Some("Nokia E52-1"),
        "модель — из ответа AT+CGMM"
    );
    for op in [
        Operation::IdentityRead,
        Operation::ContactsRead,
        Operation::SmsRead,
    ] {
        let en = entry(&d, op);
        assert_eq!(en.availability, Availability::Available, "{op:?}");
        assert_eq!(
            en.maturity,
            SupportStatus::Implemented,
            "реализовано, телефон не проверялся"
        );
    }
    // записи и установка не заявлены
    for op in [
        Operation::FilesWrite,
        Operation::ContactsWrite,
        Operation::SmsSend,
        Operation::PackagesInstall,
    ] {
        assert_eq!(
            entry(&d, op).availability,
            Availability::Unsupported,
            "{op:?}"
        );
    }
    let r = e.run(&d.id, OperationRequest::ContactsRead).unwrap();
    assert_eq!(r.data, Some(OperationData::PimCount { count: 12 }));
    let r = e.run(&d.id, OperationRequest::SmsRead).unwrap();
    assert_eq!(r.data, Some(OperationData::PimCount { count: 3 }));
}

#[test]
fn only_read_only_queries_ever_reach_the_port() {
    let (t, _) = com_phone("SER0002");
    let b = Behaviour::phone();
    let e = engine(Arc::new(FakeSource::new(t)), Some(&b));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    e.run(&d.id, OperationRequest::ContactsRead).unwrap();
    e.run(&d.id, OperationRequest::SmsRead).unwrap();
    e.run(&d.id, OperationRequest::IdentityRead).unwrap();
    let allowed = [
        "AT", "AT+CGMI", "AT+CGMM", "AT+CGMR", "AT+CPBS?", "AT+CPMS?",
    ];
    let written = b.written.lock().unwrap().clone();
    assert!(!written.is_empty());
    assert!(
        written.iter().all(|c| allowed.contains(&c.as_str())),
        "{written:?}"
    );
    // запись, отправка SMS и файлы отклоняются до порта
    let before = written.len();
    for req in [
        OperationRequest::FilesWrite {
            path: "/a".into(),
            data: vec![1],
            overwrite: OverwritePolicy::Never,
        },
        OperationRequest::FilesList { path: "/".into() },
    ] {
        let _ = e.submit(&d.id, req);
    }
    assert_eq!(b.written.lock().unwrap().len(), before, "порт не затронут");
}

#[test]
fn without_a_serial_opener_com_mode_claims_nothing() {
    let (t, _) = com_phone("SER0003");
    let e = engine(Arc::new(FakeSource::new(t)), None);
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    assert_eq!(d.identified_model, None);
    for op in [
        Operation::IdentityRead,
        Operation::ContactsRead,
        Operation::SmsRead,
    ] {
        assert_eq!(
            entry(&d, op).availability,
            Availability::Unsupported,
            "{op:?}"
        );
        assert_eq!(entry(&d, op).maturity, SupportStatus::Unsupported);
    }
    let err = e
        .submit(&d.id, OperationRequest::ContactsRead)
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
}

#[test]
fn busy_port_is_explained_not_faked() {
    let (t, _) = com_phone("SER0004");
    let b = Behaviour::phone();
    *b.fail_open.lock().unwrap() =
        Some("COM-порт занят другой программой (например, Nokia PC Suite)".into());
    let e = engine(Arc::new(FakeSource::new(t)), Some(&b));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    for op in [
        Operation::IdentityRead,
        Operation::ContactsRead,
        Operation::SmsRead,
    ] {
        let en = entry(&d, op);
        assert_ne!(en.availability, Availability::Available, "{op:?}");
        assert!(
            en.detail.as_deref().unwrap_or("").contains("занят"),
            "{:?}",
            en.detail
        );
    }
    assert_eq!(d.identified_model, None);
    assert_eq!(b.written.lock().unwrap().len(), 0);
}

#[test]
fn phone_rejection_of_one_service_does_not_hide_the_others() {
    let (t, _) = com_phone("SER0005");
    let b = Behaviour::phone();
    b.set("AT+CPBS?", "+CME ERROR: 4");
    let e = engine(Arc::new(FakeSource::new(t)), Some(&b));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    assert_eq!(
        entry(&d, Operation::IdentityRead).availability,
        Availability::Available
    );
    assert_eq!(
        entry(&d, Operation::ContactsRead).availability,
        Availability::Unsupported
    );
    assert_eq!(
        entry(&d, Operation::SmsRead).availability,
        Availability::Available
    );
    // операция, которую телефон отклонил, закрыта матрицей ещё до обращения к порту
    let before = b.written.lock().unwrap().len();
    let err = e.run(&d.id, OperationRequest::ContactsRead).err().unwrap();
    assert_eq!(err.code(), ErrorCode::CapabilityUnsupported);
    assert_eq!(b.written.lock().unwrap().len(), before);
    assert_eq!(
        e.run(&d.id, OperationRequest::SmsRead).unwrap().outcome,
        Outcome::Success
    );
}

#[test]
fn a_non_nokia_modem_on_a_nokia_usb_id_is_not_identified() {
    let (t, _) = com_phone("SER0006");
    let b = Behaviour::phone();
    b.set("AT+CGMI", "Acme Modems\r\n\r\nOK");
    let e = engine(Arc::new(FakeSource::new(t)), Some(&b));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    assert_eq!(d.identified_model, None);
    assert_ne!(
        entry(&d, Operation::IdentityRead).availability,
        Availability::Available
    );
    assert_ne!(
        entry(&d, Operation::ContactsRead).availability,
        Availability::Available
    );
}

#[test]
fn hot_unplug_during_an_at_operation_is_detected() {
    let (t, usb) = com_phone("SER0007");
    let src = Arc::new(FakeSource::new(t));
    let b = Behaviour::phone();
    let e = engine(Arc::clone(&src), Some(&b));
    let d = e.connect(&e.list_devices().unwrap()[0].id).unwrap();
    src.with(|t| t.remove(&usb));
    let r = e.run(&d.id, OperationRequest::ContactsRead);
    match r {
        Err(err) => assert!(
            matches!(
                err.code(),
                ErrorCode::DeviceDisconnected | ErrorCode::DeviceNotFound
            ),
            "{err}"
        ),
        Ok(res) => {
            assert_eq!(res.outcome, Outcome::Failed);
            assert_eq!(res.error.unwrap().code, ErrorCode::DeviceDisconnected);
        }
    }
}
