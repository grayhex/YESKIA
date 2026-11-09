//! Поддельная PnP-топология для тестов (без Windows и без телефона).

use crate::model::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct FakeTopology {
    pub snap: Snapshot,
    counter: u32,
    /// (usb instance id, путь «Хранилище/папка/файл», содержимое) — дерево файлов WPD.
    pub wpd_files: Vec<(String, String, Vec<u8>)>,
    /// Пауза между блоками чтения WPD (проверка отмены/отключения).
    pub read_delay_ms: u64,
}

impl FakeTopology {
    pub fn new() -> Self {
        let mut t = Self::default();
        t.snap.os = OsInfo {
            product: "Windows 11 Pro (fake)".into(),
            build: "26100".into(),
        };
        // корневой хаб, как в реальной системе
        t.snap.nodes.push(PnpNode {
            instance_id: "USB\\ROOT_HUB30\\4&FAKE&0&0".into(),
            class: Some("USB".into()),
            service: Some("USBHUB3".into()),
            hardware_ids: vec!["USB\\ROOT_HUB30".into()],
            ..Default::default()
        });
        t
    }

    /// USB-устройство `USB\VID_xxxx&PID_xxxx\<serial>`; возвращает instance id.
    pub fn usb(&mut self, vid: u16, pid: u16, serial: &str, service: &str) -> String {
        let id = format!("USB\\VID_{vid:04X}&PID_{pid:04X}\\{serial}");
        self.snap.nodes.push(PnpNode {
            instance_id: id.clone(),
            parent_id: Some("USB\\ROOT_HUB30\\4&FAKE&0&0".into()),
            class: Some("USB".into()),
            service: Some(service.into()),
            hardware_ids: vec![format!("USB\\VID_{vid:04X}&PID_{pid:04X}")],
            description: Some("fake usb device".into()),
            ..Default::default()
        });
        id
    }

    pub fn problem(&mut self, instance_id: &str, code: u32) {
        for n in &mut self.snap.nodes {
            if n.instance_id == instance_id {
                n.problem_code = code;
            }
        }
    }

    /// Интерфейс составного устройства: `USB\VID_xxxx&PID_xxxx&MI_xx\...`.
    pub fn interface(&mut self, parent: &str, mi: u8) -> String {
        let vidpid = parent.split('\\').nth(1).unwrap_or("VID_0000&PID_0000");
        let id = format!("USB\\{vidpid}&MI_{mi:02X}\\7&FAKE&0&{mi:04X}");
        self.snap.nodes.push(PnpNode {
            instance_id: id.clone(),
            parent_id: Some(parent.to_string()),
            class: Some("USB".into()),
            hardware_ids: vec![format!("USB\\{vidpid}&MI_{mi:02X}")],
            ..Default::default()
        });
        id
    }

    /// WPD-узел в PnP-дереве + WPD-устройство с ссылкой на USB-родителя.
    pub fn wpd(&mut self, usb_instance_id: &str, model: &str, listing: WpdListing) {
        self.counter += 1;
        self.snap.nodes.push(PnpNode {
            instance_id: format!("WPDBUSENUM\\FAKE&{}", self.counter),
            parent_id: Some(usb_instance_id.to_string()),
            class: Some("WPD".into()),
            ..Default::default()
        });
        let path = format!(
            "\\\\?\\{}#{{6ac27878-a6fa-4155-ba85-f98f491d4f33}}",
            usb_instance_id.replace('\\', "#").to_ascii_lowercase()
        );
        self.snap.wpd.push(WpdDevice {
            device_id: path,
            manufacturer: Some("Fake".into()),
            model: Some(model.into()),
            firmware: Some("1.0".into()),
            listing,
        });
    }

    /// Файл в дереве WPD устройства: `path` — «Phone memory/DCIM/img.jpg».
    pub fn wpd_file(&mut self, usb_instance_id: &str, path: &str, data: &[u8]) {
        self.wpd_files.push((
            usb_instance_id.to_ascii_lowercase(),
            path.to_string(),
            data.to_vec(),
        ));
    }

    pub fn com(&mut self, usb_instance_id: &str, name: &str, busy: Option<bool>) {
        self.counter += 1;
        let node = format!("USB\\FAKE_PORT\\{}", self.counter);
        self.snap.nodes.push(PnpNode {
            instance_id: node.clone(),
            parent_id: Some(usb_instance_id.to_string()),
            class: Some("Ports".into()),
            ..Default::default()
        });
        self.snap.com_ports.push(ComPort {
            port_name: name.into(),
            node_instance_id: node,
            busy,
        });
    }

    pub fn disk(&mut self, usb_instance_id: &str, letter: char) {
        self.counter += 1;
        let node = format!("USBSTOR\\DISK&FAKE\\{}", self.counter);
        self.snap.nodes.push(PnpNode {
            instance_id: node.clone(),
            parent_id: Some(usb_instance_id.to_string()),
            class: Some("DiskDrive".into()),
            ..Default::default()
        });
        self.snap.volumes.push(Volume {
            letter,
            removable: true,
            node_instance_id: Some(node),
        });
    }

    pub fn remove(&mut self, usb_instance_id: &str) {
        let gone: Vec<String> = self
            .snap
            .nodes
            .iter()
            .filter(|n| {
                n.instance_id == usb_instance_id || n.parent_id.as_deref() == Some(usb_instance_id)
            })
            .map(|n| n.instance_id.clone())
            .collect();
        self.snap.nodes.retain(|n| !gone.contains(&n.instance_id));
        let needle = usb_instance_id.replace('\\', "#").to_ascii_lowercase();
        self.snap
            .wpd
            .retain(|w| !w.device_id.to_ascii_lowercase().contains(&needle));
        self.snap
            .com_ports
            .retain(|p| !gone.contains(&p.node_instance_id));
        self.snap.volumes.retain(|v| {
            v.node_instance_id
                .as_ref()
                .map_or(true, |id| !gone.contains(id))
        });
    }
}

/// Изменяемый источник снимка для тестов отключения/переподключения.
pub struct FakeSource {
    pub topology: Mutex<FakeTopology>,
}

impl FakeSource {
    pub fn new(t: FakeTopology) -> Self {
        Self {
            topology: Mutex::new(t),
        }
    }
    pub fn with<R>(&self, f: impl FnOnce(&mut FakeTopology) -> R) -> R {
        f(&mut self.topology.lock().unwrap())
    }
}

impl SnapshotSource for FakeSource {
    fn collect(&self, options: &CollectOptions) -> Result<Snapshot, CollectError> {
        let mut snap = self.topology.lock().unwrap().snap.clone();
        if !options.wpd_listing {
            for w in &mut snap.wpd {
                w.listing = WpdListing::NotProbed;
            }
        }
        if !options.probe_com_busy {
            for p in &mut snap.com_ports {
                p.busy = None;
            }
        }
        Ok(snap)
    }

    fn wpd_identity(&self, wpd_device_id: &str) -> Result<WpdIdentity, CollectError> {
        let t = self.topology.lock().unwrap();
        t.snap
            .wpd
            .iter()
            .find(|w| w.device_id == wpd_device_id)
            .map(|w| WpdIdentity {
                manufacturer: w.manufacturer.clone(),
                model: w.model.clone(),
                firmware: w.firmware.clone(),
            })
            .ok_or(CollectError {
                stage: "wpd_identity".into(),
                code: Some(0x8007_0490),
            })
    }

    fn wpd_list(
        &self,
        wpd_device_id: &str,
        path: &[String],
    ) -> Result<Vec<WpdEntry>, CollectError> {
        let t = self.topology.lock().unwrap();
        let usb = wpd_id_to_instance(wpd_device_id).ok_or(CollectError {
            stage: "wpd_list".into(),
            code: Some(0x8007_0490),
        })?;
        let mut out: Vec<WpdEntry> = Vec::new();
        for (u, p, data) in &t.wpd_files {
            if *u != usb {
                continue;
            }
            let comps: Vec<&str> = p.split('/').collect();
            if comps.len() <= path.len() || !comps.iter().zip(path).all(|(a, b)| *a == b.as_str()) {
                continue;
            }
            let name = comps[path.len()].to_string();
            let is_dir = comps.len() > path.len() + 1;
            if !out.iter().any(|e| e.name == name) {
                out.push(WpdEntry {
                    name,
                    is_dir,
                    size: if is_dir { 0 } else { data.len() as u64 },
                });
            }
        }
        if out.is_empty() && !path.is_empty() {
            return Err(CollectError {
                stage: "wpd_list_not_found".into(),
                code: Some(0x8007_0002),
            });
        }
        Ok(out)
    }

    fn wpd_read(
        &self,
        wpd_device_id: &str,
        path: &[String],
        sink: &mut dyn FnMut(&[u8]) -> Result<(), ()>,
    ) -> Result<(), CollectError> {
        let (data, delay) = {
            let t = self.topology.lock().unwrap();
            let usb = wpd_id_to_instance(wpd_device_id).ok_or(CollectError {
                stage: "wpd_read".into(),
                code: Some(0x8007_0490),
            })?;
            let want = path.join("/");
            let data = t
                .wpd_files
                .iter()
                .find(|(u, p, _)| *u == usb && *p == want)
                .map(|(_, _, d)| d.clone())
                .ok_or(CollectError {
                    stage: "wpd_read_not_found".into(),
                    code: Some(0x8007_0002),
                })?;
            (data, t.read_delay_ms)
        };
        let chunk = (data.len() / 10).max(1);
        for part in data.chunks(chunk) {
            if delay > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            sink(part).map_err(|_| CollectError {
                stage: "aborted".into(),
                code: None,
            })?;
        }
        Ok(())
    }
}
