//! Mock-устройство для воспроизводимых тестов (ядро и UI).
//! Все его данные явно помечены `DeviceOrigin::Mock`.

use crate::device::*;
use crate::error::{Error, Result};
use crate::operations::*;
use crate::transport::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub const MOCK_DEVICE_ID: &str = "mock-n95";
const DEFAULT_SERIAL: &str = "356938035643809";

/// Сценарий сбоя следующей записи.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WriteFault {
    #[default]
    None,
    /// Записать половину данных и оборвать связь (partial write).
    PartialThenFail,
    /// Данные «успешно» записаны, но искажены (ловится read-back).
    CorruptOnCommit,
}

struct MockState {
    model: String,
    serial: String,
    unplugged: bool,
    connected: HashSet<String>,
    fs: BTreeMap<String, Vec<u8>>,
    committed_writes: u64,
    started_writes: u64,
    write_fault: WriteFault,
    write_steps: u32,
    step_delay_ms: u64,
    /// Следующая операция «зависает» на N мс, игнорируя checkpoint (залипший драйвер).
    hang_next_ms: u64,
    maturity: HashMap<Operation, SupportStatus>,
    probe_errors: HashSet<Operation>,
    /// Подменить устройство в момент фиксации записи (смена телефона во время записи).
    swap_on_commit: Option<(String, String)>,
}

#[derive(Clone)]
pub struct MockTransport {
    state: Arc<Mutex<MockState>>,
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTransport {
    pub fn new() -> Self {
        let mut fs = BTreeMap::new();
        fs.insert("E:/readme.txt".to_string(), b"hello yeskia".to_vec());
        fs.insert(
            "E:/Images/cat.jpg".to_string(),
            vec![0xFF, 0xD8, 0xFF, 0xD9],
        );
        let state = MockState {
            model: "Nokia N95 (MOCK)".into(),
            serial: DEFAULT_SERIAL.into(),
            unplugged: false,
            connected: HashSet::new(),
            fs,
            committed_writes: 0,
            started_writes: 0,
            write_fault: WriteFault::None,
            write_steps: 5,
            step_delay_ms: 0,
            hang_next_ms: 0,
            maturity: HashMap::new(),
            probe_errors: HashSet::new(),
            swap_on_commit: None,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn st(&self) -> std::sync::MutexGuard<'_, MockState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // --- хуки для тестов ---
    /// Имитация вытаскивания кабеля.
    pub fn unplug(&self) {
        self.st().unplugged = true;
    }
    pub fn plug(&self) {
        self.st().unplugged = false;
    }
    pub fn set_write_fault(&self, f: WriteFault) {
        self.st().write_fault = f;
    }
    /// Совместимость: сбой следующей записи.
    pub fn fail_next_write(&self) {
        self.set_write_fault(WriteFault::PartialThenFail);
    }
    pub fn set_write_profile(&self, steps: u32, step_delay_ms: u64) {
        let mut s = self.st();
        s.write_steps = steps.max(1);
        s.step_delay_ms = step_delay_ms;
    }
    pub fn hang_next_op(&self, ms: u64) {
        self.st().hang_next_ms = ms;
    }
    /// Заменить телефон на другой на том же порту (меняется серийник/модель).
    pub fn replace_device(&self, model: &str, serial: &str) {
        let mut s = self.st();
        s.model = model.into();
        s.serial = serial.into();
    }
    pub fn swap_device_on_commit(&self, model: &str, serial: &str) {
        self.st().swap_on_commit = Some((model.into(), serial.into()));
    }
    pub fn set_model(&self, model: &str) {
        self.st().model = model.into();
    }
    pub fn set_maturity(&self, op: Operation, status: SupportStatus) {
        self.st().maturity.insert(op, status);
    }
    pub fn set_probe_error(&self, op: Operation) {
        self.st().probe_errors.insert(op);
    }
    pub fn committed_writes(&self) -> u64 {
        self.st().committed_writes
    }
    pub fn started_writes(&self) -> u64 {
        self.st().started_writes
    }
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.st().fs.get(path).cloned()
    }
    pub fn put_file(&self, path: &str, data: &[u8]) {
        self.st().fs.insert(path.into(), data.to_vec());
    }

    fn maybe_hang(&self) {
        let ms = std::mem::take(&mut self.st().hang_next_ms);
        if ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
    }

    fn device(&self, connected: bool) -> Device {
        let s = self.st();
        Device {
            id: DeviceId::new(MOCK_DEVICE_ID),
            display_name: s.model.clone(),
            origin: DeviceOrigin::Mock,
            transport: TransportKind::Mock,
            connection: if connected {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            serial: Some(Sensitive::new(s.serial.clone())),
            matrix: None,
        }
    }
}

impl DiscoveryProvider for MockTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Mock
    }

    fn discover(&self) -> Result<Vec<Device>> {
        let (unplugged, connected) = {
            let s = self.st();
            (s.unplugged, s.connected.contains(MOCK_DEVICE_ID))
        };
        if unplugged {
            return Ok(vec![]);
        }
        Ok(vec![self.device(connected)])
    }
}

impl DeviceTransport for MockTransport {
    fn connect(&self, id: &DeviceId) -> Result<()> {
        let mut s = self.st();
        if id.as_str() != MOCK_DEVICE_ID || s.unplugged {
            return Err(Error::DeviceNotFound(id.as_str().into()));
        }
        s.connected.insert(id.0.clone());
        Ok(())
    }

    fn disconnect(&self, id: &DeviceId) -> Result<()> {
        self.st().connected.remove(id.as_str());
        Ok(())
    }

    fn is_present(&self, id: &DeviceId) -> bool {
        id.as_str() == MOCK_DEVICE_ID && !self.st().unplugged
    }

    fn maturity(&self, op: Operation) -> SupportStatus {
        if let Some(m) = self.st().maturity.get(&op) {
            return *m;
        }
        match op {
            Operation::IdentityRead
            | Operation::FilesList
            | Operation::FilesRead
            | Operation::FilesWrite => SupportStatus::TestedOnMock,
            _ => SupportStatus::Unsupported,
        }
    }

    fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
        ctx.checkpoint()?;
        if !self.is_present(id) {
            return Err(Error::DeviceDisconnected);
        }
        let errors = self.st().probe_errors.clone();
        let mut out = Vec::new();
        // Только чтение: запись probe не проверяет и остаётся NotTested.
        for op in [
            Operation::IdentityRead,
            Operation::FilesList,
            Operation::FilesRead,
        ] {
            if errors.contains(&op) {
                out.push(ProbeOutcome {
                    operation: op,
                    availability: Availability::Error,
                    detail: Some("mock: probe завершился ошибкой".into()),
                });
            } else {
                out.push(ProbeOutcome {
                    operation: op,
                    availability: Availability::Available,
                    detail: None,
                });
            }
        }
        Ok(out)
    }

    fn identity(&self, _id: &DeviceId, ctx: &OperationContext) -> Result<Identity> {
        self.maybe_hang();
        ctx.checkpoint()?;
        Ok(Identity {
            model: self.st().model.clone(),
            firmware: Some("mock-1.0".into()),
        })
    }

    fn files(&self) -> Option<&dyn FileProvider> {
        Some(self)
    }
    fn pim(&self) -> Option<&dyn PimProvider> {
        Some(self)
    }
}

impl PimProvider for MockTransport {}

impl FileProvider for MockTransport {
    fn list(&self, _id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<FileEntry>> {
        self.maybe_hang();
        ctx.checkpoint()?;
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        let s = self.st();
        let mut dirs = BTreeMap::new();
        let mut files = Vec::new();
        for (p, data) in &s.fs {
            if let Some(rest) = p.strip_prefix(&prefix) {
                match rest.split_once('/') {
                    Some((dir, _)) => {
                        dirs.insert(dir.to_string(), ());
                    }
                    None => files.push(FileEntry {
                        name: rest.to_string(),
                        is_dir: false,
                        size: data.len() as u64,
                    }),
                }
            }
        }
        let mut out: Vec<FileEntry> = dirs
            .into_keys()
            .map(|name| FileEntry {
                name,
                is_dir: true,
                size: 0,
            })
            .collect();
        out.extend(files);
        Ok(out)
    }

    fn stat(
        &self,
        _id: &DeviceId,
        path: &str,
        ctx: &OperationContext,
    ) -> Result<Option<FileEntry>> {
        ctx.checkpoint()?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok(self.st().fs.get(path).map(|d| FileEntry {
            name,
            is_dir: false,
            size: d.len() as u64,
        }))
    }

    fn read(&self, _id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<u8>> {
        self.maybe_hang();
        ctx.checkpoint()?;
        self.st()
            .fs
            .get(path)
            .cloned()
            .ok_or_else(|| Error::OperationFailed("файл не найден".into()))
    }

    fn write(
        &self,
        _id: &DeviceId,
        path: &str,
        data: &[u8],
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        self.maybe_hang();
        ctx.checkpoint()?;
        let (steps, delay, fault) = {
            let mut s = self.st();
            if overwrite == OverwritePolicy::Never && s.fs.contains_key(path) {
                return Err(Error::AlreadyExists);
            }
            s.started_writes += 1;
            (
                s.write_steps,
                s.step_delay_ms,
                std::mem::take(&mut s.write_fault),
            )
        };
        for step in 0..steps {
            ctx.checkpoint()?;
            if delay > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            ctx.report_progress(step as u64 + 1, steps as u64);
            if fault == WriteFault::PartialThenFail && step == steps / 2 {
                // Не атомарное устройство: половина данных уже на носителе.
                self.st()
                    .fs
                    .insert(path.to_string(), data[..data.len() / 2].to_vec());
                return Err(Error::Transport("mock: обрыв записи".into()));
            }
        }
        // Атомарная фиксация в конце (если сюда дошли — ни отмены, ни таймаута, ни отключения).
        ctx.checkpoint()?;
        let mut stored = data.to_vec();
        if fault == WriteFault::CorruptOnCommit {
            if let Some(b) = stored.first_mut() {
                *b ^= 0xFF;
            }
        }
        let mut s = self.st();
        s.fs.insert(path.to_string(), stored);
        s.committed_writes += 1;
        if let Some((model, serial)) = s.swap_on_commit.take() {
            s.model = model;
            s.serial = serial;
        }
        Ok(())
    }
}
