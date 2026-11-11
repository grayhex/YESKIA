use std::{
    collections::VecDeque,
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use yeskia_core::{adbian::*, operations::*, Error};
fn ctx() -> OperationContext {
    OperationContext::new(
        "test".into(),
        CancellationToken::new(),
        Instant::now() + Duration::from_secs(20),
        Box::new(|| true),
        Arc::new(ProgressCell::default()),
    )
}
fn frame(b: &[u8]) -> Vec<u8> {
    let mut f = (b.len() as u32).to_be_bytes().to_vec();
    f.extend(b);
    f
}
fn ok(s: &str) -> Vec<u8> {
    frame(format!("+{s}").as_bytes())
}
#[derive(Default)]
struct Spy {
    sent: Mutex<Vec<u8>>,
    closed: AtomicBool,
}
struct Stream {
    input: VecDeque<u8>,
    spy: Arc<Spy>,
    step: usize,
}
impl Channel for Stream {
    fn read(&mut self, b: &mut [u8], _: Duration) -> yeskia_core::Result<usize> {
        let n = b.len().min(self.step).min(self.input.len());
        for v in &mut b[..n] {
            *v = self.input.pop_front().unwrap();
        }
        Ok(n)
    }
    fn write(&mut self, b: &[u8], _: Duration) -> yeskia_core::Result<usize> {
        let n = b.len().min(self.step);
        self.spy.sent.lock().unwrap().extend(&b[..n]);
        Ok(n)
    }
    fn close(&mut self) {
        self.spy.closed.store(true, Ordering::SeqCst);
    }
}
fn session(mut wire: Vec<u8>) -> (Session, Arc<Spy>) {
    let mut input = ok("rshell ready");
    input.append(&mut wire);
    let spy = Arc::new(Spy::default());
    (
        Session::open(
            Box::new(Stream {
                input: input.into(),
                spy: spy.clone(),
                step: 7,
            }),
            &ctx(),
        )
        .unwrap(),
        spy,
    )
}
fn sent(spy: &Spy) -> Vec<Vec<u8>> {
    let bytes = spy.sent.lock().unwrap();
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < bytes.len() {
        let n = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        out.push(bytes[pos + 4..pos + 4 + n].to_vec());
        pos += 4 + n;
    }
    out
}
fn stat(path: &str, n: u64) -> Vec<u8> {
    ok(&format!("{path}  {n} bytes  2026-01-01 00:00:00  att 0x20"))
}
#[test]
fn golden_fragmentation_binary_and_error() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/adbian/wire.json")).unwrap();
    let bytes = |key: &str| {
        fixture[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>()
    };
    assert_eq!(bytes("greeting"), ok("rshell ready"));
    let mut wire = bytes("pong");
    wire.extend(bytes("binary"));
    wire.extend(bytes("error"));
    let (mut s, spy) = session(wire);
    assert_eq!(s.text_command("ping", &ctx()).unwrap().text, "pong");
    let r = s.text_command("cat C:\\Data\\sample.bin", &ctx()).unwrap();
    assert_eq!(r.data, [0, 255, 128]);
    assert!(
        !s.command(&Command::parse("hal 5").unwrap(), &ctx())
            .unwrap()
            .ok
    );
    assert_eq!(sent(&spy)[0], b"ping");
    assert_eq!(bytes("ping"), frame(b"ping"));
    assert!(s.healthy());
}
#[test]
fn invalid_frames_eof_and_greeting_poison_connection() {
    for wire in [
        vec![0, 0, 0, 0],
        vec![0, 8, 0, 1],
        frame(b"?unknown"),
        vec![0, 0, 0, 5, b'+', b'o'],
        frame(&[b'+', 255]),
    ] {
        let (mut s, spy) = session(wire);
        assert!(s.text_command("ping", &ctx()).is_err());
        assert!(!s.healthy());
        assert!(spy.closed.load(Ordering::SeqCst));
        assert!(matches!(
            s.text_command("ping", &ctx()),
            Err(Error::NotConnected)
        ));
    }
    let spy = Arc::new(Spy::default());
    assert!(Session::open(
        Box::new(Stream {
            input: ok("different service").into(),
            spy: spy.clone(),
            step: 1
        }),
        &ctx()
    )
    .is_err());
    assert!(spy.closed.load(Ordering::SeqCst));
}
#[test]
fn chunked_read_over_one_megabyte_rechecks_revision_and_exact_ranges() {
    let n = MAX_PUT + 123;
    let path = "C:\\Data\\large file.bin";
    let data: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
    let mut wire = stat(path, n as u64);
    let mut offset = 0;
    while offset < n {
        let mut want = (n - offset).min(GET_CHUNK);
        if offset == want && want > 1 {
            want -= 1;
        }
        for chunk in data[offset..offset + want].chunks(32 * 1024) {
            let mut tagged = vec![b'.'];
            tagged.extend(chunk);
            wire.extend(frame(&tagged));
        }
        wire.extend(ok("range"));
        offset += want;
    }
    wire.extend(stat(path, n as u64));
    let (mut s, spy) = session(wire);
    let mut out = Vec::new();
    assert_eq!(s.read_to(path, &mut out, &ctx()).unwrap(), n as u64);
    assert_eq!(out, data);
    let commands = sent(&spy);
    assert_eq!(commands.first(), commands.last());
    assert!(commands
        .iter()
        .any(|v| v == b"cat C:\\Data\\large file.bin 262144 262143"));
}
#[test]
fn changed_metadata_and_short_range_never_count_as_completed_download() {
    let p = "C:\\Data\\x";
    for wire in [
        [stat(p, 1), frame(b".x"), ok("range"), stat(p, 2)].concat(),
        [stat(p, 2), frame(b".x"), ok("range")].concat(),
    ] {
        let (mut s, _) = session(wire);
        assert!(matches!(
            s.read_to(p, &mut Vec::new(), &ctx()),
            Err(Error::VerificationFailed)
        ));
    }
}
#[test]
fn put_chunk_limit_empty_final_reply_and_uncertain_disconnect() {
    let (mut s, spy) = session([ok("send 1048576 bytes"), ok("wrote")].concat());
    let data = vec![3; MAX_PUT];
    s.put("C:/Data/a.bin", &data, &ctx()).unwrap();
    let frames = sent(&spy);
    assert_eq!(frames[0], b"put C:\\Data\\a.bin 1048576");
    assert_eq!(frames[1..].concat(), data);
    assert!(frames.iter().all(|f| f.len() <= MAX_FRAME));
    let (mut s, spy) = session(ok("wrote 0 bytes"));
    s.put("C:/Data/empty", &[], &ctx()).unwrap();
    assert_eq!(sent(&spy).len(), 1);
    let (mut s, spy) = session(ok("send 1 bytes"));
    assert!(s.put("C:/Data/x", &[1], &ctx()).is_err());
    assert!(!s.healthy());
    assert_eq!(sent(&spy).len(), 2, "no automatic replay");
    let (mut s, spy) = session(Vec::new());
    assert!(matches!(
        s.put("C:/Data/x", &vec![0; MAX_PUT + 1], &ctx()),
        Err(Error::LimitExceeded(_))
    ));
    assert!(sent(&spy).is_empty());
    assert!(s.put("Z:/x", &[1], &ctx()).is_err());
}
#[test]
fn cancellation_closes_link_without_sending_more_payload() {
    let (mut s, spy) = session(ok("pong"));
    let ctx = ctx();
    ctx.token().cancel();
    assert!(matches!(
        s.text_command("ping", &ctx),
        Err(Error::Cancelled)
    ));
    assert!(sent(&spy).is_empty());
    assert!(spy.closed.load(Ordering::SeqCst));
}
#[test]
fn stat_absence_is_not_permission_failure_and_debug_is_private() {
    let (mut s, _) = session(
        [
            frame(b"-stat failed NotFound"),
            frame(b"-stat failed PermissionDenied"),
        ]
        .concat(),
    );
    assert!(s.stat("C:/Data/a", &ctx()).unwrap().is_none());
    assert!(s.stat("C:/Data/a", &ctx()).is_err());
    let cmd = Command::parse("exec C:\\private\\sensitive.exe").unwrap();
    assert!(!format!("{cmd:?}").contains("private"));
    let r = Reply {
        ok: true,
        text: "secret".into(),
        data: b"personal".to_vec(),
    };
    assert!(!format!("{r:?}").contains("secret"));
}
#[test]
fn actual_upstream_commands_only_and_risks() {
    for line in [
        "ping",
        "pwd",
        "cd C:\\Data",
        "ls C:\\",
        "stat C:\\Data",
        "cat C:\\Data\\a 0 4",
        "get E:\\x",
        "mkdir C:\\Data\\test",
        "find C:\\Data test",
        "grep C:\\Data hex:ff01",
        "ps 0xE0AA00F2 1",
        "cenrep 0x101f8766 1",
        "hal",
        "exec C:\\sys\\bin\\test.exe",
        "rm C:\\Data\\test",
        "reboot now",
        "quit",
    ] {
        assert!(Command::parse(line).is_ok(), "{line}");
    }
    for line in [
        "",
        "kill 1",
        "rename a b",
        "chmod a",
        "flash all",
        "reboot",
        "ping\nrm a",
        "put C:\\Data\\a 5",
        "ps",
        "hal nope",
    ] {
        assert!(Command::parse(line).is_err(), "{line}");
    }
    assert_eq!(
        Command::parse("exec C:\\sys\\bin\\x.exe").unwrap().risk(),
        Risk::Destructive
    );
    assert!(!Command::parse("ps 1 2").unwrap().changes_device());
}
#[test]
fn sink_failure_does_not_report_full_transfer() {
    struct Bad;
    impl Write for Bad {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("sink"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let (mut s, _) = session([stat("C:\\Data\\a", 1), frame(b".a"), ok("range")].concat());
    assert!(s.read_to("C:/Data/a", &mut Bad, &ctx()).is_err());
}

#[test]
fn heartbeat_checks_pong_and_quit_is_an_explicit_daemon_stop() {
    let (mut s, spy) = session(ok("not pong"));
    assert!(s.ping(&ctx()).is_err());
    assert!(spy.closed.load(Ordering::SeqCst));
    assert_eq!(Command::parse("quit").unwrap().risk(), Risk::Destructive);
    let request = OperationRequest::AgentCommand {
        line: "quit".into(),
        confirm: true,
    };
    assert_eq!(request.operation(), Operation::AgentStop);
    assert!(request.operation().is_write());
}
