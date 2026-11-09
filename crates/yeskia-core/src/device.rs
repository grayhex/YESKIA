//! Модель `Device / Connection / Capability / Evidence`.
//!
//! Заявленная модель телефона НЕ подтверждает функциональность: каждая
//! возможность имеет собственный статус и список доказательств.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Откуда взято устройство. `Mock` всегда явно помечается в UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceOrigin {
    Mock,
    Real,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Mock,
    MassStorage,
    Mtp,
    PcSuite,
    Bluetooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// Уровень подтверждения возможности (см. docs/STATUS.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportStatus {
    Unsupported,
    Implemented,
    TestedOnMock,
    TestedOnHost,
    VerifiedOnDevice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    Identify,
    FileBrowse,
    FileRead,
    FileWrite,
    Backup,
    PimRead,
    PackageInstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Автотест на mock-устройстве.
    MockTest,
    /// Проверка на хосте (Windows API, реальные файлы), без телефона.
    HostTest,
    /// Реальное устройство + ручной/аппаратный прогон.
    DeviceRun,
    /// Реальный захват трафика/лог.
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub summary: String,
    /// Ссылка на артефакт (путь к трассе, id теста). Без персональных данных.
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub id: CapabilityId,
    pub status: SupportStatus,
    pub evidence: Vec<Evidence>,
}

impl Capability {
    pub fn new(id: CapabilityId, status: SupportStatus, evidence: Vec<Evidence>) -> Self {
        Self {
            id,
            status,
            evidence,
        }
    }

    /// Возможность можно вызывать, только если она не `Unsupported`.
    pub fn is_available(&self) -> bool {
        self.status != SupportStatus::Unsupported
    }
}

/// Описание устройства. Серийные номера/IMEI хранятся как `Sensitive`
/// и не попадают ни в логи, ни в `Debug`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub display_name: String,
    pub origin: DeviceOrigin,
    pub transport: TransportKind,
    pub connection: ConnectionState,
    pub capabilities: Vec<Capability>,
    pub serial: Option<Sensitive>,
}

impl Device {
    pub fn capability(&self, id: CapabilityId) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.id == id)
    }
}

/// Чувствительная строка: сериализуется для UI как маска, `Debug` скрывает значение.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Sensitive(String);

impl Sensitive {
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }
    /// Явный доступ к значению (только для транспорта, не для логов).
    pub fn reveal(&self) -> &str {
        &self.0
    }
    pub fn masked(&self) -> String {
        crate::diagnostics::mask_identifier(&self.0)
    }
}

impl std::fmt::Debug for Sensitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sensitive({})", self.masked())
    }
}

impl Serialize for Sensitive {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.masked())
    }
}
