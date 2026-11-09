//! Структурированные ошибки ядра. Попадают во фронтенд как `ErrorInfo`.

use serde::{Deserialize, Serialize};

/// Стабильные машинные коды ошибок (часть публичного контракта UI ↔ ядро).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    DeviceNotFound,
    DeviceDisconnected,
    AlreadyConnected,
    NotConnected,
    OperationCancelled,
    OperationFailed,
    OperationInProgress,
    CapabilityUnsupported,
    TransportError,
    StorageError,
    InvalidArgument,
    Internal,
}

/// Сериализуемое описание ошибки для UI. Сообщение уже очищено от IMEI/серийников.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    pub message: String,
    /// Можно ли безопасно повторить вручную. Ядро никогда не повторяет запись само.
    pub retryable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("устройство не найдено: {0}")]
    DeviceNotFound(String),
    #[error("устройство отключено во время операции")]
    DeviceDisconnected,
    #[error("устройство уже подключено")]
    AlreadyConnected,
    #[error("устройство не подключено")]
    NotConnected,
    #[error("операция отменена")]
    Cancelled,
    #[error("операция завершилась ошибкой: {0}")]
    OperationFailed(String),
    #[error("для устройства уже выполняется операция")]
    OperationInProgress,
    #[error("возможность не поддерживается: {0}")]
    CapabilityUnsupported(String),
    #[error("ошибка транспорта: {0}")]
    Transport(String),
    #[error("ошибка хранилища: {0}")]
    Storage(String),
    #[error("неверный аргумент: {0}")]
    InvalidArgument(String),
    #[error("внутренняя ошибка: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn code(&self) -> ErrorCode {
        match self {
            Error::DeviceNotFound(_) => ErrorCode::DeviceNotFound,
            Error::DeviceDisconnected => ErrorCode::DeviceDisconnected,
            Error::AlreadyConnected => ErrorCode::AlreadyConnected,
            Error::NotConnected => ErrorCode::NotConnected,
            Error::Cancelled => ErrorCode::OperationCancelled,
            Error::OperationFailed(_) => ErrorCode::OperationFailed,
            Error::OperationInProgress => ErrorCode::OperationInProgress,
            Error::CapabilityUnsupported(_) => ErrorCode::CapabilityUnsupported,
            Error::Transport(_) => ErrorCode::TransportError,
            Error::Storage(_) => ErrorCode::StorageError,
            Error::InvalidArgument(_) => ErrorCode::InvalidArgument,
            Error::Internal(_) => ErrorCode::Internal,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Error::DeviceDisconnected
                | Error::Cancelled
                | Error::OperationFailed(_)
                | Error::Transport(_)
        )
    }

    /// Преобразование в DTO; текст проходит через редактор персональных данных.
    pub fn to_info(&self) -> ErrorInfo {
        ErrorInfo {
            code: self.code(),
            message: crate::diagnostics::redact(&self.to_string()),
            retryable: self.retryable(),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Storage(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Storage(e.to_string())
    }
}
