//! Private local documents with optimistic revisions and bounded undo history.
//! SQL errors never include document contents in diagnostics.
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
pub const MAX_DOCUMENT: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Versioned<T> {
    pub revision: i64,
    pub value: T,
}
pub struct Workbench {
    path: PathBuf,
}
fn db() -> Error {
    Error::Storage("LOCAL_WORKSPACE_DATABASE".into())
}
fn key(k: &str) -> Result<()> {
    if k.is_empty()
        || k.len() > 96
        || !k
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::InvalidArgument("LOCAL_WORKSPACE_KEY".into()));
    }
    Ok(())
}
impl Workbench {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let s = Self {
            path: root.join("workspace.sqlite"),
        };
        let c = s.connect()?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS documents (name TEXT PRIMARY KEY, revision INTEGER NOT NULL, payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS document_history(name TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(name,revision));").map_err(|_|db())?;
        Ok(s)
    }
    fn connect(&self) -> Result<Connection> {
        let c = Connection::open(&self.path).map_err(|_| db())?;
        c.busy_timeout(Duration::from_secs(5)).map_err(|_| db())?;
        Ok(c)
    }
    pub fn load<T: DeserializeOwned + Default>(&self, name: &str) -> Result<Versioned<T>> {
        key(name)?;
        let c = self.connect()?;
        let row: Option<(i64, String)> = c
            .query_row(
                "SELECT revision,payload FROM documents WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| db())?;
        match row {
            None => Ok(Versioned {
                revision: 0,
                value: T::default(),
            }),
            Some((revision, payload)) => {
                if payload.len() > MAX_DOCUMENT {
                    return Err(Error::LimitExceeded("LOCAL_DOCUMENT_LIMIT".into()));
                }
                Ok(Versioned {
                    revision,
                    value: serde_json::from_str(&payload)
                        .map_err(|_| Error::Storage("LOCAL_DOCUMENT_SCHEMA".into()))?,
                })
            }
        }
    }
    pub fn save<T: Serialize + Default>(
        &self,
        name: &str,
        revision: i64,
        value: T,
    ) -> Result<Versioned<T>> {
        key(name)?;
        let payload = serde_json::to_string(&value).map_err(|_| db())?;
        if payload.len() > MAX_DOCUMENT {
            return Err(Error::LimitExceeded("LOCAL_DOCUMENT_LIMIT".into()));
        }
        let mut c = self.connect()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| db())?;
        let old: Option<(i64, String)> = tx
            .query_row(
                "SELECT revision,payload FROM documents WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| db())?;
        if old.as_ref().map(|v| v.0).unwrap_or(0) != revision {
            return Err(Error::OperationFailed("LOCAL_EDIT_CONFLICT_RELOAD".into()));
        }
        let (rev, previous) =
            old.unwrap_or((0, serde_json::to_string(&T::default()).map_err(|_| db())?));
        {
            tx.execute(
                "INSERT INTO document_history(name,revision,payload) VALUES(?1,?2,?3)",
                params![name, rev, previous],
            )
            .map_err(|_| db())?;
        }
        let next = revision.checked_add(1).ok_or_else(db)?;
        tx.execute("INSERT INTO documents(name,revision,payload) VALUES(?1,?2,?3) ON CONFLICT(name) DO UPDATE SET revision=excluded.revision,payload=excluded.payload",params![name,next,payload]).map_err(|_|db())?;
        tx.execute("DELETE FROM document_history WHERE name=?1 AND revision NOT IN (SELECT revision FROM document_history WHERE name=?1 ORDER BY revision DESC LIMIT 20)",[name]).map_err(|_|db())?;
        tx.commit().map_err(|_| db())?;
        Ok(Versioned {
            revision: next,
            value,
        })
    }
    pub fn undo<T: DeserializeOwned>(&self, name: &str, revision: i64) -> Result<Versioned<T>> {
        key(name)?;
        let mut c = self.connect()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| db())?;
        let current: i64 = tx
            .query_row(
                "SELECT revision FROM documents WHERE name=?1",
                [name],
                |r| r.get(0),
            )
            .map_err(|_| db())?;
        if current != revision {
            return Err(Error::OperationFailed("LOCAL_EDIT_CONFLICT_RELOAD".into()));
        }
        let (old,payload):(i64,String)=tx.query_row("SELECT revision,payload FROM document_history WHERE name=?1 ORDER BY revision DESC LIMIT 1",[name],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_|db())?.ok_or_else(||Error::InvalidArgument("NOTHING_TO_UNDO".into()))?;
        let value = serde_json::from_str(&payload).map_err(|_| db())?;
        let next = current.checked_add(1).ok_or_else(db)?;
        tx.execute(
            "UPDATE documents SET revision=?2,payload=?3 WHERE name=?1",
            params![name, next, payload],
        )
        .map_err(|_| db())?;
        tx.execute(
            "DELETE FROM document_history WHERE name=?1 AND revision=?2",
            params![name, old],
        )
        .map_err(|_| db())?;
        tx.commit().map_err(|_| db())?;
        Ok(Versioned {
            revision: next,
            value,
        })
    }
    pub fn list<T: DeserializeOwned>(&self, prefix: &str) -> Result<Vec<T>> {
        key(prefix)?;
        let c = self.connect()?;
        let mut s = c
            .prepare("SELECT payload FROM documents WHERE name LIKE ?1 ORDER BY name")
            .map_err(|_| db())?;
        let rows = s
            .query_map([format!("{prefix}%")], |r| r.get::<_, String>(0))
            .map_err(|_| db())?;
        rows.map(|r| {
            let raw = r.map_err(|_| db())?;
            if raw.len() > MAX_DOCUMENT {
                return Err(Error::LimitExceeded("LOCAL_DOCUMENT_LIMIT".into()));
            }
            serde_json::from_str(&raw).map_err(|_| db())
        })
        .collect()
    }
}
pub fn local_id() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| Error::Internal("LOCAL_ID".into()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
