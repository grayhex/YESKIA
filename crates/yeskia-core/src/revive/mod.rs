//! Local TLS Revival, ported from s60modern efaf53d (MIT; see docs/licenses/s60modern.txt).
//! No HTTP service, no automatic installer or privileged phone writes.
use crate::{
    pkg::sis,
    workbench::{Versioned, Workbench},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};
pub const MAX_COMPONENT: u64 = 2 * 1024 * 1024;
pub const MAX_ZIP: u64 = 4 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub reviewed: String,
    pub note: String,
    pub components: BTreeMap<String, Component>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Component {
    pub title: BTreeMap<String, String>,
    pub role: String,
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub version: String,
    pub released: String,
    pub author: String,
    pub homepage: String,
    pub source_repository: String,
    pub licence: serde_json::Value,
    pub release_notes: String,
    pub release_notes_text: String,
    #[serde(default)]
    pub package: Option<Package>,
    #[serde(default)]
    pub contents: Vec<String>,
    pub depends_on: Vec<String>,
    pub install: String,
    pub risk: String,
    pub observed_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub uid: String,
    pub signed: bool,
    pub targets: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validation {
    pub component: String,
    pub format: String,
    pub sha256: String,
    pub size: u64,
    pub package_uid: Option<u32>,
    pub signed: Option<bool>,
    pub uid_checksum: Option<bool>,
    pub controller_crc: Option<bool>,
    pub data_crc: Option<bool>,
    pub files: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cached {
    pub id: String,
    pub spec: Component,
    pub cached: bool,
    pub validation: Option<Validation>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub model: String,
    pub rm: String,
    pub firmware: String,
    pub os: String,
    pub identity_confirmed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Applicability {
    pub status: String,
    pub basis: String,
    pub components: Vec<String>,
    pub reasons: Vec<String>,
    pub firmware_verified: bool,
    pub device_verified: bool,
    pub automatic_apply: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub status: String,
    pub checked_at_ms: u64,
    pub source: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    #[serde(default)]
    pub transferred: BTreeMap<String, Observation>,
    pub components: BTreeMap<String, Observation>,
    pub phone_clock: Option<String>,
    pub phone_https: Option<Observation>,
    pub snapshot: Option<Observation>,
}
fn invalid(code: &str) -> Error {
    Error::InvalidArgument(format!("TLS_{code}"))
}
pub fn manifest() -> Result<Manifest> {
    let m: Manifest = serde_json::from_str(include_str!("tls_components.json"))
        .map_err(|_| invalid("MANIFEST"))?;
    if m.schema != 1 {
        return Err(invalid("MANIFEST_VERSION"));
    }
    for (id, c) in &m.components {
        if !["mbedtls-lib", "ssl-patch"].contains(&id.as_str())
            || c.url
                != format!(
                    "https://nnproject.cc/dl/{}",
                    if id == "mbedtls-lib" {
                        "mbedtls.sis"
                    } else {
                        "ssl.zip"
                    }
                )
            || c.size == 0
            || c.size > MAX_COMPONENT
            || c.sha256.len() != 64
            || !c.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("MANIFEST_PIN"));
        }
        crate::fsutil::validate_file_name(&c.name)?;
    }
    Ok(m)
}
pub fn manifest_version() -> String {
    crate::fsutil::sha256_hex(include_bytes!("tls_components.json"))
}
pub fn spec(id: &str) -> Result<Component> {
    manifest()?
        .components
        .remove(id)
        .ok_or_else(|| invalid("COMPONENT"))
}
pub fn applicability(target: &Target, stack: &str) -> Applicability {
    let mut out = Applicability {
        status: "unsupported".into(),
        basis: "author-documentation; not hardware evidence".into(),
        components: Vec::new(),
        reasons: Vec::new(),
        firmware_verified: false,
        device_verified: false,
        automatic_apply: false,
    };
    if stack == "qt" {
        out.reasons.push("qt-is-not-covered-by-system-ssl".into());
        return out;
    }
    if !["system-native", "j2me", "mbedtls-native"].contains(&stack) {
        out.reasons.push("unknown-application-stack".into());
        return out;
    }
    let profile = match target.model.as_str() {
        "nokia-e72" => Some(("RM-530", "9.3")),
        "nokia-e52" => Some(("", "9.3")),
        "nokia-n95" | "nokia-n95-8gb" => Some(("", "9.2")),
        "nokia-n97-mini" => Some(("", "9.4")),
        "nokia-n8" | "nokia-e7" => {
            out.status = "manual".into();
            out.reasons
                .push("symbian3-author-support; model-and-firmware-not-verified".into());
            None
        }
        _ => None,
    };
    if target.model == "nokia-7610" || target.os == "eka1" {
        out.reasons
            .push("eka1-needs-separate-bearssl-packages".into());
        return out;
    }
    if let Some((rm, os)) = profile {
        if !target.os.is_empty() && target.os != os {
            out.status = "blocked".into();
            out.reasons.push("symbian-version-mismatch".into());
            return out;
        }
        if !rm.is_empty() && !target.rm.is_empty() && target.rm.to_ascii_uppercase() != rm {
            out.status = "blocked".into();
            out.reasons.push("rm-mismatch".into());
            return out;
        }
        out.status = if target.identity_confirmed && !target.firmware.is_empty() {
            "documented"
        } else {
            "manual"
        }
        .into();
        if !target.identity_confirmed {
            out.reasons.push("identity-not-confirmed-by-device".into());
        }
        out.reasons.push("exact-firmware-not-verified".into());
    } else if out.status != "manual" {
        out.reasons.push("unknown-platform".into());
        return out;
    }
    out.components = if stack == "mbedtls-native" {
        vec!["mbedtls-lib".into()]
    } else {
        vec!["mbedtls-lib".into(), "ssl-patch".into()]
    };
    out.reasons
        .push("installserver-required-for-unsigned-sis".into());
    if out.components.len() > 1 {
        out.reasons
            .push("open4all-or-equivalent-rights-required; snapshot-before-system-change".into());
    }
    out
}
pub fn validate_against(id: &str, c: &Component, raw: &[u8]) -> Result<Validation> {
    if raw.len() as u64 != c.size || crate::fsutil::sha256_hex(raw) != c.sha256 {
        return Err(invalid("PIN_MISMATCH_REVIEW_UPSTREAM_VERSION"));
    }
    let mut v = Validation {
        component: id.into(),
        format: String::new(),
        sha256: c.sha256.clone(),
        size: c.size,
        package_uid: None,
        signed: None,
        uid_checksum: None,
        controller_crc: None,
        data_crc: None,
        files: Vec::new(),
    };
    if let Some(package) = &c.package {
        let info = sis::inspect(raw).map_err(|_| invalid("BROKEN_SIS"))?;
        let expected = u32::from_str_radix(package.uid.trim_start_matches("0x"), 16)
            .map_err(|_| invalid("MANIFEST_UID"))?;
        let expected_version: Vec<u32> = c
            .version
            .split('.')
            .map(|v| v.parse().map_err(|_| invalid("MANIFEST_VERSION")))
            .collect::<Result<_>>()?;
        if info.package.version.map(|v| v.to_vec()) != Some(expected_version)
            || info.package.uid != expected
            || !info.package.uid_matches_header
            || !info.uid_checksum_ok
            || info.controller_crc_ok == Some(false)
            || info.data_crc_ok == Some(false)
            || info.files.iter().any(|f| f.hash_ok == Some(false))
            || info.signed != package.signed
        {
            return Err(invalid("SIS_IDENTITY_OR_CRC"));
        }
        let actual: BTreeSet<_> = info.dependencies.iter().filter_map(|d| d.uid).collect();
        for uid in &package.targets {
            let uid = u32::from_str_radix(uid.trim_start_matches("0x"), 16)
                .map_err(|_| invalid("MANIFEST_TARGET"))?;
            if !actual.contains(&uid) {
                return Err(invalid("SIS_PLATFORM"));
            }
        }
        v.format = "sis".into();
        v.package_uid = Some(expected);
        v.signed = Some(info.signed);
        v.uid_checksum = Some(info.uid_checksum_ok);
        v.controller_crc = info.controller_crc_ok;
        v.data_crc = info.data_crc_ok;
        v.files = info.files.into_iter().map(|f| f.target).collect();
    } else {
        let mut zip = zip::ZipArchive::new(Cursor::new(raw)).map_err(|_| invalid("BROKEN_ZIP"))?;
        if zip.is_empty() || zip.len() > 20 {
            return Err(invalid("ZIP_LIMIT"));
        }
        let mut names = BTreeSet::new();
        let mut all_names = BTreeSet::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(|_| invalid("ZIP_ENTRY"))?;
            let name = file.name().to_string();
            if !all_names.insert(name.clone())
                || name.starts_with('/')
                || name.contains(['\\', ':'])
                || name.split('/').any(|s| s == ".." || s == ".")
                || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            {
                return Err(invalid("UNSAFE_ZIP"));
            }
            if file.is_dir() {
                if !["sys/", "sys/bin/"].contains(&name.as_str()) {
                    return Err(invalid("UNEXPECTED_ZIP_DIRECTORY"));
                }
                continue;
            }
            if !c.contents.contains(&name) || !names.insert(name.clone()) {
                return Err(invalid("UNEXPECTED_ZIP_FILE"));
            }
            total = total
                .checked_add(file.size())
                .ok_or_else(|| invalid("ZIP_LIMIT"))?;
            if total > MAX_ZIP {
                return Err(invalid("ZIP_LIMIT"));
            }
            let mut bytes = Vec::new();
            file.take(MAX_ZIP + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid("ZIP_CRC"))?;
            if bytes.len() as u64 > MAX_ZIP {
                return Err(invalid("ZIP_LIMIT"));
            }
            v.files.push(name);
        }
        if names != c.contents.iter().cloned().collect() {
            return Err(invalid("ZIP_CONTENTS"));
        }
        v.format = "zip".into();
    }
    Ok(v)
}
pub fn validate(id: &str, raw: &[u8]) -> Result<Validation> {
    validate_against(id, &spec(id)?, raw)
}
pub fn cache_path(root: &Path, id: &str) -> Result<PathBuf> {
    let c = spec(id)?;
    Ok(root.join("tls").join(c.sha256).join(c.name))
}
pub fn cache_state(root: &Path) -> Result<Vec<Cached>> {
    let mut out = Vec::new();
    for (id, c) in manifest()?.components {
        let path = cache_path(root, &id)?;
        let validation = if path.exists() {
            Some(validate(&id, &read_bounded(&path)?)?)
        } else {
            None
        };
        out.push(Cached {
            id,
            spec: c,
            cached: validation.is_some(),
            validation,
        });
    }
    Ok(out)
}
pub fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    crate::fsutil::validate_host_path(path)?;
    let file =
        std::fs::File::open(path).map_err(|_| Error::HostAccess("TLS_SOURCE_FILE".into()))?;
    if !file.metadata()?.is_file() {
        return Err(invalid("SOURCE_NOT_FILE"));
    }
    let mut raw = Vec::new();
    file.take(MAX_COMPONENT + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_COMPONENT {
        return Err(Error::LimitExceeded("TLS_COMPONENT_LIMIT".into()));
    }
    Ok(raw)
}
pub fn import_file(root: &Path, id: &str, path: &Path) -> Result<Validation> {
    let raw = read_bounded(path)?;
    let v = validate(id, &raw)?;
    let dest = cache_path(root, id)?;
    std::fs::create_dir_all(dest.parent().unwrap())?;
    if dest.exists() {
        validate(id, &read_bounded(&dest)?)?;
    } else {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)?;
        if f.write_all(&raw).and_then(|_| f.sync_all()).is_err() {
            drop(f);
            let _ = std::fs::remove_file(dest);
            return Err(Error::HostAccess("TLS_CACHE_WRITE".into()));
        }
    }
    Ok(v)
}
pub fn extract_ssl(root: &Path) -> Result<PathBuf> {
    let raw = read_bounded(&cache_path(root, "ssl-patch")?)?;
    validate("ssl-patch", &raw)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(raw)).map_err(|_| invalid("BROKEN_ZIP"))?;
    let mut data = Vec::new();
    zip.by_name("sys/bin/ssl.dll")
        .map_err(|_| invalid("ZIP_CONTENTS"))?
        .take(MAX_ZIP + 1)
        .read_to_end(&mut data)
        .map_err(|_| invalid("ZIP_CRC"))?;
    let dir = root.join("tls").join("prepared");
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(format!("{}-ssl.dll", crate::fsutil::sha256_hex(&data)));
    if !dest.exists() {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)?;
        f.write_all(&data)?;
        f.sync_all()?;
    } else if std::fs::read(&dest)? != data {
        return Err(Error::VerificationFailed);
    }
    Ok(dest)
}
pub fn progress_key(fingerprint: &str, firmware: &str) -> String {
    format!(
        "tls-{}",
        crate::fsutil::sha256_hex(
            format!("{fingerprint}|{firmware}|{}", manifest_version()).as_bytes()
        )
    )
}
pub fn record_manual(
    store: &Workbench,
    key: &str,
    revision: i64,
    item: &str,
    status: &str,
    confirm: bool,
) -> Result<Versioned<Progress>> {
    let allowed = match item {
        "mbedtls-lib" => ["installed-reported", "failed", "not-tested"].contains(&status),
        "ssl-patch" => [
            "applied-reported",
            "failed",
            "not-tested",
            "restored-reported",
        ]
        .contains(&status),
        "snapshot" => ["snapshot-reported", "not-tested"].contains(&status),
        "phone-https" => ["passed-reported", "failed", "not-tested"].contains(&status),
        _ => false,
    };
    if !confirm || !allowed {
        return Err(invalid("MANUAL_CONFIRM_REQUIRED"));
    }
    let current: Versioned<Progress> = store.load(key)?;
    if current.revision != revision {
        return Err(Error::OperationFailed("LOCAL_EDIT_CONFLICT_RELOAD".into()));
    }
    let mut next = current.value;
    if item == "ssl-patch"
        && ["applied-reported", "restored-reported"].contains(&status)
        && next
            .snapshot
            .as_ref()
            .map_or(true, |s| s.status != "snapshot-reported")
    {
        return Err(invalid("ORIGINAL_DLL_SNAPSHOT_REQUIRED"));
    }

    if item == "ssl-patch"
        && status == "applied-reported"
        && !next
            .components
            .get("mbedtls-lib")
            .is_some_and(|s| s.status == "installed-reported")
    {
        return Err(invalid("MBEDTLS_INSTALL_REPORT_REQUIRED"));
    }
    let report = Observation {
        status: status.into(),
        checked_at_ms: crate::now_ms(),
        source: "owner-manual-report; not-device-verified".into(),
    };
    match item {
        "snapshot" => next.snapshot = Some(report),
        "phone-https" => next.phone_https = Some(report),
        _ => {
            next.components.insert(item.into(), report);
        }
    }
    store.save(key, revision, next)
}
pub fn clock_check(date: &str, today: &str) -> Result<String> {
    fn parse(s: &str) -> Option<(i32, u32, u32)> {
        let mut p = s.split('-');
        let y = p.next()?.parse().ok()?;
        let m = p.next()?.parse().ok()?;
        let d = p.next()?.parse().ok()?;
        let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        let max = match m {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        };
        if p.next().is_some() || !(1900..=2200).contains(&y) || d == 0 || d > max {
            return None;
        }
        Some((y, m, d))
    }
    let d = parse(date).ok_or_else(|| invalid("PHONE_DATE"))?;
    let t = parse(today).ok_or_else(|| invalid("HOST_DATE"))?;
    Ok(if d == t {
        "manual-date-matches-host; timezone-and-clock-not-verified"
    } else {
        "manual-date-differs-from-host; correct-phone-clock-before-tls-test"
    }
    .into())
}

/// Only protocol identity may be passed here; never Bluetooth display names.
pub fn model_matches(id: &str, identified: &str) -> bool {
    let Some(profile) = crate::pkg::profile::model(id) else {
        return false;
    };
    identified.eq_ignore_ascii_case(profile.id)
        || identified.eq_ignore_ascii_case(profile.name)
        || identified
            .eq_ignore_ascii_case(profile.name.strip_prefix("Nokia ").unwrap_or(profile.name))
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema: String,
    pub manifest_sha256: String,
    pub profile: crate::preparation::Profile,
    pub profile_source: String,
    pub stack: String,
    pub applicability: Applicability,
    pub components: Vec<Cached>,
    pub progress: Progress,
    pub hardware_status: String,
    pub excluded: Vec<String>,
}
pub fn profile_scope(profile: &crate::preparation::Profile) -> String {
    format!(
        "{}|{}|{}|{}",
        profile.model, profile.rm, profile.os, profile.firmware
    )
}
pub fn report(
    root: &Path,
    store: &Workbench,
    fingerprint: &str,
    profile: crate::preparation::Profile,
    stack: &str,
) -> Result<Report> {
    profile.validate()?;
    if !["system-native", "j2me", "mbedtls-native", "qt"].contains(&stack) {
        return Err(invalid("APPLICATION_STACK"));
    }
    let key = progress_key(fingerprint, &profile_scope(&profile));
    let progress = store.load::<Progress>(&key)?.value;
    let target = Target {
        model: profile.model.clone(),
        rm: profile.rm.clone(),
        firmware: profile.firmware.clone(),
        os: profile.os.clone(),
        identity_confirmed: false,
    };
    Ok(Report {
        schema: "yeskia.tls-report/1".into(),
        manifest_sha256: manifest_version(),
        profile,
        profile_source: "owner-entered; firmware not device-verified".into(),
        stack: stack.into(),
        applicability: applicability(&target, stack),
        components: cache_state(root)?,
        progress,
        hardware_status: "not-tested; manual reports are not hardware verification".into(),
        excluded: vec![
            "phone-identifiers".into(),
            "fingerprint".into(),
            "local-paths".into(),
            "contacts".into(),
            "credentials".into(),
        ],
    })
}
pub fn record_clock(
    store: &Workbench,
    key: &str,
    revision: i64,
    date: &str,
) -> Result<Versioned<Progress>> {
    clock_check(date, date)?; // Syntax/date validity only; the UI compares with local host date.
    let mut current = store.load::<Progress>(key)?;
    if current.revision != revision {
        return Err(Error::OperationFailed("LOCAL_EDIT_CONFLICT_RELOAD".into()));
    }
    current.value.phone_clock = Some(date.into());
    store.save(key, revision, current.value)
}
