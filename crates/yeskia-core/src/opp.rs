//! OBEX Object Push for an explicitly selected public certificate.
//! Delivery opens the phone-side inbox/installer workflow; it is not proof of trust installation.
use crate::{
    pccs::{self, Channel},
    Error, Result,
};
use std::time::{Duration, Instant};
pub fn push_certificate(channel: &mut dyn Channel, der: &[u8]) -> Result<()> {
    if der.is_empty() || der.len() > 512 * 1024 {
        return Err(Error::InvalidArgument("CERT_SIZE".into()));
    }
    let (_, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|_| Error::InvalidArgument("CERT_INVALID".into()))?;
    let is_ca = cert
        .basic_constraints()
        .map_err(|_| Error::InvalidArgument("CERT_INVALID".into()))?
        .is_some_and(|value| value.value.ca);
    let mime: &[u8] = if is_ca {
        b"application/x-x509-ca-cert\0"
    } else {
        b"application/x-x509-user-cert\0"
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    let reply = channel.exchange(
        &pccs::packet(0x80, &[0x10, 0, 0xff, 0xff])?,
        Duration::from_secs(3),
    )?;
    let (code, headers) = pccs::response(&reply, 4)?;
    if code != 0xa0 {
        return Err(Error::CapabilityUnsupported("CERT_OPP_UNAVAILABLE".into()));
    }
    let mtu = u16::from_be_bytes([reply[5], reply[6]]) as usize;
    if mtu < 255 {
        return Err(pccs::invalid());
    }
    let connection = headers
        .iter()
        .find(|(id, v)| *id == 0xcb && v.len() == 4)
        .map(|(_, v)| v.clone());
    let mut pos = 0;
    let mut first = true;
    let result = (|| {
        while pos < der.len() {
            let mut h = Vec::new();
            if let Some(id) = &connection {
                h.push(0xcb);
                h.extend(id);
            }
            if first {
                let name: Vec<u8> = "YESKIA-certificate.cer"
                    .encode_utf16()
                    .chain([0])
                    .flat_map(u16::to_be_bytes)
                    .collect();
                h.extend(pccs::header(1, &name)?);
                h.extend(pccs::header(0x42, mime)?);
                h.push(0xc3);
                h.extend((der.len() as u32).to_be_bytes());
            }
            let capacity = mtu
                .min(4096)
                .checked_sub(h.len() + 6)
                .filter(|n| *n > 0)
                .ok_or_else(pccs::invalid)?;
            let end = (pos + capacity).min(der.len());
            let final_packet = end == der.len();
            h.extend(pccs::header(
                if final_packet { 0x49 } else { 0x48 },
                &der[pos..end],
            )?);
            // A lost response after PUT is an uncertain delivery. Never retry it automatically.
            let packet = pccs::packet(if final_packet { 0x82 } else { 0x02 }, &h)?;
            let reply = channel
                .exchange(
                    &packet,
                    deadline
                        .checked_duration_since(Instant::now())
                        .ok_or_else(|| Error::OperationFailed("CERT_TRANSFER_UNCERTAIN".into()))?
                        .min(Duration::from_secs(15)),
                )
                .map_err(|_| Error::OperationFailed("CERT_TRANSFER_UNCERTAIN".into()))?;
            let (code, _) = pccs::response(&reply, 0)
                .map_err(|_| Error::OperationFailed("CERT_TRANSFER_UNCERTAIN".into()))?;
            if code != if final_packet { 0xa0 } else { 0x90 } {
                return Err(Error::OperationFailed("CERT_PHONE_REFUSED".into()));
            }
            first = false;
            pos = end;
        }
        Ok(())
    })();
    let mut h = Vec::new();
    if let Some(id) = connection {
        h.push(0xcb);
        h.extend(id);
    }
    if let Ok(packet) = pccs::packet(0x81, &h) {
        let _ = channel.exchange(&packet, Duration::from_millis(500));
    }
    result
}
