use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use yeskia_adbian::{AdbianTransport, Opener, Paired};
use yeskia_core::{
    adbian::{Channel, Session, MAX_PUT},
    device::*,
    diagnostics::Diagnostics,
    operations::*,
    storage::Storage,
    transport::*,
    Engine, Error,
};
fn ctx() -> OperationContext {
    OperationContext::new(
        "fake".into(),
        CancellationToken::new(),
        Instant::now() + Duration::from_secs(10),
        Box::new(|| true),
        Arc::new(ProgressCell::default()),
    )
}
fn framed(b: &[u8]) -> Vec<u8> {
    let mut out = (b.len() as u32).to_be_bytes().to_vec();
    out.extend(b);
    out
}
fn ok(s: &str) -> Vec<u8> {
    framed(format!("+{s}").as_bytes())
}
fn err(s: &str) -> Vec<u8> {
    framed(format!("-{s}").as_bytes())
}
#[derive(Default)]
struct Phone {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
    commands: Vec<String>,
    opens: u32,
    fail_put: bool,
    stale_helper: bool,
    oversized_readback: bool,
}
struct Fake(Arc<Mutex<Phone>>);
impl Opener for Fake {
    fn paired(&self) -> yeskia_core::Result<Vec<Paired>> {
        Ok(vec![Paired {
            key: "private-routing-address".into(),
            name: "laboratory candidate".into(),
            class_of_device: 0x5a020c,
        }])
    }
    fn open(&self, _: &str, ctx: &OperationContext) -> yeskia_core::Result<Session> {
        self.0.lock().unwrap().opens += 1;
        Session::open(
            Box::new(Stream {
                phone: self.0.clone(),
                rx: ok("rshell ready").into(),
                tx: Vec::new(),
                put: None,
                closed: false,
            }),
            ctx,
        )
    }
}
struct Stream {
    phone: Arc<Mutex<Phone>>,
    rx: VecDeque<u8>,
    tx: Vec<u8>,
    put: Option<(String, usize, Vec<u8>)>,
    closed: bool,
}
impl Stream {
    fn frame(&mut self, b: Vec<u8>) {
        if let Some((path, len, mut data)) = self.put.take() {
            data.extend(b);
            if data.len() == len {
                self.phone.lock().unwrap().files.insert(path, data);
                self.rx.extend(ok("wrote"));
            } else {
                self.put = Some((path, len, data));
            }
            return;
        }
        let line = String::from_utf8(b).unwrap();
        let (verb, arg) = line.split_once(' ').unwrap_or((&line, ""));
        let path = arg.replace('\\', "/");
        let path = if path.len() > 3 {
            path.trim_end_matches('/').to_string()
        } else {
            path
        };
        let mut p = self.phone.lock().unwrap();
        p.commands.push(line.clone());
        let wire = match verb {
            "ping" => ok("pong"),
            "pwd" => ok("Z:\\"),
            "hal" | "ps" | "cenrep" => ok("synthetic value"),
            "stat" => {
                if let Some(d) = p.files.get(&path) {
                    ok(&format!(
                        "{arg}  {} bytes  2026-01-01 00:00:00  att 0x20",
                        if p.oversized_readback && p.commands.iter().any(|c| c.starts_with("put "))
                        {
                            65 * 1024 * 1024
                        } else {
                            d.len()
                        }
                    ))
                } else if p.dirs.contains(&path) {
                    ok(&format!(
                        "{arg}  0 bytes  2026-01-01 00:00:00  att 0x10  <dir>"
                    ))
                } else {
                    err("stat failed NotFound")
                }
            }
            "ls" => {
                let prefix = format!("{}/", path.trim_end_matches('/'));
                let mut names = BTreeSet::new();
                for name in p.files.keys().chain(p.dirs.iter()) {
                    if let Some(rest) = name.strip_prefix(&prefix) {
                        if !rest.is_empty() && !rest.contains('/') {
                            names.insert(format!(
                                "{rest}{}",
                                if p.dirs.contains(name) { "\\" } else { "" }
                            ));
                        }
                    }
                }
                let data = names.into_iter().collect::<Vec<_>>().join("\n") + "\n";
                if data == "\n" {
                    ok("0 entries")
                } else {
                    [
                        framed(&[b".".as_slice(), data.as_bytes()].concat()),
                        ok("entries"),
                    ]
                    .concat()
                }
            }
            "cat" => {
                let mut words = arg.rsplitn(3, ' ');
                let n = words.next().unwrap().parse::<usize>().unwrap();
                let off = words.next().unwrap().parse::<usize>().unwrap();
                let name = words.next().unwrap().replace('\\', "/");
                let d = p.files.get(&name).unwrap();
                [
                    framed(&[b".".as_slice(), &d[off..(off + n).min(d.len())]].concat()),
                    ok("range"),
                ]
                .concat()
            }
            "put" => {
                let (name, len) = arg.rsplit_once(' ').unwrap();
                let len = len.parse::<usize>().unwrap();
                let name = name.replace('\\', "/");
                if len == 0 {
                    p.files.insert(name, Vec::new());
                    ok("wrote empty")
                } else {
                    self.put = Some((name, len, Vec::new()));
                    ok(&format!("send {len} bytes"))
                }
            }
            "mkdir" => {
                p.dirs.insert(path);
                ok("created")
            }
            "rm" => {
                p.files.remove(&path);
                ok("removed")
            }
            "exec" => {
                if path == "C:/sys/bin/yeskiaagent.exe" && !p.stale_helper {
                    let request = yeskia_agent_protocol::decode_request(
                        p.files.get("C:/Data/_yeskia/request.json").unwrap(),
                    )
                    .unwrap();
                    struct Api;
                    impl yeskia_agent_protocol::SystemApi for Api {
                        fn apps(
                            &mut self,
                            _: u32,
                            _: u16,
                        ) -> Result<(Vec<yeskia_agent_protocol::AppRecord>, Option<u32>), i32>
                        {
                            Ok((
                                vec![yeskia_agent_protocol::AppRecord {
                                    uid: 0xE0000001,
                                    caption: "Synthetic app".into(),
                                    caption_truncated: false,
                                }],
                                None,
                            ))
                        }
                    }
                    let response =
                        yeskia_agent_protocol::handle(&request, &mut Api, "synthetic-helper-build");
                    p.files.insert(
                        "C:/Data/_yeskia/response.json".into(),
                        yeskia_agent_protocol::encode_response(&response).unwrap(),
                    );
                }
                ok("spawned")
            }
            "reboot" => ok("rebooting"),
            "quit" => ok("bye"),
            _ => err("unknown verb"),
        };
        drop(p);
        self.rx.extend(wire);
    }
}
impl Channel for Stream {
    fn read(&mut self, b: &mut [u8], _: Duration) -> yeskia_core::Result<usize> {
        if self.closed {
            return Ok(0);
        }
        let n = b.len().min(997).min(self.rx.len());
        if n == 0 {
            return Err(Error::Timeout);
        }
        for v in &mut b[..n] {
            *v = self.rx.pop_front().unwrap();
        }
        Ok(n)
    }
    fn write(&mut self, b: &[u8], _: Duration) -> yeskia_core::Result<usize> {
        if self.closed {
            return Ok(0);
        }
        if self.put.is_some() && self.phone.lock().unwrap().fail_put {
            self.closed = true;
            return Err(Error::DeviceDisconnected);
        }
        let n = b.len().min(4093);
        self.tx.extend(&b[..n]);
        loop {
            if self.tx.len() < 4 {
                break;
            }
            let len = u32::from_be_bytes(self.tx[..4].try_into().unwrap()) as usize;
            if self.tx.len() < 4 + len {
                break;
            }
            let data = self.tx[4..4 + len].to_vec();
            self.tx.drain(..4 + len);
            self.frame(data);
        }
        Ok(n)
    }
    fn close(&mut self) {
        self.closed = true;
    }
}
fn setup() -> (AdbianTransport, Arc<Mutex<Phone>>, DeviceId) {
    let p = Arc::new(Mutex::new(Phone {
        dirs: ["C:/", "C:/Data", "E:/"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        files: BTreeMap::from([
            ("C:/Data/a.bin".into(), b"test".to_vec()),
            ("E:/large.bin".into(), vec![9; MAX_PUT + 127]),
        ]),
        ..Default::default()
    }));
    let a = AdbianTransport::new(
        Arc::new(Fake(p.clone())),
        "installation-specific-salt".into(),
    );
    let id = a.discover().unwrap()[0].id.clone();
    a.connect(&id).unwrap();
    (a, p, id)
}
#[test]
fn discovery_probe_and_roots_do_not_invent_identity_or_write() {
    let (a, p, id) = setup();
    let d = a.discover().unwrap().remove(0);
    assert!(!format!("{d:?}").contains("private-routing-address"));
    assert!(d.identified_model.is_none());
    assert_eq!(a.probe(&id, &ctx()).unwrap().len(), 1);
    let roots = a.list(&id, "/", &ctx()).unwrap();
    assert_eq!(
        roots.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["C:", "E:"]
    );
    let list = a.list(&id, "C:/Data", &ctx()).unwrap();
    assert_eq!(list[0].size, 4);
    assert!(!p
        .lock()
        .unwrap()
        .commands
        .iter()
        .any(|s| s.starts_with("put") || s.starts_with("exec") || s.starts_with("rm")));
    assert!(a.identity(&id, &ctx()).is_err());
}
#[test]
fn file_provider_streams_large_read_and_blocks_system_write_even_when_lab_armed() {
    let (a, _p, id) = setup();
    let mut bytes = Vec::new();
    assert_eq!(
        a.read_to(&id, "E:/large.bin", &mut bytes, &ctx()).unwrap(),
        (MAX_PUT + 127) as u64
    );
    assert!(bytes.iter().all(|b| *b == 9));
    a.set_lab(&id, true, true).unwrap();
    for path in ["Z:/system.bin", "C:/sys/bin/test.exe", "E:/private/test"] {
        assert!(a
            .write(&id, path, &[1], OverwritePolicy::Never, &ctx())
            .is_err());
    }
}
#[test]
fn lab_is_per_connection_confirmed_and_disarmed_on_reconnect() {
    let (a, p, id) = setup();
    assert!(a
        .agent_command(&id, "exec C:\\sys\\bin\\test.exe", true, &ctx())
        .is_err());
    assert!(a.set_lab(&id, true, false).is_err());
    a.set_lab(&id, true, true).unwrap();
    assert!(a
        .agent_command(&id, "exec C:\\sys\\bin\\test.exe", false, &ctx())
        .is_err());
    assert!(
        a.agent_command(&id, "exec C:\\sys\\bin\\test.exe", true, &ctx())
            .unwrap()
            .ok
    );
    a.disconnect(&id).unwrap();
    a.connect(&id).unwrap();
    assert!(a
        .agent_command(&id, "rm C:\\Data\\a.bin", true, &ctx())
        .is_err());
    assert_eq!(p.lock().unwrap().opens, 2);
    a.set_lab(&id, true, true).unwrap();
    a.disarm_all();
    assert!(a.agent_command(&id, "hal", false, &ctx()).is_err());
}
#[test]
fn engine_upload_readback_works_and_disconnect_is_uncertain_without_replay() {
    let (a, p, id) = setup();
    a.disconnect(&id).unwrap();
    let a = Arc::new(a);
    let engine = Engine::new(
        vec![a.clone()],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    let connected = engine.connect(&id).unwrap();
    assert_eq!(
        connected
            .matrix
            .unwrap()
            .availability(Operation::FilesWrite),
        Availability::NotTested
    );
    let result = engine
        .run(
            &id,
            OperationRequest::FilesWrite {
                path: "C:/Data/new.bin".into(),
                data: vec![1, 2, 3],
                overwrite: OverwritePolicy::Never,
            },
        )
        .unwrap();
    assert_eq!(result.outcome, Outcome::Success);
    assert!(matches!(
        result.data,
        Some(OperationData::Written { verified: true, .. })
    ));
    p.lock().unwrap().fail_put = true;
    let r = engine
        .run(
            &id,
            OperationRequest::FilesWrite {
                path: "C:/Data/private-test.bin".into(),
                data: vec![8; 9999],
                overwrite: OverwritePolicy::Never,
            },
        )
        .unwrap();
    assert!(r.uncertain);
    assert_ne!(r.outcome, Outcome::Success);
    assert_eq!(
        p.lock()
            .unwrap()
            .commands
            .iter()
            .filter(|s| s.starts_with("put C:\\Data\\private-test.bin"))
            .count(),
        1
    );
    let report = serde_json_text(&engine);
    assert!(!report.contains("private-test"));
    assert!(!report.contains("private-routing-address"));
}
fn serde_json_text(engine: &Engine) -> String {
    format!(
        "{:?} {:?}",
        engine.storage().journal_recent(20).unwrap(),
        engine.diagnostics().snapshot()
    )
}
#[test]
fn standard_write_collision_and_mkdir_readback() {
    let (a, p, id) = setup();
    assert!(matches!(
        a.write(
            &id,
            "C:/Data/a.bin",
            b"replace",
            OverwritePolicy::Never,
            &ctx()
        ),
        Err(Error::AlreadyExists)
    ));
    a.create_dir(&id, "C:/Data/new", &ctx()).unwrap();
    assert!(p.lock().unwrap().dirs.contains("C:/Data/new"));
    assert!(a
        .write_from(
            &id,
            "C:/Data/too-big",
            &mut std::io::empty(),
            MAX_PUT as u64 + 1,
            OverwritePolicy::Never,
            &ctx()
        )
        .is_err());
}

#[test]
fn optional_helper_negotiates_only_implemented_services_and_checks_fresh_request_id() {
    let (a, p, id) = setup();
    a.set_lab(&id, true, true).unwrap();
    assert!(matches!(
        a.agent_query(
            &id,
            &yeskia_agent_protocol::Query::Capabilities,
            true,
            &ctx()
        ),
        Err(Error::CapabilityUnsupported(_))
    ));
    p.lock()
        .unwrap()
        .files
        .insert("C:/sys/bin/yeskiaagent.exe".into(), vec![0]);
    let reply = a
        .agent_query(
            &id,
            &yeskia_agent_protocol::Query::Capabilities,
            true,
            &ctx(),
        )
        .unwrap();
    assert_eq!(reply.agent_version, "0.1.0");
    assert!(matches!(
        reply.data,
        yeskia_agent_protocol::Data::Capabilities {
            contacts: false,
            calendar: false,
            sms: false,
            ..
        }
    ));
    let reply = a
        .agent_query(
            &id,
            &yeskia_agent_protocol::Query::AppsList {
                offset: 0,
                limit: 32,
            },
            true,
            &ctx(),
        )
        .unwrap();
    assert!(matches!(
        reply.data,
        yeskia_agent_protocol::Data::Apps { .. }
    ));
    p.lock().unwrap().stale_helper = true;
    let short = OperationContext::new(
        "stale-helper".into(),
        CancellationToken::new(),
        Instant::now() + Duration::from_millis(50),
        Box::new(|| true),
        Arc::new(ProgressCell::default()),
    );
    assert!(matches!(
        a.agent_query(
            &id,
            &yeskia_agent_protocol::Query::Capabilities,
            true,
            &short
        ),
        Err(Error::Timeout)
    ));
    assert_eq!(
        p.lock()
            .unwrap()
            .commands
            .iter()
            .filter(|s| s.starts_with("exec C:\\sys\\bin\\yeskiaagent.exe"))
            .count(),
        3,
        "each explicit request launches once; a stale response never causes replay"
    );
}

#[test]
fn a_readback_limit_after_writing_is_uncertain_not_a_preflight_rejection() {
    let (a, p, id) = setup();
    a.disconnect(&id).unwrap();
    p.lock().unwrap().oversized_readback = true;
    let engine = Engine::new(
        vec![Arc::new(a)],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    engine.connect(&id).unwrap();
    let result = engine
        .run(
            &id,
            OperationRequest::FilesWrite {
                path: "C:/Data/changed.bin".into(),
                data: vec![1],
                overwrite: OverwritePolicy::Never,
            },
        )
        .unwrap();
    assert_eq!(result.outcome, Outcome::Failed);
    assert!(result.uncertain);
    assert_eq!(
        p.lock()
            .unwrap()
            .commands
            .iter()
            .filter(|c| c.starts_with("put "))
            .count(),
        1
    );
}

#[test]
fn passive_discovery_never_opens_audio_peripherals_or_phones_and_names_do_not_override_class() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Mixed {
        opened: AtomicUsize,
    }
    impl Opener for Mixed {
        fn paired(&self) -> yeskia_core::Result<Vec<Paired>> {
            Ok(vec![
                Paired {
                    key: "audio".into(),
                    name: "Nokia E72".into(),
                    class_of_device: 0x240404,
                },
                Paired {
                    key: "computer".into(),
                    name: "Nokia Phone".into(),
                    class_of_device: 0x0104,
                },
                Paired {
                    key: "unknown".into(),
                    name: "E72".into(),
                    class_of_device: 0,
                },
                Paired {
                    key: "phone".into(),
                    name: "owner phone".into(),
                    class_of_device: 0x5a020c,
                },
            ])
        }
        fn open(&self, _key: &str, _ctx: &OperationContext) -> yeskia_core::Result<Session> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            Err(Error::Transport("synthetic explicit request".into()))
        }
    }
    let opener = Arc::new(Mixed {
        opened: AtomicUsize::new(0),
    });
    let transport = Arc::new(AdbianTransport::new(
        opener.clone(),
        "synthetic-salt".into(),
    ));
    let engine = Engine::new(
        vec![transport],
        Arc::new(Storage::open_in_memory().unwrap()),
        Arc::new(Diagnostics::new(100, None)),
    );
    for _ in 0..5 {
        let found = engine.list_devices().unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].display_name.contains("owner phone"));
        assert_eq!(found[0].connection, ConnectionState::Disconnected);
    }
    assert_eq!(opener.opened.load(Ordering::SeqCst), 0);
    let id = engine.list_devices().unwrap()[0].id.clone();
    assert!(engine.connect(&id).is_err());
    assert_eq!(opener.opened.load(Ordering::SeqCst), 1);
}
