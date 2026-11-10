//! Темы S60 (issue #9): импорт, статическая проверка и предпросмотр на реальных файлах SymPort.

use std::path::PathBuf;
use yeskia_core::theme::{self, Status, Theme};

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/theme")
        .join(rel)
}

fn light() -> Theme {
    theme::load_folder(&fixture("light").display().to_string()).unwrap()
}

fn status<'a>(r: &'a theme::ThemeReport, id: &str) -> &'a theme::Check {
    r.checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("нет проверки {id}"))
}

#[test]
fn light_theme_is_consistent_and_matches_symport_golden() {
    let t = light();
    let r = theme::inspect(&t, None);
    // golden SymPort: tests/golden/theme-structure.json
    assert_eq!(r.bitmaps.len(), 71);
    assert_eq!(r.icons, 46);
    assert_eq!(r.package.uid, Some(0xE0E5_2C01));
    assert_eq!(r.package.targets.len(), 4);
    assert_eq!(r.structure, "consistent", "{:?}", r.checks);
    for id in [
        "mbm.structure",
        "mif.structure",
        "skn.structure",
        "mbm.pixels",
        "pkg.parse",
        "uid.consistency",
        "pkg.paths",
        "pkg.skin_support",
        "pkg.vendor",
    ] {
        assert_eq!(
            status(&r, id).status,
            Status::Pass,
            "{id}: {}",
            status(&r, id).detail
        );
    }
    assert_eq!(
        status(&r, "uid.range").status,
        Status::Warn,
        "тестовый диапазон 0xE… — предупреждение"
    );
    // что статика установить не может — всегда unknown
    assert_eq!(status(&r, "firmware.acceptance").status, Status::Unknown);
    assert_eq!(status(&r, "device.install").status, Status::Unknown);
    assert_eq!(
        (r.firmware_acceptance, r.installation, r.physical_device),
        ("unknown", "not-tested", "not-tested")
    );
    assert!(r.limitations.iter().any(|l| l.contains("не снимок экрана")));
    assert!(
        !r.palette.is_empty()
            && r.palette
                .iter()
                .all(|c| c.hex.starts_with('#') && c.contrast_on_white >= 1.0)
    );
    assert_eq!(r.skn_pid1, Some(0xE0E5_2C01));
}

#[test]
fn dark_theme_has_a_distinct_uid() {
    let d = theme::load_folder(&fixture("dark/e52cleardark.skn").display().to_string()).unwrap();
    let r = theme::inspect(&d, None);
    assert_eq!(r.package.uid, Some(0xE0E5_2C02));
    assert_eq!(r.structure, "consistent");
    assert_ne!(r.package.uid, theme::inspect(&light(), None).package.uid);
}

#[test]
fn geometry_is_compared_to_each_models_screen_without_claiming_compatibility() {
    let t = light(); // растры 240×320
    let e52 = theme::inspect(&t, Some("nokia-e52"));
    assert_eq!(status(&e52, "geometry").status, Status::Pass);
    assert_eq!(status(&e52, "pkg.platform").status, Status::Pass);
    // E72 — 320×240: ориентация другая, автоматически не подходит
    let e72 = theme::inspect(&t, Some("nokia-e72"));
    assert_eq!(status(&e72, "geometry").status, Status::Warn);
    assert!(status(&e72, "geometry")
        .detail
        .contains("другой ориентации"));
    // другой экран — соответствие не установлено
    let n8 = theme::inspect(&t, Some("nokia-n8"));
    assert_eq!(status(&n8, "geometry").status, Status::Unknown);
    assert_ne!(
        status(&n8, "pkg.platform").status,
        Status::Pass,
        "платформа S60 3rd не принимается Symbian^3"
    );
    let old = theme::inspect(&t, Some("nokia-7610"));
    assert_eq!(status(&old, "pkg.platform").status, Status::Unknown);
    // ни для какой модели нет «совместим»
    for r in [&e52, &e72, &n8, &old] {
        assert!(r.checks.iter().all(|c| c.id != "compatible"));
        assert_eq!(r.firmware_acceptance, "unknown");
    }
}

#[test]
fn theme_from_sis_is_extracted_and_checked() {
    let raw = std::fs::read(fixture("e52clearlight.sis")).unwrap();
    let t = theme::load_sis(&raw).unwrap();
    assert_eq!(t.name, "e52clearlight");
    assert_eq!(t.signed, Some(false));
    let r = theme::inspect(&t, Some("nokia-e52"));
    assert_eq!(r.package.source, "sis");
    assert_eq!(r.package.uid, Some(0xE0E5_2C01));
    assert_eq!(r.package.platform_deps, vec![0x101F_7961, 0x1028_315F]);
    assert_eq!(r.structure, "consistent", "{:?}", r.checks);
    assert_eq!(status(&r, "pkg.paths").status, Status::Pass);
    assert_eq!(status(&r, "pkg.signature").status, Status::Warn);
    // файлы те же, что и в папке
    let f = light();
    assert_eq!((t.skn, t.mbm, t.mif), (f.skn, f.mbm, f.mif));
    // не тема
    let probe = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pkg/sdk_probe_package.sis"),
    )
    .unwrap();
    assert!(theme::load_sis(&probe).is_err());
    assert!(theme::load_sis(b"junk").is_err());
}

#[test]
fn preview_renders_real_bitmaps_with_masks() {
    let t = light();
    let p = theme::preview(&t, 100).unwrap();
    assert!(!p.is_empty());
    assert!(
        p.iter().any(|b| b.masked),
        "цветные растры с 8-битной маской"
    );
    assert_eq!(
        (p[0].width, p[0].height),
        (240, 320),
        "первый растр — фон экрана"
    );
    for b in &p {
        let png = base64_decode(&b.png_base64);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 13, 10, 26, 10]);
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), b.width);
        assert_eq!(
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
            b.height
        );
    }
    assert_eq!(theme::preview(&t, 3).unwrap().len(), 3);
    // пиксели действительно декодированы: фон не однотонно-чёрный
    let (bitmaps, _) = theme::parse_mbm(&t.mbm);
    let d = theme::decode_bitmap(&t.mbm, &bitmaps[0]).unwrap();
    assert!(d
        .rgba
        .chunks_exact(4)
        .any(|px| px[0] != px[1] || px[1] != px[2] || px[0] > 200));
}

fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' => 62,
            _ => 63,
        }
    };
    let mut out = Vec::new();
    for c in s.as_bytes().chunks(4) {
        let n = c
            .iter()
            .filter(|&&b| b != b'=')
            .fold(0u32, |a, &b| a << 6 | val(b))
            << (6 * (4 - c.iter().filter(|&&b| b != b'=').count()));
        let k = c.iter().filter(|&&b| b != b'=').count() - 1;
        for i in 0..k {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

#[test]
fn damaged_themes_fail_structure_checks_and_never_panic() {
    let t = light();
    // обрезанный mbm
    let mut bad = Theme {
        mbm: t.mbm[..t.mbm.len() / 2].to_vec(),
        skn: t.skn.clone(),
        mif: t.mif.clone(),
        name: t.name.clone(),
        package: t.package.clone(),
        signed: None,
    };
    let r = theme::inspect(&bad, None);
    assert_eq!(r.structure, "broken");
    assert_eq!(status(&r, "mbm.structure").status, Status::Fail);
    // неверная длина в skn
    bad.mbm = t.mbm.clone();
    bad.skn[0] ^= 0x01;
    assert_eq!(
        status(&theme::inspect(&bad, None), "skn.structure").status,
        Status::Fail
    );
    // mif ссылается за пределы mbm
    bad.skn = t.skn.clone();
    bad.mif = t.mif.clone();
    let (aoff, _) = (
        u32::from_le_bytes(bad.mif[8..12].try_into().unwrap()) as usize,
        0,
    );
    bad.mif[aoff..aoff + 4].copy_from_slice(&(-9999i32).to_le_bytes());
    assert_eq!(
        status(&theme::inspect(&bad, None), "mif.structure").status,
        Status::Fail
    );
    // UID пакета не совпадает с PID1 в skn
    let mut other = light();
    other.package.uid = Some(0xE0E5_2CFF);
    let r = theme::inspect(&other, None);
    assert_eq!(status(&r, "uid.consistency").status, Status::Fail);
    assert_eq!(r.structure, "broken");
    // посторонние пути установки
    let mut paths = light();
    paths.package.targets.push("!:\\sys\\bin\\evil.exe".into());
    assert_eq!(
        status(&theme::inspect(&paths, None), "pkg.paths").status,
        Status::Fail
    );

    // усечения и битовые правки не вызывают паники
    for n in (0..t.mif.len()).step_by(61) {
        let x = Theme {
            mif: t.mif[..n].to_vec(),
            ..clone(&t)
        };
        let _ = theme::inspect(&x, Some("nokia-e52"));
    }
    for n in (0..t.skn.len()).step_by(211) {
        let x = Theme {
            skn: t.skn[..n].to_vec(),
            ..clone(&t)
        };
        let _ = theme::inspect(&x, None);
    }
    for n in (0..t.mbm.len()).step_by(9973) {
        let x = Theme {
            mbm: t.mbm[..n].to_vec(),
            ..clone(&t)
        };
        let _ = theme::inspect(&x, None);
        let _ = theme::preview(&x, 5);
    }
    let mut flipped = t.mbm.clone();
    for i in (0..flipped.len()).step_by(3331) {
        flipped[i] ^= 0xA5;
        let x = Theme {
            mbm: flipped.clone(),
            ..clone(&t)
        };
        let _ = theme::inspect(&x, None);
        let _ = theme::preview(&x, 5);
        flipped[i] ^= 0xA5;
    }
}

fn clone(t: &Theme) -> Theme {
    Theme {
        name: t.name.clone(),
        skn: t.skn.clone(),
        mbm: t.mbm.clone(),
        mif: t.mif.clone(),
        package: t.package.clone(),
        signed: t.signed,
    }
}

#[test]
fn folder_loading_is_strict() {
    let r = std::env::temp_dir().join(format!("yeskia-theme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    assert!(
        theme::load_folder(&r.display().to_string()).is_err(),
        "нет .skn"
    );
    assert!(theme::load_folder("relative").is_err());
    assert!(theme::load_folder(&r.join("x.txt").display().to_string()).is_err());
    // две темы в одной папке — нужно указать файл
    for v in ["light/e52clearlight", "dark/e52cleardark"] {
        for ext in ["skn", "mbm", "mif", "pkg"] {
            let src = fixture(&format!("{v}.{ext}"));
            std::fs::copy(&src, r.join(src.file_name().unwrap())).unwrap();
        }
    }
    assert!(theme::load_folder(&r.display().to_string()).is_err());
    let t = theme::load_folder(&r.join("e52cleardark.mbm").display().to_string()).unwrap();
    assert_eq!(t.name, "e52cleardark");
    // нет парных файлов
    std::fs::remove_file(r.join("e52cleardark.mif")).unwrap();
    assert!(theme::load_folder(&r.join("e52cleardark.skn").display().to_string()).is_err());
    let _ = std::fs::remove_dir_all(&r);
}
