//! Native 3GPP PDU reads. CMGL mode=1 preserves unread/read status; no CMGF/CMGS/CMGD writes.
use crate::{at::SerialPort, Error, Result};
use serde_json::{json, Value};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
fn invalid() -> Error {
    Error::Transport("SMS_INVALID_RESPONSE".into())
}
fn command(port: &mut dyn SerialPort, text: &str, cancel: &AtomicBool) -> Result<String> {
    if cancel.load(Ordering::SeqCst) {
        return Err(Error::Cancelled);
    }
    port.write_all(format!("{text}\r").as_bytes())
        .map_err(|_| Error::DeviceDisconnected)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut data = Vec::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        let mut buffer = [0u8; 4096];
        let n = port
            .read(&mut buffer, Duration::from_millis(100))
            .map_err(|_| Error::DeviceDisconnected)?;
        data.extend(&buffer[..n]);
        if data.len() > 1024 * 1024 {
            return Err(Error::LimitExceeded("SMS_RESPONSE".into()));
        }
        let text = std::str::from_utf8(&data).map_err(|_| invalid())?;
        if text.ends_with("\r\nOK\r\n") {
            return Ok(text.to_owned());
        }
        if text.ends_with("\r\nERROR\r\n")
            || text
                .lines()
                .any(|l| l.starts_with("+CMS ERROR:") || l.starts_with("+CME ERROR:"))
        {
            return Err(Error::CapabilityUnsupported("SMS_READ_UNAVAILABLE".into()));
        }
    }
}
pub fn read(port: &mut dyn SerialPort, cancel: &AtomicBool) -> Result<Value> {
    let mode = command(port, "AT+CMGF?", cancel)?;
    if !mode.lines().any(|l| l.trim() == "+CMGF: 0") {
        return Err(Error::CapabilityUnsupported("SMS_PDU_MODE_REQUIRED".into()));
    }
    let reply = command(port, "AT+CMGL=4,1", cancel)?;
    parse_list(&reply)
}
pub fn parse_list(reply: &str) -> Result<Value> {
    if reply.lines().last().map(str::trim) != Some("OK") {
        return Err(invalid());
    }
    let mut rows = Vec::new();
    let mut lines = reply.lines().map(str::trim).filter(|l| !l.is_empty());
    while let Some(line) = lines.next() {
        if line == "OK" || line == "AT+CMGL=4,1" {
            continue;
        }
        let head = line.strip_prefix("+CMGL:").ok_or_else(invalid)?;
        let fields: Vec<_> = head.split(',').map(str::trim).collect();
        if fields.len() < 3 {
            return Err(invalid());
        }
        let id = fields[0].parse::<u32>().map_err(|_| invalid())?;
        let size = fields
            .last()
            .unwrap()
            .parse::<usize>()
            .map_err(|_| invalid())?;
        let hex = lines.next().ok_or_else(invalid)?;
        if hex.len() > 4096 || hex.len() % 2 != 0 {
            return Err(invalid());
        }
        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
                    .ok_or_else(invalid)
            })
            .collect::<Result<_>>()?;
        if bytes
            .first()
            .map_or(true, |smsc| bytes.len() != 1 + *smsc as usize + size)
        {
            return Err(invalid());
        }
        let mut row = decode(&bytes)?;
        row["native_id"] = json!(format!("sms:{id}"));
        row["state"] = json!(match fields[1] {
            "0" => "unread",
            "1" => "read",
            "2" => "unsent",
            "3" => "sent",
            _ => return Err(invalid()),
        });
        rows.push(row);
        if rows.len() > 5000 {
            return Err(invalid());
        }
    }
    Ok(Value::Array(rows))
}
fn take<'a>(bytes: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let out = bytes.get(*pos..*pos + n).ok_or_else(invalid)?;
    *pos += n;
    Ok(out)
}
fn byte(bytes: &[u8], pos: &mut usize) -> Result<u8> {
    Ok(take(bytes, pos, 1)?[0])
}
fn number(bytes: &[u8], pos: &mut usize) -> Result<String> {
    let n = byte(bytes, pos)? as usize;
    let kind = byte(bytes, pos)?;
    if n > 40 {
        return Err(invalid());
    }
    let raw = take(bytes, pos, n.div_ceil(2))?;
    if kind & 0x70 == 0x50 {
        return gsm(raw, n * 4 / 7, 0);
    }
    let mut out = if kind & 0x70 == 0x10 {
        "+".into()
    } else {
        String::new()
    };
    for i in 0..n {
        let digit = (raw[i / 2] >> (if i % 2 == 0 { 0 } else { 4 })) & 15;
        if digit > 9 {
            return Err(invalid());
        }
        out.push((b'0' + digit) as char);
    }
    Ok(out)
}
const GSM:&str="@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞ\u{1b}ÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà";
fn gsm(data: &[u8], count: usize, skip: usize) -> Result<String> {
    if skip > count {
        return Err(invalid());
    }
    let alphabet: Vec<_> = GSM.chars().collect();
    let mut out = String::new();
    let mut escaped = false;
    for index in skip..count {
        let bit = index * 7;
        let a = *data.get(bit / 8).ok_or_else(invalid)? as u16;
        let b = data.get(bit / 8 + 1).copied().unwrap_or(0) as u16;
        let code = (((a | (b << 8)) >> (bit % 8)) & 127) as usize;
        if escaped {
            out.push(match code {
                10 => '\u{c}',
                20 => '^',
                40 => '{',
                41 => '}',
                47 => '\\',
                60 => '[',
                61 => '~',
                62 => ']',
                64 => '|',
                101 => '€',
                _ => return Err(invalid()),
            });
            escaped = false;
        } else if code == 27 {
            escaped = true;
        } else {
            out.push(*alphabet.get(code).ok_or_else(invalid)?);
        }
    }
    if escaped {
        return Err(invalid());
    }
    Ok(out)
}
pub fn decode(bytes: &[u8]) -> Result<Value> {
    let mut pos = 0;
    let smsc = byte(bytes, &mut pos)? as usize;
    take(bytes, &mut pos, smsc)?;
    let flags = byte(bytes, &mut pos)?;
    let kind = flags & 3;
    let incoming = kind == 0;
    if kind == 1 {
        byte(bytes, &mut pos)?;
    } else if kind != 0 {
        return Err(invalid());
    }
    let number = number(bytes, &mut pos)?;
    byte(bytes, &mut pos)?;
    let dcs = byte(bytes, &mut pos)?;
    let encoding = match dcs {
        0x00..=0x1f => (dcs >> 2) & 3,
        0xc0..=0xdf => 0,
        0xe0..=0xef => 2,
        0xf0..=0xff => {
            if dcs & 4 == 0 {
                0
            } else {
                1
            }
        }
        _ => return Err(invalid()),
    };
    let mut date = String::new();
    if incoming {
        let stamp = take(bytes, &mut pos, 7)?;
        let mut parts = Vec::new();
        for b in &stamp[..6] {
            let n = (b & 15) * 10 + (b >> 4);
            if b & 15 > 9 || b >> 4 > 9 {
                return Err(invalid());
            }
            parts.push(n);
        }
        date = format!(
            "20{:02}-{:02}-{:02}T{:02}:{:02}:{:02}",
            parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
        );
    } else {
        let validity = (flags >> 3) & 3;
        take(
            bytes,
            &mut pos,
            match validity {
                0 => 0,
                2 => 1,
                _ => 7,
            },
        )?;
    }
    let length = byte(bytes, &mut pos)? as usize;
    let byte_len = if encoding == 0 {
        (length * 7).div_ceil(8)
    } else {
        length
    };
    let data = take(bytes, &mut pos, byte_len)?;
    if pos != bytes.len() {
        return Err(invalid());
    }
    let mut offset = 0;
    let mut part = None;
    let mut parts = None;
    if flags & 0x40 != 0 {
        let end = data.first().copied().ok_or_else(invalid)? as usize + 1;
        if end > data.len() {
            return Err(invalid());
        }
        offset = end;
        let mut i = 1;
        while i < end {
            let id = *data.get(i).ok_or_else(invalid)?;
            let n = *data.get(i + 1).ok_or_else(invalid)? as usize;
            i += 2;
            let v = data
                .get(i..i + n)
                .filter(|_| i + n <= end)
                .ok_or_else(invalid)?;
            if id == 0 && n == 3 {
                parts = Some(v[1]);
                part = Some(v[2]);
            } else if id == 8 && n == 4 {
                parts = Some(v[2]);
                part = Some(v[3]);
            }
            i += n;
        }
    }
    let text = match encoding {
        0 => gsm(data, length, (offset * 8).div_ceil(7))?,
        2 => {
            let raw = &data[offset..];
            if raw.len() % 2 != 0 {
                return Err(invalid());
            }
            let units: Vec<_> = raw
                .chunks_exact(2)
                .map(|p| u16::from_be_bytes([p[0], p[1]]))
                .collect();
            String::from_utf16(&units).map_err(|_| invalid())?
        }
        _ => return Err(Error::CapabilityUnsupported("SMS_BINARY".into())),
    };
    Ok(
        json!({"number":number,"text":text,"date":date,"direction":if incoming{"incoming"}else{"outgoing"},"part":part,"parts":parts}),
    )
}
