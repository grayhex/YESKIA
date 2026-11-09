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
    QueueFull,
    CapabilityUnsupported,
    Timeout,
    FingerprintMismatch,
    AlreadyExists,
    UnsafePath,
    VerificationFailed,
    TransportError,
    StorageError,
    InvalidArgument,
    Internal,
}

/// Что предложить пользователю. UI локализует; ядро автоматически ничего не повторяет.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAction {
    None,
    Reconnect,
    CheckCable,
    ChooseAnotherPath,
    WaitAndRetry,
    RetryManually,
    /// Исход записи неизвестен: сначала проверить содержимое устройства.
    VerifyDeviceContents,
}

/// Сериализуемое описание ошибки для UI. Сообщение уже очищено от IMEI/серийников.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    pub message: String,
    /// Можно ли повторить вручную. Ядро никогда не повторяет запись само.
    pub retryable: bool,
    pub action: UserAction,
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
    #[error("очередь операций устройства переполнена")]
    QueueFull,
    #[error("операция не поддерживается на этом соединении: {0}")]
    CapabilityUnsupported(String),
    #[error("превышено время ожидания операции")]
    Timeout,
    #[error("на соединении другое устройство (fingerprint не совпал)")]
    FingerprintMismatch,
    #[error("целевой файл уже существует")]
    AlreadyExists,
    #[error("небезопасный путь: {0}")]
    UnsafePath(String),
    #[error("запись не подтверждена обратным чтением")]
    VerificationFailed,
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
            Error::QueueFull => ErrorCode::QueueFull,
            Error::CapabilityUnsupported(_) => ErrorCode::CapabilityUnsupported,
            Error::Timeout => ErrorCode::Timeout,
            Error::FingerprintMismatch => ErrorCode::FingerprintMismatch,
            Error::AlreadyExists => ErrorCode::AlreadyExists,
            Error::UnsafePath(_) => ErrorCode::UnsafePath,
            Error::VerificationFailed => ErrorCode::VerificationFailed,
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
                | Error::QueueFull
                | Error::Timeout
                | Error::VerificationFailed
                | Error::Transport(_)
        )
    }

    pub fn user_action(&self) -> UserAction {
        match self {
            Error::DeviceNotFound(_) | Error::Transport(_) => UserAction::CheckCable,
            Error::DeviceDisconnected | Error::NotConnected | Error::FingerprintMismatch => {
                UserAction::Reconnect
            }
            Error::QueueFull => UserAction::WaitAndRetry,
            Error::AlreadyExists | Error::UnsafePath(_) => UserAction::ChooseAnotherPath,
            Error::VerificationFailed => UserAction::VerifyDeviceContents,
            Error::OperationFailed(_) | Error::Timeout | Error::Cancelled => {
                UserAction::RetryManually
            }
            _ => UserAction::None,
        }
    }

    /// Ошибки, возникающие до любого обращения к устройству на запись:
    /// после них содержимое устройства заведомо не менялось.
    pub fn is_precondition(&self) -> bool {
        matches!(
            self,
            Error::AlreadyExists
                | Error::UnsafePath(_)
                | Error::InvalidArgument(_)
                | Error::CapabilityUnsupported(_)
                | Error::NotConnected
                | Error::QueueFull
                | Error::FingerprintMismatch
        )
    }

    /// Преобразование в DTO; текст проходит через редактор персональных данных.
    pub fn to_info(&self) -> ErrorInfo {
        ErrorInfo {
            code: self.code(),
            message: crate::diagnostics::redact(&self.to_string()),
            retryable: self.retryable(),
            action: self.user_action(),
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
