//! Mock-устройство для воспроизводимых тестов (ядро и UI).
//! Все его данные явно помечены `DeviceOrigin::Mock`.

use crate::device::*;
use crate::error::{Error, Result};
use crate::operations::*;
use crate::transport::Transport;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

pub const MOCK_DEVICE_ID: &str = "mock-n95";

#[derive(Default)]
struct MockState {
    unplugged: bool,
    connected: HashSet<String>,
    /// Сколько записей реально зафиксировано (для проверки «нет повторной записи»).
    committed_writes: u64,
    /// Сколько раз запись была запущена.
    started_writes: u64,
    fail_next_write: bool,
    /// Количество шагов записи (больше — дольше можно отменять/«отключать»).
    write_steps: u32,
    step_delay_ms: u64,
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
        let state = MockState {
            write_steps: 5,
            ..Default::default()
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn st(&self) -> std::sync::MutexGuard<'_, MockState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Имитация вытаскивания кабеля.
    pub fn unplug(&self) {
        self.st().unplugged = true;
    }
    pub fn plug(&self) {
        self.st().unplugged = false;
    }
    pub fn fail_next_write(&self) {
        self.st().fail_next_write = true;
    }
    pub fn set_write_profile(&self, steps: u32, step_delay_ms: u64) {
        let mut s = self.st();
        s.write_steps = steps;
        s.step_delay_ms = step_delay_ms;
    }
    pub fn committed_writes(&self) -> u64 {
        self.st().committed_writes
    }
    pub fn started_writes(&self) -> u64 {
        self.st().started_writes
    }

    fn device(&self, connected: bool) -> Device {
        let ev = |s: &str| Evidence {
            kind: EvidenceKind::MockTest,
            summary: s.to_string(),
            reference: None,
        };
        Device {
            id: DeviceId::new(MOCK_DEVICE_ID),
            display_name: "Nokia N95 (MOCK)".into(),
            origin: DeviceOrigin::Mock,
            transport: TransportKind::Mock,
            connection: if connected {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            capabilities: vec![
                Capability::new(
                    CapabilityId::Identify,
                    SupportStatus::TestedOnMock,
                    vec![ev("identify на mock")],
                ),
                Capability::new(
                    CapabilityId::FileBrowse,
                    SupportStatus::TestedOnMock,
                    vec![ev("list_directory на mock")],
                ),
                Capability::new(
                    CapabilityId::FileWrite,
                    SupportStatus::TestedOnMock,
                    vec![ev("write_file на mock")],
                ),
                Capability::new(CapabilityId::PimRead, SupportStatus::Unsupported, vec![]),
            ],
            serial: Some(Sensitive::new("356938035643809")),
        }
    }
}

impl Transport for MockTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Mock
    }

    fn enumerate(&self) -> Result<Vec<Device>> {
        let s = self.st();
        if s.unplugged {
            return Ok(vec![]);
        }
        let connected = s.connected.contains(MOCK_DEVICE_ID);
        drop(s);
        Ok(vec![self.device(connected)])
    }

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

    fn execute(
        &self,
        id: &DeviceId,
        req: &OperationRequest,
        ctx: &OperationContext,
    ) -> Result<OperationData> {
        ctx.checkpoint()?;
        match req {
            OperationRequest::Identify => Ok(OperationData::Identity {
                model: "Nokia N95 (MOCK)".into(),
                firmware: Some("mock-1.0".into()),
            }),
            OperationRequest::ListDirectory { path } => {
                if path.contains("..") {
                    return Err(Error::InvalidArgument("путь не должен содержать ..".into()));
                }
                Ok(OperationData::Entries {
                    entries: vec![
                        FileEntry {
                            name: "Images".into(),
                            is_dir: true,
                            size: 0,
                        },
                        FileEntry {
                            name: "readme.txt".into(),
                            is_dir: false,
                            size: 12,
                        },
                    ],
                })
            }
            OperationRequest::WriteFile { size, .. } => {
                let (steps, delay, fail) = {
                    let mut s = self.st();
                    s.started_writes += 1;
                    let fail = std::mem::take(&mut s.fail_next_write);
                    (s.write_steps, s.step_delay_ms, fail)
                };
                for step in 0..steps {
                    ctx.checkpoint()?;
                    if delay > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                    }
                    if fail && step == steps / 2 {
                        return Err(Error::OperationFailed("mock: сбой записи".into()));
                    }
                }
                // Финальная проверка и атомарная фиксация.
                ctx.checkpoint()?;
                if !self.is_present(id) {
                    return Err(Error::DeviceDisconnected);
                }
                self.st().committed_writes += 1;
                Ok(OperationData::Written { bytes: *size })
            }
        }
    }
}
