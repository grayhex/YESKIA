//! Вердикт статической совместимости (issue #8). Анализ пакетов — в `pkg::inspect`.

use serde::{Deserialize, Serialize};

/// `Compatible` зарезервирован: без профиля прошивки и проверенной модели он **не выдаётся**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionVerdict {
    Compatible,
    Incompatible,
    Unknown,
}
