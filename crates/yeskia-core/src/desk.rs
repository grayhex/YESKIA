//! Typed, local-only desktop workbench commands. No logs contain imported contacts.
use crate::{contacts as c, preparation as p, revive as t, workbench::Workbench, Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    ContactsLoad,
    ContactsSave {
        revision: i64,
        book: c::Book,
    },
    ContactsUndo {
        revision: i64,
    },
    ContactsImport {
        path: PathBuf,
        options: c::ImportOptions,
    },
    ContactsCsvPreview {
        path: PathBuf,
        options: c::ImportOptions,
    },
    ContactsExport {
        path: PathBuf,
        book: c::Book,
        format: String,
    },
    ContactsDuplicates {
        book: c::Book,
    },
    ContactsMerge {
        book: c::Book,
        ids: Vec<String>,
        primary: String,
    },
    ContactsDiff {
        before: c::Book,
        after: c::Book,
    },
    TlsState,
    TlsImport {
        id: String,
        path: PathBuf,
    },
    TlsExtract,
    TlsApplicability {
        target: t::Target,
        stack: String,
    },
    TlsProgress {
        fingerprint: String,
        firmware: String,
    },
    TlsRecord {
        fingerprint: String,
        firmware: String,
        revision: i64,
        item: String,
        status: String,
        confirm: bool,
    },
    TlsReport {
        fingerprint: String,
        profile: p::Profile,
        stack: String,
    },
    TlsClock {
        fingerprint: String,
        firmware: String,
        revision: i64,
        date: String,
    },
    PrepProfile,
    PrepSaveProfile {
        profile: p::Profile,
    },
}
fn json<T: Serialize>(v: T) -> Result<serde_json::Value> {
    serde_json::to_value(v).map_err(|_| Error::Internal("WORKBENCH_RESPONSE".into()))
}
pub fn write_new(path: &Path, raw: &[u8]) -> Result<()> {
    crate::fsutil::validate_host_path(path)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| Error::HostAccess("CHOOSE_NEW_EXPORT_FILENAME".into()))?;
    if f.write_all(raw).and_then(|_| f.sync_all()).is_err() {
        drop(f);
        let _ = std::fs::remove_file(path);
        return Err(Error::HostAccess("EXPORT_WRITE".into()));
    }
    Ok(())
}
fn read_contacts(path: &Path) -> Result<Vec<u8>> {
    crate::fsutil::validate_host_path(path)?;
    let f =
        std::fs::File::open(path).map_err(|_| Error::HostAccess("CONTACT_SOURCE_FILE".into()))?;
    if !f.metadata()?.is_file() {
        return Err(Error::InvalidArgument("CONTACT_SOURCE_FILE".into()));
    }
    let mut bytes = Vec::new();
    f.take(c::MAX_IMPORT as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > c::MAX_IMPORT {
        return Err(Error::LimitExceeded("CONTACT_IMPORT_LIMIT".into()));
    }
    Ok(bytes)
}
pub fn execute(root: &Path, request: Request) -> Result<serde_json::Value> {
    let store = Workbench::open(root)?;
    match request {
        Request::ContactsLoad => json(c::load(&store)?),
        Request::ContactsSave { revision, book } => json(c::save(&store, revision, book)?),
        Request::ContactsUndo { revision } => json(store.undo::<c::Book>("contacts", revision)?),
        Request::ContactsImport { path, options } => {
            json(c::import(&read_contacts(&path)?, &options)?)
        }
        Request::ContactsCsvPreview { path, options } => {
            let bytes = read_contacts(&path)?;
            json(c::preview_csv(&bytes, &options)?)
        }
        Request::ContactsExport { path, book, format } => {
            c::validate(&book)?;
            write_new(&path, &c::export(&book, &format)?)?;
            json(())
        }
        Request::ContactsDuplicates { book } => {
            c::validate(&book)?;
            json(c::duplicates(&book))
        }
        Request::ContactsMerge { book, ids, primary } => {
            c::validate(&book)?;
            json(c::merge_preview(&book, &ids, &primary)?)
        }
        Request::ContactsDiff { before, after } => {
            c::validate(&before)?;
            c::validate(&after)?;
            json(c::diff(&before, &after))
        }
        Request::TlsState => json(t::cache_state(root)?),
        Request::TlsImport { id, path } => json(t::import_file(root, &id, &path)?),
        Request::TlsExtract => json(t::extract_ssl(root)?),
        Request::TlsApplicability { target, stack } => json(t::applicability(&target, &stack)),
        Request::TlsProgress {
            fingerprint,
            firmware,
        } => json(store.load::<t::Progress>(&t::progress_key(&fingerprint, &firmware))?),
        Request::TlsRecord {
            fingerprint,
            firmware,
            revision,
            item,
            status,
            confirm,
        } => json(t::record_manual(
            &store,
            &t::progress_key(&fingerprint, &firmware),
            revision,
            &item,
            &status,
            confirm,
        )?),
        Request::TlsReport {
            fingerprint,
            profile,
            stack,
        } => json(t::report(root, &store, &fingerprint, profile, &stack)?),
        Request::TlsClock {
            fingerprint,
            firmware,
            revision,
            date,
        } => json(t::record_clock(
            &store,
            &t::progress_key(&fingerprint, &firmware),
            revision,
            &date,
        )?),
        Request::PrepProfile => json(store.load::<p::Profile>("bench-profile")?),
        Request::PrepSaveProfile { profile } => {
            profile.validate()?;
            let old = store.load::<p::Profile>("bench-profile")?;
            json(store.save("bench-profile", old.revision, profile)?)
        }
    }
}
