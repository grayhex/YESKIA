//! Операции, отмена и результаты.

use crate::device::{DeviceId, DeviceOrigin, Evidence};
use crate::error::{Error, ErrorInfo, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationRequest {
    Identify,
    ListDirectory {
        path: String,
    },
    /// Запись файла. Не повторяется автоматически ни при каких ошибках.
    WriteFile {
        path: String,
        size: u64,
    },
}

impl OperationRequest {
    pub fn name(&self) -> &'static str {
        match self {
            OperationRequest::Identify => "identify",
            OperationRequest::ListDirectory { .. } => "list_directory",
            OperationRequest::WriteFile { .. } => "write_file",
        }
    }
    pub fn is_write(&self) -> bool {
        matches!(self, OperationRequest::WriteFile { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationData {
    Identity {
        model: String,
        firmware: Option<String>,
    },
    Entries {
        entries: Vec<FileEntry>,
    },
    Written {
        bytes: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResult {
    pub operation_id: String,
    pub device_id: DeviceId,
    pub operation: String,
    pub origin: DeviceOrigin,
    pub outcome: Outcome,
    pub data: Option<OperationData>,
    pub error: Option<ErrorInfo>,
    pub evidence: Vec<Evidence>,
}

/// Токен отмены (дёшево клонируется).
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Контекст, который транспорт получает на время операции.
pub struct OperationContext {
    pub operation_id: String,
    token: CancellationToken,
    presence: Box<dyn Fn() -> bool + Send + Sync>,
}

impl OperationContext {
    pub fn new(
        operation_id: String,
        token: CancellationToken,
        presence: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            operation_id,
            token,
            presence,
        }
    }

    /// Точка проверки между шагами: отмена и физическое присутствие устройства.
    pub fn checkpoint(&self) -> Result<()> {
        if self.token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if !(self.presence)() {
            return Err(Error::DeviceDisconnected);
        }
        Ok(())
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}
