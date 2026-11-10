//! Профили моделей для статической совместимости. Источник: реестр моделей s60modern
//! (`symport/phone/device_models.json`, MIT). Профиль — **заявленная** платформа модели, а не
//! проверка прошивки конкретного телефона.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub s60: &'static str,
    pub symbian: Option<&'static str>,
    /// `s60v3`/`s60v5`/`symbian3`/`s60v2`
    pub family: &'static str,
    pub platform_uids: &'static [u32],
    pub native_platform_uid: Option<u32>,
}

pub const MODELS: &[ModelProfile] = &[
    ModelProfile {
        id: "nokia-e52",
        name: "Nokia E52",
        s60: "3rd FP2",
        symbian: Some("9.3"),
        family: "s60v3",
        platform_uids: &[0x101F_7961, 0x1020_32BE, 0x1027_52AE],
        native_platform_uid: Some(0x101F_7961),
    },
    ModelProfile {
        id: "nokia-e72",
        name: "Nokia E72",
        s60: "3rd FP2",
        symbian: Some("9.3"),
        family: "s60v3",
        platform_uids: &[0x101F_7961, 0x1020_32BE, 0x1027_52AE],
        native_platform_uid: Some(0x101F_7961),
    },
    ModelProfile {
        id: "nokia-n95",
        name: "Nokia N95",
        s60: "3rd FP1",
        symbian: Some("9.2"),
        family: "s60v3",
        platform_uids: &[0x101F_7961, 0x1020_32BE],
        native_platform_uid: Some(0x1020_32BE),
    },
    ModelProfile {
        id: "nokia-n95-8gb",
        name: "Nokia N95 8GB",
        s60: "3rd FP1",
        symbian: Some("9.2"),
        family: "s60v3",
        platform_uids: &[0x101F_7961, 0x1020_32BE],
        native_platform_uid: Some(0x1020_32BE),
    },
    ModelProfile {
        id: "nokia-n95-family",
        name: "Nokia N95 (variant unknown)",
        s60: "3rd FP1",
        symbian: Some("9.2"),
        family: "s60v3",
        platform_uids: &[0x101F_7961, 0x1020_32BE],
        native_platform_uid: Some(0x1020_32BE),
    },
    ModelProfile {
        id: "nokia-n97-mini",
        name: "Nokia N97 mini",
        s60: "5th",
        symbian: Some("9.4"),
        family: "s60v5",
        platform_uids: &[0x1028_315F],
        native_platform_uid: Some(0x1028_315F),
    },
    ModelProfile {
        id: "nokia-n8",
        name: "Nokia N8-00",
        s60: "Symbian^3",
        symbian: None,
        family: "symbian3",
        platform_uids: &[0x2002_2E6D],
        native_platform_uid: Some(0x2002_2E6D),
    },
    ModelProfile {
        id: "nokia-e7",
        name: "Nokia E7-00",
        s60: "Symbian^3",
        symbian: None,
        family: "symbian3",
        platform_uids: &[0x2002_2E6D],
        native_platform_uid: Some(0x2002_2E6D),
    },
    ModelProfile {
        id: "nokia-7610",
        name: "Nokia 7610",
        s60: "2nd",
        symbian: Some("7.0s"),
        family: "s60v2",
        platform_uids: &[],
        native_platform_uid: None,
    },
];

pub fn model(id: &str) -> Option<&'static ModelProfile> {
    MODELS.iter().find(|m| m.id == id)
}

#[derive(Debug, Clone, Copy)]
pub struct Platform {
    pub uid: u32,
    pub name: &'static str,
    pub symbian: &'static str,
    pub family: &'static str,
    pub source: &'static str,
}

/// Идентификаторы платформ (`S60ProductID`) с источниками.
pub const PLATFORMS: &[Platform] = &[
    Platform {
        uid: 0x101F_7961,
        name: "S60 3rd Edition",
        symbian: "9.1",
        family: "s60-3rd",
        source: "profiles/nokia-e52.toml packaging.platform_dependency (documented, SymPort)",
    },
    Platform {
        uid: 0x1020_32BE,
        name: "S60 3rd Edition FP1",
        symbian: "9.2",
        family: "s60-3rd",
        source: "jellyfin-s60v3 klient/klient.pro comments (Symbian 9.2)",
    },
    Platform {
        uid: 0x1027_52AE,
        name: "S60 3rd Edition FP2",
        symbian: "9.3",
        family: "s60-3rd",
        source: "jellyfin-s60v3 klient/klient.pro comments (Symbian 9.3)",
    },
    Platform {
        uid: 0x1028_315F,
        name: "S60 5th Edition / Symbian^1",
        symbian: "9.4",
        family: "s60-5th",
        source: "jellyfin-s60v3 klient/klient.pro comments (Symbian^1)",
    },
    Platform {
        uid: 0x2002_2E6D,
        name: "Symbian^3",
        symbian: "^3",
        family: "symbian3",
        source: "jellyfin-s60v3 klient/klient.pro comments (Symbian^3)",
    },
    Platform {
        uid: 0x2003_A678,
        name: "Symbian Belle",
        symbian: "Belle",
        family: "symbian3",
        source: "jellyfin-s60v3 klient/klient.pro comments (Symbian Belle)",
    },
];

pub fn platform(uid: u32) -> Option<&'static Platform> {
    PLATFORMS.iter().find(|p| p.uid == uid)
}

/// Семейство runtime по объявленному имени зависимости (эвристика по имени).
pub fn runtime_for_name(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase();
    if n == "qt" {
        Some("qt")
    } else if n.starts_with("qt mobility") || n.starts_with("qtmobility") {
        Some("qt-mobility")
    } else if n.starts_with("python") || n.starts_with("pys60") {
        Some("pys60")
    } else if n.starts_with("open c") || n.starts_with("openc") || n.starts_with("pips") {
        Some("open-c")
    } else {
        None
    }
}

/// UID runtime, наблюдавшийся у зависимости «Qt» (jellyfin-s60v3 0.3.22).
pub fn runtime_for_uid(uid: u32) -> Option<&'static str> {
    (uid == 0x2001_E61C).then_some("qt")
}
