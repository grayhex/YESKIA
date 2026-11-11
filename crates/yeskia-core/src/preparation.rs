//! Persistent first-run bench. Manual reports can never create device evidence.
use crate::{
    revive,
    workbench::{Versioned, Workbench},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub model: String,
    pub rm: String,
    pub firmware: String,
    pub os: String,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            model: "nokia-e72".into(),
            rm: "RM-530".into(),
            firmware: "054.005".into(),
            os: "9.3".into(),
        }
    }
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        if ![
            "nokia-e72",
            "nokia-e52",
            "nokia-n95",
            "nokia-n95-8gb",
            "nokia-n97-mini",
            "nokia-n8",
            "nokia-e7",
            "nokia-7610",
        ]
        .contains(&self.model.as_str())
            || self.rm.len() > 16
            || self.firmware.len() > 32
            || !self
                .rm
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !self
                .firmware
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'.')
            || !["9.2", "9.3", "9.4", "symbian3", "eka1"].contains(&self.os.as_str())
        {
            return Err(Error::InvalidArgument("PREPARATION_PROFILE".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Context {
    pub fingerprint: String,
    pub transport: String,
    pub driver_signature: String,
    pub mode: String,
    pub profile: Profile,
}
impl Context {
    pub fn key(&self) -> String {
        format!(
            "prep-{}",
            crate::fsutil::sha256_hex(
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    self.fingerprint,
                    self.transport,
                    self.driver_signature,
                    self.mode,
                    self.profile.model,
                    self.profile.rm,
                    self.profile.os,
                    self.profile.firmware,
                    revive::manifest_version()
                )
                .as_bytes()
            )
        )
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    #[default]
    NotStarted,
    Manual,
    Verified,
    Blocked,
    NotSupported,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Step {
    pub status: Status,
    pub source: String,
    pub checked_at_ms: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Progress {
    pub steps: BTreeMap<String, Step>,
}
pub const STEPS: &[&str] = &[
    "physical",
    "identity",
    "connection",
    "files-backup",
    "pim-backup",
    "network",
    "clock",
    "certificates",
    "rights",
    "tls",
    "contacts",
    "readiness",
];
pub fn load(store: &Workbench, context: &Context) -> Result<Versioned<Progress>> {
    context.profile.validate()?;
    store.load(&context.key())
}
pub fn record_manual(
    store: &Workbench,
    context: &Context,
    revision: i64,
    step: &str,
    status: Status,
) -> Result<Versioned<Progress>> {
    if !STEPS.contains(&step) || matches!(status, Status::Verified) {
        return Err(Error::InvalidArgument(
            "PREPARATION_MANUAL_IS_NOT_DEVICE_PROOF".into(),
        ));
    }
    let mut current = load(store, context)?;
    if current.revision != revision {
        return Err(Error::OperationFailed("LOCAL_EDIT_CONFLICT_RELOAD".into()));
    }
    current.value.steps.insert(
        step.into(),
        Step {
            status,
            source: "owner-manual-report".into(),
            checked_at_ms: crate::now_ms(),
        },
    );
    store.save(&context.key(), revision, current.value)
}
/// Called only by the native adapter after a completed operation, never by a UI status setter.
pub fn record_device_proof(
    store: &Workbench,
    context: &Context,
    step: &str,
    source: &str,
) -> Result<()> {
    if context.fingerprint == "offline"
        || context.transport == "mock"
        || !["connection", "files-backup", "pim-backup"].contains(&step)
    {
        return Err(Error::InvalidArgument("PREPARATION_PROOF_CONTEXT".into()));
    }
    let mut current = load(store, context)?;
    current.value.steps.insert(
        step.into(),
        Step {
            status: Status::Verified,
            source: source.into(),
            checked_at_ms: crate::now_ms(),
        },
    );
    store.save(&context.key(), current.revision, current.value)?;
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Readiness {
    pub schema: u32,
    pub bench: String,
    pub model: String,
    pub rm: String,
    pub firmware: String,
    pub os: String,
    pub transport: String,
    pub tls_manifest: String,
    pub steps: BTreeMap<String, Step>,
    pub ready: bool,
    pub excluded: Vec<String>,
}
pub fn report(context: &Context, progress: &Progress) -> Readiness {
    Readiness {
        schema: 1,
        bench: "E72-1".into(),
        model: context.profile.model.clone(),
        rm: context.profile.rm.clone(),
        firmware: context.profile.firmware.clone(),
        os: context.profile.os.clone(),
        transport: context.transport.clone(),
        tls_manifest: revive::manifest_version(),
        steps: progress.steps.clone(),
        ready: STEPS.iter().filter(|s| **s != "readiness").all(|s| {
            progress
                .steps
                .get(*s)
                .is_some_and(|v| matches!(v.status, Status::Verified))
        }),
        excluded: vec![
            "phone-identifiers".into(),
            "contacts".into(),
            "APN".into(),
            "passwords".into(),
            "local-paths".into(),
        ],
    }
}
