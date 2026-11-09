//! Модель снимка Windows PnP/WPD/COM. Содержит **чувствительные** поля (instance id, серийники
//! в путях), поэтому никогда не сериализуется наружу: в отчёт попадает только `DoctorReport`.

/// Узел PnP-дерева.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PnpNode {
    /// Например `USB\VID_0421&PID_0223\6&1A2B3C&0&2` — содержит серийник, наружу не уходит.
    pub instance_id: String,
    pub parent_id: Option<String>,
    pub class: Option<String>,
    pub hardware_ids: Vec<String>,
    pub service: Option<String>,
    pub description: Option<String>,
    /// CM_PROB_*; 0 — без проблем.
    pub problem_code: u32,
    pub driver_provider: Option<String>,
    pub driver_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComPort {
    /// `COM3`.
    pub port_name: String,
    /// PnP-узел порта (для связи с родительским USB-устройством).
    pub node_instance_id: String,
    /// `None` — занятость не проверялась (проверка кратковременно открывает порт).
    pub busy: Option<bool>,
}

/// Итог read-only листинга WPD (только чтение объектов верхнего уровня).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WpdListing {
    NotProbed,
    Ok { storages: Vec<StorageSummary> },
    Failed { code: u32 },
}

/// Только количество объектов: имена файлов не собираются.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSummary {
    pub object_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WpdDevice {
    /// WPD device id (путь вида `\\?\usb#vid_0421&pid_0223#...#{guid}`) — содержит instance id.
    pub device_id: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub listing: WpdListing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub letter: char,
    pub removable: bool,
    /// PnP-узел тома, если его удалось определить.
    pub node_instance_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OsInfo {
    pub product: String,
    pub build: String,
}

/// Ошибка этапа сбора: только этап и код (без текстов ОС, которые могут содержать пути).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectError {
    pub stage: String,
    pub code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub nodes: Vec<PnpNode>,
    pub com_ports: Vec<ComPort>,
    pub wpd: Vec<WpdDevice>,
    pub volumes: Vec<Volume>,
    pub os: OsInfo,
    pub errors: Vec<CollectError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectOptions {
    /// Открыть каждое WPD-устройство и прочитать контейнеры/объекты (без записи).
    pub wpd_listing: bool,
    /// Кратковременно открыть COM-порт, чтобы узнать, занят ли он (по умолчанию выключено).
    pub probe_com_busy: bool,
}

impl CollectOptions {
    /// Дешёвый опрос для обнаружения подключения/отключения.
    pub const CHEAP: CollectOptions = CollectOptions {
        wpd_listing: false,
        probe_com_busy: false,
    };
}

impl Default for CollectOptions {
    fn default() -> Self {
        Self {
            wpd_listing: true,
            probe_com_busy: false,
        }
    }
}

/// Источник снимка: Windows-реализация или поддельная топология в тестах.
pub trait SnapshotSource: Send + Sync {
    fn collect(&self, options: &CollectOptions) -> Result<Snapshot, CollectError>;

    /// Read-only чтение свойств WPD-устройства (производитель/модель/прошивка).
    fn wpd_identity(&self, wpd_device_id: &str) -> Result<WpdIdentity, CollectError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WpdIdentity {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
}

/// Приводит WPD device id к виду instance id: `\\?\usb#vid_0421&pid_0223#abc#{guid}` →
/// `usb\vid_0421&pid_0223\abc` (нижний регистр).
pub fn wpd_id_to_instance(wpd_device_id: &str) -> Option<String> {
    let s = wpd_device_id.strip_prefix("\\\\?\\")?;
    let cut = s.rfind("#{")?;
    Some(s[..cut].replace('#', "\\").to_ascii_lowercase())
}
