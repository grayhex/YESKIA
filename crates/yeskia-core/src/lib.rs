//! YESKIA core: локальное ядро без привязки к HTTP/Web.
//!
//! Модули: `device`, `transport`, `operations`, `storage`, `catalog`,
//! `inspector`, `diagnostics`, `engine`.

pub mod app;
pub mod at;
pub mod backup;
pub mod catalog;
pub mod device;
pub mod diagnostics;
pub mod engine;
pub mod error;
pub mod fsutil;
pub mod host;
pub mod inspector;
pub mod library;
pub mod media;
pub mod mock;
pub mod operations;
pub mod paths;
pub mod pkg;
pub mod storage;
pub mod system;
pub mod theme;
pub mod transport;
pub mod volume;

pub use engine::Engine;
pub use error::{Error, ErrorCode, ErrorInfo, Result};

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
