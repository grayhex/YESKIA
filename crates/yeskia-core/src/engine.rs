//! `Engine` — единая точка входа для UI: устройства, подключение, операции.
//!
//! Гарантии:
//! * на устройство одновременно выполняется не более одной операции;
//! * ядро никогда не повторяет операцию (особенно запись) автоматически;
//! * отключение устройства отменяет текущую операцию и переводит
//!   соединение в `Disconnected`;
//! * каждая операция попадает в журнал (`running` → итоговое состояние).

use crate::device::*;
use crate::diagnostics::Diagnostics;
use crate::error::{Error, Result};
use crate::operations::*;
use crate::storage::{JournalState, Storage};
use crate::transport::Transport;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

struct Running {
    operation_id: String,
    token: CancellationToken,
}

#[derive(Default)]
struct Inner {
    /// устройство → индекс транспорта
    connected: HashMap<DeviceId, usize>,
    running: HashMap<DeviceId, Running>,
}

pub struct Engine {
    transports: Vec<Arc<dyn Transport>>,
    inner: Mutex<Inner>,
    storage: Arc<Storage>,
    diag: Arc<Diagnostics>,
    counter: AtomicU64,
}

/// Снимает отметку «операция выполняется» при любом выходе (в т.ч. при панике).
struct RunningGuard<'a> {
    engine: &'a Engine,
    device: DeviceId,
}

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.engine.lock().running.remove(&self.device);
    }
}

fn capability_for(req: &OperationRequest) -> CapabilityId {
    match req {
        OperationRequest::Identify => CapabilityId::Identify,
        OperationRequest::ListDirectory { .. } => CapabilityId::FileBrowse,
        OperationRequest::WriteFile { .. } => CapabilityId::FileWrite,
    }
}

impl Engine {
    pub fn new(
        transports: Vec<Arc<dyn Transport>>,
        storage: Arc<Storage>,
        diag: Arc<Diagnostics>,
    ) -> Self {
        Self {
            transports,
            inner: Mutex::new(Inner::default()),
            storage,
            diag,
            counter: AtomicU64::new(1),
        }
    }

    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }
    pub fn diagnostics(&self) -> &Arc<Diagnostics> {
        &self.diag
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Список устройств; заодно обнаруживает пропавшие подключённые устройства.
    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let mut all = Vec::new();
        for t in &self.transports {
            all.extend(t.enumerate()?);
        }
        let mut inner = self.lock();
        let lost: Vec<DeviceId> = inner
            .connected
            .keys()
            .filter(|id| !all.iter().any(|d| &d.id == *id))
            .cloned()
            .collect();
        for id in lost {
            inner.connected.remove(&id);
            if let Some(r) = inner.running.get(&id) {
                r.token.cancel();
            }
            self.diag
                .warn("подключённое устройство пропало; соединение закрыто");
        }
        for d in &mut all {
            d.connection = if inner.connected.contains_key(&d.id) {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            };
        }
        Ok(all)
    }

    fn find(&self, id: &DeviceId) -> Result<(usize, Device)> {
        for (i, t) in self.transports.iter().enumerate() {
            if let Some(d) = t.enumerate()?.into_iter().find(|d| &d.id == id) {
                return Ok((i, d));
            }
        }
        Err(Error::DeviceNotFound(id.as_str().to_string()))
    }

    pub fn connect(&self, id: &DeviceId) -> Result<Device> {
        if self.lock().connected.contains_key(id) {
            return Err(Error::AlreadyConnected);
        }
        let (idx, mut device) = self.find(id)?;
        self.transports[idx].connect(id)?;
        self.lock().connected.insert(id.clone(), idx);
        device.connection = ConnectionState::Connected;
        self.diag
            .info(&format!("подключено: {}", device.display_name));
        Ok(device)
    }

    pub fn disconnect(&self, id: &DeviceId) -> Result<()> {
        let idx = {
            let mut inner = self.lock();
            let idx = inner.connected.remove(id).ok_or(Error::NotConnected)?;
            if let Some(r) = inner.running.get(id) {
                r.token.cancel();
            }
            idx
        };
        // Ошибка закрытия транспорта не должна оставлять соединение «висящим».
        if let Err(e) = self.transports[idx].disconnect(id) {
            self.diag.warn(&format!("ошибка закрытия транспорта: {e}"));
        }
        self.diag.info("устройство отключено");
        Ok(())
    }

    /// Отмена операции по id. Возвращает `false`, если такой нет.
    pub fn cancel(&self, operation_id: &str) -> bool {
        let inner = self.lock();
        match inner
            .running
            .values()
            .find(|r| r.operation_id == operation_id)
        {
            Some(r) => {
                r.token.cancel();
                true
            }
            None => false,
        }
    }

    /// Выполняет операцию синхронно (вызывать из блокирующего потока).
    /// `Err` — только нарушенные предусловия; сбои самой операции описаны в `OperationResult`.
    pub fn run(&self, id: &DeviceId, request: OperationRequest) -> Result<OperationResult> {
        let (idx, device) = self.find(id).map_err(|e| match e {
            Error::DeviceNotFound(_) if self.lock().connected.contains_key(id) => {
                self.lock().connected.remove(id);
                Error::DeviceDisconnected
            }
            other => other,
        })?;

        let cap = capability_for(&request);
        if !device
            .capability(cap)
            .map(|c| c.is_available())
            .unwrap_or(false)
        {
            return Err(Error::CapabilityUnsupported(format!("{cap:?}")));
        }

        let operation_id = format!("op-{}", self.counter.fetch_add(1, Ordering::SeqCst));
        let token = CancellationToken::new();
        {
            let mut inner = self.lock();
            if !inner.connected.contains_key(id) {
                return Err(Error::NotConnected);
            }
            if inner.running.contains_key(id) {
                return Err(Error::OperationInProgress);
            }
            inner.running.insert(
                id.clone(),
                Running {
                    operation_id: operation_id.clone(),
                    token: token.clone(),
                },
            );
        }
        let _guard = RunningGuard {
            engine: self,
            device: id.clone(),
        };

        let name = request.name();
        self.storage
            .journal_start(&operation_id, id.as_str(), name)?;
        self.diag.info(&format!("{operation_id}: {name} начата"));

        let transport = Arc::clone(&self.transports[idx]);
        let presence_t = Arc::clone(&transport);
        let presence_id = id.clone();
        let ctx = OperationContext::new(
            operation_id.clone(),
            token,
            Box::new(move || presence_t.is_present(&presence_id)),
        );

        // Единственная попытка: повторов нет.
        let exec = transport.execute(id, &request, &ctx);
        let exec = match exec {
            Err(Error::Transport(_)) | Err(Error::OperationFailed(_))
                if !transport.is_present(id) =>
            {
                Err(Error::DeviceDisconnected)
            }
            other => other,
        };

        if matches!(exec, Err(Error::DeviceDisconnected)) {
            self.lock().connected.remove(id);
        }

        let (outcome, data, error, jstate) = match exec {
            Ok(d) => (Outcome::Success, Some(d), None, JournalState::Success),
            Err(Error::Cancelled) => (
                Outcome::Cancelled,
                None,
                Some(Error::Cancelled.to_info()),
                JournalState::Cancelled,
            ),
            Err(e) => (
                Outcome::Failed,
                None,
                Some(e.to_info()),
                JournalState::Failed,
            ),
        };
        let code = error.as_ref().map(|e| format!("{:?}", e.code));
        if let Err(e) = self
            .storage
            .journal_finish(&operation_id, jstate, code.as_deref())
        {
            self.diag.error(&format!("журнал: {e}"));
        }
        self.diag
            .info(&format!("{operation_id}: {name} → {outcome:?}"));

        let evidence = if device.origin == DeviceOrigin::Mock {
            vec![Evidence {
                kind: EvidenceKind::MockTest,
                summary:
                    "Выполнено на mock-устройстве; не подтверждает работу на реальном телефоне"
                        .into(),
                reference: None,
            }]
        } else {
            vec![]
        };

        Ok(OperationResult {
            operation_id,
            device_id: id.clone(),
            operation: name.to_string(),
            origin: device.origin,
            outcome,
            data,
            error,
            evidence,
        })
    }
}
