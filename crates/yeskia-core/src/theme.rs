//! Темы S60 3rd/5th (скины `.skn` + `.mbm` + `.mif`): локальный импорт, статическая проверка и
//! предпросмотр ресурсов. Порт `tools/s60/s60inspect.py` и `symport/theme/inspect.py` (MIT).
//!
//! Границы:
//! * проверяется **внутренняя согласованность файлов** (длина, UID, смещения, индексы растров, пути
//!   установки); принятие темы прошивкой и установка на телефон — всегда `unknown`;
//! * предпросмотр — это растры, лежащие в `.mbm`, а не снимок экрана телефона и не «как будет выглядеть
//!   система»: никакого вымышленного системного интерфейса Symbian;
//! * сборка (`makesis`/`signsis`/компилятор скинов) здесь не выполняется и подпись не ставится;
//! * тема не меняет геометрию системы (часы, меню, клавиши).

use crate::error::{Error, Result};
use crate::pkg::{profile, sis};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MBM_UID1: u32 = 0x1000_0037;
pub const MBM_UID2: u32 = 0x1000_0042;
pub const MIF_UID: i32 = 0x3423_2342;
const S60_3_UID: u32 = 0x101F_7961;
const SKINS_SUPPORT_UID: u32 = 0xA000_00EB;
const SCALABLE_SKINS_UID: u32 = 0x1020_7113;
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_BITMAPS: usize = 4096;
const MAX_PIXELS: u64 = 4_000_000;

fn invalid<T>(m: impl Into<String>) -> Result<T> {
    Err(Error::InvalidArgument(m.into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    Warn,
    Unknown,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub id: String,
    pub status: Status,
    pub detail: String,
}

/// Описание пакета темы: из `.pkg` рядом с файлами или из SIS.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PackageMeta {
    /// `pkg` | `sis` | `none`
    pub source: String,
    pub name: String,
    pub uid: Option<u32>,
    pub vendor: String,
    pub platform_deps: Vec<u32>,
    pub package_deps: Vec<u32>,
    /// Целевые пути установки.
    pub targets: Vec<String>,
}

pub struct Theme {
    pub name: String,
    pub skn: Vec<u8>,
    pub mbm: Vec<u8>,
    pub mif: Vec<u8>,
    pub package: PackageMeta,
    /// Для темы из SIS: подпись есть/нет (проверка подписи — в инспекторе пакетов).
    pub signed: Option<bool>,
}

// ---------- загрузка ----------

fn read_limited(p: &Path) -> Result<Vec<u8>> {
    let lp = crate::fsutil::long_path(p);
    let meta = std::fs::metadata(&lp)
        .map_err(|_| Error::HostAccess(format!("файл недоступен: {}", p.display())))?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(Error::LimitExceeded(
            "файл темы должен быть обычным файлом до 16 МиБ".into(),
        ));
    }
    Ok(std::fs::read(lp)?)
}

/// Папка с файлами темы или путь к одному из них (`.skn`/`.mbm`/`.mif`/`.pkg`): рядом ищутся
/// остальные с тем же именем. В папке должна быть ровно одна тема.
pub fn load_folder(path: &str) -> Result<Theme> {
    let p = Path::new(path);
    crate::fsutil::validate_host_path(p)?;
    let lp = crate::fsutil::long_path(p);
    let (dir, base): (PathBuf, String) = if lp.is_dir() {
        let mut skins: Vec<String> = std::fs::read_dir(&lp)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.to_ascii_lowercase().ends_with(".skn"))
            .map(|n| n[..n.len() - 4].to_string())
            .collect();
        skins.sort();
        match skins.len() {
            1 => (p.to_path_buf(), skins.remove(0)),
            0 => return invalid("в папке нет файла .skn"),
            _ => return invalid("в папке несколько тем: укажите файл .skn"),
        }
    } else {
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::UnsafePath("имя файла не в Unicode".into()))?
            .to_string();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !["skn", "mbm", "mif", "pkg"].contains(&ext.as_str()) {
            return invalid("укажите файл .skn, .mbm, .mif, .pkg или папку темы");
        }
        (
            p.parent()
                .ok_or_else(|| Error::UnsafePath("нет папки".into()))?
                .to_path_buf(),
            stem,
        )
    };
    let get = |ext: &str| -> Result<Vec<u8>> { read_limited(&dir.join(format!("{base}.{ext}"))) };
    let pkg_path = dir.join(format!("{base}.pkg"));
    let package = if crate::fsutil::long_path(&pkg_path).is_file() {
        parse_pkg(&String::from_utf8_lossy(&read_limited(&pkg_path)?))?
    } else {
        PackageMeta {
            source: "none".into(),
            ..Default::default()
        }
    };
    Ok(Theme {
        skn: get("skn")?,
        mbm: get("mbm")?,
        mif: get("mif")?,
        name: base,
        package,
        signed: None,
    })
}

fn tail_name(dest: &str) -> String {
    dest.rsplit(['\\', '/'])
        .next()
        .unwrap_or(dest)
        .to_ascii_lowercase()
}

/// Тема внутри SIS: `.skn`, `.mbm`, `.mif` берутся по расширению целевого пути.
pub fn load_sis(raw: &[u8]) -> Result<Theme> {
    let parsed = sis::parse(raw).map_err(|e| Error::InvalidArgument(e.0))?;
    if parsed.info.data_units != 1 {
        return invalid("пакеты с несколькими блоками данных не поддерживаются");
    }
    let mut parts: [Option<(String, Vec<u8>)>; 3] = [None, None, None];
    for f in parsed.info.files.iter().filter(|f| f.operation != 8) {
        let name = tail_name(&f.target);
        let slot = if name.ends_with(".skn") {
            0
        } else if name.ends_with(".mbm") {
            1
        } else if name.ends_with(".mif") {
            2
        } else {
            continue;
        };
        let blob = parsed
            .blobs
            .get(f.file_index as usize)
            .ok_or_else(|| Error::InvalidArgument("в пакете нет данных файла".into()))?;
        if parts[slot]
            .replace((f.target.clone(), blob.clone()))
            .is_some()
        {
            return invalid("в пакете несколько файлов одного типа темы");
        }
    }
    let [Some((skn_t, skn)), Some((mbm_t, mbm)), Some((mif_t, mif))] = parts else {
        return invalid("в пакете нет полного набора .skn, .mbm и .mif");
    };
    let info = parsed.info;
    let package = PackageMeta {
        source: "sis".into(),
        name: info.package.names.first().cloned().unwrap_or_default(),
        uid: Some(info.package.uid),
        vendor: info.package.vendor_unique_name.clone(),
        platform_deps: info
            .dependencies
            .iter()
            .filter(|d| d.kind == "target-device")
            .filter_map(|d| d.uid)
            .collect(),
        package_deps: info
            .dependencies
            .iter()
            .filter(|d| d.kind == "dependency")
            .filter_map(|d| d.uid)
            .collect(),
        targets: vec![skn_t.clone(), mbm_t, mif_t],
    };
    let name = tail_name(&skn_t).trim_end_matches(".skn").to_string();
    Ok(Theme {
        name,
        skn,
        mbm,
        mif,
        package,
        signed: Some(info.signed),
    })
}

/// Разбор подмножества `.pkg`, которое выдаёт Nokia Skin Compiler.
pub fn parse_pkg(text: &str) -> Result<PackageMeta> {
    let mut m = PackageMeta {
        source: "pkg".into(),
        ..Default::default()
    };
    let mut have_header = false;
    let hex = |s: &str| -> Option<u32> {
        let s = s.trim();
        u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
            .ok()
            .or_else(|| s.parse().ok())
    };
    let quoted = |s: &str| -> Option<String> {
        let a = s.find('"')? + 1;
        let b = s[a..].find('"')? + a;
        Some(s[a..b].to_string())
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('&') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            m.name = quoted(rest)
                .ok_or_else(|| Error::InvalidArgument("не разобран заголовок .pkg".into()))?;
            let open = rest
                .find('(')
                .ok_or_else(|| Error::InvalidArgument("в заголовке .pkg нет UID".into()))?;
            let close = rest[open..]
                .find(')')
                .map(|c| c + open)
                .ok_or_else(|| Error::InvalidArgument("в заголовке .pkg нет UID".into()))?;
            m.uid = Some(
                hex(&rest[open + 1..close])
                    .ok_or_else(|| Error::InvalidArgument("неверный UID в .pkg".into()))?,
            );
            have_header = true;
        } else if line.starts_with('[') {
            let close = line.find(']').ok_or_else(|| {
                Error::InvalidArgument("не разобрана платформенная зависимость".into())
            })?;
            m.platform_deps.push(
                hex(&line[1..close])
                    .ok_or_else(|| Error::InvalidArgument("неверный UID платформы".into()))?,
            );
        } else if line.starts_with('(') {
            let close = line
                .find(')')
                .ok_or_else(|| Error::InvalidArgument("не разобрана зависимость".into()))?;
            m.package_deps.push(
                hex(&line[1..close])
                    .ok_or_else(|| Error::InvalidArgument("неверный UID зависимости".into()))?,
            );
        } else if let Some(rest) = line.strip_prefix(':') {
            m.vendor = quoted(rest).unwrap_or_default();
        } else if line.starts_with('"') {
            let (src, rest) = {
                let a = 1;
                let b = line[a..]
                    .find('"')
                    .map(|c| c + a)
                    .ok_or_else(|| Error::InvalidArgument("не разобрана строка файла".into()))?;
                (&line[a..b], &line[b + 1..])
            };
            let rest = rest
                .trim_start()
                .strip_prefix('-')
                .ok_or_else(|| Error::InvalidArgument("не разобрана строка файла".into()))?;
            let dest = quoted(rest)
                .ok_or_else(|| Error::InvalidArgument("не разобрана строка файла".into()))?;
            let _ = src;
            if !dest.is_empty() {
                m.targets.push(dest);
            }
        }
    }
    if !have_header {
        return Err(Error::InvalidArgument(
            "заголовок .pkg (#{...},(0xUID),...) не найден".into(),
        ));
    }
    Ok(m)
}

// ---------- структура файлов ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BitmapHeader {
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub color: u32,
    pub compression: u32,
    #[serde(skip)]
    pub offset: usize,
    #[serde(skip)]
    pub size: usize,
}

fn u32le(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Заголовки растров `.mbm` и найденные проблемы.
pub fn parse_mbm(d: &[u8]) -> (Vec<BitmapHeader>, Vec<String>) {
    let mut problems = Vec::new();
    if d.len() < 24 {
        return (vec![], vec!["mbm слишком короткий".into()]);
    }
    let (u1, u2) = (u32le(d, 0).unwrap(), u32le(d, 4).unwrap());
    if (u1, u2) != (MBM_UID1, MBM_UID2) {
        problems.push(format!("mbm: UID1/UID2 = {u1:08x}/{u2:08x}"));
    }
    let trailer = u32le(d, 16).unwrap() as usize;
    let Some(n) = u32le(d, trailer).map(|n| n as usize) else {
        problems.push("mbm: trailer за пределами файла".into());
        return (vec![], problems);
    };
    if n > MAX_BITMAPS || trailer + 4 + 4 * n > d.len() {
        problems.push("mbm: таблица смещений за пределами файла или слишком велика".into());
        return (vec![], problems);
    }
    let mut out = Vec::new();
    for i in 0..n {
        let o = u32le(d, trailer + 4 + 4 * i).unwrap() as usize;
        let h: Option<Vec<u32>> = (0..10).map(|k| u32le(d, o.saturating_add(4 * k))).collect();
        let Some(h) = h.filter(|_| o.checked_add(40).is_some_and(|e| e <= d.len())) else {
            problems.push(format!("mbm[{i}]: заголовок за пределами файла"));
            continue;
        };
        let (size, hdr, w, ht) = (h[0] as usize, h[1], h[2], h[3]);
        if hdr != 40
            || o.checked_add(size).map_or(true, |e| e > d.len())
            || w == 0
            || ht == 0
            || size < 40
        {
            problems.push(format!(
                "mbm[{i}]: некорректный заголовок (size={size} hdr={hdr} {w}x{ht})"
            ));
            continue;
        }
        out.push(BitmapHeader {
            width: w,
            height: ht,
            bpp: h[6],
            color: h[7],
            compression: h[9],
            offset: o,
            size,
        });
    }
    (out, problems)
}

/// Проверка `.mif`: число иконок и ссылки на растры `.mbm`.
pub fn check_mif(d: &[u8], mbm_count: usize) -> (Vec<String>, usize) {
    let mut problems = Vec::new();
    if d.len() < 16 {
        return (vec!["mif слишком короткий".into()], 0);
    }
    let i = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let (uid, ver, aoff, alen) = (i(0), i(4), i(8), i(12));
    if uid != MIF_UID {
        problems.push(format!("mif: UID {:08x}", uid as u32));
    }
    if ver != 2 {
        problems.push(format!("mif: версия {ver} (ожидалась 2)"));
    }
    if alen < 0 || aoff < 0 || alen as usize > 100_000 {
        return (vec!["mif: неверная таблица".into()], 0);
    }
    let (aoff, alen) = (aoff as usize, alen as usize);
    if alen % 2 == 1 {
        problems.push(format!("mif: число элементов нечётное ({alen})"));
    }
    if aoff.checked_add(8 * alen).map_or(true, |e| e > d.len()) {
        problems.push("mif: таблица смещений за пределами файла".into());
        return (problems, 0);
    }
    let mut used = std::collections::HashSet::new();
    for k in 0..alen {
        let off = i(aoff + 8 * k);
        if off > 0 {
            problems.push(format!("mif[{k}]: иконка внутри mif (SVG) — не ожидалась"));
        } else if off == 0 && k % 2 == 1 {
            // пустая маска
        } else if (-(off as i64)) as usize >= mbm_count {
            problems.push(format!(
                "mif[{k}]: индекс mbm {} вне диапазона (в mbm {mbm_count})",
                -(off as i64)
            ));
        } else if !used.insert(-(off as i64)) {
            problems.push(format!(
                "mif[{k}]: индекс mbm {} использован дважды",
                -(off as i64)
            ));
        }
    }
    if used.len() != mbm_count {
        problems.push(format!(
            "mif: не все растры mbm адресуются из mif ({} из {mbm_count})",
            used.len()
        ));
    }
    (problems, alen / 2)
}

/// Проверка `.skn`: заявленная длина, тип/версия, PID1, имена `.mbm`/`.mif` в теле.
pub fn check_skn(d: &[u8], base: &str) -> (Vec<String>, Option<u32>) {
    let mut problems = Vec::new();
    if d.len() < 12 {
        return (vec!["skn слишком короткий".into()], None);
    }
    let declared = u32le(d, 0).unwrap() as usize;
    if declared != d.len() {
        problems.push(format!(
            "skn: заявленная длина {declared} != размеру файла {}",
            d.len()
        ));
    }
    for ext in ["mbm", "mif"] {
        let name: Vec<u8> = format!("{base}.{ext}")
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        if !d.windows(name.len()).any(|w| w == name.as_slice()) {
            problems.push(format!("skn: не найдено имя .{ext} в теле"));
        }
    }
    (problems, u32le(d, 8))
}

// ---------- отчёт ----------

#[derive(Debug, Clone, Serialize)]
pub struct ThemeReport {
    pub schema: &'static str,
    pub name: String,
    pub bitmaps: Vec<BitmapHeader>,
    pub icons: usize,
    pub skn_pid1: Option<u32>,
    pub package: PackageMeta,
    pub signed: Option<bool>,
    pub uid_class: Option<&'static str>,
    pub checks: Vec<Check>,
    /// `consistent` — файлы согласованы между собой; `broken` — есть ошибки структуры.
    pub structure: &'static str,
    pub firmware_acceptance: &'static str,
    pub installation: &'static str,
    pub physical_device: &'static str,
    pub palette: Vec<PaletteColor>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaletteColor {
    pub hex: String,
    /// Доля пикселей наибольшего растра.
    pub share: f32,
    pub contrast_on_white: f32,
    pub contrast_on_black: f32,
}

pub fn uid_class(uid: u32) -> &'static str {
    match uid {
        0xE000_0000..=0xEFFF_FFFF => "test",
        0xA000_0000..=0xBFFF_FFFF => "unprotected",
        0..=0x7FFF_FFFF => "protected",
        _ => "other",
    }
}

fn lum(r: u8, g: u8, b: u8) -> f64 {
    let ch = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
}

/// Контраст WCAG двух цветов `#RRGGBB`.
pub fn contrast(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f32 {
    let (a, b) = (lum(fg.0, fg.1, fg.2), lum(bg.0, bg.1, bg.2));
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (((hi + 0.05) / (lo + 0.05)) * 100.0).round() as f32 / 100.0
}

fn check(id: &str, status: Status, detail: impl Into<String>) -> Check {
    Check {
        id: id.into(),
        status,
        detail: detail.into(),
    }
}

/// Статический анализ темы. `model` — профиль для проверки платформы и размера экрана (необязателен).
pub fn inspect(t: &Theme, model: Option<&str>) -> ThemeReport {
    let mut checks = Vec::new();
    let (bitmaps, mp) = parse_mbm(&t.mbm);
    checks.push(check(
        "mbm.structure",
        if mp.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        if mp.is_empty() {
            format!("{} растров", bitmaps.len())
        } else {
            mp.join("; ")
        },
    ));
    let (ip, icons) = check_mif(&t.mif, bitmaps.len());
    checks.push(check(
        "mif.structure",
        if ip.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        if ip.is_empty() {
            format!("{icons} иконок, каждый индекс указывает на растр .mbm")
        } else {
            ip.join("; ")
        },
    ));
    let (sp, pid1) = check_skn(&t.skn, &t.name);
    checks.push(check(
        "skn.structure",
        if sp.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        if sp.is_empty() {
            "заявленная длина совпадает с размером, имена .mbm/.mif есть в теле".to_string()
        } else {
            sp.join("; ")
        },
    ));

    // растры: декодируются ли данные
    let mut undecodable = Vec::new();
    for (i, b) in bitmaps.iter().enumerate() {
        if decode_bitmap(&t.mbm, b).is_err() {
            undecodable.push(i);
        }
    }
    checks.push(check(
        "mbm.pixels",
        if undecodable.is_empty() {
            Status::Pass
        } else {
            Status::Warn
        },
        if undecodable.is_empty() {
            "данные всех растров распаковываются и совпадают с размером".to_string()
        } else {
            format!(
                "не удалось разобрать растры: {:?}",
                &undecodable[..undecodable.len().min(10)]
            )
        },
    ));

    // пакет
    let p = &t.package;
    match (p.source.as_str(), p.uid) {
        ("none", _) => checks.push(check(
            "pkg.parse",
            Status::Skip,
            "файла .pkg или SIS нет: проверки пакета пропущены",
        )),
        (_, Some(uid)) => {
            checks.push(check(
                "pkg.parse",
                Status::Pass,
                format!(
                    "«{}» UID {uid:#010x}, файлов в пакете: {}",
                    p.name,
                    p.targets.len()
                ),
            ));
            if let Some(p1) = pid1 {
                checks.push(check(
                    "uid.consistency",
                    if p1 == uid {
                        Status::Pass
                    } else {
                        Status::Fail
                    },
                    format!("UID пакета {uid:#010x} и PID1 в .skn {p1:#010x}"),
                ));
            }
            let class = uid_class(uid);
            checks.push(check(
                "uid.range",
                if class == "test" { Status::Warn } else { Status::Pass },
                match class {
                    "test" => "тестовый диапазон 0xE…: общий для всех разработчиков; не публикуйте пакет с таким UID",
                    "protected" => "защищённый диапазон: нужен UID от Symbian Signed, самоподписанный пакет будет отклонён",
                    "unprotected" => "незащищённый диапазон: для уникальности нужен выделенный UID",
                    _ => "диапазон UID не распознан",
                },
            ));
            let hexid = format!("{uid:08x}");
            let want = [
                format!("\\resource\\skins\\{hexid}\\"),
                format!("\\private\\10207114\\import\\{hexid}\\"),
            ];
            let bad: Vec<_> = p
                .targets
                .iter()
                .filter(|d| {
                    let l = d.to_ascii_lowercase();
                    let l = l.trim_start_matches("!:");
                    !want.iter().any(|w| l.starts_with(w.as_str()))
                })
                .collect();
            if !p.targets.is_empty() {
                checks.push(check(
                    "pkg.paths",
                    if bad.is_empty() {
                        Status::Pass
                    } else {
                        Status::Fail
                    },
                    if bad.is_empty() {
                        "пути установки — resource\\skins и private\\10207114\\import".to_string()
                    } else {
                        format!("неожиданные пути установки: {bad:?}")
                    },
                ));
            }
            let missing: Vec<String> = [SKINS_SUPPORT_UID, SCALABLE_SKINS_UID]
                .iter()
                .filter(|u| !p.package_deps.contains(u))
                .map(|u| format!("{u:#x}"))
                .collect();
            checks.push(check(
                "pkg.skin_support",
                if missing.is_empty() {
                    Status::Pass
                } else {
                    Status::Warn
                },
                if missing.is_empty() {
                    "требует пакеты поддержки скинов 0xA00000EB и 0x10207113".to_string()
                } else {
                    format!("не объявлены зависимости: {missing:?}")
                },
            ));
            checks.push(check(
                "pkg.vendor",
                if p.vendor.is_empty() {
                    Status::Warn
                } else {
                    Status::Pass
                },
                format!(
                    "поставщик «{}» (текст из пакета, не подтверждённая личность)",
                    p.vendor
                ),
            ));
            // платформа относительно выбранной модели
            if let Some(m) = model.and_then(profile::model) {
                let declared: Vec<u32> = p.platform_deps.clone();
                let (st, d) = if m.family == "s60v2" {
                    (
                        Status::Unknown,
                        "S60 2nd Edition: проверенных правил нет".to_string(),
                    )
                } else if declared.is_empty() {
                    (Status::Unknown, "пакет не объявляет платформу".to_string())
                } else if declared.iter().any(|u| m.platform_uids.contains(u)) {
                    (
                        Status::Pass,
                        format!("платформа пакета принимается профилем «{}»", m.name),
                    )
                } else if !declared.contains(&S60_3_UID) || m.family != "s60v3" {
                    (
                        Status::Warn,
                        format!("платформа пакета не входит в профиль «{}»", m.name),
                    )
                } else {
                    (Status::Unknown, "платформа не подтверждена".to_string())
                };
                checks.push(check("pkg.platform", st, d));
            }
        }
        _ => checks.push(check("pkg.parse", Status::Fail, "UID пакета не найден")),
    }
    if let Some(false) = t.signed {
        checks.push(check(
            "pkg.signature",
            Status::Warn,
            "SIS не подписан: подпись и доверие телефона проверяет инспектор пакетов и сам телефон",
        ));
    }

    // геометрия относительно экрана модели
    let largest = bitmaps
        .iter()
        .filter(|b| b.bpp >= 16)
        .max_by_key(|b| b.width as u64 * b.height as u64);
    if let (Some(big), Some(m)) = (largest, model.and_then(profile::model)) {
        let (sw, sh) = m.screen;
        let same = (big.width, big.height) == (sw, sh);
        let swapped = (big.width, big.height) == (sh, sw);
        checks.push(check(
            "geometry",
            if same { Status::Pass } else if swapped { Status::Warn } else { Status::Unknown },
            if same {
                format!("крупнейший растр {}×{} совпадает с экраном «{}» по документации", big.width, big.height, m.name)
            } else if swapped {
                format!("крупнейший растр {}×{} — экран «{}» в другой ориентации ({sw}×{sh}); автоматически не подходит", big.width, big.height, m.name)
            } else {
                format!("крупнейший растр {}×{}, экран «{}» {sw}×{sh}: соответствие не установлено", big.width, big.height, m.name)
            },
        ));
    }

    checks.push(check(
        "firmware.acceptance",
        Status::Unknown,
        "принятие темы прошивкой не устанавливается статической проверкой",
    ));
    checks.push(check(
        "device.install",
        Status::Unknown,
        "установка на телефон не проверялась",
    ));

    let palette = largest.map(|b| dominant(&t.mbm, b)).unwrap_or_default();
    let broken = checks.iter().any(|c| c.status == Status::Fail);
    ThemeReport {
        schema: "yeskia.theme/1",
        name: t.name.clone(),
        icons,
        skn_pid1: pid1,
        package: t.package.clone(),
        signed: t.signed,
        uid_class: t.package.uid.map(uid_class),
        structure: if broken { "broken" } else { "consistent" },
        firmware_acceptance: "unknown",
        installation: "not-tested",
        physical_device: "not-tested",
        palette,
        limitations: vec![
            "Статическая проверка: .skn проверяется как оболочка (длина, UID, имена), не разбирается поэлементно.".into(),
            "Предпросмотр — растры из .mbm, а не снимок экрана телефона: системный интерфейс Symbian не воспроизводится.".into(),
            "Тема не меняет геометрию системы (часы, Active Standby, меню, клавиши).".into(),
            "Сборка, подпись и установка выполняются внешними инструментами и самим телефоном.".into(),
        ],
        bitmaps,
        checks,
    }
}

// ---------- растры ----------

pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// RGBA, 4 байта на пиксель.
    pub rgba: Vec<u8>,
}

fn unrle(data: &[u8], unit: usize, want: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(want);
    let mut i = 0;
    while i < data.len() && out.len() < want {
        let c = data[i] as i8;
        i += 1;
        if c >= 0 {
            let v = data
                .get(i..i + unit)
                .ok_or_else(|| Error::InvalidArgument("обрыв RLE".into()))?;
            for _ in 0..=(c as usize) {
                out.extend_from_slice(v);
            }
            i += unit;
        } else {
            let n = (-(c as i32)) as usize * unit;
            out.extend_from_slice(
                data.get(i..i + n)
                    .ok_or_else(|| Error::InvalidArgument("обрыв RLE".into()))?,
            );
            i += n;
        }
        if out.len() > want {
            return Err(Error::InvalidArgument("RLE длиннее заявленного".into()));
        }
    }
    Ok(out)
}

/// Распаковка растра в RGBA. Поддержаны 8 бит (серый), 16 бит (RGB565), 24 бита (BGR), 32 бита (BGRA).
pub fn decode_bitmap(mbm: &[u8], b: &BitmapHeader) -> Result<Decoded> {
    let (w, h) = (b.width as u64, b.height as u64);
    if w * h > MAX_PIXELS {
        return Err(Error::LimitExceeded("растр слишком велик".into()));
    }
    let stride = ((w * b.bpp as u64).div_ceil(32) * 4) as usize;
    let want = stride * h as usize;
    let body = &mbm[b.offset + 40..b.offset + b.size];
    let raw = match b.compression {
        0 => body.to_vec(),
        1 => unrle(body, 1, want)?,
        2 | 3 => unrle(body, 2, want)?,
        4 => unrle(body, 3, want)?,
        5 => unrle(body, 4, want)?,
        n => {
            return Err(Error::InvalidArgument(format!(
                "неизвестное сжатие растра {n}"
            )))
        }
    };
    if raw.len() != want {
        return Err(Error::InvalidArgument(
            "размер данных растра не совпадает".into(),
        ));
    }
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let row = &raw[y * stride..(y + 1) * stride];
        for x in 0..w as usize {
            let px: [u8; 4] = match b.bpp {
                8 => [row[x], row[x], row[x], 255],
                16 => {
                    let v = u16::from_le_bytes([row[2 * x], row[2 * x + 1]]);
                    let (r, g, bl) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                    [
                        (r << 3 | r >> 2) as u8,
                        (g << 2 | g >> 4) as u8,
                        (bl << 3 | bl >> 2) as u8,
                        255,
                    ]
                }
                24 => [row[3 * x + 2], row[3 * x + 1], row[3 * x], 255],
                32 => [row[4 * x + 2], row[4 * x + 1], row[4 * x], row[4 * x + 3]],
                n => {
                    return Err(Error::InvalidArgument(format!(
                        "глубина {n} бит не поддерживается"
                    )))
                }
            };
            rgba.extend_from_slice(&px);
        }
    }
    Ok(Decoded {
        width: b.width,
        height: b.height,
        rgba,
    })
}

fn dominant(mbm: &[u8], b: &BitmapHeader) -> Vec<PaletteColor> {
    let Ok(d) = decode_bitmap(mbm, b) else {
        return vec![];
    };
    let mut counts: std::collections::HashMap<(u8, u8, u8), u32> = std::collections::HashMap::new();
    for px in d.rgba.chunks_exact(4) {
        *counts
            .entry((px[0] & 0xF8, px[1] & 0xF8, px[2] & 0xF8))
            .or_default() += 1;
    }
    let total = (d.rgba.len() / 4) as f32;
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter()
        .take(6)
        .map(|((r, g, bl), n)| PaletteColor {
            hex: format!("#{r:02X}{g:02X}{bl:02X}"),
            share: (n as f32 / total * 1000.0).round() / 1000.0,
            contrast_on_white: contrast((r, g, bl), (255, 255, 255)),
            contrast_on_black: contrast((r, g, bl), (0, 0, 0)),
        })
        .collect()
}

// ---------- PNG и предпросмотр ----------

fn crc32(data: &[u8]) -> u32 {
    let mut c = flate2::Crc::new();
    c.update(data);
    c.sum()
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend(crc32(&body).to_be_bytes());
}

/// PNG (RGBA 8 бит) из пикселей.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
    let _ = z.write_all(&raw);
    let idat = z.finish().unwrap_or_default();
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend(width.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct BitmapPreview {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    /// PNG в base64 (для `data:image/png`). Растр с маской показан с прозрачностью маски.
    pub png_base64: String,
    pub masked: bool,
}

/// Предпросмотр растров (до `limit` штук). За цветным растром идущий 8-битный серый растр
/// того же размера считается его маской (так устроены скины S60) — это эвристика предпросмотра.
pub fn preview(t: &Theme, limit: usize) -> Result<Vec<BitmapPreview>> {
    let (bitmaps, problems) = parse_mbm(&t.mbm);
    if bitmaps.is_empty() {
        return Err(Error::InvalidArgument(if problems.is_empty() {
            "в .mbm нет растров".into()
        } else {
            problems.join("; ")
        }));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < bitmaps.len() && out.len() < limit {
        let b = &bitmaps[i];
        let Ok(mut img) = decode_bitmap(&t.mbm, b) else {
            i += 1;
            continue;
        };
        let mut masked = false;
        if b.bpp >= 16 {
            if let Some(m) = bitmaps.get(i + 1) {
                if m.bpp == 8 && m.color == 0 && (m.width, m.height) == (b.width, b.height) {
                    if let Ok(mask) = decode_bitmap(&t.mbm, m) {
                        for (px, mp) in img.rgba.chunks_exact_mut(4).zip(mask.rgba.chunks_exact(4))
                        {
                            px[3] = mp[0];
                        }
                        masked = true;
                        i += 1;
                    }
                }
            }
        }
        out.push(BitmapPreview {
            index: i - usize::from(masked),
            width: img.width,
            height: img.height,
            bpp: b.bpp,
            png_base64: base64(&png(img.width, img.height, &img.rgba)),
            masked,
        });
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_wcag_examples() {
        assert_eq!(contrast((0, 0, 0), (255, 255, 255)), 21.0);
        assert_eq!(contrast((255, 255, 255), (255, 255, 255)), 1.0);
        let c = contrast((0x76, 0x76, 0x76), (255, 255, 255));
        assert!((4.5..4.6).contains(&c), "{c}"); // классический порог 4.5:1
    }

    #[test]
    fn base64_and_png_are_well_formed() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        let p = png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 128]);
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 13, 10, 26, 10]);
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
    }

    #[test]
    fn rle_decodes_runs_and_literals_and_rejects_overrun() {
        // run: 3 × 0xAA, literal: 2 байта
        assert_eq!(
            unrle(&[2, 0xAA, 0xFE, 1, 2], 1, 5).unwrap(),
            vec![0xAA, 0xAA, 0xAA, 1, 2]
        );
        assert!(unrle(&[2, 0xAA], 1, 2).is_err());
        assert!(unrle(&[0xFE, 1], 1, 2).is_err(), "обрыв литерала");
        assert_eq!(
            unrle(&[1, 0x34, 0x12], 2, 4).unwrap(),
            vec![0x34, 0x12, 0x34, 0x12]
        );
    }

    #[test]
    fn pkg_parser_reads_header_dependencies_and_files() {
        let text = "&EN\n#{\"Моя тема\"},(0xe0e52c01),1,0,0\n[0x101f7961], 0, 0, 0, {\"S60\"}\n(0xA00000EB), 0, 0, 0, {\"x\"}\n:\"vendor\"\n\"a.mbm\" - \"!:\\resource\\skins\\e0e52c01\\a.mbm\"\n\"\" - \"!:\\private\\x.ini\",FN\n";
        let p = parse_pkg(text).unwrap();
        assert_eq!(
            (p.name.as_str(), p.uid, p.vendor.as_str()),
            ("Моя тема", Some(0xE0E5_2C01), "vendor")
        );
        assert_eq!(p.platform_deps, vec![0x101F_7961]);
        assert_eq!(p.package_deps, vec![0xA000_00EB]);
        assert_eq!(p.targets.len(), 2);
        assert!(parse_pkg("; пусто").is_err());
        assert!(parse_pkg("#{\"x\"},(zzz),1,0,0").is_err());
    }

    #[test]
    fn uid_classes() {
        assert_eq!(uid_class(0xE0E5_2C01), "test");
        assert_eq!(uid_class(0xA000_0001), "unprotected");
        assert_eq!(uid_class(0x1020_0000), "protected");
        assert_eq!(uid_class(0xF000_0000), "other");
    }
}
