//! Nokia USB OBEX: native SetupAPI enumeration and bounded overlapped I/O.
//! Interface GUID and sync route ported from s60modern pccs_windows.py (MIT, efaf53d).
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    core::{GUID, PCWSTR},
    Win32::{
        Devices::DeviceAndDriverInstallation::*,
        Foundation::*,
        Storage::FileSystem::*,
        System::{Threading::*, IO::*},
    },
};
use yeskia_core::{
    pccs::{self, Channel},
    Error, Result,
};
const INTERFACE: GUID = GUID::from_u128(0x4f919100_4adf_11d5_882d_00b0d02fe381);
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet {
    fn drop(&mut self) {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}
fn id_of(dev: u32) -> Option<String> {
    let mut b = [0u16; 512];
    if unsafe { CM_Get_Device_IDW(dev, &mut b, 0) } != CR_SUCCESS {
        return None;
    }
    Some(String::from_utf16_lossy(
        &b[..b.iter().position(|c| *c == 0).unwrap_or(b.len())],
    ))
}
fn belongs(mut dev: u32, selected: &str) -> bool {
    for _ in 0..16 {
        if id_of(dev).is_some_and(|id| id.eq_ignore_ascii_case(selected)) {
            return true;
        }
        let mut parent = 0;
        if unsafe { CM_Get_Parent(&mut parent, dev, 0) } != CR_SUCCESS {
            break;
        }
        dev = parent;
    }
    false
}
fn paths(selected: &str) -> Result<Vec<Vec<u16>>> {
    unsafe {
        let set = DeviceSet(
            SetupDiGetClassDevsW(
                Some(&INTERFACE),
                None,
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
            .map_err(|_| Error::Transport("OBEX_ENUMERATION".into()))?,
        );
        let mut paths = Vec::new();
        for index in 0..64 {
            let mut iface = SP_DEVICE_INTERFACE_DATA {
                cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInterfaces(set.0, None, &INTERFACE, index, &mut iface).is_err() {
                break;
            }
            let mut needed = 0;
            let _ =
                SetupDiGetDeviceInterfaceDetailW(set.0, &iface, None, 0, Some(&mut needed), None);
            if !(8..=8192).contains(&needed) {
                continue;
            }
            let mut storage = vec![0u64; (needed as usize).div_ceil(8)];
            let detail = storage.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut dev = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &iface,
                Some(detail),
                needed,
                None,
                Some(&mut dev),
            )
            .is_err()
                || !belongs(dev.DevInst, selected)
            {
                continue;
            }
            let count = (needed as usize - 4) / 2;
            let slice = std::slice::from_raw_parts(
                std::ptr::addr_of!((*detail).DevicePath).cast::<u16>(),
                count,
            );
            let Some(end) = slice.iter().position(|c| *c == 0) else {
                continue;
            };
            let mut path = slice[..end].to_vec();
            path.push(0);
            paths.push(path);
        }
        Ok(paths)
    }
}
pub struct UsbChannel<'a> {
    handle: Handle,
    cancel: &'a AtomicBool,
}
impl<'a> UsbChannel<'a> {
    fn open(path: &[u16], cancel: &'a AtomicBool) -> Result<Self> {
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        }
        .map_err(|e| {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult()
                || e.code() == ERROR_SHARING_VIOLATION.to_hresult()
            {
                Error::HostAccess("OBEX_BUSY".into())
            } else {
                Error::DeviceDisconnected
            }
        })?;
        Ok(Self {
            handle: Handle(handle),
            cancel,
        })
    }
    fn io(&self, buffer: &mut [u8], write: bool, deadline: Instant) -> Result<usize> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        let event = Handle(
            unsafe { CreateEventW(None, true, false, None) }
                .map_err(|_| Error::Transport("OBEX_EVENT".into()))?,
        );
        let mut ov = OVERLAPPED {
            hEvent: event.0,
            ..Default::default()
        };
        let mut count = 0;
        let result = unsafe {
            if write {
                WriteFile(self.handle.0, Some(buffer), Some(&mut count), Some(&mut ov))
            } else {
                ReadFile(self.handle.0, Some(buffer), Some(&mut count), Some(&mut ov))
            }
        };
        if let Err(e) = result {
            if e.code() != ERROR_IO_PENDING.to_hresult() {
                return Err(Error::DeviceDisconnected);
            }
        }
        loop {
            let wait = unsafe { WaitForSingleObject(event.0, 50) };
            if wait == WAIT_OBJECT_0 {
                unsafe { GetOverlappedResult(self.handle.0, &ov, &mut count, false) }
                    .map_err(|_| Error::DeviceDisconnected)?;
                return Ok(count as usize);
            }
            if self.cancel.load(Ordering::SeqCst)
                || Instant::now() >= deadline
                || wait != WAIT_TIMEOUT
            {
                // Never release OVERLAPPED/buffer while the driver still owns them.
                unsafe {
                    let _ = CancelIoEx(self.handle.0, Some(&ov));
                    let _ = GetOverlappedResult(self.handle.0, &ov, &mut count, true);
                }
                return Err(if self.cancel.load(Ordering::SeqCst) {
                    Error::Cancelled
                } else {
                    Error::Timeout
                });
            }
        }
    }
}
impl Channel for UsbChannel<'_> {
    fn exchange(&mut self, packet: &[u8], timeout: Duration) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut request = packet.to_vec();
        let mut pos = 0;
        while pos < request.len() {
            let n = self.io(&mut request[pos..], true, deadline)?;
            if n == 0 {
                return Err(Error::DeviceDisconnected);
            }
            pos += n;
        }
        let mut reply = Vec::new();
        loop {
            let mut buffer = vec![0u8; 65535];
            let n = self.io(&mut buffer, false, deadline)?;
            if n == 0 {
                return Err(Error::DeviceDisconnected);
            }
            reply.extend(&buffer[..n]);
            if reply.len() >= 3 {
                let size = u16::from_be_bytes([reply[1], reply[2]]) as usize;
                if size < 3 || reply.len() > size {
                    return Err(pccs::invalid());
                }
                if reply.len() == size {
                    return Ok(reply);
                }
            }
        }
    }
}
pub fn read(selected: &str, domain: &str, cancel: &AtomicBool) -> Result<serde_json::Value> {
    let interfaces = paths(selected)?;
    if interfaces.is_empty() {
        return Err(Error::CapabilityUnsupported(
            "OBEX_INTERFACE_MISSING".into(),
        ));
    }
    let mut failure = Error::CapabilityUnsupported("PCCS_TARGET".into());
    for path in interfaces {
        if cancel.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        let mut channel = match UsbChannel::open(&path, cancel) {
            Ok(c) => c,
            Err(e) => {
                failure = e;
                continue;
            }
        };
        let mut session = match pccs::Session::connect(&mut channel) {
            Ok(s) => s,
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(e) => {
                failure = e;
                continue;
            }
        };
        let items = session.read_store(domain)?;
        return yeskia_core::pim_text::rows(&items, domain);
    }
    Err(failure)
}

pub fn send_certificate(selected: &str, der: &[u8], cancel: &AtomicBool) -> Result<()> {
    let interfaces = paths(selected)?;
    let mut failure = Error::CapabilityUnsupported("CERT_OPP_UNAVAILABLE".into());
    for path in interfaces {
        if cancel.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        let mut channel = match UsbChannel::open(&path, cancel) {
            Ok(c) => c,
            Err(e) => {
                failure = e;
                continue;
            }
        };
        match yeskia_core::opp::push_certificate(&mut channel, der) {
            // These failures can only arise before PUT. Uncertain delivery must never be retried.
            Err(
                e @ (Error::CapabilityUnsupported(_)
                | Error::Transport(_)
                | Error::DeviceDisconnected
                | Error::Timeout),
            ) => failure = e,
            other => return other,
        }
    }
    Err(failure)
}
