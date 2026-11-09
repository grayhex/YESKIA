//! Реальные данные хоста для UI (не mock).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub is_windows: bool,
    pub logical_cpus: u32,
    pub data_dir: String,
    /// Всегда false: ядро не требует сети.
    pub network_required: bool,
}

impl SystemInfo {
    pub fn collect() -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            is_windows: cfg!(windows),
            logical_cpus: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(1),
            data_dir: crate::storage::data_dir().display().to_string(),
            network_required: false,
        }
    }
}
