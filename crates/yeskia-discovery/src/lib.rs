//! Native Windows discovery и Doctor (issue #4).
//!
//! * `model` — снимок PnP/WPD/COM и трейт `SnapshotSource`;
//! * `doctor` — классификация состояний и privacy-safe отчёт;
//! * `fake` — поддельная топология для тестов;
//! * `windows_impl` (только Windows) — SetupAPI/CfgMgr32/WPD, **только чтение**;
//! * `adapter` (feature `engine`) — `DiscoveryProvider`/`DeviceTransport` для `yeskia-core`.
//!
//! Ничего не устанавливается, драйверы не подменяются, запись на устройство не выполняется.

pub mod doctor;
pub mod fake;
pub mod model;

#[cfg(windows)]
pub mod windows_impl;

#[cfg(all(windows, feature = "engine"))]
pub mod windows_serial;

#[cfg(feature = "engine")]
pub mod adapter;

pub use doctor::{analyze, analyze_with_refs, DoctorReport};
pub use model::{CollectOptions, SnapshotSource};

/// Источник снимка для текущей платформы. На не-Windows возвращает пустой снимок с ошибкой этапа.
pub fn system_source() -> Box<dyn SnapshotSource> {
    #[cfg(windows)]
    {
        Box::new(windows_impl::WindowsSource::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(UnsupportedPlatform)
    }
}

#[cfg(not(windows))]
struct UnsupportedPlatform;

#[cfg(not(windows))]
impl SnapshotSource for UnsupportedPlatform {
    fn collect(&self, _o: &CollectOptions) -> Result<model::Snapshot, model::CollectError> {
        Ok(model::Snapshot {
            os: model::OsInfo {
                product: std::env::consts::OS.into(),
                build: String::new(),
            },
            errors: vec![model::CollectError {
                stage: "unsupported_platform".into(),
                code: None,
            }],
            ..Default::default()
        })
    }
    fn wpd_identity(&self, _id: &str) -> Result<model::WpdIdentity, model::CollectError> {
        Err(model::CollectError {
            stage: "unsupported_platform".into(),
            code: None,
        })
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Сводка для консоли (`yeskia-app --doctor`): тот же privacy-safe отчёт.
pub fn run_doctor(options: &CollectOptions) -> DoctorReport {
    let source = system_source();
    let snap = source.collect(options).unwrap_or_else(|e| model::Snapshot {
        errors: vec![e],
        ..Default::default()
    });
    analyze(&snap, now_ms())
}

#[cfg(all(windows, feature = "engine"))]
pub mod windows_obex;
