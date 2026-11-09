//! Абстракция транспорта. UI и сетевые/USB-реализации разделены:
//! фронтенд знает только `Engine`, реальные адаптеры (WPD/MTP, Mass Storage,
//! PC Suite) подключаются как отдельные реализации `Transport`.

use crate::device::{Device, DeviceId, TransportKind};
use crate::error::Result;
use crate::operations::{OperationContext, OperationData, OperationRequest};

pub trait Transport: Send + Sync {
    fn kind(&self) -> TransportKind;
    /// Перечисляет доступные сейчас устройства (без подключения).
    fn enumerate(&self) -> Result<Vec<Device>>;
    fn connect(&self, id: &DeviceId) -> Result<()>;
    fn disconnect(&self, id: &DeviceId) -> Result<()>;
    /// Физически ли устройство ещё на связи.
    fn is_present(&self, id: &DeviceId) -> bool;
    /// Выполняет операцию. Реализация обязана вызывать `ctx.checkpoint()`
    /// между шагами и фиксировать запись только атомарно в конце.
    fn execute(
        &self,
        id: &DeviceId,
        request: &OperationRequest,
        ctx: &OperationContext,
    ) -> Result<OperationData>;
}
