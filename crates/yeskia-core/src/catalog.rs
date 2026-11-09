//! Офлайн-каталог приложений/тем (issue #8). Пока — только типы-заготовки.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Sis,
    Sisx,
    Jar,
    Theme,
    Ringtone,
}
