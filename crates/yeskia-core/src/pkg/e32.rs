//! Заголовок образа E32 (Symbian EXE/DLL): CPU, ABI, ядро, возможности (capabilities).
//!
//! Разобранный заголовок — не утверждение, что загрузчик прошивки примет образ. Импорты не
//! декодируются (`imports: None`): отсутствие данных не означает отсутствие зависимостей.

use super::sis::uid_checksum;
use serde::Serialize;

const CRC_INITIALISER: u32 = 0xC90F_DAA2;

pub const CAPABILITIES: [&str; 20] = [
    "TCB",
    "CommDD",
    "PowerMgmt",
    "MultimediaDD",
    "ReadDeviceData",
    "WriteDeviceData",
    "DRM",
    "TrustedUI",
    "ProtServ",
    "DiskAdmin",
    "NetworkControl",
    "AllFiles",
    "SwEvent",
    "NetworkServices",
    "LocalServices",
    "ReadUserData",
    "WriteUserData",
    "Location",
    "SurroundingsDD",
    "UserEnvironment",
];

/// Возможности, которые пользователь может выдать при установке (самоподписанный пакет).
/// Остальные требуют сертификата, которому доверяет телефон.
pub const USER_GRANTABLE: [&str; 6] = [
    "NetworkServices",
    "LocalServices",
    "ReadUserData",
    "WriteUserData",
    "Location",
    "UserEnvironment",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct E32Info {
    pub uids: [String; 3],
    pub uid3: u32,
    pub uid_checksum_ok: bool,
    pub is_dll: bool,
    pub header_format: String,
    pub abi: String,
    /// `eka1` | `eka2` | `unknown(n)`
    pub kernel: String,
    pub cpu: String,
    pub compression: String,
    pub code_size: u32,
    pub data_size: u32,
    pub header_crc_ok: Option<bool>,
    pub secure_id: Option<String>,
    pub vendor_id: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub restricted_capabilities: Option<Vec<String>>,
    pub dll_ref_count: u32,
    /// `Some([])` — заголовок не объявляет импортов; `None` — таблица не декодируется.
    pub imports: Option<Vec<String>>,
    pub imports_note: String,
}

pub fn is_e32(data: &[u8]) -> bool {
    data.len() >= 24 && &data[16..20] == b"EPOC"
}

fn header_crc(data: &[u8], header_size: usize) -> u32 {
    let mut crc: u32 = 0;
    for (i, &b) in data[..header_size].iter().enumerate() {
        let byte = if (20..24).contains(&i) {
            ((CRC_INITIALISER >> ((i - 20) * 8)) & 0xFF) as u8
        } else {
            b
        };
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

pub fn analyze(data: &[u8]) -> Result<E32Info, String> {
    if !is_e32(data) {
        return Err("not an E32 image (missing 'EPOC' signature at offset 16)".into());
    }
    if data.len() < 128 {
        return Err("truncated E32 header".into());
    }
    let u = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let h = |o: usize| u16::from_le_bytes(data[o..o + 2].try_into().unwrap());
    let flags = u(44);
    let hdr_fmt = (flags >> 24) & 0xF;
    let eptype = (flags >> 5) & 0x7;
    let abi = (flags >> 3) & 0x3;
    let comp = u(28);
    let cpu_raw = h(122);
    let header_size = u(100) as usize;
    let header_crc_ok = (header_size >= 24 && header_size <= data.len())
        .then(|| u(20) == header_crc(data, header_size));
    let (mut secure_id, mut vendor_id, mut caps, mut restricted) = (None, None, None, None);
    if hdr_fmt == 2 && data.len() >= 148 {
        secure_id = Some(format!("{:#010x}", u(128)));
        vendor_id = Some(format!("{:#010x}", u(132)));
        let bits = u(136) as u64 | ((u(140) as u64) << 32);
        let names: Vec<String> = CAPABILITIES
            .iter()
            .enumerate()
            .filter(|(i, _)| bits >> i & 1 == 1)
            .map(|(_, n)| n.to_string())
            .collect();
        restricted = Some(
            names
                .iter()
                .filter(|n| !USER_GRANTABLE.contains(&n.as_str()))
                .cloned()
                .collect(),
        );
        caps = Some(names);
    }
    Ok(E32Info {
        uids: [
            format!("{:#010x}", u(0)),
            format!("{:#010x}", u(4)),
            format!("{:#010x}", u(8)),
        ],
        uid3: u(8),
        uid_checksum_ok: u(12) == uid_checksum(&data[..12]).map_err(|e| e.0)?,
        is_dll: flags & 1 == 1,
        header_format: match hdr_fmt {
            0 => "original".into(),
            1 => "J".into(),
            2 => "V".into(),
            n => format!("unknown({n})"),
        },
        abi: match abi {
            0 => "gcc98r2".into(),
            1 => "eabi".into(),
            n => format!("unknown({n})"),
        },
        kernel: match eptype {
            0 => "eka1".into(),
            1 => "eka2".into(),
            n => format!("unknown({n})"),
        },
        cpu: match cpu_raw {
            0 => "unknown".into(),
            0x1000 => "x86".into(),
            0x2000 => "armv4".into(),
            0x2001 => "armv5".into(),
            0x2002 => "armv6".into(),
            0x4000 => "mcore".into(),
            n => format!("unknown({n:#x})"),
        },
        compression: match comp {
            0 => "none".into(),
            0x101F_7AFC => "deflate".into(),
            0x1028_22AA => "bytepair".into(),
            n => format!("unknown({n:#x})"),
        },
        code_size: u(48),
        data_size: u(52),
        header_crc_ok,
        secure_id,
        vendor_id,
        capabilities: caps,
        restricted_capabilities: restricted,
        dll_ref_count: u(84),
        imports: (comp == 0 && u(84) == 0).then(Vec::new),
        imports_note: if comp == 0 && u(84) == 0 {
            "no imported DLLs declared in the header".into()
        } else {
            "import table is not decoded in this version; dependency on firmware DLLs is unknown"
                .into()
        },
    })
}
