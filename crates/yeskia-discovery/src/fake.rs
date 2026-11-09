//! Поддельная PnP-топология для тестов (без Windows и без телефона).

use crate::model::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct FakeTopology {
    pub snap: Snapshot,
    counter: u32,
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
}
