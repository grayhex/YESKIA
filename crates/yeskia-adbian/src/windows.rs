//! Windows Bluetooth Classic + SPP SDP (OS decodes SDP); no Linux sockets or COM emulation.
use super::*;
use ::windows::{
    core::{GUID, PSTR, PWSTR},
    Win32::{
        Devices::Bluetooth::*,
        Foundation::{ERROR_NOT_FOUND, ERROR_NO_MORE_ITEMS, HANDLE},
        Networking::WinSock::*,
    },
};
use std::{
    mem::size_of,
    sync::{atomic::AtomicUsize, mpsc, OnceLock},
};
use yeskia_core::adbian::Channel;
const SPP: GUID = GUID::from_u128(0x00001101_0000_1000_8000_00805f9b34fb);
fn fail(stage: &str) -> Error {
    Error::Transport(format!("ADBIAN_WINDOWS_{stage}"))
}
fn startup() -> Result<()> {
    static START: OnceLock<bool> = OnceLock::new();
    if *START.get_or_init(|| unsafe { WSAStartup(0x0202, &mut WSADATA::default()) == 0 }) {
        Ok(())
    } else {
        Err(fail("WINSOCK"))
    }
}
#[derive(Default)]
pub struct WindowsBluetooth {
    cache: Mutex<HashMap<String, u8>>,
}
fn address(key: &str) -> Result<u64> {
    u64::from_str_radix(key, 16).map_err(|_| fail("ADDRESS"))
}
fn context(addr: u64) -> Vec<u16> {
    let s = format!(
        "({:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
        (addr >> 40) & 255,
        (addr >> 32) & 255,
        (addr >> 24) & 255,
        (addr >> 16) & 255,
        (addr >> 8) & 255,
        addr & 255
    );
    s.encode_utf16().chain([0]).collect()
}
fn bounded<T>(buffer: &[usize], ptr: *const T, count: usize) -> bool {
    let begin = buffer.as_ptr() as usize;
    let end = begin + std::mem::size_of_val(buffer);
    let p = ptr as usize;
    !ptr.is_null()
        && p % std::mem::align_of::<T>() == 0
        && p >= begin
        && count
            .checked_mul(size_of::<T>())
            .and_then(|n| p.checked_add(n))
            .is_some_and(|n| n <= end)
}
fn service_name(buffer: &[usize], ptr: *const u16) -> Option<String> {
    let mut units = Vec::new();
    for i in 0..256 {
        let p = ptr.wrapping_add(i);
        if !bounded(buffer, p, 1) {
            return None;
        }
        let n = unsafe { *p };
        if n == 0 {
            return String::from_utf16(&units).ok();
        }
        units.push(n);
    }
    None
}
// Lookup may block in the Bluetooth provider. A separate worker owns its aligned buffers;
// the caller cancels WSALookupServiceNext via WSALookupServiceEnd on timeout/cancellation.
struct Lookup(Arc<AtomicUsize>);
impl Drop for Lookup {
    fn drop(&mut self) {
        let h = self.0.swap(0, Ordering::SeqCst);
        if h != 0 {
            unsafe {
                WSALookupServiceEnd(HANDLE(h as *mut _));
            }
        }
    }
}
fn sdp_worker(addr: u64, shared: Arc<AtomicUsize>) -> Result<Vec<u8>> {
    startup()?;
    let mut cls = SPP;
    let mut remote = context(addr);
    let query = WSAQUERYSETW {
        dwSize: size_of::<WSAQUERYSETW>() as u32,
        lpServiceClassId: &mut cls,
        dwNameSpace: NS_BTH,
        lpszContext: PWSTR(remote.as_mut_ptr()),
        ..Default::default()
    };
    let mut h = HANDLE::default();
    if unsafe {
        WSALookupServiceBeginW(
            &query,
            LUP_FLUSHCACHE | LUP_RETURN_NAME | LUP_RETURN_ADDR,
            &mut h,
        )
    } != 0
    {
        return Err(fail("SDP_BEGIN"));
    }
    shared.store(h.0 as usize, Ordering::SeqCst);
    let _close = Lookup(shared);
    let mut out = Vec::new();
    for _ in 0..64 {
        let mut buf = vec![0usize; 2048];
        loop {
            let mut len = std::mem::size_of_val(buf.as_slice()) as u32;
            let r = unsafe {
                WSALookupServiceNextW(
                    h,
                    LUP_RETURN_NAME | LUP_RETURN_ADDR,
                    &mut len,
                    Some(buf.as_mut_ptr().cast()),
                )
            };
            if r != 0 {
                let e = unsafe { WSAGetLastError() };
                if e == WSAEFAULT
                    && len <= 256 * 1024
                    && len as usize > std::mem::size_of_val(buf.as_slice())
                {
                    buf.resize((len as usize).div_ceil(size_of::<usize>()), 0);
                    continue;
                }
                if e == WSA_E_NO_MORE || e == WSAENOMORE {
                    return Ok(out);
                }
                return Err(fail("SDP_NEXT"));
            }
            let q = unsafe { &*buf.as_ptr().cast::<WSAQUERYSETW>() };
            if service_name(&buf, q.lpszServiceInstanceName.0).as_deref() != Some("rshell") {
                break;
            }
            if q.dwNumberOfCsAddrs > 64
                || !bounded(&buf, q.lpcsaBuffer, q.dwNumberOfCsAddrs as usize)
            {
                return Err(fail("SDP_ADDRESS"));
            }
            for i in 0..q.dwNumberOfCsAddrs as usize {
                let info = unsafe { &*q.lpcsaBuffer.add(i) };
                if info.RemoteAddr.iSockaddrLength < size_of::<SOCKADDR_BTH>() as i32
                    || !bounded(&buf, info.RemoteAddr.lpSockaddr.cast::<SOCKADDR_BTH>(), 1)
                {
                    continue;
                }
                let b = unsafe {
                    std::ptr::read_unaligned(info.RemoteAddr.lpSockaddr.cast::<SOCKADDR_BTH>())
                };
                let port = b.port;
                if b.addressFamily == AF_BTH && b.btAddr == addr && (1..=30).contains(&port) {
                    out.push(port as u8);
                }
            }
            break;
        }
    }
    Ok(out)
}
fn sdp(addr: u64, ctx: &OperationContext) -> Result<Vec<u8>> {
    let shared = Arc::new(AtomicUsize::new(0));
    let cancel = Lookup(shared.clone());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(sdp_worker(addr, shared));
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        ctx.checkpoint()?;
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(r) => {
                drop(cancel);
                return r;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(fail("SDP_WORKER")),
        }
    }
}
struct Socket {
    raw: SOCKET,
}
impl Socket {
    fn wait(&self, write: bool, timeout: Duration) -> Result<()> {
        let mut set = FD_SET {
            fd_count: 1,
            ..Default::default()
        };
        set.fd_array[0] = self.raw;
        let mut errors = set;
        let tv = TIMEVAL {
            tv_sec: timeout.as_secs() as i32,
            tv_usec: timeout.subsec_micros() as i32,
        };
        let r = unsafe {
            select(
                0,
                if write { None } else { Some(&mut set) },
                if write { Some(&mut set) } else { None },
                Some(&mut errors),
                Some(&tv),
            )
        };
        if r == 0 {
            return Err(Error::Timeout);
        }
        if r < 0 || errors.fd_count > 0 {
            return Err(fail("SOCKET"));
        }
        Ok(())
    }
    fn connect(addr: u64, port: u8, ctx: &OperationContext) -> Result<Self> {
        startup()?;
        let raw = unsafe { socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32) }
            .map_err(|_| fail("SOCKET"))?;
        let s = Self { raw };
        let mut nonblock = 1;
        if unsafe { ioctlsocket(raw, FIONBIO, &mut nonblock) } != 0 {
            return Err(fail("NONBLOCK"));
        }
        // RFCOMM socket authentication + encryption: pairing is still not per-command consent.
        let yes = 1i32.to_ne_bytes();
        if unsafe {
            setsockopt(
                raw,
                SOL_RFCOMM as i32,
                SO_BTH_AUTHENTICATE as i32,
                Some(&yes),
            )
        } != 0
            || unsafe { setsockopt(raw, SOL_RFCOMM as i32, SO_BTH_ENCRYPT as i32, Some(&yes)) } != 0
        {
            return Err(fail("SECURITY"));
        }
        let b = SOCKADDR_BTH {
            addressFamily: AF_BTH,
            btAddr: addr,
            port: port as u32,
            ..Default::default()
        };
        if unsafe {
            connect(
                raw,
                (&b as *const SOCKADDR_BTH).cast(),
                size_of::<SOCKADDR_BTH>() as i32,
            )
        } != 0
        {
            if unsafe { WSAGetLastError() } != WSAEWOULDBLOCK {
                return Err(fail("CONNECT"));
            }
            loop {
                ctx.checkpoint()?;
                match s.wait(true, Duration::from_millis(100)) {
                    Err(Error::Timeout) => continue,
                    r => r?,
                }
                break;
            }
            let mut code = 0i32;
            let mut len = size_of::<i32>() as i32;
            if unsafe {
                getsockopt(
                    raw,
                    SOL_SOCKET,
                    SO_ERROR,
                    PSTR((&mut code as *mut i32).cast()),
                    &mut len,
                )
            } != 0
                || code != 0
            {
                return Err(fail("CONNECT"));
            }
        }
        Ok(s)
    }
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.close();
    }
}
impl Channel for Socket {
    fn read(&mut self, b: &mut [u8], timeout: Duration) -> Result<usize> {
        self.wait(false, timeout)?;
        let n = unsafe { recv(self.raw, b, SEND_RECV_FLAGS(0)) };
        if n < 0 {
            if unsafe { WSAGetLastError() } == WSAEWOULDBLOCK {
                Err(Error::Timeout)
            } else {
                Err(fail("RECV"))
            }
        } else {
            Ok(n as usize)
        }
    }
    fn write(&mut self, b: &[u8], timeout: Duration) -> Result<usize> {
        self.wait(true, timeout)?;
        let n = unsafe { send(self.raw, b, SEND_RECV_FLAGS(0)) };
        if n < 0 {
            if unsafe { WSAGetLastError() } == WSAEWOULDBLOCK {
                Err(Error::Timeout)
            } else {
                Err(fail("SEND"))
            }
        } else {
            Ok(n as usize)
        }
    }
    fn close(&mut self) {
        if self.raw != INVALID_SOCKET {
            unsafe {
                let _ = closesocket(self.raw);
            }
            self.raw = INVALID_SOCKET;
        }
    }
}
impl Opener for WindowsBluetooth {
    fn paired(&self) -> Result<Vec<Paired>> {
        let p = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: true.into(),
            fReturnRemembered: false.into(),
            fIssueInquiry: false.into(),
            fReturnConnected: true.into(),
            ..Default::default()
        };
        let mut info = BLUETOOTH_DEVICE_INFO {
            dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        let h = match unsafe { BluetoothFindFirstDevice(&p, &mut info) } {
            Ok(h) => h,
            Err(e)
                if e.code() == ERROR_NO_MORE_ITEMS.to_hresult()
                    || e.code() == ERROR_NOT_FOUND.to_hresult() =>
            {
                return Ok(Vec::new())
            }
            Err(_) => return Err(fail("PAIRED_DISCOVERY")),
        };
        struct Find(HBLUETOOTH_DEVICE_FIND);
        impl Drop for Find {
            fn drop(&mut self) {
                unsafe {
                    let _ = BluetoothFindDeviceClose(self.0);
                }
            }
        }
        let _guard = Find(h);
        let mut out = Vec::new();
        loop {
            if info.fAuthenticated.as_bool() && super::is_phone_class(info.ulClassofDevice) {
                let end = info
                    .szName
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(info.szName.len());
                out.push(Paired {
                    key: format!("{:012x}", unsafe { info.Address.Anonymous.ullLong }),
                    name: String::from_utf16_lossy(&info.szName[..end]),
                    class_of_device: info.ulClassofDevice,
                });
            }
            info.dwSize = size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            if unsafe { BluetoothFindNextDevice(h, &mut info) }.is_err() {
                break;
            }
        }
        Ok(out)
    }
    fn open(&self, key: &str, ctx: &OperationContext) -> Result<Session> {
        // Revalidate cached local pairing/class before any active SDP or RFCOMM call.
        if !self
            .paired()?
            .iter()
            .any(|p| p.key == key && super::is_phone_class(p.class_of_device))
        {
            return Err(Error::CapabilityUnsupported(
                "BLUETOOTH_PHONE_CLASS_REQUIRED".into(),
            ));
        }
        let addr = address(key)?;
        let mut candidates = sdp(addr, ctx).unwrap_or_default();
        ctx.checkpoint()?;
        if let Some(ch) = self.cache.lock().map_err(|_| locked())?.get(key).copied() {
            candidates.push(ch);
        }
        candidates.extend(1..=30);
        let mut tried = [false; 31];
        for port in candidates {
            ctx.checkpoint()?;
            if !(1..=30).contains(&port) || tried[port as usize] {
                continue;
            }
            tried[port as usize] = true;
            let attempt = OperationContext::new(
                "rfcomm-greeting".into(),
                ctx.token().clone(),
                Instant::now() + Duration::from_secs(2),
                Box::new(|| true),
                Arc::new(ProgressCell::default()),
            );
            if let Ok(socket) = Socket::connect(addr, port, &attempt) {
                if let Ok(session) = Session::open(Box::new(socket), &attempt) {
                    self.cache
                        .lock()
                        .map_err(|_| locked())?
                        .insert(key.into(), port);
                    return Ok(session);
                }
            }
        }
        self.cache.lock().map_err(|_| locked())?.remove(key);
        Err(fail("NO_RSHELL_GREETING"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdp_service_name_decoding_is_bounded_and_strict() {
        let mut storage = vec![0usize; 64];
        let name: Vec<u16> = "rshell".encode_utf16().chain([0]).collect();
        unsafe {
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                storage.as_mut_ptr().cast::<u16>(),
                name.len(),
            );
        }
        assert_eq!(
            service_name(&storage, storage.as_ptr().cast()).as_deref(),
            Some("rshell")
        );
        assert_eq!(service_name(&storage, std::ptr::null()), None);
        assert!(!bounded(
            &storage,
            storage.as_ptr().wrapping_add(storage.len()),
            1
        ));
        let invalid = [0xD800u16, 0];
        unsafe {
            std::ptr::copy_nonoverlapping(invalid.as_ptr(), storage.as_mut_ptr().cast::<u16>(), 2);
        }
        assert!(service_name(&storage, storage.as_ptr().cast()).is_none());
    }
    #[test]
    fn windows_bluetooth_context_has_the_sdp_address_format() {
        let s = String::from_utf16(&context(0x000102030405)[..19]).unwrap();
        assert_eq!(s, "(00:01:02:03:04:05)");
        assert!(address("not a routing key").is_err());
    }
}
