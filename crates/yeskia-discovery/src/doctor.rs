//! Doctor: различает «видно USB» и «есть реально рабочий сервис».
//!
//! Отчёт (`DoctorReport`) privacy-safe: без IMEI, серийников, PnP instance id, MAC, имён файлов
//! и пользовательских friendly name. Только VID/PID, классы/сервисы драйверов, коды проблем и
//! счётчики. Идентичность устройства НЕ определяется по VID/PID: `identity_confirmed` всегда
//! `false` до подтверждения протоколом, а `nokia_vendor_hint` — лишь подсказка.

use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const NOKIA_VID: u16 = 0x0421;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorState {
    /// Драйвер не установлен (CM_PROB_FAILED_INSTALL / NOT_CONFIGURED).
    DriverMissing,
    /// Драйвер есть, но устройство в ошибке (иной ненулевой код проблемы).
    DriverError,
    /// Есть COM-порт, и он занят другим приложением.
    ComBusy,
    /// Есть COM-порт (занятость не проверялась или порт свободен).
    ComPresent,
    /// Только WPD/MTP: нет COM и нет буквы диска.
    WpdOnly,
    /// Только Mass Storage (том/диск).
    MassStorage,
    /// USB-устройство видно, но ни один известный сервис не обнаружен.
    UnknownMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolStatus {
    NotProbed,
    /// WPD вернул контейнеры/объекты (read-only листинг): аппаратный ответ по MTP получен.
    MtpResponding,
    MtpFailed,
}

/// MTP/WPD не равно PC Suite и PIM: эти уровни здесь не проверяются.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proof {
    NotProven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    NoUsbDevices,
    MultipleDevices,
    NotNokiaVendor,
    VendorHintOnly,
    DriverMissing,
    DriverError,
    ComPresent,
    ComBusy,
    WpdOnly,
    MassStorageOnly,
    UnknownMode,
    MtpListingOk,
    MtpListingFailed,
    MtpIsNotPcSuite,
    PartialData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverInfo {
    pub class: Option<String>,
    pub service: Option<String>,
    pub provider: Option<String>,
    pub version: Option<String>,
    pub problem_code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpdSummary {
    /// WPD-устройство связано с этим USB-родителем по instance id (а не по friendly name).
    pub associated_by_instance: bool,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub storages: Option<u32>,
    pub top_level_objects: Option<u32>,
    pub error_code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorDevice {
    /// `device-1`, `device-2` … — порядок по VID/PID; не связан с серийниками.
    pub alias: String,
    pub vid: String,
    pub pid: String,
    pub nokia_vendor_hint: bool,
    /// Всегда `false`: модель подтверждается только идентификацией по протоколу.
    pub identity_confirmed: bool,
    pub state: DoctorState,
    pub protocol: ProtocolStatus,
    pub pc_suite: Proof,
    pub pim: Proof,
    pub driver: DriverInfo,
    pub wpd: Option<WpdSummary>,
    pub com_ports: u32,
    pub com_busy: u32,
    pub volumes: u32,
    pub findings: Vec<FindingCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPlatform {
    pub os: String,
    pub build: String,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectIssue {
    pub stage: String,
    pub code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub schema: u32,
    pub generated_at_ms: u64,
    pub platform: ReportPlatform,
    pub devices: Vec<DoctorDevice>,
    pub findings: Vec<FindingCode>,
    pub issues: Vec<CollectIssue>,
    /// Заголовок для отчёта: какие данные НЕ собираются.
    pub excluded: Vec<String>,
}

/// Внутренняя связка alias → идентификаторы (чувствительно, наружу не отдаётся).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRef {
    pub alias: String,
    pub instance_id: String,
    pub wpd_device_id: Option<String>,
    pub vid: u16,
    pub pid: u16,
    pub state: DoctorState,
    pub nokia_vendor_hint: bool,
    /// Буквы томов, привязанных к этому устройству через PnP-цепочку (Mass Storage).
    pub volumes: Vec<char>,
}

fn enumerator(instance_id: &str) -> &str {
    instance_id.split('\\').next().unwrap_or("")
}

/// `USB\VID_0421&PID_0223\...` → (0x0421, 0x0223). Для интерфейсов (`&MI_`) — тоже, см. `is_interface`.
fn parse_vid_pid(ids: &[String]) -> Option<(u16, u16)> {
    for id in ids {
        let up = id.to_ascii_uppercase();
        let vid = up
            .find("VID_")
            .and_then(|i| u16::from_str_radix(up.get(i + 4..i + 8)?, 16).ok());
        let pid = up
            .find("PID_")
            .and_then(|i| u16::from_str_radix(up.get(i + 4..i + 8)?, 16).ok());
        if let (Some(v), Some(p)) = (vid, pid) {
            return Some((v, p));
        }
    }
    None
}

fn is_interface(n: &PnpNode) -> bool {
    n.instance_id.to_ascii_uppercase().contains("&MI_")
        || n.hardware_ids
            .iter()
            .any(|h| h.to_ascii_uppercase().contains("&MI_"))
}

fn is_hub(n: &PnpNode) -> bool {
    n.service
        .as_deref()
        .is_some_and(|s| s.to_ascii_lowercase().starts_with("usbhub"))
        || n.instance_id.to_ascii_uppercase().contains("ROOT_HUB")
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Анализирует снимок. Возвращает безопасный отчёт и внутренние ссылки на устройства.
pub fn analyze_with_refs(snap: &Snapshot, now_ms: u64) -> (DoctorReport, Vec<DeviceRef>) {
    let by_id: HashMap<String, &PnpNode> = snap
        .nodes
        .iter()
        .map(|n| (n.instance_id.to_ascii_lowercase(), n))
        .collect();
    let mut children: HashMap<String, Vec<&PnpNode>> = HashMap::new();
    for n in &snap.nodes {
        if let Some(p) = &n.parent_id {
            children.entry(p.to_ascii_lowercase()).or_default().push(n);
        }
    }

    // Кандидаты: USB-устройства (не интерфейсы MI_ и не хабы).
    let mut candidates: Vec<(&PnpNode, u16, u16)> = snap
        .nodes
        .iter()
        .filter(|n| eq_ci(enumerator(&n.instance_id), "USB") && !is_interface(n) && !is_hub(n))
        .filter_map(|n| parse_vid_pid(&n.hardware_ids).map(|(v, p)| (n, v, p)))
        .collect();
    candidates.sort_by(|a, b| (a.1, a.2, &a.0.instance_id).cmp(&(b.1, b.2, &b.0.instance_id)));

    let mut devices = Vec::new();
    let mut refs = Vec::new();

    for (idx, (node, vid, pid)) in candidates.iter().enumerate() {
        let alias = format!("device-{}", idx + 1);
        // Потомки (рекурсивно).
        let mut desc: Vec<&PnpNode> = vec![node];
        let mut seen: HashSet<String> = HashSet::new();
        let mut i = 0;
        while i < desc.len() {
            let key = desc[i].instance_id.to_ascii_lowercase();
            if seen.insert(key.clone()) {
                if let Some(ch) = children.get(&key) {
                    desc.extend(ch.iter().copied());
                }
            }
            i += 1;
        }
        let desc_ids: HashSet<String> = desc
            .iter()
            .map(|n| n.instance_id.to_ascii_lowercase())
            .collect();

        // Драйвер/проблемы: ищем среди устройства и потомков, приоритет — само устройство.
        let problem = desc
            .iter()
            .map(|n| n.problem_code)
            .find(|c| *c != 0)
            .unwrap_or(0);
        let driver_node = if node.problem_code != 0 {
            node
        } else {
            desc.iter()
                .find(|n| n.problem_code != 0)
                .copied()
                .unwrap_or(node)
        };
        let driver = DriverInfo {
            class: node.class.clone(),
            service: node.service.clone(),
            provider: node.driver_provider.clone(),
            version: node.driver_version.clone(),
            problem_code: problem,
        };
        let _ = driver_node;

        // COM-порты потомков.
        let ports: Vec<&ComPort> = snap
            .com_ports
            .iter()
            .filter(|p| desc_ids.contains(&p.node_instance_id.to_ascii_lowercase()))
            .collect();
        let com_busy = ports.iter().filter(|p| p.busy == Some(true)).count() as u32;

        // WPD: связь по instance id родителя (интерфейс MI_ → его родитель), а не по имени.
        let wpd_dev = snap.wpd.iter().find(|w| {
            wpd_id_to_instance(&w.device_id).is_some_and(|inst| {
                let mut cur = by_id.get(&inst).copied();
                // интерфейс составного устройства → его родитель
                if let Some(n) = cur {
                    if is_interface(n) {
                        cur = n
                            .parent_id
                            .as_ref()
                            .and_then(|p| by_id.get(&p.to_ascii_lowercase()).copied());
                    }
                }
                cur.is_some_and(|n| eq_ci(&n.instance_id, &node.instance_id))
                    || desc_ids.contains(&inst)
            })
        });
        let wpd_node_present = desc
            .iter()
            .any(|n| n.class.as_deref().is_some_and(|c| eq_ci(c, "WPD")));
        let wpd_present = wpd_dev.is_some() || wpd_node_present;

        // Тома/диски (USBSTOR, DiskDrive).
        let disk_child = desc.iter().any(|n| {
            eq_ci(enumerator(&n.instance_id), "USBSTOR")
                || n.class.as_deref().is_some_and(|c| eq_ci(c, "DiskDrive"))
        });
        let mut vol_letters: Vec<char> = snap
            .volumes
            .iter()
            .filter(|v| {
                v.removable
                    && v.node_instance_id
                        .as_ref()
                        .is_some_and(|id| desc_ids.contains(&id.to_ascii_lowercase()))
            })
            .map(|v| v.letter)
            .collect();
        vol_letters.sort_unstable();
        vol_letters.dedup();
        let vols = vol_letters.len() as u32;
        let mass = disk_child || vols > 0;

        // Состояние (приоритет: драйвер → COM → WPD → диск → неизвестно).
        let state = if problem == 28 || problem == 1 {
            DoctorState::DriverMissing
        } else if problem != 0 {
            DoctorState::DriverError
        } else if com_busy > 0 {
            DoctorState::ComBusy
        } else if !ports.is_empty() {
            DoctorState::ComPresent
        } else if wpd_present {
            DoctorState::WpdOnly
        } else if mass {
            DoctorState::MassStorage
        } else {
            DoctorState::UnknownMode
        };

        let (protocol, wpd_summary) = match wpd_dev {
            Some(w) => {
                let (proto, storages, objects, err) = match &w.listing {
                    WpdListing::NotProbed => (ProtocolStatus::NotProbed, None, None, None),
                    WpdListing::Ok { storages } => (
                        ProtocolStatus::MtpResponding,
                        Some(storages.len() as u32),
                        Some(storages.iter().map(|s| s.object_count).sum()),
                        None,
                    ),
                    WpdListing::Failed { code } => {
                        (ProtocolStatus::MtpFailed, None, None, Some(*code))
                    }
                };
                (
                    proto,
                    Some(WpdSummary {
                        associated_by_instance: true,
                        model: w.model.clone(),
                        firmware: w.firmware.clone(),
                        storages,
                        top_level_objects: objects,
                        error_code: err,
                    }),
                )
            }
            None => (ProtocolStatus::NotProbed, None),
        };

        let nokia = *vid == NOKIA_VID;
        let mut findings = Vec::new();
        findings.push(if nokia {
            FindingCode::VendorHintOnly
        } else {
            FindingCode::NotNokiaVendor
        });
        findings.push(match state {
            DoctorState::DriverMissing => FindingCode::DriverMissing,
            DoctorState::DriverError => FindingCode::DriverError,
            DoctorState::ComBusy => FindingCode::ComBusy,
            DoctorState::ComPresent => FindingCode::ComPresent,
            DoctorState::WpdOnly => FindingCode::WpdOnly,
            DoctorState::MassStorage => FindingCode::MassStorageOnly,
            DoctorState::UnknownMode => FindingCode::UnknownMode,
        });
        match protocol {
            ProtocolStatus::MtpResponding => findings.push(FindingCode::MtpListingOk),
            ProtocolStatus::MtpFailed => findings.push(FindingCode::MtpListingFailed),
            ProtocolStatus::NotProbed => {}
        }
        if wpd_present {
            findings.push(FindingCode::MtpIsNotPcSuite);
        }

        devices.push(DoctorDevice {
            alias: alias.clone(),
            vid: format!("{vid:04X}"),
            pid: format!("{pid:04X}"),
            nokia_vendor_hint: nokia,
            identity_confirmed: false,
            state,
            protocol,
            pc_suite: Proof::NotProven,
            pim: Proof::NotProven,
            driver,
            wpd: wpd_summary,
            com_ports: ports.len() as u32,
            com_busy,
            volumes: vols,
            findings,
        });
        refs.push(DeviceRef {
            alias,
            instance_id: node.instance_id.clone(),
            wpd_device_id: wpd_dev.map(|w| w.device_id.clone()),
            vid: *vid,
            pid: *pid,
            state,
            nokia_vendor_hint: nokia,
            volumes: vol_letters,
        });
    }

    let mut findings = Vec::new();
    if devices.is_empty() {
        findings.push(FindingCode::NoUsbDevices);
    }
    if devices.len() > 1 {
        findings.push(FindingCode::MultipleDevices);
    }
    if !snap.errors.is_empty() {
        findings.push(FindingCode::PartialData);
    }

    let report = DoctorReport {
        schema: 1,
        generated_at_ms: now_ms,
        platform: ReportPlatform {
            os: snap.os.product.clone(),
            build: snap.os.build.clone(),
            arch: std::env::consts::ARCH.to_string(),
        },
        devices,
        findings,
        issues: snap
            .errors
            .iter()
            .map(|e| CollectIssue {
                stage: e.stage.clone(),
                code: e.code,
            })
            .collect(),
        excluded: [
            "IMEI",
            "серийные номера",
            "PnP instance ID",
            "MAC-адреса",
            "имена и содержимое файлов",
            "пользовательские имена устройств",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    (report, refs)
}

pub fn analyze(snap: &Snapshot, now_ms: u64) -> DoctorReport {
    analyze_with_refs(snap, now_ms).0
}
