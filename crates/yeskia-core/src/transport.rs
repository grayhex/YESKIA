//! Абстракции транспорта. UI знает только `Engine`; реальные адаптеры (WPD/MTP,
//! Mass Storage, Serial/PC Suite, Bluetooth/OBEX) подключаются как независимые реализации.

use crate::device::{Availability, Device, DeviceId, SupportStatus, TransportKind};
use crate::error::{Error, Result};
use crate::operations::{FileEntry, Operation, OperationContext, OverwritePolicy};
use std::io::{Read, Write};

/// Перечисление устройств. Успешное перечисление **не** означает доступность операций.
pub trait DiscoveryProvider: Send + Sync {
    fn kind(&self) -> TransportKind;
    fn discover(&self) -> Result<Vec<Device>>;
}

/// Результат read-only probe для одной операции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub operation: Operation,
    pub availability: Availability,
    pub detail: Option<String>,
}

/// Транспорт конкретного соединения.
pub trait DeviceTransport: DiscoveryProvider {
    fn connect(&self, id: &DeviceId) -> Result<()>;
    fn disconnect(&self, id: &DeviceId) -> Result<()>;
    /// Физически ли устройство ещё на связи (быстрая проверка, вызывается часто).
    fn is_present(&self, id: &DeviceId) -> bool;

    /// Зрелость реализации операции в **коде** адаптера. `Unsupported` — адаптер не умеет.
    fn maturity(&self, op: Operation) -> SupportStatus;

    /// Зрелость операции для конкретного устройства (режим USB определяет доступный сервис).
    /// По умолчанию совпадает с `maturity`.
    fn maturity_for(&self, _id: &DeviceId, op: Operation) -> SupportStatus {
        self.maturity(op)
    }

    /// **Только чтение.** Probe не имеет права писать на устройство. Операции записи
    /// не проверяются probe и остаются `NotTested` до явной операции пользователя.
    fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>>;

    fn identity(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Identity>;

    fn files(&self) -> Option<&dyn FileProvider> {
        None
    }
    fn pim(&self) -> Option<&dyn PimProvider> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub model: String,
    pub firmware: Option<String>,
}

/// Файловые операции. Реализация обязана вызывать `ctx.checkpoint()` между шагами
/// и не выполнять `write` поверх существующего файла без `OverwritePolicy::Replace`.
pub trait FileProvider: Send + Sync {
    fn list(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<FileEntry>>;
    fn stat(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Option<FileEntry>>;
    fn read(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<u8>>;
    fn write(
        &self,
        id: &DeviceId,
        path: &str,
        data: &[u8],
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()>;

    /// Потоковое чтение в `sink`. Реализация между блоками вызывает `ctx.checkpoint()` и
    /// `ctx.report_progress()`. По умолчанию читает файл целиком (годится для малых файлов).
    fn read_to(
        &self,
        id: &DeviceId,
        path: &str,
        sink: &mut dyn Write,
        ctx: &OperationContext,
    ) -> Result<u64> {
        let data = self.read(id, path, ctx)?;
        sink.write_all(&data)?;
        Ok(data.len() as u64)
    }

    /// Потоковая запись из `source` (`len` — ожидаемый размер). По умолчанию через память.
    fn write_from(
        &self,
        id: &DeviceId,
        path: &str,
        source: &mut dyn Read,
        _len: u64,
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        let mut buf = Vec::new();
        source.read_to_end(&mut buf)?;
        self.write(id, path, &buf, overwrite, ctx)
    }
}

/// Контакты и SMS. Данные не покидают ядро через журнал; методы по умолчанию — «не поддерживается».
pub trait PimProvider: Send + Sync {
    fn contacts_count(&self, _id: &DeviceId, _ctx: &OperationContext) -> Result<u64> {
        Err(Error::CapabilityUnsupported("contacts.read".into()))
    }
    fn sms_count(&self, _id: &DeviceId, _ctx: &OperationContext) -> Result<u64> {
        Err(Error::CapabilityUnsupported("sms.read".into()))
    }
}
