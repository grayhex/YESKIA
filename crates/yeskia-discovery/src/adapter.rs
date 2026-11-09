//! Адаптер Windows-обнаружения к контрактам `yeskia-core`: `DiscoveryProvider` + `DeviceTransport`.
//!
//! Только чтение. Предлагаются устройства с подсказкой вендора Nokia (VID `0421`); модель
//! **не** выводится из VID/PID — имя нейтральное, модель берётся из свойств WPD после `connect`.
//! Не-Nokia MTP-устройства видны Doctor'у, но в список YESKIA не попадают.

use crate::doctor::{analyze_with_refs, DeviceRef, DoctorState};
use crate::model::{CollectOptions, SnapshotSource};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::Arc;
use yeskia_core::device::*;
use yeskia_core::error::{Error, Result};
use yeskia_core::operations::{Operation, OperationContext};
use yeskia_core::transport::*;

pub struct PnpTransport {
    source: Arc<dyn SnapshotSource>,
    /// Ключ сессии: id устройства стабилен до перезапуска, но не раскрывает instance id.
    key: RandomState,
}

impl PnpTransport {
    pub fn new(source: Arc<dyn SnapshotSource>) -> Self {
        Self {
            source,
            key: RandomState::new(),
        }
    }

    fn id_for(&self, instance_id: &str) -> DeviceId {
        DeviceId::new(format!(
            "usb-{:016x}",
            self.key.hash_one(instance_id.to_ascii_lowercase())
        ))
    }

    fn refs(&self) -> Result<Vec<DeviceRef>> {
        let snap = self
            .source
            .collect(&CollectOptions::CHEAP)
            .map_err(|e| Error::Transport(format!("сбор PnP: {}", e.stage)))?;
        Ok(analyze_with_refs(&snap, 0)
            .1
            .into_iter()
            .filter(|r| r.nokia_vendor_hint)
            .collect())
    }

    fn find(&self, id: &DeviceId) -> Result<DeviceRef> {
        self.refs()?
            .into_iter()
            .find(|r| &self.id_for(&r.instance_id) == id)
            .ok_or_else(|| Error::DeviceNotFound(id.as_str().to_string()))
    }
}

fn kind_and_label(r: &DeviceRef) -> (TransportKind, &'static str) {
    match r.state {
        DoctorState::WpdOnly => (TransportKind::Mtp, "WPD/MTP"),
        DoctorState::ComPresent | DoctorState::ComBusy => (TransportKind::PcSuite, "COM"),
        DoctorState::MassStorage => (TransportKind::MassStorage, "Mass Storage"),
        DoctorState::DriverMissing => (TransportKind::Usb, "нет драйвера"),
        DoctorState::DriverError => (TransportKind::Usb, "ошибка драйвера"),
        DoctorState::UnknownMode => (TransportKind::Usb, "режим не определён"),
    }
}

impl DiscoveryProvider for PnpTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Usb
    }

    fn discover(&self) -> Result<Vec<Device>> {
        Ok(self
            .refs()?
            .into_iter()
            .map(|r| {
                let (transport, label) = kind_and_label(&r);
                Device {
                    id: self.id_for(&r.instance_id),
                    display_name: format!("USB {:04X}:{:04X} ({label})", r.vid, r.pid),
                    origin: DeviceOrigin::Real,
                    transport,
                    connection: ConnectionState::Disconnected,
                    serial: Some(Sensitive::new(r.instance_id)),
                    matrix: None,
                }
            })
            .collect())
    }
}

impl DeviceTransport for PnpTransport {
    fn connect(&self, id: &DeviceId) -> Result<()> {
        self.find(id).map(|_| ())
    }

    fn disconnect(&self, _id: &DeviceId) -> Result<()> {
        Ok(())
    }

    fn is_present(&self, id: &DeviceId) -> bool {
        self.find(id).is_ok()
    }

    fn maturity(&self, op: Operation) -> SupportStatus {
        match op {
            // Реализовано, но на реальном устройстве ещё не проверено.
            Operation::IdentityRead => SupportStatus::Implemented,
            _ => SupportStatus::Unsupported,
        }
    }

    fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        let outcome = match &r.wpd_device_id {
            None => ProbeOutcome {
                operation: Operation::IdentityRead,
                availability: Availability::Unsupported,
                detail: Some("в текущем режиме USB нет WPD-сервиса".into()),
            },
            Some(wpd) => match self.source.wpd_identity(wpd) {
                Ok(_) => ProbeOutcome {
                    operation: Operation::IdentityRead,
                    availability: Availability::Available,
                    detail: None,
                },
                Err(e) => ProbeOutcome {
                    operation: Operation::IdentityRead,
                    availability: Availability::Error,
                    detail: Some(format!(
                        "WPD: этап {}, код {:#010X}",
                        e.stage,
                        e.code.unwrap_or(0)
                    )),
                },
            },
        };
        Ok(vec![outcome])
    }

    fn identity(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Identity> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        let wpd = r
            .wpd_device_id
            .ok_or_else(|| Error::CapabilityUnsupported("identity.read".into()))?;
        let i = self
            .source
            .wpd_identity(&wpd)
            .map_err(|e| Error::Transport(format!("WPD: этап {}", e.stage)))?;
        Ok(Identity {
            model: i.model.unwrap_or_else(|| "модель не сообщена".to_string()),
            firmware: i.firmware,
        })
    }
}
