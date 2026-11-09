//! Модель `Device / Connection / CapabilityMatrix / Evidence`.
//!
//! Матрица строится на **конкретном соединении** по результатам read-only probe и реальных
//! операций. Модель телефона и успешное USB-перечисление ничего не подтверждают.

use crate::operations::Operation;
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
    /// USB-устройство, режим работы не определён (до идентификации по протоколу).
    Usb,
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

/// Уровень зрелости/подтверждения возможности (см. docs/STATUS.md). Не путать с доступностью.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportStatus {
    Unsupported,
    Implemented,
    TestedOnMock,
    TestedOnHost,
    VerifiedOnDevice,
}

/// Доступность операции на этом соединении.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Available,
    /// Не проверялось (например, запись нельзя проверить read-only probe).
    NotTested,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Автотест на mock-устройстве.
    MockTest,
    /// Проверка на хосте (Windows API, реальные файлы), без телефона.
    HostTest,
    /// Реальное устройство: read-only probe или выполненная операция.
    DeviceRun,
    /// Реальный захват трафика/лог.
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// Кто/что дало подтверждение: `probe`, `operation:files.write`, `unit-test`…
    pub source: String,
    pub summary: String,
    pub checked_at_ms: u64,
    /// Отпечаток устройства, на котором получено подтверждение.
    pub device_fingerprint: Option<String>,
    /// Ссылка на артефакт (путь к трассе, id теста). Без персональных данных.
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub operation: Operation,
    pub availability: Availability,
    pub maturity: SupportStatus,
    pub detail: Option<String>,
    pub evidence: Vec<Evidence>,
}

/// Матрица возможностей одного соединения.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub device_fingerprint: String,
    pub probed_at_ms: u64,
    pub entries: Vec<CapabilityEntry>,
}

impl CapabilityMatrix {
    pub fn entry(&self, op: Operation) -> Option<&CapabilityEntry> {
        self.entries.iter().find(|e| e.operation == op)
    }
    pub fn availability(&self, op: Operation) -> Availability {
        self.entry(op)
            .map(|e| e.availability)
            .unwrap_or(Availability::NotTested)
    }
    pub fn available_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.availability == Availability::Available)
            .count()
    }
}

/// Описание устройства. Серийные номера/IMEI хранятся как `Sensitive`
/// и не попадают ни в логи, ни в `Debug`. `matrix` есть только у подключённого устройства.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub display_name: String,
    pub origin: DeviceOrigin,
    pub transport: TransportKind,
    pub connection: ConnectionState,
    pub serial: Option<Sensitive>,
    /// Модель, **подтверждённая идентификацией по протоколу** на этом соединении (не VID/PID и не
    /// имя в списке). `None` до подключения или если идентификация недоступна.
    pub identified_model: Option<String>,
    pub matrix: Option<CapabilityMatrix>,
}

/// Чувствительная строка: сериализуется для UI как маска, `Debug` скрывает значение.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Sensitive(String);

impl Sensitive {
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }
    /// Явный доступ к значению (только для транспорта и fingerprint, не для логов).
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

/// Односторонний отпечаток устройства с солью установки: по нему нельзя восстановить
/// серийник/IMEI перебором без соли.
pub fn fingerprint(salt: &str, device: &Device) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b"|");
    h.update(format!("{:?}", device.transport).as_bytes());
    h.update(b"|");
    h.update(device.id.as_str().as_bytes());
    h.update(b"|");
    h.update(device.display_name.as_bytes());
    h.update(b"|");
    h.update(
        device
            .serial
            .as_ref()
            .map(|s| s.reveal())
            .unwrap_or("")
            .as_bytes(),
    );
    let digest = h.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}
