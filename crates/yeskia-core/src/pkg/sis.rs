//! Статический разбор SIS 9.x (SIS/SISX). Порт алгоритмической части `symport/sis`.
//!
//! Проверяются: контрольная сумма UID, CRC16 контроллера и данных (если поля есть), структура
//! контроллера (хранится или zlib), список файлов и SHA-1 каждого встроенного файла. Содержимое
//! пакета **не исполняется**. Условные блоки и вложенные SIS не раскрываются (только
//! перечисляются), цепочки подписей проверяются отдельно (`cert`). Это не замена установке на
//! телефоне.

use serde::Serialize;
use sha1::{Digest, Sha1};
use std::io::Read;

pub const SIS_UID1: u32 = 0x1020_1A7A;
/// Общий предел распакованных данных пакета (как в SymPort).
pub const MAX_UNCOMPRESSED: u64 = 128 * 1024 * 1024;
/// Предел числа элементов в массивах — защита от «взрыва» памяти на злонамеренном пакете.
const MAX_ITEMS: usize = 100_000;

// Идентификаторы типов полей.
pub const T_STRING: u32 = 1;
pub const T_ARRAY: u32 = 2;
pub const T_COMPRESSED: u32 = 3;
pub const T_VERSION: u32 = 4;
pub const T_VERSION_RANGE: u32 = 5;
pub const T_UID: u32 = 9;
pub const T_CONTENTS: u32 = 12;
pub const T_CONTROLLER: u32 = 13;
pub const T_INFO: u32 = 14;
pub const T_PREREQUISITES: u32 = 17;
pub const T_DEPENDENCY: u32 = 18;
pub const T_CERTIFICATE_CHAIN: u32 = 22;
pub const T_FILE_DESCRIPTION: u32 = 24;
pub const T_HASH: u32 = 25;
pub const T_INSTALL_BLOCK: u32 = 28;
pub const T_DATA: u32 = 30;
pub const T_DATA_UNIT: u32 = 31;
pub const T_FILE_DATA: u32 = 32;
pub const T_CONTROLLER_CHECKSUM: u32 = 34;
pub const T_DATA_CHECKSUM: u32 = 35;
pub const T_SIGNATURE: u32 = 36;
pub const T_BLOB: u32 = 37;
pub const T_SIGNATURE_ALGORITHM: u32 = 38;
pub const T_SIGNATURE_CHAIN: u32 = 39;
pub const T_CAPABILITIES: u32 = 41;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SisError(pub String);

fn bad<T>(msg: impl Into<String>) -> Result<T, SisError> {
    Err(SisError(msg.into()))
}

/// CRC16 Symbian: полином 0x1021, начальное значение 0, без финальной инверсии.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Контрольное слово UID-типа (три UID): пара CRC16 по чётным/нечётным байтам.
pub fn uid_checksum(first12: &[u8]) -> Result<u32, SisError> {
    if first12.len() < 12 {
        return bad("need 12 bytes of UIDs");
    }
    let mut crc = [0u16; 2];
    for (i, &b) in first12.iter().take(12).enumerate() {
        let mut v = crc[i % 2] ^ ((b as u16) << 8);
        for _ in 0..8 {
            v = if v & 0x8000 != 0 {
                (v << 1) ^ 0x1021
            } else {
                v << 1
            };
        }
        crc[i % 2] = v;
    }
    Ok(((crc[1] as u32) << 16) | crc[0] as u32)
}

/// Чтение с проверкой границ.
#[derive(Clone)]
pub struct Cursor<'a> {
    data: &'a [u8],
    pub pos: usize,
    end: usize,
}

pub struct RawField<'a> {
    pub ftype: u32,
    pub payload: &'a [u8],
    /// Сырые байты поля вместе с заголовком и выравниванием (для CRC).
    pub raw: &'a [u8],
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self::at(data, 0)
    }
    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self {
            data,
            pos,
            end: data.len(),
        }
    }
    pub fn done(&self) -> bool {
        self.pos >= self.end
    }
    pub fn peek_type(&self) -> Option<u32> {
        if self.pos + 4 <= self.end {
            Some(u32::from_le_bytes(
                self.data[self.pos..self.pos + 4].try_into().ok()?,
            ))
        } else {
            None
        }
    }
    pub fn read_u32(&mut self) -> Result<u32, SisError> {
        if self.pos + 4 > self.end {
            return bad("truncated integer");
        }
        let v = u32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
    fn fits(&self, n: usize) -> bool {
        self.pos.checked_add(n).is_some_and(|v| v <= self.end)
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], SisError> {
        if !self.fits(n) {
            return bad("truncated data");
        }
        let b = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(b)
    }

    /// Поле с заголовком типа.
    pub fn field(&mut self, expect: Option<u32>) -> Result<RawField<'a>, SisError> {
        let start = self.pos;
        let ftype = self.read_u32()?;
        if let Some(e) = expect {
            if ftype != e {
                return bad(format!(
                    "expected field {e}, found {ftype} at offset {start}"
                ));
            }
        }
        let payload = self.body(start)?;
        Ok(RawField {
            ftype,
            payload,
            raw: &self.data[start..self.pos],
        })
    }

    /// Поле внутри массива (без заголовка типа).
    pub fn array_item(&mut self) -> Result<&'a [u8], SisError> {
        let start = self.pos;
        self.body(start)
    }

    fn body(&mut self, start: usize) -> Result<&'a [u8], SisError> {
        let length = self.read_u32()?;
        if length & 0x8000_0000 != 0 {
            return bad("large (64-bit length) fields are not supported");
        }
        let length = length as usize;
        let padded = (length + 3) & !3;
        if !self.fits(padded) {
            return bad(format!(
                "field of {length} bytes exceeds its container (offset {start})"
            ));
        }
        let payload = &self.data[self.pos..self.pos + length];
        if self.data[self.pos + length..self.pos + padded]
            .iter()
            .any(|&b| b != 0)
        {
            return bad("non-zero field padding");
        }
        self.pos += padded;
        Ok(payload)
    }

    /// Массив: (тип элементов, полезные нагрузки).
    pub fn array(&mut self, expect_element: Option<u32>) -> Result<(u32, Vec<&'a [u8]>), SisError> {
        let f = self.field(Some(T_ARRAY))?;
        let mut c = Cursor::new(f.payload);
        let etype = c.read_u32()?;
        if let Some(e) = expect_element {
            if etype != e {
                return bad(format!("array of {etype}, expected {e}"));
            }
        }
        let mut items = Vec::new();
        while !c.done() {
            if items.len() >= MAX_ITEMS {
                return bad("too many array items");
            }
            items.push(c.array_item()?);
        }
        Ok((etype, items))
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Распаковка поля «сжатые данные»: алгоритм 0 — хранится, 1 — zlib (или «сырой» deflate).
pub fn decompress(payload: &[u8]) -> Result<Vec<u8>, SisError> {
    if payload.len() < 12 {
        return bad("truncated compressed field");
    }
    let algo = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let size = u64::from_le_bytes(payload[4..12].try_into().unwrap());
    if size > MAX_UNCOMPRESSED {
        return bad("uncompressed field exceeds 128 MiB limit");
    }
    let body = &payload[12..];
    match algo {
        0 => {
            if size != body.len() as u64 {
                return bad("stored data length mismatch");
            }
            Ok(body.to_vec())
        }
        1 => {
            let inflate = |zlib: bool| -> Result<Vec<u8>, String> {
                let mut out = Vec::new();
                let r = if zlib {
                    flate2::read::ZlibDecoder::new(body)
                        .take(size + 1)
                        .read_to_end(&mut out)
                } else {
                    flate2::read::DeflateDecoder::new(body)
                        .take(size + 1)
                        .read_to_end(&mut out)
                };
                r.map_err(|e| e.to_string())?;
                Ok(out)
            };
            let out = match inflate(true) {
                Ok(o) => o,
                Err(_) => {
                    inflate(false).map_err(|e| SisError(format!("cannot inflate data: {e}")))?
                }
            };
            if out.len() as u64 > size {
                return bad("inflated data exceeds declared size or is truncated");
            }
            if out.len() as u64 != size {
                return bad("inflated data length mismatch");
            }
            Ok(out)
        }
        n => bad(format!("unknown compression algorithm {n}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SisPackage {
    pub uid: u32,
    pub uid_matches_header: bool,
    pub names: Vec<String>,
    pub vendor_unique_name: String,
    pub vendor_names: Vec<String>,
    pub version: Option<[u32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SisFile {
    pub target: String,
    pub mime_type: String,
    pub capabilities_raw: Option<String>,
    pub hash_algorithm: u32,
    pub sha1: Option<String>,
    pub operation: u32,
    pub operation_options: u32,
    pub length: u64,
    pub uncompressed_length: u64,
    pub file_index: u32,
    /// `None` — не проверялось (несколько блоков данных или операция без данных).
    pub data_present: Option<bool>,
    pub actual_length: Option<u64>,
    pub actual_sha1: Option<String>,
    pub hash_ok: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SisDependency {
    /// `target-device` (платформы/продукты) или `dependency` (пакеты, которые должны быть установлены).
    pub kind: String,
    pub uid: Option<u32>,
    pub names: Vec<String>,
    pub from_version: Option<[i32; 3]>,
    pub to_version: Option<[i32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SisInfo {
    pub uid1: u32,
    pub uid2: u32,
    pub uid3: u32,
    pub size: u64,
    pub uid_checksum_ok: bool,
    /// `None` — необязательное поле CRC отсутствует (не проверено, не «верно»).
    pub controller_crc_ok: Option<bool>,
    pub data_crc_ok: Option<bool>,
    pub trailing_bytes: Option<u64>,
    pub package: SisPackage,
    pub files: Vec<SisFile>,
    pub signed: bool,
    pub signature_count: usize,
    pub dependencies: Vec<SisDependency>,
    /// Размеры не раскрытых блоков (вложенные контроллеры, условия).
    pub undecoded_blocks: Vec<usize>,
    pub data_units: usize,
    pub data_note: Option<String>,
}

/// Вспомогательный результат: информация и распакованные файлы.
pub struct Parsed {
    pub info: SisInfo,
    pub blobs: Vec<Vec<u8>>,
}

pub fn inspect(data: &[u8]) -> Result<SisInfo, SisError> {
    Ok(parse(data)?.info)
}

/// Содержимое встроенных файлов по цели установки (только пакеты с одним блоком данных).
pub fn extract(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, SisError> {
    let p = parse(data)?;
    if p.info.data_units != 1 {
        return bad("extraction supports single-data-unit packages only");
    }
    Ok(p.info
        .files
        .iter()
        .filter(|f| f.operation != 8)
        .filter_map(|f| {
            p.blobs
                .get(f.file_index as usize)
                .map(|b| (f.target.clone(), b.clone()))
        })
        .collect())
}

/// Тело контроллера (после распаковки) — нужно для подписей.
pub fn controller_body(data: &[u8]) -> Result<Vec<u8>, SisError> {
    if data.len() < 16 {
        return bad("truncated UID header");
    }
    let mut root = Cursor::at(data, 16);
    let contents = root.field(Some(T_CONTENTS))?;
    let mut c = Cursor::new(contents.payload);
    if c.peek_type() == Some(T_CONTROLLER_CHECKSUM) {
        c.field(None)?;
    }
    if c.peek_type() == Some(T_DATA_CHECKSUM) {
        c.field(None)?;
    }
    let comp = c.field(Some(T_COMPRESSED))?;
    let bytes = decompress(comp.payload)?;
    let mut cc = Cursor::new(&bytes);
    Ok(cc.field(Some(T_CONTROLLER))?.payload.to_vec())
}

pub fn parse(data: &[u8]) -> Result<Parsed, SisError> {
    if data.len() < 16 {
        return bad("truncated UID header");
    }
    let w = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let (uid1, uid2, uid3, chk) = (w(0), w(4), w(8), w(12));
    if uid1 != SIS_UID1 {
        return bad(format!(
            "not a SIS 9.x file (UID1 {uid1:#010x}, expected {SIS_UID1:#010x})"
        ));
    }
    let uid_checksum_ok = chk == uid_checksum(&data[..12])?;

    let mut root = Cursor::at(data, 16);
    let contents = root.field(Some(T_CONTENTS))?;
    let trailing_bytes = (!root.done()).then(|| (data.len() - root.pos) as u64);
    let mut c = Cursor::new(contents.payload);
    let crc_c = if c.peek_type() == Some(T_CONTROLLER_CHECKSUM) {
        Some(c.field(None)?.payload)
    } else {
        None
    };
    let crc_d = if c.peek_type() == Some(T_DATA_CHECKSUM) {
        Some(c.field(None)?.payload)
    } else {
        None
    };
    let comp = c.field(Some(T_COMPRESSED))?;
    let datafield = c.field(Some(T_DATA))?;
    if !c.done() {
        return bad("unsupported extra top-level fields (embedded controllers?)");
    }
    let crc_ok = |stored: Option<&[u8]>, raw: &[u8]| -> Option<bool> {
        stored.map(|s| s.len() >= 2 && u16::from_le_bytes([s[0], s[1]]) == crc16(raw))
    };
    let controller_crc_ok = crc_ok(crc_c, comp.raw);
    let data_crc_ok = crc_ok(crc_d, datafield.raw);

    let controller_bytes = decompress(comp.payload)?;
    let mut cc = Cursor::new(&controller_bytes);
    let ctrl = cc.field(Some(T_CONTROLLER))?;
    let mut k = Cursor::new(ctrl.payload);
    let info_f = k.field(Some(T_INFO))?;
    let mut ic = Cursor::new(info_f.payload);
    let uid_payload = ic.field(Some(T_UID))?.payload;
    if uid_payload.len() < 4 {
        return bad("truncated package UID");
    }
    let package_uid = u32::from_le_bytes(uid_payload[..4].try_into().unwrap());
    let vendor_unique = ic.field(Some(T_STRING))?.payload;
    let (_, names) = ic.array(Some(T_STRING))?;
    let (_, vendors) = ic.array(Some(T_STRING))?;
    let version = ic.field(Some(T_VERSION))?.payload;
    let package = SisPackage {
        uid: package_uid,
        uid_matches_header: package_uid == uid3,
        names: names.iter().map(|b| utf16(b)).collect(),
        vendor_unique_name: utf16(vendor_unique),
        vendor_names: vendors.iter().map(|b| utf16(b)).collect(),
        version: (version.len() >= 12).then(|| {
            [
                u32::from_le_bytes(version[0..4].try_into().unwrap()),
                u32::from_le_bytes(version[4..8].try_into().unwrap()),
                u32::from_le_bytes(version[8..12].try_into().unwrap()),
            ]
        }),
    };

    let mut signature_count = 0usize;
    let mut files = Vec::new();
    let mut dependencies = Vec::new();
    let mut undecoded = Vec::new();
    while !k.done() {
        match k.peek_type() {
            Some(T_INSTALL_BLOCK) => {
                let blk = k.field(None)?;
                let mut bc = Cursor::new(blk.payload);
                let (_, descs) = bc.array(Some(T_FILE_DESCRIPTION))?;
                for d in descs {
                    if files.len() >= MAX_ITEMS {
                        return bad("too many files in package");
                    }
                    files.push(file_description(d)?);
                }
                for _ in 0..2 {
                    if !bc.done() {
                        let (_, items) = bc.array(None)?;
                        if !items.is_empty() {
                            undecoded.push(items.len());
                        }
                    }
                }
            }
            Some(T_PREREQUISITES) => {
                let pre = k.field(None)?;
                let mut pc = Cursor::new(pre.payload);
                let mut idx = 0;
                while !pc.done() {
                    let (_, items) = pc.array(Some(T_DEPENDENCY))?;
                    for it in items {
                        dependencies.push(dependency(
                            it,
                            if idx == 0 {
                                "target-device"
                            } else {
                                "dependency"
                            },
                        )?);
                    }
                    idx += 1;
                }
            }
            Some(T_SIGNATURE_CHAIN) => {
                let sig = k.field(None)?;
                let mut sc = Cursor::new(sig.payload);
                let (_, items) = sc.array(Some(T_SIGNATURE))?;
                signature_count += items.len();
            }
            _ => {
                k.field(None)?; // параметры, языки, свойства, индекс данных — пропускаются
            }
        }
    }

    // Встроенные данные.
    let mut dc = Cursor::new(datafield.payload);
    let (_, units) = dc.array(Some(T_DATA_UNIT))?;
    let mut blobs: Vec<Vec<u8>> = Vec::new();
    let mut total = 0u64;
    for u in &units {
        let mut uc = Cursor::new(u);
        let (_, items) = uc.array(Some(T_FILE_DATA))?;
        for b in items {
            let mut bc = Cursor::new(b);
            let comp = bc.field(Some(T_COMPRESSED))?;
            let blob = decompress(comp.payload)?;
            total += blob.len() as u64;
            if total > MAX_UNCOMPRESSED {
                return bad("SIS total uncompressed data exceeds 128 MiB");
            }
            blobs.push(blob);
        }
    }
    let data_units = units.len();
    let mut data_note = None;
    if data_units == 1 {
        for f in &mut files {
            if f.operation == 8 {
                if f.length != 0 || f.uncompressed_length != 0 || f.sha1.is_some() {
                    return bad("null file operation declares unexpected payload");
                }
                f.note = Some("null operation: no payload to hash".into());
                continue;
            }
            match blobs.get(f.file_index as usize) {
                None => {
                    f.data_present = Some(false);
                    f.hash_ok = Some(false);
                }
                Some(b) => {
                    f.data_present = Some(true);
                    f.actual_length = Some(b.len() as u64);
                    let sha = hex(&Sha1::digest(b));
                    f.hash_ok = Some(
                        f.sha1.as_deref() == Some(sha.as_str())
                            && b.len() as u64 == f.uncompressed_length,
                    );
                    f.actual_sha1 = Some(sha);
                }
            }
        }
    } else {
        data_note = Some("multiple data units: embedded file hashes not checked".to_string());
    }

    Ok(Parsed {
        info: SisInfo {
            uid1,
            uid2,
            uid3,
            size: data.len() as u64,
            uid_checksum_ok,
            controller_crc_ok,
            data_crc_ok,
            trailing_bytes,
            package,
            files,
            signed: signature_count > 0,
            signature_count,
            dependencies,
            undecoded_blocks: undecoded,
            data_units,
            data_note,
        },
        blobs,
    })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn dependency(payload: &[u8], kind: &str) -> Result<SisDependency, SisError> {
    let mut item = SisDependency {
        kind: kind.to_string(),
        uid: None,
        names: Vec::new(),
        from_version: None,
        to_version: None,
    };
    let mut c = Cursor::new(payload);
    while !c.done() {
        let f = c.field(None)?;
        match f.ftype {
            T_UID if f.payload.len() >= 4 => {
                item.uid = Some(u32::from_le_bytes(f.payload[..4].try_into().unwrap()));
            }
            T_VERSION_RANGE => {
                let mut vc = Cursor::new(f.payload);
                let mut versions = Vec::new();
                while !vc.done() {
                    let v = vc.field(Some(T_VERSION))?.payload;
                    if v.len() >= 12 {
                        versions.push([
                            i32::from_le_bytes(v[0..4].try_into().unwrap()),
                            i32::from_le_bytes(v[4..8].try_into().unwrap()),
                            i32::from_le_bytes(v[8..12].try_into().unwrap()),
                        ]);
                    }
                }
                item.from_version = versions.first().copied();
                item.to_version = versions.get(1).copied();
            }
            T_ARRAY => {
                let mut ac = Cursor::new(f.payload);
                ac.read_u32()?;
                while !ac.done() {
                    item.names.push(utf16(ac.array_item()?));
                }
            }
            _ => {}
        }
    }
    Ok(item)
}

fn file_description(payload: &[u8]) -> Result<SisFile, SisError> {
    let mut c = Cursor::new(payload);
    let target = c.field(Some(T_STRING))?.payload;
    let mime = c.field(Some(T_STRING))?.payload;
    let caps = if c.peek_type() == Some(T_CAPABILITIES) {
        Some(hex(c.field(None)?.payload))
    } else {
        None
    };
    let hashp = c.field(Some(T_HASH))?.payload;
    let mut hc = Cursor::new(hashp);
    let algo = hc.read_u32()?;
    let blob = hc.field(Some(T_BLOB))?.payload;
    let operation = c.read_u32()?;
    let operation_options = c.read_u32()?;
    let length = u64::from_le_bytes(c.take(8)?.try_into().unwrap());
    let uncompressed_length = u64::from_le_bytes(c.take(8)?.try_into().unwrap());
    let file_index = c.read_u32()?;
    Ok(SisFile {
        target: utf16(target),
        mime_type: utf16(mime),
        capabilities_raw: caps,
        hash_algorithm: algo,
        sha1: (algo == 1).then(|| hex(blob)),
        operation,
        operation_options,
        length,
        uncompressed_length,
        file_index,
        data_present: None,
        actual_length: None,
        actual_sha1: None,
        hash_ok: None,
        note: None,
    })
}
