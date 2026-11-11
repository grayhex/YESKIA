use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use yeskia_core::{contacts, desk, preparation as p, workbench::Workbench};
#[derive(Default)]
pub(crate) struct WorkbenchState {
    pub active: Mutex<Option<p::Context>>,
    pub operations: Mutex<BTreeMap<String, p::Context>>,
    tls_operations: Mutex<BTreeMap<String, TlsPending>>,
}
#[tauri::command]
pub(crate) async fn local_workbench(
    state: tauri::State<'_, State>,
    request: desk::Request,
) -> Cmd<serde_json::Value> {
    blocking(&state, move |_| desk::execute(&data_dir(), request)).await
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PrepRequest {
    Load {
        device_id: Option<String>,
    },
    Record {
        device_id: Option<String>,
        revision: i64,
        expected_key: String,
        step: String,
        status: p::Status,
    },
    Report {
        device_id: Option<String>,
    },
}
#[derive(Serialize)]
struct PrepView {
    key: String,
    context: p::Context,
    progress: yeskia_core::workbench::Versioned<p::Progress>,
}
fn context(core: &AppCore, id: Option<&str>) -> yeskia_core::Result<p::Context> {
    let store = Workbench::open(&data_dir())?;
    let profile = store.load::<p::Profile>("bench-profile")?.value;
    profile.validate()?;
    let device = if let Some(id) = id {
        Some(
            core.engine
                .list_devices()?
                .into_iter()
                .find(|d| d.id.as_str() == id)
                .ok_or_else(|| Error::DeviceNotFound("PREPARATION_DEVICE".into()))?,
        )
    } else {
        None
    };
    let report = yeskia_discovery::run_doctor(&CollectOptions {
        wpd_listing: false,
        probe_com_busy: false,
    });
    let mut drivers = report
        .devices
        .iter()
        .map(|d| serde_json::to_string(&d.driver).unwrap_or_default())
        .collect::<Vec<_>>();
    drivers.sort();
    Ok(p::Context {
        fingerprint: device
            .as_ref()
            .and_then(|d| d.matrix.as_ref())
            .map(|m| m.device_fingerprint.clone())
            .unwrap_or_else(|| "offline".into()),
        transport: device
            .as_ref()
            .map(|d| {
                serde_json::to_value(d.transport)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string()
            })
            .unwrap_or_else(|| "offline".into()),
        driver_signature: yeskia_core::fsutil::sha256_hex(drivers.join("|").as_bytes()),
        mode: if core.get_settings()?.developer_mode {
            "developer"
        } else {
            "standard"
        }
        .into(),
        profile,
    })
}
#[tauri::command]
pub(crate) async fn preparation(
    state: tauri::State<'_, State>,
    work: tauri::State<'_, WorkbenchState>,
    request: PrepRequest,
) -> Cmd<serde_json::Value> {
    let core = Arc::clone(&state.0);
    let (id, record, export) = match request {
        PrepRequest::Load { device_id } => (device_id, None, false),
        PrepRequest::Record {
            device_id,
            revision,
            expected_key,
            step,
            status,
        } => (
            device_id,
            Some((revision, expected_key, step, status)),
            false,
        ),
        PrepRequest::Report { device_id } => (device_id, None, true),
    };
    let output = tauri::async_runtime::spawn_blocking(
        move || -> yeskia_core::Result<(p::Context, serde_json::Value)> {
            let c = context(&core, id.as_deref())?;
            let store = Workbench::open(&data_dir())?;
            if record.is_none() && !export && c.fingerprint != "offline" && c.transport != "mock" {
                let observed = core.engine.list_devices()?.into_iter().any(|d| {
                    d.origin == yeskia_core::device::DeviceOrigin::Real
                        && d.connection == yeskia_core::device::ConnectionState::Connected
                        && d.matrix.is_some_and(|m| {
                            m.device_fingerprint == c.fingerprint
                                && m.entries.iter().any(|e| {
                                    e.availability == yeskia_core::device::Availability::Available
                                        && e.evidence.iter().any(|p| {
                                            p.kind == yeskia_core::device::EvidenceKind::DeviceRun
                                                && p.device_fingerprint.as_deref()
                                                    == Some(c.fingerprint.as_str())
                                        })
                                })
                        })
                });
                if observed {
                    let previous = p::load(&store, &c)?;
                    if !previous
                        .value
                        .steps
                        .get("connection")
                        .is_some_and(|s| matches!(s.status, p::Status::Verified))
                    {
                        p::record_device_proof(
                            &store,
                            &c,
                            "connection",
                            "native-read-only-protocol-probe",
                        )?;
                    }
                }
            }
            let progress = if let Some((rev, expected_key, step, status)) = record {
                if c.key() != expected_key {
                    return Err(Error::OperationFailed(
                        "PREPARATION_CONTEXT_CHANGED_RELOAD".into(),
                    ));
                }
                p::record_manual(&store, &c, rev, &step, status)?
            } else {
                p::load(&store, &c)?
            };
            let value = if export {
                serde_json::to_value(p::report(&c, &progress.value))
            } else {
                serde_json::to_value(PrepView {
                    key: c.key(),
                    context: c.clone(),
                    progress,
                })
            }
            .map_err(|_| Error::Internal("PREPARATION_RESPONSE".into()))?;
            Ok((c, value))
        },
    )
    .await
    .map_err(|_| Error::Internal("PREPARATION_TASK".into()).to_info())?
    .map_err(|e| e.to_info())?;
    *work
        .active
        .lock()
        .map_err(|_| Error::Internal("PREPARATION_LOCK".into()).to_info())? = Some(output.0);
    Ok(output.1)
}
#[tauri::command]
pub(crate) async fn contacts_phone_read(
    state: tauri::State<'_, State>,
    native: tauri::State<'_, NativeState>,
    work: tauri::State<'_, WorkbenchState>,
    device_id: String,
) -> Cmd<serde_json::Value> {
    let before = context(&state.0, Some(&device_id)).map_err(|e| e.to_info())?;
    let result =
        super::read_personal_data(state.clone(), native, device_id.clone(), "contacts".into())
            .await?;
    let after = context(&state.0, Some(&device_id)).map_err(|e| e.to_info())?;
    if before.key() != after.key() || before.fingerprint == "offline" {
        return Err(Error::NotConnected.to_info());
    }
    let rows = result
        .as_array()
        .ok_or_else(|| Error::OperationFailed("PIM_CONTACT_ROWS".into()).to_info())?;
    let kind = if before.transport == "mock" {
        "mock"
    } else {
        "native-pccs-read"
    };
    let book =
        contacts::native_snapshot(rows, &before.fingerprint, kind).map_err(|e| e.to_info())?;
    let store = Workbench::open(&data_dir()).map_err(|e| e.to_info())?;
    let key = format!(
        "phone-contacts-{}",
        yeskia_core::fsutil::sha256_hex(before.fingerprint.as_bytes())
    );
    let old = store
        .load::<contacts::Book>(&key)
        .map_err(|e| e.to_info())?;
    let changes = contacts::diff(&old.value, &book);
    let hash = yeskia_core::fsutil::sha256_hex(
        &serde_json::to_vec(&book)
            .map_err(|_| Error::Internal("CONTACT_SERIALIZE".into()).to_info())?,
    );
    store
        .save(&key, old.revision, book.clone())
        .map_err(|e| e.to_info())?;
    if before.transport != "mock"
        && work
            .active
            .lock()
            .ok()
            .and_then(|v| v.clone())
            .is_some_and(|c| c.key() == before.key())
    {
        p::record_device_proof(
            &store,
            &before,
            "pim-backup",
            "native-pccs-contacts-snapshot",
        )
        .map_err(|e| e.to_info())?;
    }
    Ok(
        serde_json::json!({"book":book,"previous":old.value,"changes":changes,"hash":hash,"read_at_ms":now_ms(),"source":kind,"write_status":contacts::writable_status()}),
    )
}
#[tauri::command]
pub(crate) fn tls_open_author(id: String) -> Cmd<()> {
    let spec = yeskia_core::revive::spec(&id).map_err(|e| e.to_info())?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(spec.url)
            .spawn()
            .map(|_| ())
            .map_err(|_| Error::HostAccess("OPEN_AUTHOR_URL".into()).to_info())
    }
    #[cfg(not(windows))]
    {
        let _ = spec;
        Err(Error::HostAccess("OPEN_AUTHOR_URL_WINDOWS_ONLY".into()).to_info())
    }
}
#[derive(Clone)]
struct TlsPending {
    fingerprint: String,
    firmware: String,
    id: String,
    sha256: String,
}
#[tauri::command]
pub(crate) async fn tls_stage(
    state: tauri::State<'_, State>,
    work: tauri::State<'_, WorkbenchState>,
    device_id: String,
    id: String,
    target: yeskia_core::revive::Target,
    acknowledge: bool,
) -> Cmd<String> {
    let (operation, pending) = blocking(&state, move |core| {
        let applies = yeskia_core::revive::applicability(&target, "system-native");
        if !acknowledge
            || !applies.components.contains(&id)
            || ["blocked", "unsupported"].contains(&applies.status.as_str())
        {
            return Err(Error::InvalidArgument(
                "TLS_APPLICABILITY_OR_CONFIRMATION".into(),
            ));
        }
        if id != "mbedtls-lib" {
            return Err(Error::CapabilityUnsupported(
                "TLS_DLL_REQUIRES_MANUAL_PRIVILEGED_APPLY".into(),
            ));
        }
        let device = core
            .engine
            .list_devices()?
            .into_iter()
            .find(|d| d.id.as_str() == device_id)
            .ok_or_else(|| Error::DeviceNotFound("TLS_DEVICE".into()))?;
        let fingerprint = device.matrix.ok_or(Error::NotConnected)?.device_fingerprint;
        if let Some(actual) = device.identified_model {
            if !yeskia_core::revive::model_matches(&target.model, &actual) {
                return Err(Error::InvalidArgument("TLS_MODEL_MISMATCH".into()));
            }
        }
        let path = yeskia_core::revive::cache_path(&data_dir(), &id)?;
        let bytes = yeskia_core::revive::read_bounded(&path)?;
        let validated = yeskia_core::revive::validate(&id, &bytes)?;
        let pending = TlsPending {
            fingerprint,
            firmware: format!(
                "{}|{}|{}|{}",
                target.model, target.rm, target.os, target.firmware
            ),
            id,
            sha256: validated.sha256,
        };
        let operation = core.start_operation(
            &DeviceId::new(device_id),
            OperationRequest::FilesWrite {
                path: "C:/Data/MBedTLS.sis".into(),
                data: bytes,
                overwrite: yeskia_core::operations::OverwritePolicy::Never,
            },
        )?;
        Ok((operation, pending))
    })
    .await?;
    // Lock poisoning cannot discard an already submitted device operation.
    if let Ok(mut map) = work.tls_operations.lock() {
        map.insert(operation.clone(), pending);
    }

    Ok(operation)
}

pub(crate) fn observe_result(
    work: &WorkbenchState,
    result: &OperationResult,
) -> yeskia_core::Result<()> {
    let store = Workbench::open(&data_dir())?;
    if let Some(context) = work
        .operations
        .lock()
        .map_err(|_| Error::Internal("PREPARATION_LOCK".into()))?
        .remove(&result.operation_id)
    {
        if result.origin == yeskia_core::device::DeviceOrigin::Real
            && matches!(
                result.data,
                Some(yeskia_core::operations::OperationData::BackupCreated {
                    complete: true,
                    verified: true,
                    ..
                })
            )
            && result
                .evidence
                .iter()
                .any(|e| e.device_fingerprint.as_deref() == Some(context.fingerprint.as_str()))
        {
            p::record_device_proof(
                &store,
                &context,
                "files-backup",
                "file-provider-backup-complete-and-verified",
            )?;
        }
    }
    if let Some(tls) = work
        .tls_operations
        .lock()
        .map_err(|_| Error::Internal("TLS_LOCK".into()))?
        .remove(&result.operation_id)
    {
        let verified = matches!(&result.data,Some(yeskia_core::operations::OperationData::Written{sha256,verified:true,..}) if *sha256==tls.sha256)
            && result.outcome == yeskia_core::operations::Outcome::Success
            && !result.uncertain;
        let status = if result.uncertain {
            "unknown-check-phone-before-retry"
        } else if verified {
            if result.origin == yeskia_core::device::DeviceOrigin::Real {
                "transferred-read-back-verified"
            } else {
                "transferred-on-mock"
            }
        } else {
            "not-confirmed"
        };
        let key = yeskia_core::revive::progress_key(&tls.fingerprint, &tls.firmware);
        let mut old = store.load::<yeskia_core::revive::Progress>(&key)?;
        old.value.transferred.insert(
            tls.id,
            yeskia_core::revive::Observation {
                status: status.into(),
                source: "file-provider; package staging is not installation".into(),
                checked_at_ms: now_ms(),
            },
        );
        store.save(&key, old.revision, old.value)?;
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
