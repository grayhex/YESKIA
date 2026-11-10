//! Отчёт инспектора `yeskia.inspector/1`: структура пакета, подписи, зависимости, статическая
//! совместимость. Отчёт говорит только о том, что проверено; остальное — `unknown`/`not-tested`.
//!
//! Правила честности:
//! * `compatible` **не выдаётся** (нет профиля прошивки и проверенной модели): итог — `unknown`
//!   или `incompatible`;
//! * подпись «математически верна» ≠ «телефон доверяет»; доверие всегда `unknown`;
//! * неизвестные UID и зависимости остаются `unknown`/`required-unverified`;
//! * установка, запуск и физическое устройство — всегда `not-tested`.

use super::{cert, e32, jar, profile, sis};
use crate::inspector::InspectionVerdict;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const REPORT_SCHEMA: &str = "yeskia.inspector/1";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PkgError(pub String);

impl From<sis::SisError> for PkgError {
    fn from(e: sis::SisError) -> Self {
        PkgError(e.0)
    }
}
impl From<jar::JarError> for PkgError {
    fn from(e: jar::JarError) -> Self {
        PkgError(e.0)
    }
}

/// Формат пакета по расширению имени.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Sis,
    Sisx,
    Jar,
    Jad,
}

impl Format {
    pub fn from_name(name: &str) -> Option<Format> {
        match name.rsplit('.').next()?.to_ascii_lowercase().as_str() {
            "sis" => Some(Format::Sis),
            "sisx" => Some(Format::Sisx),
            "jar" => Some(Format::Jar),
            "jad" => Some(Format::Jad),
            _ => None,
        }
    }
    pub fn is_sis(self) -> bool {
        matches!(self, Format::Sis | Format::Sisx)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Target {
    /// Идентификатор профиля модели (`nokia-e52`), пусто — модель не выбрана.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub rm: String,
    #[serde(default)]
    pub firmware: String,
    /// Версия ОС, выбранная пользователем (`9.3`).
    #[serde(default)]
    pub os: String,
}

/// Пакет, который пользователь положил в библиотеку и который может предоставить зависимость.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provided {
    pub uid: u32,
    pub name: String,
    pub version: Option<[u32; 3]>,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub target: Target,
    /// Момент проверки дат сертификатов (секунды Unix).
    pub at: i64,
    pub provided: Vec<Provided>,
    /// Для JAD: парный JAR (имя, байты).
    pub jar: Option<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformDep {
    pub uid: String,
    pub label: String,
    pub symbian: Option<String>,
    pub min_version: Option<String>,
    pub max_version: Option<String>,
    /// `accepted-by-target` | `not-accepted-by-target` | `target-unknown` | `unknown-platform-id`
    /// | `product-version-unknown` | `release-unverified`
    pub relation: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageDep {
    pub uid: Option<String>,
    pub name: String,
    pub runtime: Option<String>,
    pub min_version: Option<String>,
    pub max_version: Option<String>,
    /// `required-unverified` | `provided-in-library` | `provided-version-too-old` |
    /// `provided-version-unknown`. «Предоставлено библиотекой» ≠ «установлено на телефоне».
    pub status: String,
    pub matched_package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyAnalysis {
    pub platforms: Vec<PlatformDep>,
    /// `supported-static` | `unsupported` | `unknown`
    pub platform_verdict: String,
    pub packages: Vec<PackageDep>,
    /// `no-declared-dependencies` | `unresolved` | `provided-in-library` | `blocked`.
    /// Импорты DLL не декодируются, поэтому «satisfied» не выдаётся никогда.
    pub verdict: String,
    pub imports: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Binary {
    pub target: String,
    #[serde(flatten)]
    pub info: e32::E32Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Integrity {
    pub uid_checksum_ok: bool,
    pub package_uid_matches_header: bool,
    pub controller_crc_ok: Option<bool>,
    pub data_crc_ok: Option<bool>,
    pub payload_hashes_ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub name: String,
    pub format: Format,
    pub sha256: String,
    pub size: u64,
    pub sis: Option<sis::SisInfo>,
    pub midlet: Option<jar::JarInfo>,
    pub descriptor: Option<jar::JadInfo>,
    pub integrity: Option<Integrity>,
    pub signatures: Option<cert::SignatureReport>,
    pub target: Target,
    pub target_evidence: &'static str,
    pub dependencies: Option<DependencyAnalysis>,
    pub binaries: Vec<Binary>,
    pub capabilities: Vec<String>,
    pub issues: Vec<String>,
    pub compatibility: InspectionVerdict,
    pub compatibility_scope: &'static str,
    pub emulator: &'static str,
    pub installation: &'static str,
    pub physical_device: &'static str,
    pub limitations: Vec<String>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recommendation {
    pub issue: String,
    pub advice: &'static str,
}

fn advice(code: &str) -> &'static str {
    match code {
        "expired-certificate" => "Проверьте реальную дату телефона. Истёкший сертификат — не доказательство плохой подписи; нужен обновлённый пакет или своя подпись. Не оставляйте на телефоне неверную дату.",
        "certificate-not-yet-valid" => "Дата проверки раньше начала действия сертификата: сверьте дату проверки и дату телефона.",
        "invalid-certificate" => "Подпись или цепочка не прошли математическую проверку: пакет изменён или повреждён. Скачайте его заново у издателя.",
        "unsupported-signature" => "Алгоритм подписи не поддерживается этой версией (например, DSA). Результат не «верно» и не «неверно».",
        "restricted-capabilities" => "Пакет запрашивает привилегированные возможности: нужны сертификат с нужными правами и политика телефона. Самоподпись не выдаёт их на штатной прошивке.",
        "unsupported-os" => "Нужен SIS для Symbian 9.x / выбранной редакции S60 и ARMv5. Подпись не сделает несовместимый бинарный файл совместимым.",
        "missing-dependency" => "Установите зависимость документированной издателем версии. Наличие зависимости на телефоне нельзя вывести из файла на карте.",
        "dependency-unverified" => "Объявлена зависимость, наличие которой на телефоне не проверено.",
        "unsigned-package" => "Пакет не подписан. Проверьте на телефоне: Диспетчер приложений → Настройки → Установка ПО. «Все» не даёт ограниченных возможностей.",
        "corrupt-package" => "Контрольные суммы или хеши файлов не сходятся: пакет повреждён или изменён. Не передавайте его на телефон.",
        "imports-undecoded" => "Импорты бинарных файлов не декодируются: отсутствие найденных зависимостей ничего не доказывает.",
        "platform-unsupported" => "Объявленная платформа пакета не принимается выбранной моделью.",
        "platform-target-unknown" => "Платформа пакета неизвестна или модель не выбрана: совместимость не выводится.",
        "platform-undeclared" => "Пакет не объявляет платформу.",
        "legacy-platform-not-assessed" => "S60 2nd Edition (Symbian 7.0s): проверенных правил нет, результат остаётся «неизвестно».",
        "target-os-mismatch" => "Выбранная версия ОС не совпадает с документированной для модели.",
        _ => "Запишите точный текст сообщения установщика и повторите проверку пакета.",
    }
}

fn ver3(v: Option<[i32; 3]>) -> Option<String> {
    v.map(|v| format!("{}.{}.{}", v[0], v[1], v[2]))
}
fn hex8(v: u32) -> String {
    format!("{v:#010x}")
}

fn version_ok(
    have: Option<[u32; 3]>,
    from: Option<[i32; 3]>,
    to: Option<[i32; 3]>,
) -> Option<bool> {
    let (have, from) = (have?, from?);
    let wide = |v: [i32; 3]| [v[0] as i64, v[1] as i64, v[2] as i64];
    let h = [have[0] as i64, have[1] as i64, have[2] as i64];
    Some(h >= wide(from) && to.map_or(true, |t| h <= wide(t)))
}

fn analyze_dependencies(
    info: &sis::SisInfo,
    target: &Target,
    prof: Option<&profile::ModelProfile>,
    provided: &[Provided],
    issues: &mut Vec<String>,
) -> DependencyAnalysis {
    let mut platforms = Vec::new();
    for d in info
        .dependencies
        .iter()
        .filter(|d| d.kind == "target-device")
    {
        let known = d.uid.and_then(profile::platform);
        let version_required =
            d.from_version.is_some_and(|v| v != [0, 0, 0]) || d.to_version.is_some();
        let relation = match (d.uid, prof) {
            (Some(uid), Some(p)) if p.platform_uids.contains(&uid) => {
                if version_required {
                    "product-version-unknown"
                } else {
                    "accepted-by-target"
                }
            }
            (Some(uid), Some(p)) if p.family == "symbian3" && uid == 0x2003_A678 => {
                "release-unverified"
            }
            (Some(_), Some(_)) if known.is_some() => "not-accepted-by-target",
            (Some(_), None) if known.is_some() => "target-unknown",
            _ => "unknown-platform-id",
        };
        platforms.push(PlatformDep {
            uid: d.uid.map(hex8).unwrap_or_default(),
            label: known
                .map(|k| k.name.to_string())
                .or_else(|| d.names.first().cloned())
                .unwrap_or_else(|| "?".into()),
            symbian: known.map(|k| k.symbian.to_string()),
            min_version: ver3(d.from_version),
            max_version: ver3(d.to_version),
            relation: relation.into(),
            source: known.map(|k| k.source.to_string()),
        });
    }
    let mut platform_verdict = if platforms.is_empty() {
        issues.push("platform-undeclared".into());
        "unknown"
    } else if prof.is_none() {
        issues.push("platform-target-unknown".into());
        "unknown"
    } else if platforms.iter().any(|p| p.relation == "accepted-by-target") {
        "supported-static"
    } else if platforms.iter().any(|p| {
        matches!(
            p.relation.as_str(),
            "unknown-platform-id" | "product-version-unknown" | "release-unverified"
        )
    }) {
        issues.push("platform-target-unknown".into());
        "unknown"
    } else {
        issues.push("platform-unsupported".into());
        "unsupported"
    };
    if let Some(p) = prof {
        if !target.os.is_empty() && p.symbian.is_some_and(|s| s != target.os) {
            issues.push("target-os-mismatch".into());
            platform_verdict = "unsupported";
        }
        if p.family == "s60v2" {
            issues.push("legacy-platform-not-assessed".into());
            platform_verdict = "unknown";
        }
    }

    let mut packages = Vec::new();
    for d in info.dependencies.iter().filter(|d| d.kind == "dependency") {
        let name = if d.names.is_empty() {
            d.uid.map(hex8).unwrap_or_default()
        } else {
            d.names.join(" / ")
        };
        let runtime = profile::runtime_for_name(&d.names.join(" "))
            .or_else(|| d.uid.and_then(profile::runtime_for_uid))
            .map(str::to_string);
        let hit = provided.iter().find(|p| Some(p.uid) == d.uid);
        let status = match hit {
            Some(p) => match version_ok(p.version, d.from_version, d.to_version) {
                Some(true) => "provided-in-library",
                Some(false) => "provided-version-too-old",
                None => "provided-version-unknown",
            },
            None => "required-unverified",
        };
        match status {
            "required-unverified" => issues.push("dependency-unverified".into()),
            "provided-version-too-old" => issues.push("missing-dependency".into()),
            _ => {}
        }
        packages.push(PackageDep {
            uid: d.uid.map(hex8),
            name,
            runtime,
            min_version: ver3(d.from_version),
            max_version: ver3(d.to_version),
            status: status.into(),
            matched_package: hit.map(|p| p.name.clone()),
        });
    }
    let verdict = if packages.is_empty() {
        "no-declared-dependencies"
    } else if packages
        .iter()
        .any(|p| p.status == "provided-version-too-old")
    {
        "blocked"
    } else if packages.iter().all(|p| p.status == "provided-in-library") {
        "provided-in-library"
    } else {
        "unresolved"
    };
    DependencyAnalysis {
        platforms,
        platform_verdict: platform_verdict.into(),
        packages,
        verdict: verdict.into(),
        imports: "not-decoded".into(),
    }
}

/// Анализ пакета. Ошибка — только если файл вообще нельзя разобрать как заявленный формат.
pub fn inspect_package(name: &str, raw: &[u8], opts: &Options) -> Result<Report, PkgError> {
    let format = Format::from_name(name)
        .ok_or_else(|| PkgError("Unsupported package format (SIS, SISX, JAR, JAD)".into()))?;
    if raw.is_empty() || raw.len() > jar::MAX_PACKAGE {
        return Err(PkgError("Package must be between 1 byte and 32 MiB".into()));
    }
    let t = &opts.target;
    let prof = profile::model(&t.model);
    let mut r = Report {
        schema: REPORT_SCHEMA,
        name: name.to_string(),
        format,
        sha256: sis::hex(&Sha256::digest(raw)),
        size: raw.len() as u64,
        sis: None,
        midlet: None,
        descriptor: None,
        integrity: None,
        signatures: None,
        target: Target {
            model: t.model.chars().take(120).collect(),
            rm: t.rm.chars().take(120).collect(),
            firmware: t.firmware.chars().take(120).collect(),
            os: t.os.chars().take(120).collect(),
        },
        target_evidence: "выбор пользователя; не аттестация устройства",
        dependencies: None,
        binaries: Vec::new(),
        capabilities: Vec::new(),
        issues: Vec::new(),
        compatibility: InspectionVerdict::Unknown,
        compatibility_scope: "только статический анализ; установка, доверие сертификатам телефона и запуск не установлены",
        emulator: "not-tested",
        installation: "not-tested",
        physical_device: "not-tested",
        limitations: vec![
            "Математика сертификатов не означает доверие телефона: корневые сертификаты, отзыв и привязки IMEI/SID/capabilities не проверялись.".into(),
            "Импорты бинарных файлов и экспорты DLL прошивки неизвестны: зависимость от системных библиотек не оценивается.".into(),
            "Вложенные и условные блоки SIS, пакеты с несколькими блоками данных раскрыты не полностью.".into(),
        ],
        recommendations: Vec::new(),
    };
    if prof.is_none() {
        r.limitations.push(
            "Профиль модели не выбран или неизвестен: совместимость с конкретным телефоном не выводится."
                .into(),
        );
    }
    let mut issues: Vec<String> = Vec::new();
    let mut incompatible = false;

    if format.is_sis() {
        let parsed = sis::parse(raw)?;
        let info = parsed.info;
        let hashes_ok = if info.data_units == 1 {
            Some(info.files.iter().all(|f| f.hash_ok != Some(false)))
        } else {
            None
        };
        let integrity = Integrity {
            uid_checksum_ok: info.uid_checksum_ok,
            package_uid_matches_header: info.package.uid_matches_header,
            controller_crc_ok: info.controller_crc_ok,
            data_crc_ok: info.data_crc_ok,
            payload_hashes_ok: hashes_ok,
        };
        if !integrity.uid_checksum_ok
            || !integrity.package_uid_matches_header
            || integrity.controller_crc_ok == Some(false)
            || integrity.data_crc_ok == Some(false)
            || hashes_ok == Some(false)
        {
            issues.push("corrupt-package".into());
            incompatible = true;
        }
        let sigs = cert::inspect_signatures(raw, opts.at)?;
        match sigs.status.as_str() {
            "unsigned" => issues.push("unsigned-package".into()),
            "invalid" => {
                issues.push("invalid-certificate".into());
                incompatible = true;
            }
            "unsupported" => issues.push("unsupported-signature".into()),
            _ => {}
        }
        for c in sigs.chains.iter().flat_map(|c| &c.certificates) {
            match c.validity.as_str() {
                "expired" => issues.push("expired-certificate".into()),
                "not-yet-valid" => issues.push("certificate-not-yet-valid".into()),
                _ => {}
            }
        }
        // Бинарные файлы (E32) — только заголовки.
        if info.data_units == 1 {
            for (path, blob) in parsed.blobs.iter().enumerate().filter_map(|(i, b)| {
                info.files
                    .iter()
                    .find(|f| f.file_index as usize == i && f.operation != 8)
                    .map(|f| (f.target.clone(), b))
            }) {
                if !e32::is_e32(blob) {
                    continue;
                }
                match e32::analyze(blob) {
                    Ok(b) => {
                        if b.restricted_capabilities
                            .as_ref()
                            .is_some_and(|c| !c.is_empty())
                        {
                            issues.push("restricted-capabilities".into());
                        }
                        let legacy = prof.is_some_and(|p| p.family == "s60v2");
                        if !legacy
                            && (b.kernel == "eka1"
                                || (prof.is_some()
                                    && matches!(b.cpu.as_str(), "x86" | "mcore" | "armv6")))
                        {
                            issues.push("unsupported-os".into());
                            incompatible = true;
                        }
                        r.capabilities
                            .extend(b.capabilities.clone().unwrap_or_default());
                        r.binaries.push(Binary {
                            target: path,
                            info: b,
                        });
                    }
                    Err(e) => r.limitations.push(format!("{path}: {e}")),
                }
            }
            if !r.binaries.is_empty() {
                issues.push("imports-undecoded".into());
            }
        } else {
            r.limitations
                .push("Несколько блоков данных: бинарные файлы не разобраны.".into());
        }
        let dep = analyze_dependencies(&info, t, prof, &opts.provided, &mut issues);
        if dep.platform_verdict == "unsupported" {
            issues.push("unsupported-os".into());
            incompatible = true;
        }
        if dep.verdict == "blocked" {
            incompatible = true;
        }
        r.dependencies = Some(dep);
        r.signatures = Some(sigs);
        r.integrity = Some(integrity);
        r.sis = Some(info);
    } else if format == Format::Jar {
        r.midlet = Some(jar::inspect_jar(raw)?);
        r.limitations.push(
            "JAR: проверены структура и манифест; MIDP/CLDC и разрешения телефона не проверялись."
                .into(),
        );
    } else {
        let paired = opts.jar.as_ref().map(|(n, b)| (n.as_str(), b.as_slice()));
        r.descriptor = Some(jar::inspect_jad(raw, paired)?);
        if paired.is_none() {
            r.limitations
                .push("JAD без парного JAR: имя и размер JAR не сверены.".into());
        }
    }

    r.capabilities.sort();
    r.capabilities.dedup();
    let mut seen = std::collections::HashSet::new();
    issues.retain(|i| seen.insert(i.clone()));
    // S60 2nd Edition: ни «совместимо», ни выводы по правилам 3-й/5-й редакций.
    r.compatibility = if prof.is_some_and(|p| p.family == "s60v2") || !incompatible {
        InspectionVerdict::Unknown
    } else {
        InspectionVerdict::Incompatible
    };
    r.recommendations = issues
        .iter()
        .map(|i| Recommendation {
            issue: i.clone(),
            advice: advice(i),
        })
        .collect();
    r.issues = issues;
    Ok(r)
}

/// Эвристический разбор текста ошибки установщика (источник подписан: сообщение пользователя).
pub fn diagnose(message: &str) -> Result<Recommendation, PkgError> {
    if message.chars().count() > 2000 {
        return Err(PkgError(
            "Installer message must be up to 2000 characters".into(),
        ));
    }
    let text = message.to_lowercase();
    let table: [(&str, &[&str]); 6] = [
        ("expired-certificate", &["expired", "истёк", "истек"]),
        (
            "restricted-capabilities",
            &["capabilit", "возможност", "доступ не разреш"],
        ),
        (
            "unsupported-os",
            &["unsupported os", "not compatible", "несовместим"],
        ),
        ("missing-dependency", &["dependenc", "зависим"]),
        ("invalid-certificate", &["certificate", "сертификат"]),
        ("unsigned-package", &["unsigned", "не подпис"]),
    ];
    let code = table
        .iter()
        .find(|(_, words)| words.iter().any(|w| text.contains(w)))
        .map_or("unknown", |(c, _)| *c);
    Ok(Recommendation {
        issue: code.to_string(),
        advice: advice(code),
    })
}
