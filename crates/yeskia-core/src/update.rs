//! Обновление приложения **из файла**: приложение само ничего не скачивает (работает офлайн), а
//! проверяет установщик, который пользователь получил сам, и только по явному подтверждению запускает
//! его — не раньше, чем завершены все операции с телефоном.
//!
//! Проверки:
//! * манифест `{version, file, sha256, size, notes}` ограничен по размеру, версия строго новее текущей
//!   (откат и повтор отвергаются);
//! * установщик: имя и размер из манифеста, SHA-256 потоком, повторно перед запуском (TOCTOU);
//! * подпись манифеста — Ed25519 по зашитым доверенным ключам издателя. **Пока ключ не настроен**
//!   (`TRUSTED_KEYS` пуст) источник считается неподписанным: интерфейс показывает ограничения
//!   (SmartScreen/издатель не подтверждены), запуск возможен только с отдельным явным согласием;
//! * запуск блокируется при незавершённых операциях с телефоном и заданиях записи на диск.

use crate::error::{Error, Result};
use crate::fsutil::{hash_file, long_path};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Доверенные публичные ключи Ed25519 издателя релизов. Пусто, пока владелец не настроил подпись:
/// см. docs/DISTRIBUTION.md («Ключ подписи обновлений»).
pub const TRUSTED_KEYS: &[[u8; 32]] = &[];

const MAX_MANIFEST: usize = 64 * 1024;
const MAX_INSTALLER: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trust {
    /// Подпись манифеста проверена доверенным ключом.
    Signed,
    /// Подписи нет или нет настроенного ключа: проверена только контрольная сумма.
    UnsignedSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateCheck {
    pub current: String,
    pub version: String,
    pub notes: String,
    pub installer: String,
    pub size: u64,
    pub sha256: String,
    pub trust: Trust,
    /// Что пользователь должен знать до запуска (честные ограничения).
    pub limitations: Vec<String>,
}

/// `1.2.3` → `(1,2,3)`; суффиксы (`-beta`) и лишние части отвергаются: версия должна быть однозначной.
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().split('.');
    let p = |s: Option<&str>| -> Option<u32> {
        let s = s?;
        (!s.is_empty() && s.len() <= 5 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())?
    };
    let r = (p(it.next())?, p(it.next())?, p(it.next())?);
    it.next().is_none().then_some(r)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok())
        .collect()
}

/// Проверка подписи манифеста любым из ключей. `Ok(true)` — верна; `Ok(false)` — подписи нет/ключей нет;
/// `Err` — подпись есть и не сходится.
pub fn verify_signature(
    manifest: &[u8],
    signature_hex: Option<&str>,
    keys: &[[u8; 32]],
) -> Result<bool> {
    let Some(sig) = signature_hex else {
        return Ok(false);
    };
    if keys.is_empty() {
        return Ok(false);
    }
    let sig = hex_decode(sig).ok_or_else(|| Error::InvalidArgument("подпись — не hex".into()))?;
    if keys.iter().any(|k| {
        UnparsedPublicKey::new(&ED25519, k)
            .verify(manifest, &sig)
            .is_ok()
    }) {
        Ok(true)
    } else {
        Err(Error::VerificationFailed)
    }
}

/// Полная проверка файла обновления. Ничего не запускает.
pub fn check(
    installer: &Path,
    manifest_path: &Path,
    signature_hex: Option<&str>,
    current_version: &str,
    keys: &[[u8; 32]],
) -> Result<UpdateCheck> {
    crate::fsutil::validate_host_path(installer)?;
    crate::fsutil::validate_host_path(manifest_path)?;
    let mraw = std::fs::read(long_path(manifest_path))
        .map_err(|_| Error::HostAccess("манифест недоступен".into()))?;
    if mraw.is_empty() || mraw.len() > MAX_MANIFEST {
        return Err(Error::LimitExceeded(
            "манифест должен быть до 64 КиБ".into(),
        ));
    }
    let signed = verify_signature(&mraw, signature_hex, keys)?;
    let m: UpdateManifest = serde_json::from_slice(&mraw)
        .map_err(|_| Error::InvalidArgument("манифест обновления не разобран".into()))?;
    let cur = parse_version(current_version)
        .ok_or_else(|| Error::Internal("текущая версия не разобрана".into()))?;
    let new = parse_version(&m.version).ok_or_else(|| {
        Error::InvalidArgument("версия в манифесте должна быть вида 1.2.3".into())
    })?;
    if new <= cur {
        return Err(Error::InvalidArgument(format!(
            "версия {} не новее текущей {current_version}: откат и повтор не выполняются",
            m.version
        )));
    }
    crate::fsutil::validate_file_name(&m.file)?;
    let name = installer
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::UnsafePath("имя файла не в Unicode".into()))?;
    if name != m.file {
        return Err(Error::InvalidArgument(
            "имя установщика не совпадает с указанным в манифесте".into(),
        ));
    }
    if !name.to_ascii_lowercase().ends_with(".exe") && !name.to_ascii_lowercase().ends_with(".msi")
    {
        return Err(Error::InvalidArgument(
            "ожидается установщик .exe или .msi".into(),
        ));
    }
    let meta = std::fs::metadata(long_path(installer))
        .map_err(|_| Error::HostAccess("установщик недоступен".into()))?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_INSTALLER || meta.len() != m.size {
        return Err(Error::InvalidArgument(
            "размер установщика не совпадает с манифестом или вне пределов".into(),
        ));
    }
    let (_, sha) = hash_file(&long_path(installer), None)?;
    if !m.sha256.eq_ignore_ascii_case(&sha) {
        return Err(Error::VerificationFailed);
    }
    let mut limitations = vec![
        "Приложение ничего не скачивает: установщик получен вами самостоятельно.".to_string(),
        "Проверены размер и SHA-256 по манифесту; установка выполняется самим установщиком."
            .to_string(),
    ];
    if !signed {
        limitations.push(
            "Подпись издателя не проверена (ключ подписи не настроен или подписи нет): источник не подтверждён. \
             Windows SmartScreen может предупредить о неизвестном издателе — это ожидаемо для неподписанной сборки."
                .into(),
        );
    }
    Ok(UpdateCheck {
        current: current_version.to_string(),
        version: m.version,
        notes: m.notes.chars().take(2000).collect(),
        installer: name.to_string(),
        size: m.size,
        sha256: sha,
        trust: if signed {
            Trust::Signed
        } else {
            Trust::UnsignedSource
        },
        limitations,
    })
}

/// Запуск установщика отдельным процессом (без аргументов). Только Windows.
pub fn launch(installer: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new(installer)
            .spawn()
            .map(|_| ())
            .map_err(|e| Error::HostAccess(format!("не удалось запустить установщик: {e}")))
    }
    #[cfg(not(windows))]
    {
        let _ = installer;
        Err(Error::CapabilityUnsupported(
            "запуск установщика поддерживается только в Windows".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_strict() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version(" 0.10.0 "), Some((0, 10, 0)));
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3-beta",
            "-1.0.0",
            "1..3",
            "99999999.0.0",
        ] {
            assert_eq!(parse_version(bad), None, "{bad:?}");
        }
        assert!(parse_version("0.2.0") > parse_version("0.1.9"));
        assert!(parse_version("0.10.0") > parse_version("0.9.0"));
    }

    #[test]
    fn hex_decoding() {
        assert_eq!(hex_decode("00ff"), Some(vec![0, 255]));
        assert_eq!(hex_decode("0"), None);
        assert_eq!(hex_decode("zz"), None);
    }
}
