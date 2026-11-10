//! Java ME: JAR (ZIP с манифестом) и JAD (дескриптор). Только структурная проверка и метаданные;
//! классы не загружаются и не исполняются. Порт `validate_package` из SymPort.

use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

pub const MAX_PACKAGE: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
const MAX_EXPANDED: u64 = 64 * 1024 * 1024;
const MAX_MANIFEST: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct JarError(pub String);

fn bad<T>(m: impl Into<String>) -> Result<T, JarError> {
    Err(JarError(m.into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JarInfo {
    pub manifest: BTreeMap<String, String>,
    pub entries: usize,
    pub midp: String,
    pub cldc: String,
    pub permissions: String,
    pub optional_permissions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JadInfo {
    pub descriptor: BTreeMap<String, String>,
    /// JAD сверен с конкретным JAR (имя, размер, идентичность).
    pub paired: bool,
}

/// Главная секция манифеста/дескриптора: `Ключ: значение`, продолжения строк начинаются с пробела.
/// Дубликаты и строки без двоеточия — ошибка; секции подписей (после пустой строки) не читаются.
pub fn parse_attributes(raw: &[u8]) -> Result<BTreeMap<String, String>, JarError> {
    if raw.len() > MAX_MANIFEST {
        return bad("Manifest exceeds 64 KiB");
    }
    let text = std::str::from_utf8(raw)
        .map_err(|_| JarError("Manifest must be UTF-8".into()))?
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if let (Some(rest), true) = (line.strip_prefix(' '), !lines.is_empty()) {
            lines.last_mut().unwrap().push_str(rest);
        } else {
            lines.push(line.to_string());
        }
    }
    let mut out = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((k, v)) = line.split_once(':') else {
            return bad("Malformed or duplicate manifest attribute");
        };
        if out.insert(k.to_string(), v.trim().to_string()).is_some() {
            return bad("Malformed or duplicate manifest attribute");
        }
    }
    Ok(out)
}

fn require(m: &BTreeMap<String, String>, keys: &[&str], what: &str) -> Result<(), JarError> {
    for k in keys {
        if m.get(*k).is_none_or_empty() {
            return bad(format!("{what} missing {k}"));
        }
    }
    Ok(())
}

trait NoneOrEmpty {
    fn is_none_or_empty(&self) -> bool;
}
impl NoneOrEmpty for Option<&String> {
    fn is_none_or_empty(&self) -> bool {
        self.map_or(true, |s| s.is_empty())
    }
}

pub fn inspect_jar(raw: &[u8]) -> Result<JarInfo, JarError> {
    if raw.is_empty() || raw.len() > MAX_PACKAGE {
        return bad("Package must be between 1 byte and 32 MiB");
    }
    let invalid = |_| JarError("Invalid JAR archive or missing manifest".into());
    let mut zip = zip::ZipArchive::new(Cursor::new(raw)).map_err(invalid)?;
    if zip.len() > MAX_ENTRIES {
        return bad("JAR expansion limits exceeded");
    }
    let mut names = std::collections::HashSet::new();
    let mut total = 0u64;
    let mut classes = false;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(invalid)?;
        let name = f.name().to_string();
        if !names.insert(name.clone())
            || name.starts_with('/')
            || name.contains('\\')
            || name.split('/').any(|p| p == "..")
        {
            return bad("Unsafe or duplicate JAR entries");
        }
        total = total.saturating_add(f.size());
        classes |= name.ends_with(".class");
    }
    if total > MAX_EXPANDED {
        return bad("JAR expansion limits exceeded");
    }
    if !classes {
        return bad("JAR has no classes");
    }
    // Проверка целостности: каждая запись читается до конца (CRC), не больше заявленного размера.
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(invalid)?;
        let declared = f.size();
        let mut limited = f.take(declared.saturating_add(1));
        let n = std::io::copy(&mut limited, &mut std::io::sink())
            .map_err(|_| JarError("Corrupted JAR entry".into()))?;
        if n != declared {
            return bad("Corrupted JAR entry");
        }
    }
    let mut manifest_raw = Vec::new();
    {
        let f = zip.by_name("META-INF/MANIFEST.MF").map_err(invalid)?;
        f.take(MAX_MANIFEST as u64 + 1)
            .read_to_end(&mut manifest_raw)
            .map_err(|_| JarError("Corrupted JAR entry".into()))?;
    }
    let manifest = parse_attributes(&manifest_raw)?;
    require(
        &manifest,
        &["MIDlet-Name", "MIDlet-Version", "MIDlet-Vendor", "MIDlet-1"],
        "JAR",
    )?;
    let get = |k: &str, d: &str| manifest.get(k).cloned().unwrap_or_else(|| d.to_string());
    Ok(JarInfo {
        entries: names.len(),
        midp: get("MicroEdition-Profile", "unknown"),
        cldc: get("MicroEdition-Configuration", "unknown"),
        permissions: get("MIDlet-Permissions", ""),
        optional_permissions: get("MIDlet-Permissions-Opt", ""),
        manifest,
    })
}

/// Последний сегмент пути из `MIDlet-Jar-URL` (без запроса и фрагмента).
fn url_file_name(url: &str) -> &str {
    let no_frag = url.split('#').next().unwrap_or(url);
    let no_query = no_frag.split('?').next().unwrap_or(no_frag);
    no_query.rsplit('/').next().unwrap_or(no_query)
}

pub fn inspect_jad(raw: &[u8], jar: Option<(&str, &[u8])>) -> Result<JadInfo, JarError> {
    let d = parse_attributes(raw)?;
    require(
        &d,
        &[
            "MIDlet-Name",
            "MIDlet-Version",
            "MIDlet-Vendor",
            "MIDlet-Jar-URL",
            "MIDlet-Jar-Size",
        ],
        "JAD",
    )?;
    let Some((jar_name, jar_raw)) = jar else {
        return Ok(JadInfo {
            descriptor: d,
            paired: false,
        });
    };
    let size: u64 = d["MIDlet-Jar-Size"]
        .parse()
        .map_err(|_| JarError("JAD/JAR filename or size mismatch".into()))?;
    if url_file_name(&d["MIDlet-Jar-URL"]) != jar_name || size != jar_raw.len() as u64 {
        return bad("JAD/JAR filename or size mismatch");
    }
    let m = inspect_jar(jar_raw)?.manifest;
    for k in ["MIDlet-Name", "MIDlet-Version", "MIDlet-Vendor"] {
        if d[k] != m[k] {
            return bad("JAD/JAR identity mismatch");
        }
    }
    Ok(JadInfo {
        descriptor: d,
        paired: true,
    })
}
