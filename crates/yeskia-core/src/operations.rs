//! Операции, очередь, отмена, прогресс и результаты.

use crate::device::{DeviceId, DeviceOrigin, Evidence};
use crate::error::{Error, ErrorInfo, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Идентификаторы операций матрицы возможностей.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    #[serde(rename = "identity.read")]
    IdentityRead,
    #[serde(rename = "files.list")]
    FilesList,
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "files.write")]
    FilesWrite,
    #[serde(rename = "contacts.read")]
    ContactsRead,
    #[serde(rename = "contacts.write")]
    ContactsWrite,
    #[serde(rename = "sms.read")]
    SmsRead,
    #[serde(rename = "sms.send")]
    SmsSend,
    /// Установка пакета — отдельная операция. Отправка SIS через `files.write` — НЕ установка.
    #[serde(rename = "packages.install")]
    PackagesInstall,
}

impl Operation {
    pub const ALL: [Operation; 9] = [
        Operation::IdentityRead,
        Operation::FilesList,
        Operation::FilesRead,
        Operation::FilesWrite,
        Operation::ContactsRead,
        Operation::ContactsWrite,
        Operation::SmsRead,
        Operation::SmsSend,
        Operation::PackagesInstall,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Operation::IdentityRead => "identity.read",
            Operation::FilesList => "files.list",
            Operation::FilesRead => "files.read",
            Operation::FilesWrite => "files.write",
            Operation::ContactsRead => "contacts.read",
            Operation::ContactsWrite => "contacts.write",
            Operation::SmsRead => "sms.read",
            Operation::SmsSend => "sms.send",
            Operation::PackagesInstall => "packages.install",
        }
    }

    /// Операции, меняющие состояние устройства: их исход при сбое может быть неизвестен
    /// и они никогда не повторяются автоматически и не используются в probe.
    pub fn is_write(self) -> bool {
        matches!(
            self,
            Operation::FilesWrite
                | Operation::ContactsWrite
                | Operation::SmsSend
                | Operation::PackagesInstall
        )
    }
}

/// Политика перезаписи. По умолчанию — запрет (никакого silent overwrite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    #[default]
    Never,
    /// Явное решение пользователя заменить существующий файл.
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OperationRequest {
    #[serde(rename = "identity.read")]
    IdentityRead,
    #[serde(rename = "files.list")]
    FilesList { path: String },
    #[serde(rename = "files.read")]
    FilesRead { path: String },
    #[serde(rename = "files.write")]
    FilesWrite {
        path: String,
        data: Vec<u8>,
        #[serde(default)]
        overwrite: OverwritePolicy,
    },
    /// Скачать файл с устройства в `dest` (абсолютный путь файла на компьютере).
    /// Пишется во временный файл, затем размещается без перезаписи (если не `Replace`).
    #[serde(rename = "files.download")]
    FilesDownload {
        path: String,
        dest: String,
        #[serde(default)]
        overwrite: OverwritePolicy,
    },
    /// Загрузить файл с компьютера (`source`) на устройство. Записью считается `files.write`.
    #[serde(rename = "files.upload")]
    FilesUpload {
        source: String,
        path: String,
        #[serde(default)]
        overwrite: OverwritePolicy,
    },
    #[serde(rename = "contacts.read")]
    ContactsRead,
    #[serde(rename = "sms.read")]
    SmsRead,
}

impl OperationRequest {
    pub fn operation(&self) -> Operation {
        match self {
            OperationRequest::IdentityRead => Operation::IdentityRead,
            OperationRequest::FilesList { .. } => Operation::FilesList,
            OperationRequest::FilesRead { .. } => Operation::FilesRead,
            OperationRequest::FilesWrite { .. } | OperationRequest::FilesUpload { .. } => {
                Operation::FilesWrite
            }
            OperationRequest::FilesDownload { .. } => Operation::FilesRead,
            OperationRequest::ContactsRead => Operation::ContactsRead,
            OperationRequest::SmsRead => Operation::SmsRead,
        }
    }
    pub fn path(&self) -> Option<&str> {
        match self {
            OperationRequest::FilesList { path }
            | OperationRequest::FilesRead { path }
            | OperationRequest::FilesWrite { path, .. }
            | OperationRequest::FilesDownload { path, .. }
            | OperationRequest::FilesUpload { path, .. } => Some(path),
            _ => None,
        }
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
    /// Содержимое остаётся в ядре; наружу — размер и контрольная сумма.
    FileRead {
        bytes: u64,
        sha256: String,
    },
    Written {
        bytes: u64,
        sha256: String,
        verified: bool,
        replaced: bool,
    },
    /// Файл скачан на компьютер: размер совпал с устройством, хеш подтверждён чтением с диска.
    Downloaded {
        bytes: u64,
        sha256: String,
        verified: bool,
        replaced: bool,
    },
    /// Только количество: контакты/SMS не попадают ни в журнал, ни в `Debug`.
    PimCount {
        count: u64,
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
    pub operation: Operation,
    pub origin: DeviceOrigin,
    pub outcome: Outcome,
    pub data: Option<OperationData>,
    pub error: Option<ErrorInfo>,
    /// Для записи: исход на устройстве неизвестен (сбой/отмена/таймаут/отключение посреди
    /// записи, провал read-back). Автоповтора нет; решает пользователь после проверки.
    pub uncertain: bool,
    pub duration_ms: u64,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
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

#[derive(Debug, Default)]
pub struct ProgressCell {
    done: AtomicU64,
    total: AtomicU64,
}

impl ProgressCell {
    pub fn set(&self, done: u64, total: u64) {
        self.total.store(total, Ordering::SeqCst);
        self.done.store(done, Ordering::SeqCst);
    }
    pub fn get(&self) -> Progress {
        Progress {
            done: self.done.load(Ordering::SeqCst),
            total: self.total.load(Ordering::SeqCst),
        }
    }
}

/// Контекст, который транспорт получает на время операции.
pub struct OperationContext {
    pub operation_id: String,
    token: CancellationToken,
    deadline: Instant,
    presence: Box<dyn Fn() -> bool + Send + Sync>,
    progress: Arc<ProgressCell>,
}

impl OperationContext {
    pub fn new(
        operation_id: String,
        token: CancellationToken,
        deadline: Instant,
        presence: Box<dyn Fn() -> bool + Send + Sync>,
        progress: Arc<ProgressCell>,
    ) -> Self {
        Self {
            operation_id,
            token,
            deadline,
            presence,
            progress,
        }
    }

    /// Точка проверки между шагами: отмена, таймаут и физическое присутствие устройства.
    pub fn checkpoint(&self) -> Result<()> {
        if self.token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Timeout);
        }
        if !(self.presence)() {
            return Err(Error::DeviceDisconnected);
        }
        Ok(())
    }

    pub fn report_progress(&self, done: u64, total: u64) {
        self.progress.set(done, total);
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}
