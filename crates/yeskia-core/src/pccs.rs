//! Native read-only port of s60modern's nokia_pccs.py (MIT, efaf53d).
//! USB OBEX sync negotiation only; no CommitChanges/CreateItems/DeleteItems builders.
use crate::{Error, Result};
use std::time::{Duration, Instant};
pub const TARGET: &[u8] = &[
    0xf9, 0xec, 0x7b, 0xc4, 0x95, 0x3c, 0x11, 0xd2, 0x98, 0x4e, 0x52, 0x54, 0x00, 0xdc, 0x9e, 0x09,
];
const TYPE: &[u8] = b"application/vnd.nokia.conml+wbxml\0";
pub const MAX_BODY: usize = 8 * 1024 * 1024;
const FORMAT: &[u8] = &[
    1, 8, 0, 0, 0, 32, 67, 111, 110, 116, 97, 99, 116, 115, 0, 0, 0, 0, 0, 0, 0, 0, 63, 0, 1, 0, 1,
    12, 0, 0, 0, 50, 116, 101, 120, 116, 47, 120, 45, 118, 99, 97, 114, 100, 3, 0, 0, 0, 14, 50,
    46, 49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
pub trait Channel {
    fn exchange(&mut self, packet: &[u8], timeout: Duration) -> Result<Vec<u8>>;
}
pub fn invalid() -> Error {
    Error::Transport("PCCS_INVALID_RESPONSE".into())
}
pub fn command_name(logical: &[u8]) -> Vec<u8> {
    let mut data = logical.to_vec();
    if data.len() % 2 != 0 {
        data.push(0);
    }
    data.extend([0, 0]);
    for pair in data.chunks_exact_mut(2) {
        pair.swap(0, 1);
    }
    data
}
pub fn header(id: u8, value: &[u8]) -> Result<Vec<u8>> {
    let size = u16::try_from(value.len() + 3).map_err(|_| invalid())?;
    let mut data = vec![id];
    data.extend(size.to_be_bytes());
    data.extend(value);
    Ok(data)
}
pub fn packet(op: u8, body: &[u8]) -> Result<Vec<u8>> {
    let size = u16::try_from(body.len() + 3).map_err(|_| invalid())?;
    let mut out = vec![op];
    out.extend(size.to_be_bytes());
    out.extend(body);
    Ok(out)
}
type Headers = Vec<(u8, Vec<u8>)>;
pub fn response(data: &[u8], fixed: usize) -> Result<(u8, Headers)> {
    if data.len() < 3 + fixed || u16::from_be_bytes([data[1], data[2]]) as usize != data.len() {
        return Err(invalid());
    }
    let mut headers = Vec::new();
    let mut pos = 3 + fixed;
    while pos < data.len() {
        let id = data[pos];
        let kind = id >> 6;
        let (start, size) = if kind < 2 {
            let b = data.get(pos + 1..pos + 3).ok_or_else(invalid)?;
            let size = u16::from_be_bytes([b[0], b[1]]) as usize;
            if size < 3 {
                return Err(invalid());
            }
            (pos + 3, size)
        } else {
            (pos + 1, if kind == 2 { 2 } else { 5 })
        };
        let value = data.get(start..pos + size).ok_or_else(invalid)?;
        headers.push((id, value.to_vec()));
        pos += size;
    }
    Ok((data[0], headers))
}
pub struct Session<'a> {
    channel: &'a mut dyn Channel,
    id: [u8; 4],
    deadline: Instant,
}
impl<'a> Session<'a> {
    pub fn connect(channel: &'a mut dyn Channel) -> Result<Self> {
        let mut body = vec![0x10, 0, 0xff, 0xff];
        body.extend(header(0x46, TARGET)?);
        let (code, h) = response(
            &channel.exchange(&packet(0x80, &body)?, Duration::from_secs(3))?,
            4,
        )?;
        let id = h
            .iter()
            .find(|(i, _)| *i == 0xcb)
            .map(|(_, v)| v.as_slice())
            .ok_or_else(invalid)?;
        if code != 0xa0 || !h.iter().any(|(i, v)| *i == 0x4a && v == TARGET) {
            return Err(Error::CapabilityUnsupported("PCCS_TARGET".into()));
        }
        Ok(Self {
            channel,
            id: id.try_into().map_err(|_| invalid())?,
            deadline: Instant::now() + Duration::from_secs(120),
        })
    }
    fn exchange(&mut self, p: &[u8]) -> Result<Vec<u8>> {
        let remain = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(Error::Timeout)?;
        self.channel
            .exchange(p, remain.min(Duration::from_secs(15)))
    }
    fn headers(&self, name: &[u8]) -> Result<Vec<u8>> {
        let mut h = vec![0xcb];
        h.extend(self.id);
        h.extend(header(1, &command_name(name))?);
        Ok(h)
    }
    fn get(&mut self, name: &[u8]) -> Result<Vec<u8>> {
        let mut h = self.headers(name)?;
        h.extend(header(0x42, TYPE)?);
        h.extend(header(0x4a, b"PC Suite")?);
        let mut request = packet(0x83, &h)?;
        let mut body = Vec::new();
        for _ in 0..4096 {
            let (code, headers) = response(&self.exchange(&request)?, 0)?;
            for (id, value) in headers {
                if id == 0x48 || id == 0x49 {
                    if body.len() + value.len() > MAX_BODY {
                        return Err(invalid());
                    }
                    body.extend(value);
                }
            }
            if code == 0xa0 {
                return Ok(body);
            }
            if code != 0x90 {
                return Err(Error::OperationFailed("PCCS_REFUSED".into()));
            }
            request = packet(0x83, &[])?;
        }
        Err(invalid())
    }
    fn set_format(&mut self, format: &[u8]) -> Result<()> {
        let mut h = self.headers(b"SYNC: SetRemoteStoreFormat")?;
        h.push(0xc3);
        h.extend((format.len() as u32).to_be_bytes());
        h.extend(header(0x42, TYPE)?);
        h.extend(header(0x4a, b"PC Suite")?);
        h.extend(header(0x49, format)?);
        let request = packet(0x82, &h)?;
        let (code, _) = response(&self.exchange(&request)?, 0)?;
        if code == 0xa0 || code == 0x90 {
            Ok(())
        } else {
            Err(Error::OperationFailed("PCCS_FORMAT_REFUSED".into()))
        }
    }
    pub fn read_store(&mut self, kind: &str) -> Result<Vec<Item>> {
        let (uid, extra, name) = match kind {
            "contacts" => (
                [0xdd, 0x6d, 0x1f, 0x10],
                [0x9c, 0x9a, 0x23, 0x5d],
                "C:Contacts.cdb",
            ),
            "calendar" | "tasks" => (
                [0xde, 0x6d, 0x1f, 0x10],
                [0x54, 0x2a, 0x31, 0x75],
                "Calendar",
            ),
            _ => return Err(Error::CapabilityUnsupported("PCCS_DOMAIN".into())),
        };
        let mut open = b"SYNC: OpenStore: ".to_vec();
        open.extend(uid);
        open.extend(extra);
        open.extend((name.len() as u16).to_le_bytes());
        open.extend(name.as_bytes());
        let answer = self.get(&open)?;
        let result = (|| {
            if le32(&answer, 0)? != 1 {
                return Err(Error::OperationFailed("PCCS_STORE_REFUSED".into()));
            }
            self.get(b"SYNC: ResetChangeInfo")?;
            let format = self.get(b"SYNC: GetStoreFormat")?;
            self.set_format(if kind == "contacts" {
                FORMAT
            } else {
                format.get(4..).ok_or_else(invalid)?
            })?;
            let ids = parse_ids(&self.get(b"SYNC: ListChanges")?)?;
            let mut out = Vec::new();
            let mut total = 0;
            // Only one/two ids were exercised on the source E72. Keep that batch size.
            for batch in ids.chunks(2) {
                let mut name = b"SYNC: ReadItems: ".to_vec();
                name.extend((batch.len() as u32).to_le_bytes());
                for id in batch {
                    name.extend(id.to_le_bytes());
                }
                let part = parse_items(&self.get(&name)?)?;
                if part.len() != batch.len()
                    || part.iter().any(|i| !batch.contains(&i.id))
                    || (part.len() == 2 && part[0].id == part[1].id)
                {
                    return Err(invalid());
                }
                for item in part {
                    total += item.payload.len();
                    if total > MAX_BODY {
                        return Err(invalid());
                    }
                    out.push(item);
                }
            }
            Ok(out)
        })();
        let _ = self.get(b"SYNC: CloseStore");
        result
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        let mut h = vec![0xcb];
        h.extend(self.id);
        if let Ok(p) = packet(0x81, &h) {
            let _ = self.channel.exchange(&p, Duration::from_millis(500));
        }
    }
}
fn le32(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn le16(b: &[u8], p: usize) -> Result<usize> {
    Ok(u16::from_le_bytes(
        b.get(p..p + 2)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ) as usize)
}
pub fn parse_ids(data: &[u8]) -> Result<Vec<u32>> {
    let n = le32(data, 0)? as usize;
    if n > 20000 || data.len() < 4 + n * 4 {
        return Err(invalid());
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in 0..n {
        let id = le32(data, 4 + p * 4)?;
        if !seen.insert(id) {
            return Err(invalid());
        }
        out.push(id);
    }
    Ok(out)
}
#[derive(Debug)]
pub struct Item {
    pub id: u32,
    pub mime: String,
    pub payload: Vec<u8>,
}
pub fn parse_items(data: &[u8]) -> Result<Vec<Item>> {
    let n = le32(data, 0)? as usize;
    if n > 20000 || data.len() > MAX_BODY {
        return Err(invalid());
    }
    let mut pos = 4;
    let mut out = Vec::new();
    for _ in 0..n {
        let block = le32(data, pos)? as usize;
        let id = le32(data, pos + 4)?;
        let body = pos + 8;
        let start = (body..data.len().min(body + 64))
            .find(|&i| {
                data[i] == 0x80
                    && le16(data, i + 1).is_ok_and(|size| {
                        (5..=40).contains(&size)
                            && data.get(i + 3..i + 8) == Some(b"text/")
                            && i + 3 + size <= data.len()
                    })
            })
            .ok_or_else(invalid)?;
        let size = le16(data, start + 1)?;
        let end = start + 3 + size;
        let mime = String::from_utf8(data[start + 3..end].to_vec()).map_err(|_| invalid())?;
        let vsize = le16(data, end)?;
        if vsize > 16 {
            return Err(invalid());
        }
        let p = end + 2 + vsize;
        let length = le32(data, p)? as usize;
        if length > MAX_BODY {
            return Err(invalid());
        }
        let end = p + 4 + length;
        let payload = data.get(p + 4..end).ok_or_else(invalid)?.to_vec();
        let next = pos.checked_add(4 + block).ok_or_else(invalid)?;
        pos = if next >= end && next <= data.len() {
            next
        } else {
            end
        };
        out.push(Item { id, mime, payload });
    }
    Ok(out)
}
