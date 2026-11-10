//! Адаптер Windows-обнаружения к контрактам `yeskia-core`: `DiscoveryProvider` + `DeviceTransport`
//! + `FileProvider` (Mass Storage-том и read-only WPD/MTP).
//!
//! * Предлагаются устройства с подсказкой вендора Nokia (VID `0421`); модель **не** выводится из
//!   VID/PID — имя нейтральное, модель берётся из свойств WPD после `connect`.
//! * Что доступно, определяется режимом USB **этого** соединения: Mass Storage → файлы тома
//!   (запись — только после явной операции, `not-tested`), WPD → read-only листинг/скачивание,
//!   запись `unsupported`; COM → AT-канал 3GPP (только чтение: идентификация и счётчики хранилищ),
//!   если задан открыватель порта (`with_serial`), иначе ничего; неизвестный режим → ничего.
//! * Mass Storage ограничен томами, привязанными к устройству через PnP-цепочку; выйти за их
//!   пределы нельзя. Не-Nokia MTP-устройства видны Doctor'у, но в список YESKIA не попадают.

use crate::doctor::{analyze_with_refs, DeviceRef, DoctorState};
use crate::model::{CollectError, CollectOptions, SnapshotSource, WpdEntry};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use yeskia_core::at::{self, Query, SerialOpener, SerialPort};
use yeskia_core::device::*;
use yeskia_core::error::{Error, Result};
use yeskia_core::operations::{FileEntry, Operation, OperationContext, OverwritePolicy};
use yeskia_core::paths::validate_device_path;
use yeskia_core::transport::*;
use yeskia_core::volume::VolumeFileProvider;

/// Этапы сбора, без которых перечисление устройств недостоверно.
const CRITICAL_STAGES: &[&str] = &["setupapi_enum"];

type VolumeResolver = Arc<dyn Fn(char) -> PathBuf + Send + Sync>;

pub struct PnpTransport {
    source: Arc<dyn SnapshotSource>,
    /// Ключ сессии: id устройства стабилен до перезапуска, но не раскрывает instance id.
    key: RandomState,
    /// Буква тома → корень в файловой системе (`F:` → `F:\`); в тестах — временные папки.
    resolver: VolumeResolver,
    /// Открытие COM-портов для AT-канала. Без него COM-режим остаётся `unsupported`.
    serial: Option<Arc<dyn SerialOpener>>,
}

impl PnpTransport {
    pub fn new(source: Arc<dyn SnapshotSource>) -> Self {
        Self::with_volume_resolver(source, Arc::new(|c| PathBuf::from(format!("{c}:\\"))))
    }

    pub fn with_volume_resolver(source: Arc<dyn SnapshotSource>, resolver: VolumeResolver) -> Self {
        Self {
            source,
            key: RandomState::new(),
            resolver,
            serial: None,
        }
    }

    /// Включает AT-канал (стандарт 3GPP, только чтение) для устройств с виртуальным COM-портом.
    /// Это не протокол Nokia PC Suite; на реальном телефоне функция `not-tested`.
    pub fn with_serial(mut self, opener: Arc<dyn SerialOpener>) -> Self {
        self.serial = Some(opener);
        self
    }

    /// Открывает порт устройства, выполняет `f` и закрывает порт: порт не удерживается между операциями.
    fn with_port<T>(
        &self,
        id: &DeviceId,
        f: impl FnOnce(&mut dyn SerialPort) -> Result<T>,
    ) -> Result<T> {
        let r = self.find(id)?;
        let Service::Serial(name) = self.service(&r) else {
            return Err(Error::CapabilityUnsupported("serial".into()));
        };
        let opener = self
            .serial
            .as_ref()
            .ok_or_else(|| Error::CapabilityUnsupported("serial".into()))?;
        let mut port = opener.open(&name)?;
        f(port.as_mut())
    }

    /// Explicit PIM request, bound to the selected physical device. No external process.
    pub fn read_personal(
        &self,
        id: &DeviceId,
        domain: &str,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value> {
        if domain == "sms" {
            let selected = self.find(id)?;
            let name = selected
                .com_port
                .ok_or_else(|| Error::CapabilityUnsupported("SMS_PORT_MISSING".into()))?;
            let opener = self
                .serial
                .as_ref()
                .ok_or_else(|| Error::CapabilityUnsupported("SMS_PORT_MISSING".into()))?;
            return yeskia_core::sms::read(opener.open(&name)?.as_mut(), cancel);
        }
        let selected = self.find(id)?;
        #[cfg(windows)]
        {
            crate::windows_obex::read(&selected.instance_id, domain, cancel)
        }
        #[cfg(not(windows))]
        {
            let _ = (selected, domain, cancel);
            Err(Error::CapabilityUnsupported(
                "NATIVE_WINDOWS_REQUIRED".into(),
            ))
        }
    }

    pub fn send_certificate(
        &self,
        id: &DeviceId,
        der: &[u8],
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<()> {
        let selected = self.find(id)?;
        #[cfg(windows)]
        {
            crate::windows_obex::send_certificate(&selected.instance_id, der, cancel)
        }
        #[cfg(not(windows))]
        {
            let _ = (selected, der, cancel);
            Err(Error::CapabilityUnsupported(
                "NATIVE_WINDOWS_REQUIRED".into(),
            ))
        }
    }

    fn id_for(&self, instance_id: &str) -> DeviceId {
        DeviceId::new(format!(
            "usb-{:016x}",
            self.key.hash_one(instance_id.to_ascii_lowercase())
        ))
    }

    fn refs(&self) -> Result<Vec<DeviceRef>> {
        let snap = self
            .source
            .collect(&CollectOptions::CHEAP)
            .map_err(|e| Error::Transport(format!("сбор PnP: {}", e.stage)))?;
        // Сбой самого PnP-перечисления — это не «устройств нет», а «не знаем»: ошибка транспорта,
        // чтобы Engine не закрывал живые соединения. Частичные сбои (WPD, COM) остаются в Doctor.
        if let Some(e) = snap
            .errors
            .iter()
            .find(|e| CRITICAL_STAGES.contains(&e.stage.as_str()))
        {
            return Err(Error::Transport(format!(
                "PnP-перечисление не удалось: {}",
                e.stage
            )));
        }
        Ok(analyze_with_refs(&snap, 0)
            .1
            .into_iter()
            .filter(|r| r.nokia_vendor_hint)
            .collect())
    }

    fn find(&self, id: &DeviceId) -> Result<DeviceRef> {
        self.refs()?
            .into_iter()
            .find(|r| &self.id_for(&r.instance_id) == id)
            .ok_or_else(|| Error::DeviceNotFound(id.as_str().to_string()))
    }

    /// Сервис устройства по его режиму USB.
    fn service(&self, r: &DeviceRef) -> Service {
        if r.state == DoctorState::MassStorage && !r.volumes.is_empty() {
            Service::MassStorage(r.volumes.clone())
        } else if let Some(w) = &r.wpd_device_id {
            Service::Wpd(w.clone())
        } else if let (DoctorState::ComPresent, Some(port), Some(_)) =
            (r.state, &r.com_port, &self.serial)
        {
            Service::Serial(port.clone())
        } else {
            Service::None
        }
    }

    fn volume_for(&self, letters: &[char], path: &str) -> Result<(VolumeFileProvider, String)> {
        let norm = validate_device_path(path)?;
        let (letter, rest) = match norm.as_bytes() {
            [d, b':', b'/', ..] => (
                (*d as char).to_ascii_uppercase(),
                format!("/{}", &norm[3..]),
            ),
            _ if letters.len() == 1 => (letters[0], norm.clone()),
            _ => return Err(Error::UnsafePath("укажите диск, например F:/".into())),
        };
        // Только тома этого устройства: произвольные пути и чужие диски недоступны.
        if !letters.iter().any(|l| l.eq_ignore_ascii_case(&letter)) {
            return Err(Error::UnsafePath(
                "том не принадлежит этому устройству".into(),
            ));
        }
        let provider = VolumeFileProvider::new(&(self.resolver)(letter))?;
        Ok((provider, format!("{letter}:{rest}")))
    }
}

enum Service {
    MassStorage(Vec<char>),
    Wpd(String),
    /// Виртуальный COM-порт: AT-команды 3GPP только для чтения.
    Serial(String),
    None,
}

/// Время ожидания ответа на одну AT-команду внутри операции.
const AT_TIMEOUT: Duration = Duration::from_secs(4);

fn kind_and_label(r: &DeviceRef) -> (TransportKind, &'static str) {
    if r.state != DoctorState::MassStorage && r.wpd_device_id.is_some() {
        return (TransportKind::Mtp, "WPD/MTP");
    }
    match r.state {
        DoctorState::WpdOnly => (TransportKind::Mtp, "WPD/MTP"),
        DoctorState::ComPresent | DoctorState::ComBusy => (TransportKind::PcSuite, "COM"),
        DoctorState::MassStorage => (TransportKind::MassStorage, "Mass Storage"),
        DoctorState::DriverMissing => (TransportKind::Usb, "нет драйвера"),
        DoctorState::DriverError => (TransportKind::Usb, "ошибка драйвера"),
        DoctorState::UnknownMode => (TransportKind::Usb, "режим не определён"),
    }
}

fn wpd_err(e: CollectError) -> Error {
    match e.code {
        Some(0x8007_0002) | Some(0x8007_0003) => {
            Error::OperationFailed("объект не найден на устройстве".into())
        }
        _ => Error::Transport(format!("WPD: этап {}", e.stage)),
    }
}

/// `/Хранилище/Папка/файл` → компоненты. Для WPD допустим только корень `/`.
fn wpd_components(path: &str) -> Result<Vec<String>> {
    let norm = validate_device_path(path)?;
    if !norm.starts_with('/') {
        return Err(Error::UnsafePath("для WPD путь начинается с /".into()));
    }
    Ok(norm
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

impl DiscoveryProvider for PnpTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Usb
    }

    fn discover(&self) -> Result<Vec<Device>> {
        Ok(self
            .refs()?
            .into_iter()
            .map(|r| {
                let (transport, label) = kind_and_label(&r);
                Device {
                    id: self.id_for(&r.instance_id),
                    display_name: format!("USB {:04X}:{:04X} ({label})", r.vid, r.pid),
                    origin: DeviceOrigin::Real,
                    transport,
                    connection: ConnectionState::Disconnected,
                    serial: Some(Sensitive::new(r.instance_id)),
                    identified_model: None,
                    matrix: None,
                }
            })
            .collect())
    }
}

impl DeviceTransport for PnpTransport {
    fn connect(&self, id: &DeviceId) -> Result<()> {
        self.find(id).map(|_| ())
    }

    fn disconnect(&self, _id: &DeviceId) -> Result<()> {
        Ok(())
    }

    fn is_present(&self, id: &DeviceId) -> bool {
        self.find(id).is_ok()
    }

    fn maturity(&self, _op: Operation) -> SupportStatus {
        SupportStatus::Unsupported
    }

    fn maturity_for(&self, id: &DeviceId, op: Operation) -> SupportStatus {
        let Ok(r) = self.find(id) else {
            return SupportStatus::Unsupported;
        };
        match (self.service(&r), op) {
            // Реализовано, но на реальном телефоне ещё не проверено.
            (
                Service::Wpd(_),
                Operation::IdentityRead | Operation::FilesList | Operation::FilesRead,
            ) => SupportStatus::Implemented,
            // Файловая система тома проверена автотестами на реальной ФС хоста (без телефона).
            (
                Service::MassStorage(_),
                Operation::FilesList | Operation::FilesRead | Operation::FilesWrite,
            ) => SupportStatus::TestedOnHost,
            // AT по COM: код и контракт проверены на синтетических диалогах, телефон не проверялся.
            (
                Service::Serial(_),
                Operation::IdentityRead | Operation::ContactsRead | Operation::SmsRead,
            ) => SupportStatus::Implemented,
            // WPD: запись не реализована; всё остальное — пока нет.
            _ => SupportStatus::Unsupported,
        }
    }

    fn probe(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Vec<ProbeOutcome>> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        let mut out = Vec::new();
        let unsupported = |op, why: &str| ProbeOutcome {
            operation: op,
            availability: Availability::Unsupported,
            detail: Some(why.to_string()),
        };
        let from = |op, res: Result<()>| ProbeOutcome {
            operation: op,
            availability: if res.is_ok() {
                Availability::Available
            } else {
                Availability::Error
            },
            detail: res.err().map(|e| e.to_string()),
        };
        match self.service(&r) {
            Service::Wpd(wpd) => {
                out.push(from(
                    Operation::IdentityRead,
                    self.source.wpd_identity(&wpd).map(|_| ()).map_err(wpd_err),
                ));
                // read-only: перечисление хранилищ
                out.push(from(
                    Operation::FilesList,
                    self.source.wpd_list(&wpd, &[]).map(|_| ()).map_err(wpd_err),
                ));
            }
            Service::MassStorage(letters) => {
                let res = self
                    .volume_for(&letters, &format!("{}:/", letters[0]))
                    .and_then(|(p, path)| p.list(id, &path, ctx).map(|_| ()));
                out.push(from(Operation::FilesList, res));
                out.push(unsupported(
                    Operation::IdentityRead,
                    "в режиме Mass Storage нет идентификации по протоколу",
                ));
            }
            Service::Serial(_) => {
                // Один сеанс: проверка связи и идентификация, затем только счётчики хранилищ.
                // Содержимое контактов и сообщений не читается; ничего не записывается.
                let res = self.with_port(id, |p| {
                    let ident = at::identify(p, AT_TIMEOUT, ctx).map(|_| ());
                    let query = |p: &mut dyn SerialPort, q| -> Result<()> {
                        ctx.checkpoint()?;
                        at::exec(p, q, AT_TIMEOUT, ctx).map(|_| ())
                    };
                    let contacts = match &ident {
                        Ok(()) => Some(query(p, Query::PhonebookStatus)),
                        Err(_) => None,
                    };
                    let sms = match &ident {
                        Ok(()) => Some(query(p, Query::MessageStorageStatus)),
                        Err(_) => None,
                    };
                    Ok((ident, contacts, sms))
                });
                let as_outcome = |op, r: Result<()>| match r {
                    Ok(()) => ProbeOutcome {
                        operation: op,
                        availability: Availability::Available,
                        detail: None,
                    },
                    Err(Error::CapabilityUnsupported(_)) => ProbeOutcome {
                        operation: op,
                        availability: Availability::Unsupported,
                        detail: Some("телефон отклонил AT-запрос".into()),
                    },
                    Err(e) => ProbeOutcome {
                        operation: op,
                        availability: Availability::Error,
                        detail: Some(e.to_string()),
                    },
                };
                match res {
                    Ok((ident, contacts, sms)) => {
                        let blocked = ident.is_err();
                        out.push(as_outcome(Operation::IdentityRead, ident));
                        for (op, r) in [
                            (Operation::ContactsRead, contacts),
                            (Operation::SmsRead, sms),
                        ] {
                            out.push(match r {
                                Some(r) => as_outcome(op, r),
                                None => ProbeOutcome {
                                    operation: op,
                                    availability: Availability::NotTested,
                                    detail: Some(
                                        if blocked {
                                            "идентификация по AT не удалась"
                                        } else {
                                            ""
                                        }
                                        .into(),
                                    ),
                                },
                            });
                        }
                    }
                    // порт не открылся (занят другой программой и т. п.)
                    Err(e) => {
                        for op in [
                            Operation::IdentityRead,
                            Operation::ContactsRead,
                            Operation::SmsRead,
                        ] {
                            out.push(as_outcome(op, Err(clone_err(&e))));
                        }
                    }
                }
            }
            Service::None => {
                out.push(unsupported(
                    Operation::IdentityRead,
                    "в текущем режиме USB нет WPD-сервиса",
                ));
            }
        }
        Ok(out)
    }

    fn identity(&self, id: &DeviceId, ctx: &OperationContext) -> Result<Identity> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        if matches!(self.service(&r), Service::Serial(_)) {
            let i = self.with_port(id, |p| at::identify(p, AT_TIMEOUT, ctx))?;
            return Ok(Identity {
                model: i.model,
                firmware: i.revision,
            });
        }
        let Service::Wpd(wpd) = self.service(&r) else {
            return Err(Error::CapabilityUnsupported("identity.read".into()));
        };
        let i = self.source.wpd_identity(&wpd).map_err(wpd_err)?;
        Ok(Identity {
            model: i.model.unwrap_or_else(|| "модель не сообщена".to_string()),
            firmware: i.firmware,
        })
    }

    fn files(&self) -> Option<&dyn FileProvider> {
        Some(self)
    }

    fn pim(&self) -> Option<&dyn PimProvider> {
        Some(self)
    }
}

/// Только счётчики занятости хранилищ (AT+CPBS? / AT+CPMS?): записи и сообщения не читаются.
impl PimProvider for PnpTransport {
    fn contacts_count(&self, id: &DeviceId, ctx: &OperationContext) -> Result<u64> {
        ctx.checkpoint()?;
        self.with_port(id, |p| {
            at::drain(p);
            let r = at::exec(p, Query::PhonebookStatus, AT_TIMEOUT, ctx)?;
            Ok(at::parse_cpbs(&r)?.used)
        })
        .map_err(|e| match e {
            Error::CapabilityUnsupported(_) => Error::CapabilityUnsupported("contacts.read".into()),
            e => e,
        })
    }

    fn sms_count(&self, id: &DeviceId, ctx: &OperationContext) -> Result<u64> {
        ctx.checkpoint()?;
        self.with_port(id, |p| {
            at::drain(p);
            let r = at::exec(p, Query::MessageStorageStatus, AT_TIMEOUT, ctx)?;
            Ok(at::parse_cpms(&r)?.used)
        })
        .map_err(|e| match e {
            Error::CapabilityUnsupported(_) => Error::CapabilityUnsupported("sms.read".into()),
            e => e,
        })
    }
}

/// `Error` не `Clone`: копия для одинакового исхода по нескольким операциям.
fn clone_err(e: &Error) -> Error {
    match e {
        Error::CapabilityUnsupported(s) => Error::CapabilityUnsupported(s.clone()),
        Error::DeviceDisconnected => Error::DeviceDisconnected,
        Error::Timeout => Error::Timeout,
        Error::HostAccess(s) => Error::HostAccess(s.clone()),
        other => Error::Transport(other.to_string()),
    }
}

impl FileProvider for PnpTransport {
    fn list(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<FileEntry>> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        match self.service(&r) {
            Service::MassStorage(letters) => {
                // Несколько томов: корень «/» показывает сами тома.
                if letters.len() > 1 && validate_device_path(path)? == "/" {
                    return Ok(letters
                        .iter()
                        .map(|l| FileEntry {
                            name: format!("{l}:"),
                            is_dir: true,
                            size: 0,
                        })
                        .collect());
                }
                let (p, path) = self.volume_for(&letters, path)?;
                p.list(id, &path, ctx)
            }
            Service::Wpd(wpd) => {
                let comps = wpd_components(path)?;
                let mut v: Vec<FileEntry> = self
                    .source
                    .wpd_list(&wpd, &comps)
                    .map_err(wpd_err)?
                    .into_iter()
                    .map(|e| FileEntry {
                        name: e.name,
                        is_dir: e.is_dir,
                        size: e.size,
                    })
                    .collect();
                v.sort_by(|a, b| {
                    (!a.is_dir, a.name.to_lowercase()).cmp(&(!b.is_dir, b.name.to_lowercase()))
                });
                Ok(v)
            }
            Service::None | Service::Serial(_) => {
                Err(Error::CapabilityUnsupported("files.list".into()))
            }
        }
    }

    fn stat(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Option<FileEntry>> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        match self.service(&r) {
            Service::MassStorage(letters) => {
                let (p, path) = self.volume_for(&letters, path)?;
                p.stat(id, &path, ctx)
            }
            Service::Wpd(wpd) => {
                let mut comps = wpd_components(path)?;
                let Some(name) = comps.pop() else {
                    return Ok(None);
                };
                let siblings: Vec<WpdEntry> = match self.source.wpd_list(&wpd, &comps) {
                    Ok(v) => v,
                    Err(e) if matches!(e.code, Some(0x8007_0002) | Some(0x8007_0003)) => {
                        return Ok(None)
                    }
                    Err(e) => return Err(wpd_err(e)),
                };
                Ok(siblings
                    .into_iter()
                    .find(|e| e.name == name)
                    .map(|e| FileEntry {
                        name: e.name,
                        is_dir: e.is_dir,
                        size: e.size,
                    }))
            }
            Service::None | Service::Serial(_) => {
                Err(Error::CapabilityUnsupported("files.list".into()))
            }
        }
    }

    fn create_dir(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<()> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        match self.service(&r) {
            Service::MassStorage(letters) => {
                let (p, path) = self.volume_for(&letters, path)?;
                p.create_dir(id, &path, ctx)
            }
            // WPD в этой версии строго read-only.
            _ => Err(Error::CapabilityUnsupported("files.write".into())),
        }
    }

    fn read(&self, id: &DeviceId, path: &str, ctx: &OperationContext) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        self.read_to(id, path, &mut v, ctx)?;
        Ok(v)
    }

    fn read_to(
        &self,
        id: &DeviceId,
        path: &str,
        sink: &mut dyn Write,
        ctx: &OperationContext,
    ) -> Result<u64> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        match self.service(&r) {
            Service::MassStorage(letters) => {
                let (p, path) = self.volume_for(&letters, path)?;
                p.read_to(id, &path, sink, ctx)
            }
            Service::Wpd(wpd) => {
                let comps = wpd_components(path)?;
                let total = self.stat(id, path, ctx)?.map(|e| e.size).unwrap_or(0);
                let mut done = 0u64;
                let mut abort: Option<Error> = None;
                let res = self.source.wpd_read(&wpd, &comps, &mut |chunk| {
                    if let Err(e) = ctx.checkpoint() {
                        abort = Some(e);
                        return Err(());
                    }
                    if let Err(e) = sink.write_all(chunk) {
                        abort = Some(e.into());
                        return Err(());
                    }
                    done += chunk.len() as u64;
                    ctx.report_progress(done, total.max(done));
                    Ok(())
                });
                if let Some(e) = abort {
                    return Err(e);
                }
                res.map_err(wpd_err)?;
                Ok(done)
            }
            Service::None | Service::Serial(_) => {
                Err(Error::CapabilityUnsupported("files.read".into()))
            }
        }
    }

    fn write(
        &self,
        id: &DeviceId,
        path: &str,
        data: &[u8],
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        let mut c = std::io::Cursor::new(data);
        self.write_from(id, path, &mut c, data.len() as u64, overwrite, ctx)
    }

    fn write_from(
        &self,
        id: &DeviceId,
        path: &str,
        source: &mut dyn std::io::Read,
        len: u64,
        overwrite: OverwritePolicy,
        ctx: &OperationContext,
    ) -> Result<()> {
        ctx.checkpoint()?;
        let r = self.find(id)?;
        match self.service(&r) {
            Service::MassStorage(letters) => {
                let (p, path) = self.volume_for(&letters, path)?;
                p.write_from(id, &path, source, len, overwrite, ctx)
            }
            // WPD в этой версии строго read-only.
            _ => Err(Error::CapabilityUnsupported("files.write".into())),
        }
    }
}
