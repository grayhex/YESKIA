//! Сбор снимка в Windows: SetupAPI + Configuration Manager (PnP), WPD (MTP), COM, тома.
//!
//! **Только чтение.** Не устанавливает драйверы, не меняет привязку драйверов, не пишет на
//! устройство: WPD открывается с `GENERIC_READ`, читаются лишь идентификаторы объектов верхнего
//! уровня и их количество (имена файлов не собираются). COM-порт при включённой проверке
//! открывается кратковременно только для определения «занят/свободен» (без ввода-вывода).
//!
//! Статус: `implemented` — проверено компиляцией и запуском на Windows-раннере CI без телефона;
//! поведение на реальном Nokia не проверялось.

use crate::model::*;
use std::ffi::c_void;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Devices::PortableDevices::*;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ioctl::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::IO::DeviceIoControl;

pub struct WindowsSource;

impl WindowsSource {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsSource {
    fn default() -> Self {
        Self::new()
    }
}

fn err(stage: &str, code: u32) -> CollectError {
    CollectError {
        stage: stage.to_string(),
        code: Some(code),
    }
}

fn hr_code(e: &windows::core::Error) -> u32 {
    e.code().0 as u32
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// REG_MULTI_SZ → список строк.
fn multi_sz(buf: &[u8]) -> Vec<String> {
    let w: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    w.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

fn single_sz(buf: &[u8]) -> String {
    let w: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    from_wide(&w)
}

/// RAII для COM: `CoUninitialize` только если инициализация удалась.
struct ComGuard(bool);

impl ComGuard {
    fn new() -> Self {
        // Если поток уже инициализирован в другой модели (RPC_E_CHANGED_MODE) — работаем как есть.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        ComGuard(hr.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

unsafe fn registry_string(subkey: &str, value: &str) -> Option<String> {
    let sub = wide(subkey);
    let val = wide(value);
    let mut size: u32 = 0;
    let rc = RegGetValueW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(sub.as_ptr()),
        PCWSTR(val.as_ptr()),
        RRF_RT_REG_SZ,
        None,
        None,
        Some(&mut size),
    );
    if rc != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
    let rc = RegGetValueW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(sub.as_ptr()),
        PCWSTR(val.as_ptr()),
        RRF_RT_REG_SZ,
        None,
        Some(buf.as_mut_ptr() as *mut c_void),
        Some(&mut size),
    );
    (rc == ERROR_SUCCESS).then(|| from_wide(&buf))
}

fn os_info() -> OsInfo {
    const KEY: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let build = unsafe { registry_string(KEY, "CurrentBuildNumber") }.unwrap_or_default();
    let display = unsafe { registry_string(KEY, "DisplayVersion") }.unwrap_or_default();
    // ProductName в реестре у Windows 11 по-прежнему «Windows 10 …», поэтому определяем по сборке.
    let product = match build.parse::<u32>() {
        Ok(b) if b >= 22000 => "Windows 11",
        Ok(_) => "Windows 10 or older",
        Err(_) => "Windows",
    };
    OsInfo {
        product: format!("{product} {display}").trim().to_string(),
        build,
    }
}

unsafe fn reg_prop(
    set: HDEVINFO,
    data: &SP_DEVINFO_DATA,
    prop: SETUP_DI_REGISTRY_PROPERTY,
) -> Option<Vec<u8>> {
    let mut needed: u32 = 0;
    let _ = SetupDiGetDeviceRegistryPropertyW(set, data, prop, None, None, Some(&mut needed));
    if needed == 0 {
        return None;
    }
    let mut buf = vec![0u8; needed as usize];
    SetupDiGetDeviceRegistryPropertyW(set, data, prop, None, Some(&mut buf), Some(&mut needed))
        .ok()
        .map(|_| buf)
}

fn enumerate_pnp(errors: &mut Vec<CollectError>) -> Vec<PnpNode> {
    let mut nodes = Vec::new();
    unsafe {
        let set = match SetupDiGetClassDevsW(
            None,
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_ALLCLASSES,
        ) {
            Ok(s) => s,
            Err(e) => {
                errors.push(err("setupapi_enum", hr_code(&e)));
                return nodes;
            }
        };
        let mut index = 0u32;
        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, index, &mut data).is_err() {
                break;
            }
            index += 1;

            let mut id_buf = [0u16; 512];
            if SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id_buf), None).is_err() {
                continue;
            }
            let instance_id = from_wide(&id_buf);

            let mut status = CM_DEVNODE_STATUS_FLAGS(0);
            let mut problem = CM_PROB(0);
            let _ = CM_Get_DevNode_Status(&mut status, &mut problem, data.DevInst, 0);

            let mut parent = 0u32;
            let parent_id = if CM_Get_Parent(&mut parent, data.DevInst, 0) == CR_SUCCESS {
                let mut pbuf = [0u16; 512];
                (CM_Get_Device_IDW(parent, &mut pbuf, 0) == CR_SUCCESS).then(|| from_wide(&pbuf))
            } else {
                None
            };

            nodes.push(PnpNode {
                instance_id,
                parent_id,
                class: reg_prop(set, &data, SPDRP_CLASS).map(|b| single_sz(&b)),
                hardware_ids: reg_prop(set, &data, SPDRP_HARDWAREID)
                    .map(|b| multi_sz(&b))
                    .unwrap_or_default(),
                service: reg_prop(set, &data, SPDRP_SERVICE).map(|b| single_sz(&b)),
                // Описание драйвера — общий текст типа «USB Composite Device», не пользовательское имя.
                description: reg_prop(set, &data, SPDRP_DEVICEDESC).map(|b| single_sz(&b)),
                problem_code: problem.0,
                driver_provider: None,
                driver_version: None,
            });
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    nodes
}

/// COM-порты: класс `Ports`, имя `COMn` берётся из «(COMn)» в friendly name.
fn enumerate_com(errors: &mut Vec<CollectError>, probe_busy: bool) -> Vec<ComPort> {
    let mut ports = Vec::new();
    unsafe {
        let class: GUID = GUID::from_u128(0x4d36e978_e325_11ce_bfc1_08002be10318); // GUID_DEVCLASS_PORTS
        let set = match SetupDiGetClassDevsW(Some(&class), PCWSTR::null(), None, DIGCF_PRESENT) {
            Ok(s) => s,
            Err(e) => {
                errors.push(err("setupapi_ports", hr_code(&e)));
                return ports;
            }
        };
        let mut index = 0u32;
        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, index, &mut data).is_err() {
                break;
            }
            index += 1;
            let mut id_buf = [0u16; 512];
            if SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id_buf), None).is_err() {
                continue;
            }
            let friendly = reg_prop(set, &data, SPDRP_FRIENDLYNAME)
                .map(|b| single_sz(&b))
                .unwrap_or_default();
            let Some(start) = friendly.rfind("(COM") else {
                continue;
            };
            let Some(end) = friendly[start..].find(')') else {
                continue;
            };
            let port_name = friendly[start + 1..start + end].to_string();
            let busy = probe_busy.then(|| com_busy(&port_name)).flatten();
            ports.push(ComPort {
                port_name,
                node_instance_id: from_wide(&id_buf),
                busy,
            });
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    ports
}

/// Занят ли порт: монопольное открытие без ввода-вывода. `None` — не удалось определить.
fn com_busy(port: &str) -> Option<bool> {
    let path = wide(&format!("\\\\.\\{port}"));
    unsafe {
        match CreateFileW(
            PCWSTR(path.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        ) {
            Ok(h) => {
                let _ = CloseHandle(h);
                Some(false)
            }
            Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => Some(true),
            Err(_) => None,
        }
    }
}

/// Номер физического диска для пути (том `\\.\E:` или интерфейс диска) через IOCTL, без доступа к данным.
unsafe fn storage_device_number(path: &[u16]) -> Option<u32> {
    let h = CreateFileW(
        PCWSTR(path.as_ptr()),
        0, // доступ «только атрибуты»: чтение данных не запрашивается
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        None,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        None,
    )
    .ok()?;
    let mut sdn = STORAGE_DEVICE_NUMBER::default();
    let mut returned = 0u32;
    let ok = DeviceIoControl(
        h,
        IOCTL_STORAGE_GET_DEVICE_NUMBER,
        None,
        0,
        Some(&mut sdn as *mut STORAGE_DEVICE_NUMBER as *mut c_void),
        std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
        Some(&mut returned),
        None,
    )
    .is_ok();
    let _ = CloseHandle(h);
    ok.then_some(sdn.DeviceNumber)
}

/// Номер диска → instance id PnP-узла диска (USBSTOR\DISK&… для USB-накопителей).
fn disk_nodes() -> std::collections::HashMap<u32, String> {
    let mut map = std::collections::HashMap::new();
    unsafe {
        let set = match SetupDiGetClassDevsW(
            Some(&GUID_DEVINTERFACE_DISK),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        ) {
            Ok(s) => s,
            Err(_) => return map,
        };
        let mut index = 0u32;
        loop {
            let mut iface = SP_DEVICE_INTERFACE_DATA {
                cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVINTERFACE_DISK, index, &mut iface)
                .is_err()
            {
                break;
            }
            index += 1;

            // Двухэтапное чтение переменной по длине структуры с путём интерфейса.
            let mut needed = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(set, &iface, None, 0, Some(&mut needed), None);
            if needed == 0 {
                continue;
            }
            // u64-буфер гарантирует выравнивание для SP_DEVICE_INTERFACE_DETAIL_DATA_W.
            let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
            let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut devinfo = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &iface,
                Some(detail),
                needed,
                None,
                Some(&mut devinfo),
            )
            .is_err()
            {
                continue;
            }
            let path_ptr = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
            let mut path: Vec<u16> = Vec::new();
            let mut k = 0;
            while *path_ptr.add(k) != 0 {
                path.push(*path_ptr.add(k));
                k += 1;
            }
            path.push(0);
            let mut id_buf = [0u16; 512];
            if CM_Get_Device_IDW(devinfo.DevInst, &mut id_buf, 0) != CR_SUCCESS {
                continue;
            }
            if let Some(n) = storage_device_number(&path) {
                map.insert(n, from_wide(&id_buf));
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    map
}

/// Съёмные тома с привязкой к PnP-узлу диска (через номер физического диска).
fn removable_volumes() -> Vec<Volume> {
    let mut out = Vec::new();
    let disks = disk_nodes();
    unsafe {
        let mask = GetLogicalDrives();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = wide(&format!("{letter}:\\"));
            let removable = GetDriveTypeW(PCWSTR(root.as_ptr())) == 2; // DRIVE_REMOVABLE
                                                                       // Для USB-накопителей Windows часто сообщает DRIVE_FIXED, поэтому связь ищем у всех томов,
                                                                       // но в отчёт попадают только тома, привязанные к USB-устройству (см. Doctor).
            let node = storage_device_number(&wide(&format!("\\\\.\\{letter}:")))
                .and_then(|n| disks.get(&n).cloned());
            out.push(Volume {
                letter,
                removable: removable || node.is_some(),
                node_instance_id: node,
            });
        }
    }
    out
}

unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as *const c_void));
    s
}

unsafe fn open_wpd(id: &[u16]) -> windows::core::Result<IPortableDevice> {
    let device: IPortableDevice = CoCreateInstance(&PortableDevice, None, CLSCTX_INPROC_SERVER)?;
    let client: IPortableDeviceValues =
        CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
    client.SetStringValue(&WPD_CLIENT_NAME, windows::core::w!("YESKIA"))?;
    client.SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 0)?;
    client.SetUnsignedIntegerValue(&WPD_CLIENT_MINOR_VERSION, 1)?;
    client.SetUnsignedIntegerValue(&WPD_CLIENT_REVISION, 0)?;
    // Только чтение: GENERIC_READ.
    client.SetUnsignedIntegerValue(&WPD_CLIENT_DESIRED_ACCESS, GENERIC_READ.0)?;
    device.Open(PCWSTR(id.as_ptr()), &client)?;
    Ok(device)
}

/// Идентификаторы потомков объекта (один уровень). Только перечисление.
unsafe fn child_ids(
    content: &IPortableDeviceContent,
    parent: PCWSTR,
) -> windows::core::Result<Vec<String>> {
    let en = content.EnumObjects(0, parent, None)?;
    let mut out = Vec::new();
    loop {
        let mut batch = [PWSTR::null(); 32];
        let mut fetched = 0u32;
        let hr = en.Next(&mut batch, &mut fetched);
        for p in batch.iter().take(fetched as usize) {
            out.push(take_pwstr(*p));
        }
        // S_OK — полный пакет, S_FALSE — последний; ошибка — выходим
        if hr != S_OK || fetched == 0 {
            if hr.is_err() {
                return Err(windows::core::Error::from(hr));
            }
            break;
        }
    }
    Ok(out)
}

unsafe fn wpd_listing(device: &IPortableDevice) -> WpdListing {
    let result = (|| -> windows::core::Result<Vec<StorageSummary>> {
        let content = device.Content()?;
        let root = windows::core::w!("DEVICE");
        let storages = child_ids(&content, root)?;
        let mut out = Vec::new();
        for s in storages {
            let w = wide(&s);
            let n = child_ids(&content, PCWSTR(w.as_ptr()))?.len();
            out.push(StorageSummary {
                object_count: n as u32,
            });
        }
        Ok(out)
    })();
    match result {
        Ok(storages) => WpdListing::Ok { storages },
        Err(e) => WpdListing::Failed { code: hr_code(&e) },
    }
}

unsafe fn wpd_props(device: &IPortableDevice) -> windows::core::Result<WpdIdentity> {
    let content = device.Content()?;
    let props = content.Properties()?;
    let keys: IPortableDeviceKeyCollection =
        CoCreateInstance(&PortableDeviceKeyCollection, None, CLSCTX_INPROC_SERVER)?;
    keys.Add(&WPD_DEVICE_MANUFACTURER)?;
    keys.Add(&WPD_DEVICE_MODEL)?;
    keys.Add(&WPD_DEVICE_FIRMWARE_VERSION)?;
    let values = props.GetValues(windows::core::w!("DEVICE"), &keys)?;
    let get = |k: &windows::Win32::Foundation::PROPERTYKEY| {
        values
            .GetStringValue(k)
            .ok()
            .map(|p| take_pwstr(p))
            .filter(|s| !s.is_empty())
    };
    Ok(WpdIdentity {
        manufacturer: get(&WPD_DEVICE_MANUFACTURER),
        model: get(&WPD_DEVICE_MODEL),
        firmware: get(&WPD_DEVICE_FIRMWARE_VERSION),
    })
}

fn enumerate_wpd(listing: bool, errors: &mut Vec<CollectError>) -> Vec<WpdDevice> {
    let mut out = Vec::new();
    unsafe {
        let mgr: IPortableDeviceManager =
            match CoCreateInstance(&PortableDeviceManager, None, CLSCTX_INPROC_SERVER) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(err("wpd_manager", hr_code(&e)));
                    return out;
                }
            };
        let mut count = 0u32;
        if let Err(e) = mgr.GetDevices(std::ptr::null_mut(), &mut count) {
            errors.push(err("wpd_enumerate", hr_code(&e)));
            return out;
        }
        if count == 0 {
            return out;
        }
        let mut ids = vec![PWSTR::null(); count as usize];
        if let Err(e) = mgr.GetDevices(ids.as_mut_ptr(), &mut count) {
            errors.push(err("wpd_enumerate", hr_code(&e)));
            return out;
        }
        for p in ids.into_iter().take(count as usize) {
            let device_id = take_pwstr(p);
            let w = wide(&device_id);
            let mut dev = WpdDevice {
                device_id,
                manufacturer: None,
                model: None,
                firmware: None,
                listing: WpdListing::NotProbed,
            };
            if listing {
                match open_wpd(&w) {
                    Ok(d) => {
                        if let Ok(i) = wpd_props(&d) {
                            dev.manufacturer = i.manufacturer;
                            dev.model = i.model;
                            dev.firmware = i.firmware;
                        }
                        dev.listing = wpd_listing(&d);
                        let _ = d.Close();
                    }
                    Err(e) => dev.listing = WpdListing::Failed { code: hr_code(&e) },
                }
            }
            out.push(dev);
        }
    }
    out
}

/// Имя объекта WPD: исходное имя файла, иначе отображаемое имя.
unsafe fn object_info(
    props: &IPortableDeviceProperties,
    id: PCWSTR,
) -> windows::core::Result<(String, bool, u64)> {
    let keys: IPortableDeviceKeyCollection =
        CoCreateInstance(&PortableDeviceKeyCollection, None, CLSCTX_INPROC_SERVER)?;
    keys.Add(&WPD_OBJECT_ORIGINAL_FILE_NAME)?;
    keys.Add(&WPD_OBJECT_NAME)?;
    keys.Add(&WPD_OBJECT_CONTENT_TYPE)?;
    keys.Add(&WPD_OBJECT_SIZE)?;
    let v = props.GetValues(id, &keys)?;
    let name = v
        .GetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME)
        .ok()
        .map(|p| take_pwstr(p))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            v.GetStringValue(&WPD_OBJECT_NAME)
                .ok()
                .map(|p| take_pwstr(p))
        })
        .unwrap_or_default();
    let ctype = v.GetGuidValue(&WPD_OBJECT_CONTENT_TYPE).unwrap_or_default();
    // Хранилища — функциональные объекты, папки — WPD_CONTENT_TYPE_FOLDER.
    let is_dir = ctype == WPD_CONTENT_TYPE_FOLDER || ctype == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT;
    let size = v
        .GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE)
        .unwrap_or(0);
    Ok((name, is_dir, size))
}

/// Идёт по компонентам пути (`[Хранилище, Папка, …]`) и возвращает id объекта (`"DEVICE"` для корня).
unsafe fn resolve_object(
    content: &IPortableDeviceContent,
    path: &[String],
) -> windows::core::Result<Option<Vec<u16>>> {
    let props = content.Properties()?;
    let mut current = wide("DEVICE");
    for comp in path {
        let mut found = None;
        for child in child_ids(content, PCWSTR(current.as_ptr()))? {
            let w = wide(&child);
            let (name, _, _) = object_info(&props, PCWSTR(w.as_ptr()))?;
            if name.to_lowercase() == comp.to_lowercase() {
                found = Some(w);
                break;
            }
        }
        match found {
            Some(w) => current = w,
            None => return Ok(None),
        }
    }
    Ok(Some(current))
}

const HR_NOT_FOUND: u32 = 0x8007_0002;

fn wpd_list_impl(device_id: &str, path: &[String]) -> Result<Vec<WpdEntry>, CollectError> {
    let w = wide(device_id);
    unsafe {
        let dev = open_wpd(&w).map_err(|e| err("wpd_open", hr_code(&e)))?;
        let r = (|| -> windows::core::Result<Option<Vec<WpdEntry>>> {
            let content = dev.Content()?;
            let Some(obj) = resolve_object(&content, path)? else {
                return Ok(None);
            };
            let props = content.Properties()?;
            let mut out = Vec::new();
            for child in child_ids(&content, PCWSTR(obj.as_ptr()))? {
                let cw = wide(&child);
                let (name, is_dir, size) = object_info(&props, PCWSTR(cw.as_ptr()))?;
                if !name.is_empty() {
                    out.push(WpdEntry {
                        name,
                        is_dir,
                        size: if is_dir { 0 } else { size },
                    });
                }
            }
            Ok(Some(out))
        })();
        let _ = dev.Close();
        match r {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Err(err("wpd_list_not_found", HR_NOT_FOUND)),
            Err(e) => Err(err("wpd_list", hr_code(&e))),
        }
    }
}

fn wpd_read_impl(
    device_id: &str,
    path: &[String],
    sink: &mut dyn FnMut(&[u8]) -> Result<(), ()>,
) -> Result<(), CollectError> {
    let w = wide(device_id);
    unsafe {
        let dev = open_wpd(&w).map_err(|e| err("wpd_open", hr_code(&e)))?;
        let r = (|| -> windows::core::Result<Result<(), CollectError>> {
            let content = dev.Content()?;
            let Some(obj) = resolve_object(&content, path)? else {
                return Ok(Err(err("wpd_read_not_found", HR_NOT_FOUND)));
            };
            let resources = content.Transfer()?;
            let mut optimal = 0u32;
            let mut stream: Option<IStream> = None;
            resources.GetStream(
                PCWSTR(obj.as_ptr()),
                &WPD_RESOURCE_DEFAULT,
                STGM_READ.0,
                &mut optimal,
                &mut stream,
            )?;
            let stream = stream.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
            let mut buf = vec![0u8; (optimal as usize).clamp(64 * 1024, 1024 * 1024)];
            loop {
                let mut read = 0u32;
                let hr = stream.Read(
                    buf.as_mut_ptr() as *mut c_void,
                    buf.len() as u32,
                    Some(&mut read),
                );
                if hr.is_err() {
                    return Err(windows::core::Error::from(hr));
                }
                if read == 0 {
                    break;
                }
                if sink(&buf[..read as usize]).is_err() {
                    return Ok(Err(err("aborted", 0)));
                }
            }
            Ok(Ok(()))
        })();
        let _ = dev.Close();
        match r {
            Ok(inner) => inner,
            Err(e) => Err(err("wpd_read", hr_code(&e))),
        }
    }
}

impl SnapshotSource for WindowsSource {
    fn collect(&self, options: &CollectOptions) -> Result<Snapshot, CollectError> {
        let _com = ComGuard::new();
        let mut errors = Vec::new();
        let nodes = enumerate_pnp(&mut errors);
        let com_ports = enumerate_com(&mut errors, options.probe_com_busy);
        let wpd = enumerate_wpd(options.wpd_listing, &mut errors);
        Ok(Snapshot {
            nodes,
            com_ports,
            wpd,
            volumes: removable_volumes(),
            os: os_info(),
            errors,
        })
    }

    fn wpd_identity(&self, wpd_device_id: &str) -> Result<WpdIdentity, CollectError> {
        let _com = ComGuard::new();
        let w = wide(wpd_device_id);
        unsafe {
            let d = open_wpd(&w).map_err(|e| err("wpd_open", hr_code(&e)))?;
            let r = wpd_props(&d).map_err(|e| err("wpd_properties", hr_code(&e)));
            let _ = d.Close();
            r
        }
    }

    fn wpd_list(
        &self,
        wpd_device_id: &str,
        path: &[String],
    ) -> Result<Vec<WpdEntry>, CollectError> {
        let _com = ComGuard::new();
        wpd_list_impl(wpd_device_id, path)
    }

    fn wpd_read(
        &self,
        wpd_device_id: &str,
        path: &[String],
        sink: &mut dyn FnMut(&[u8]) -> Result<(), ()>,
    ) -> Result<(), CollectError> {
        let _com = ComGuard::new();
        wpd_read_impl(wpd_device_id, path, sink)
    }
}
