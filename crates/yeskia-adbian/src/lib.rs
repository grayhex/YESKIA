//! Native host adapter. Paired endpoints are candidates, never evidence of a model or service.
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use yeskia_core::{
    adbian::{Command, Reply, Session, MAX_PUT},
    device::*,
    operations::*,
    transport::*,
    Error, Result,
};

#[cfg(windows)]
mod windows;

/// Transport-private routing key (e.g. MAC); never derives Debug or serializes.
#[derive(Clone)]
pub struct Paired {
    pub key: String,
    pub name: String,
}
pub trait Opener: Send + Sync {
    fn paired(&self) -> Result<Vec<Paired>>;
    /// Windows performs SPP SDP for the named rshell service, then validated cached/scan fallback.
    fn open(&self, key: &str, ctx: &OperationContext) -> Result<Session>;
}
struct Connection {
    session: Mutex<Session>,
    live: AtomicBool,
    armed: AtomicBool,
}
pub struct AdbianTransport {
    opener: Arc<dyn Opener>,
    salt: String,
    endpoints: Mutex<HashMap<DeviceId, Paired>>,
    connections: Mutex<HashMap<DeviceId, Arc<Connection>>>,
    connecting: Mutex<HashMap<DeviceId, CancellationToken>>,
}
fn locked() -> Error {
    Error::Internal("ADBIAN_STATE_UNAVAILABLE".into())
}
fn refused() -> Error {
    Error::InvalidArgument("LAB_ARM_AND_CONFIRM_REQUIRED".into())
}
fn ctx_connect() -> OperationContext {
    OperationContext::new(
        "agent-connect".into(),
        CancellationToken::new(),
        Instant::now() + Duration::from_secs(90),
        Box::new(|| true),
        Arc::new(ProgressCell::default()),
    )
}
impl AdbianTransport {
    pub fn new(opener: Arc<dyn Opener>, installation_salt: String) -> Self {
        Self {
            opener,
            salt: installation_salt,
            endpoints: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
            connecting: Mutex::new(HashMap::new()),
        }
    }
    pub fn system(salt: String) -> Self {
        #[cfg(windows)]
        {
            Self::new(Arc::new(windows::WindowsBluetooth::default()), salt)
        }
        #[cfg(not(windows))]
        {
            Self::new(Arc::new(Unsupported), salt)
        }
    }
    fn id(&self, key: &str) -> DeviceId {
        let hash = Sha256::digest(format!("{}|adbian|{}", self.salt, key).as_bytes());
        DeviceId::new(format!(
            "adbian-{}",
            hash[..16]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ))
    }
    fn connection(&self, id: &DeviceId) -> Result<Arc<Connection>> {
        self.connections
            .lock()
            .map_err(|_| locked())?
            .get(id)
            .filter(|c| c.live.load(Ordering::SeqCst))
            .cloned()
            .ok_or(Error::NotConnected)
    }
    fn with<T>(
        &self,
        id: &DeviceId,
        ctx: &OperationContext,
        f: impl FnOnce(&mut Session) -> Result<T>,
    ) -> Result<T> {
        let c = self.connection(id)?;
        // The Engine serializes phone operations. Refuse an accidental competing caller.
        let mut session = c
            .session
            .try_lock()
            .map_err(|_| Error::OperationFailed("ADBIAN_BUSY".into()))?;
        ctx.checkpoint()?;
        let r = f(&mut session);
        if matches!(
            r,
            Err(Error::Cancelled | Error::Timeout | Error::DeviceDisconnected)
        ) {
            session.close();
        }
        if !session.healthy() {
            c.live.store(false, Ordering::SeqCst);
            c.armed.store(false, Ordering::SeqCst);
        }
        r
    }
    pub fn set_lab(
        &self,
        id: &DeviceId,
        armed: bool,
        acknowledge_experimental_e72: bool,
    ) -> Result<()> {
        let c = self.connection(id)?;
        if armed && !acknowledge_experimental_e72 {
            return Err(refused());
        }
        c.armed.store(armed, Ordering::SeqCst);
        Ok(())
    }
    pub fn cancel_connect(&self, id: &DeviceId) -> Result<()> {
        if let Some(token) = self.connecting.lock().map_err(|_| locked())?.get(id) {
            token.cancel();
        }
        Ok(())
    }
    pub fn disarm_all(&self) {
        if let Ok(cs) = self.connections.lock() {
            for c in cs.values() {
                c.armed.store(false, Ordering::SeqCst);
            }
        }
    }
    fn require_lab(&self, id: &DeviceId) -> Result<()> {
        if !self.connection(id)?.armed.load(Ordering::SeqCst) {
            return Err(refused());
        }
        Ok(())
    }
    fn writable(path: &str) -> Result<()> {
        let p = path.replace('\\', "/").to_ascii_lowercase();
        let user_c = p.starts_with("c:/data/") || p == "c:/data";
        let user_e = p.starts_with("e:/")
            && !["sys", "private", "system", "resource"]
                .iter()
                .any(|s| p == format!("e:/{s}") || p.starts_with(&format!("e:/{s}/")));
        if user_c || user_e {
            Ok(())
        } else {
            Err(Error::CapabilityUnsupported(
                "ADBIAN_USE_CONFIRMED_LAB_FOR_SYSTEM_WRITE".into(),
            ))
        }
    }
    fn entry(path: &str, stat: yeskia_core::adbian::Stat) -> FileEntry {
        FileEntry {
            name: path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(path)
                .into(),
            is_dir: stat.is_dir,
            size: stat.bytes,
        }
    }
    fn put_checked(
        session: &mut Session,
        path: &str,
        data: &[u8],
        ctx: &OperationContext,
    ) -> Result<(u64, String)> {
        let hash = yeskia_core::fsutil::sha256_hex(data);
        session.put(path, data, ctx)?;
        let mut back = Vec::new();
        session.read_to(path, &mut back, ctx).map_err(|e| {
            if e.is_precondition() {
                Error::VerificationFailed
            } else {
                e
            }
        })?;
        if yeskia_core::fsutil::sha256_hex(&back) != hash {
            return Err(Error::VerificationFailed);
        }
        Ok((data.len() as u64, hash))
    }
}
impl DiscoveryProvider for AdbianTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Bluetooth
    }
    fn discover(&self) -> Result<Vec<Device>> {
        let paired = self.opener.paired()?;
        let mut endpoints = self.endpoints.lock().map_err(|_| locked())?;
        let mut devices = Vec::new();
        for p in paired {
            let id = self.id(&p.key);
            devices.push(Device {
                id: id.clone(),
                display_name: format!("Bluetooth · {}", p.name),
                origin: DeviceOrigin::Real,
                transport: TransportKind::Bluetooth,
                connection: ConnectionState::Disconnected,
                serial: None,
                identified_model: None,
                matrix: None,
            });
            endpoints.insert(id, p);
        }
        // Keep private routes for existing connections, but never return vanished candidates.
        Ok(devices)
    }
}
impl DeviceTransport for AdbianTransport {
    fn connect(&self, id: &DeviceId) -> Result<()> {
        if self.is_present(id) {
            return Err(Error::AlreadyConnected);
        }
        let endpoint = self
            .endpoints
            .lock()
            .map_err(|_| locked())?
            .get(id)
            .cloned()
            .ok_or_else(|| Error::DeviceNotFound("ADBIAN_CANDIDATE".into()))?;
        let ctx = ctx_connect();
        {
            let mut connecting = self.connecting.lock().map_err(|_| locked())?;
            if connecting.contains_key(id) {
                return Err(Error::AlreadyConnected);
            }
            connecting.insert(id.clone(), ctx.token().clone());
        }
        let opened = self.opener.open(&endpoint.key, &ctx);
        self.connecting.lock().map_err(|_| locked())?.remove(id);
        let session = opened?;
        ctx.checkpoint()?;
        self.connections.lock().map_err(|_| locked())?.insert(
            id.clone(),
            Arc::new(Connection {
                session: Mutex::new(session),
                live: AtomicBool::new(true),
                armed: AtomicBool::new(false),
            }),
        );
        Ok(())
    }
    fn disconnect(&self, id: &DeviceId) -> Result<()> {
        self.cancel_connect(id)?;
        if let Some(c) = self.connections.lock().map_err(|_| locked())?.remove(id) {
            c.live.store(false, Ordering::SeqCst);
            c.armed.store(false, Ordering::SeqCst);
            if let Ok(mut s) = c.session.try_lock() {
                s.close();
            }
        }
        Ok(())
    }
    fn is_present(&self, id: &DeviceId) -> bool {
        self.connection(id).is_ok()
    }
    fn maturity(&self, op: Operation) -> SupportStatus {
        match op {
            Operation::AgentStop
            | Operation::FilesList
            | Operation::FilesRead
            | Operation::FilesWrite
            | Operation::FilesMkdir
            | Operation::FilesDelete
            | Operation::PackagesTransfer
            | Operation::DiagnosticsRead
            | Operation::SystemRead
            | Operation::ProcessExec
            | Operation::DeviceReset => SupportStatus::Implemented,
            _ => SupportStatus::Unsupported,
        }
    }
    fn evidence_source(&self) -> &'static str {
        "adbian-agent"
    }
    fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
        self.with(id, ctx, |s| {
            let pong = s.text_command("ping", ctx)?;
            if pong.text != "pong" {
                return Err(Error::Transport("ADBIAN_PING_MISMATCH".into()));
            }
            Ok(())
        })?;
        // A heartbeat proves the service only. File roots/read/write/PIM remain untested.
        Ok(vec![ProbeOutcome {
            operation: Operation::DiagnosticsRead,
            availability: Availability::Available,
            detail: Some("ADBIAN_PING_ONLY".into()),
        }])
    }
    fn identity(&self, _id: &DeviceId, _ctx: &OperationContext) -> Result<Identity> {
        Err(Error::CapabilityUnsupported(
            "ADBIAN_NO_MODEL_COMMAND".into(),
        ))
    }
    fn files(&self) -> Option<&dyn FileProvider> {
        Some(self)
    }
    fn agent_query(
        &self,
        id: &DeviceId,
        query: &yeskia_agent_protocol::Query,
        confirm: bool,
        ctx: &OperationContext,
    ) -> Result<yeskia_agent_protocol::Response> {
        self.require_lab(id)?;
        if !confirm {
            return Err(refused());
        }
        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce).map_err(|_| locked())?;
        let request = yeskia_agent_protocol::Request {
            protocol: yeskia_agent_protocol::PROTOCOL,
            request_id: nonce.iter().map(|b| format!("{b:02x}")).collect(),
            query: query.clone(),
        };
        if !request.valid() {
            return Err(Error::InvalidArgument("YESKIA_AGENT_QUERY".into()));
        }
        self.with(id, ctx, |s| {
            self.require_lab(id)?;
            if s.stat("C:/sys/bin/yeskiaagent.exe", ctx)?.is_none() {
                return Err(Error::CapabilityUnsupported(
                    "YESKIA_HELPER_NOT_INSTALLED".into(),
                ));
            }
            if s.stat("C:/Data/_yeskia", ctx)?.is_none() {
                s.text_command("mkdir C:\\Data\\_yeskia", ctx)?;
            }
            let bytes = serde_json::to_vec(&request).map_err(|_| locked())?;
            Self::put_checked(s, "C:/Data/_yeskia/request.json", &bytes, ctx)?;
            s.text_command("exec C:\\sys\\bin\\yeskiaagent.exe", ctx)?;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                ctx.checkpoint()?;
                if Instant::now() >= deadline {
                    return Err(Error::Timeout);
                }
                if let Some(st) = s.stat("C:/Data/_yeskia/response.json", ctx)? {
                    if st.bytes > yeskia_agent_protocol::MAX_DOCUMENT as u64 {
                        return Err(Error::LimitExceeded("YESKIA_AGENT_DOCUMENT".into()));
                    }
                    let mut reply = Vec::new();
                    s.read_to("C:/Data/_yeskia/response.json", &mut reply, ctx)?;
                    if let Ok(response) =
                        yeskia_agent_protocol::decode_response(&reply, &request.request_id, query)
                    {
                        return Ok(response);
                    }
                    // A stale response is not evidence for this invocation. Never relaunch the helper.
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    }
    fn agent_ping(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Reply> {
        self.with(id, ctx, |s| {
            s.ping(ctx)?;
            Ok(Reply {
                ok: true,
                text: "pong".into(),
                data: Vec::new(),
            })
        })
    }
    fn agent_command(
        &self,
        id: &DeviceId,
        line: &str,
        confirm: bool,
        ctx: &OperationContext,
    ) -> Result<Reply> {
        self.require_lab(id)?;
        let cmd = Command::parse(line)?;
        if cmd.changes_device() && !confirm {
            return Err(refused());
        }
        self.with(id, ctx, |s| {
            self.require_lab(id)?;
            s.command(&cmd, ctx)
        })
    }
    fn agent_put(
        &self,
        id: &DeviceId,
        source: &str,
        path: &str,
        confirm: bool,
        ctx: &OperationContext,
    ) -> Result<(u64, String)> {
        self.require_lab(id)?;
        if !confirm {
            return Err(refused());
        }
        yeskia_core::fsutil::validate_host_path(std::path::Path::new(source))?;
        let f =
            std::fs::File::open(source).map_err(|_| Error::HostAccess("ADBIAN_SOURCE".into()))?;
        let mut data = Vec::new();
        f.take(MAX_PUT as u64 + 1).read_to_end(&mut data)?;
        if data.len() > MAX_PUT {
            return Err(Error::LimitExceeded("ADBIAN_PUT_1_MIB".into()));
        }
        self.with(id, ctx, |s| {
            self.require_lab(id)?;
            if s.stat(path, ctx)?.is_some() {
                return Err(Error::AlreadyExists);
            }
            Self::put_checked(s, path, &data, ctx)
        })
    }
}
impl FileProvider for AdbianTransport {
    fn list(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<FileEntry>> {
        self.with(id, ctx, |s| {
            if path == "/" {
                let mut roots = Vec::new();
                for drive in ["C:/", "E:/", "Z:/"] {
                    match s.stat(drive, ctx) {
                        Ok(Some(st)) if st.is_dir => roots.push(FileEntry {
                            name: drive.trim_end_matches('/').into(),
                            is_dir: true,
                            size: 0,
                        }),
                        Ok(_) | Err(Error::OperationFailed(_)) => {}
                        Err(e) => return Err(e),
                    }
                }
                return Ok(roots);
            }
            if !s.stat(path, ctx)?.is_some_and(|st| st.is_dir) {
                return Err(Error::NotFoundOnSource);
            }
            let r = s.text_command(&format!("ls {}", path.replace('/', "\\")), ctx)?;
            let listing = std::str::from_utf8(&r.data)
                .map_err(|_| Error::Transport("ADBIAN_LIST_UTF8".into()))?;
            let mut entries = Vec::new();
            for name in listing.lines() {
                ctx.checkpoint()?;
                let name = name.trim_end_matches('\\');
                yeskia_core::fsutil::validate_file_name(name)?;
                if entries.len() >= 5000 {
                    return Err(Error::LimitExceeded("ADBIAN_DIRECTORY_LIMIT".into()));
                }
                let child = format!("{}/{}", path.trim_end_matches('/'), name);
                if let Some(st) = s.stat(&child, ctx)? {
                    entries.push(Self::entry(&child, st));
                }
            }
            Ok(entries)
        })
    }
    fn stat(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Option<FileEntry>> {
        self.with(id, ctx, |s| {
            Ok(s.stat(path, ctx)?.map(|st| Self::entry(path, st)))
        })
    }
    fn read(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<u8>> {
        self.with(id, ctx, |s| {
            if s.stat(path, ctx)?.ok_or(Error::NotFoundOnSource)?.bytes > 64 * 1024 * 1024 {
                return Err(Error::LimitExceeded("ADBIAN_USE_STREAMING_DOWNLOAD".into()));
            }
            let mut data = Vec::new();
            s.read_to(path, &mut data, ctx)?;
            Ok(data)
        })
    }
    fn read_to(
        &self,
        id: &DeviceId,
        path: &str,
        sink: &mut dyn Write,
        ctx: &OperationContext,
    ) -> Result<u64> {
        self.with(id, ctx, |s| s.read_to(path, sink, ctx))
    }
    fn write(
        &self,
        id: &DeviceId,
        path: &str,
        data: &[u8],
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        Self::writable(path)?;
        self.with(id, ctx, |s| {
            if overwrite == OverwritePolicy::Never && s.stat(path, ctx)?.is_some() {
                return Err(Error::AlreadyExists);
            }
            s.put(path, data, ctx)
        })
    }
    fn write_from(
        &self,
        id: &DeviceId,
        path: &str,
        source: &mut dyn Read,
        len: u64,
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        if len > MAX_PUT as u64 {
            return Err(Error::LimitExceeded("ADBIAN_PUT_1_MIB".into()));
        }
        let mut data = Vec::new();
        source.take(MAX_PUT as u64 + 1).read_to_end(&mut data)?;
        ctx.checkpoint()?;
        if data.len() as u64 != len {
            return Err(Error::VerificationFailed);
        }
        self.write(id, path, &data, overwrite, ctx)
    }
    fn create_dir(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<()> {
        Self::writable(path)?;
        self.with(id, ctx, |s| {
            if let Some(st) = s.stat(path, ctx)? {
                return if st.is_dir {
                    Ok(())
                } else {
                    Err(Error::AlreadyExists)
                };
            }
            s.text_command(&format!("mkdir {}", path.replace('/', "\\")), ctx)?;
            if !s.stat(path, ctx)?.is_some_and(|st| st.is_dir) {
                return Err(Error::VerificationFailed);
            }
            Ok(())
        })
    }
}
#[cfg(not(windows))]
struct Unsupported;
#[cfg(not(windows))]
impl Opener for Unsupported {
    fn paired(&self) -> Result<Vec<Paired>> {
        Ok(Vec::new())
    }
    fn open(&self, _: &str, _: &OperationContext) -> Result<Session> {
        Err(Error::CapabilityUnsupported(
            "ADBIAN_WINDOWS_BLUETOOTH".into(),
        ))
    }
}
