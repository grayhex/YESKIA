//! vCard/vCalendar normalization, ported from s60modern (nokia_pccs.py).
use crate::{
    pccs::{invalid, Item, MAX_BODY},
    Result,
};
use serde_json::{json, Value};
use std::collections::HashMap;
fn clean(s: &str, limit: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(if n == 'n' || n == 'N' { '\n' } else { n });
            }
        } else {
            out.push(c);
        }
    }
    out
}
fn split(s: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut escaped = false;
    for c in s.chars() {
        if c == ';' && !escaped {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
        escaped = c == '\\' && !escaped;
    }
    out
}
fn qp(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'=' {
            let pair = data.get(i + 1..i + 3).ok_or_else(invalid)?;
            let text = std::str::from_utf8(pair).map_err(|_| invalid())?;
            out.push(u8::from_str_radix(text, 16).map_err(|_| invalid())?);
            i += 3;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    Ok(out)
}
fn properties(payload: &[u8]) -> Result<Vec<(String, String)>> {
    if payload.len() > MAX_BODY || payload.contains(&0) {
        return Err(invalid());
    }
    let data = payload.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(payload);
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for raw in data.split(|c| *c == b'\n' || *c == b'\r') {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        if raw.is_empty() {
            continue;
        }
        if let Some(prev) = lines.last_mut() {
            let head = prev.split(|b| *b == b':').next().unwrap_or_default();
            let upper = String::from_utf8_lossy(head).to_ascii_uppercase();
            if (upper.contains("ENCODING=BASE64") || upper.split(';').any(|p| p == "ENCODING=B"))
                && !raw.contains(&b':')
            {
                prev.extend(raw);
                continue;
            }
            if prev.ends_with(b"=")
                && String::from_utf8_lossy(head)
                    .to_ascii_uppercase()
                    .contains("QUOTED-PRINTABLE")
            {
                prev.pop();
                prev.extend(raw);
                continue;
            }
            if raw.starts_with(b" ") || raw.starts_with(b"\t") {
                prev.extend(&raw[1..]);
                continue;
            }
        }
        lines.push(raw.to_vec());
    }
    let mut out = Vec::new();
    for line in lines {
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            return Err(invalid());
        };
        let head = std::str::from_utf8(&line[..colon]).map_err(|_| invalid())?;
        let mut parts = head.split(';');
        let name = parts
            .next()
            .unwrap_or_default()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let params: Vec<_> = parts.map(str::to_ascii_uppercase).collect();
        // Binary photo/key properties are irrelevant to a contact row.
        if params
            .iter()
            .any(|p| p == "ENCODING=BASE64" || p == "ENCODING=B")
        {
            continue;
        }
        let bytes = if params
            .iter()
            .any(|p| p == "QUOTED-PRINTABLE" || p == "ENCODING=QUOTED-PRINTABLE")
        {
            qp(&line[colon + 1..])?
        } else {
            line[colon + 1..].to_vec()
        };
        let charset = params.iter().find_map(|p| p.strip_prefix("CHARSET="));
        let encoding = match charset {
            Some(label) => encoding_rs::Encoding::for_label(label.trim_matches('"').as_bytes())
                .ok_or_else(invalid)?,
            None => {
                if std::str::from_utf8(&bytes).is_ok() {
                    encoding_rs::UTF_8
                } else {
                    encoding_rs::WINDOWS_1252
                }
            }
        };
        let (text, errors) = encoding.decode_without_bom_handling(&bytes);
        if errors {
            return Err(invalid());
        }
        out.push((name, text.into_owned()));
    }
    Ok(out)
}
fn contact(item: &Item) -> Result<Value> {
    let props = properties(&item.payload)?;
    if !props
        .first()
        .is_some_and(|(k, v)| k == "BEGIN" && v.eq_ignore_ascii_case("VCARD"))
        || !props
            .last()
            .is_some_and(|(k, v)| k == "END" && v.eq_ignore_ascii_case("VCARD"))
        || props
            .iter()
            .skip(1)
            .take(props.len().saturating_sub(2))
            .any(|(k, _)| k == "BEGIN" || k == "END")
    {
        return Err(invalid());
    }
    let (mut name, mut given, mut family) = (String::new(), String::new(), String::new());
    let (mut phones, mut emails) = (Vec::new(), Vec::new());
    for (key, v) in props {
        match key.as_str() {
            "FN" => name = clean(&unescape(&v), 300),
            "N" => {
                let p = split(&v);
                family = clean(&unescape(&p[0]), 300);
                given = clean(&unescape(p.get(1).map(String::as_str).unwrap_or("")), 300);
            }
            "TEL" => {
                let v = clean(v.strip_prefix("tel:").unwrap_or(&v), 300);
                if !v.is_empty() && !phones.contains(&v) && phones.len() < 30 {
                    phones.push(v);
                }
            }
            "EMAIL" => {
                let v = clean(&v, 300);
                if !v.is_empty() && !emails.contains(&v) && emails.len() < 30 {
                    emails.push(v);
                }
            }
            _ => {}
        }
    }
    if name.is_empty() {
        name = format!("{given} {family}").trim().to_owned();
    }
    if name.is_empty() {
        name = phones
            .first()
            .or(emails.first())
            .cloned()
            .ok_or_else(invalid)?;
    }
    Ok(
        json!({"native_id":format!("contacts:{}",item.id),"name":name,"given":given,"family":family,"phones":phones,"emails":emails}),
    )
}
fn when(v: &str) -> String {
    let b = v.as_bytes();
    if b.len() >= 8 && b[..8].iter().all(u8::is_ascii_digit) {
        let date = format!("{}-{}-{}", &v[..4], &v[4..6], &v[6..8]);
        if b.len() == 8 {
            return date;
        }
        if b.len() >= 13 && b[8] == b'T' && b[9..13].iter().all(u8::is_ascii_digit) {
            let seconds = if b.len() >= 15 && b[13..15].iter().all(u8::is_ascii_digit) {
                &v[13..15]
            } else {
                "00"
            };
            return format!(
                "{date}T{}:{}:{seconds}{}",
                &v[9..11],
                &v[11..13],
                if b.last() == Some(&b'Z') { "Z" } else { "" }
            );
        }
    }
    clean(v, 40)
}
fn calendar(item: &Item, domain: &str) -> Result<Vec<Value>> {
    let props = properties(&item.payload)?;
    let mut stack = Vec::new();
    let mut fields = HashMap::new();
    let mut rows = Vec::new();
    let mut roots = 0;
    for (name, value) in props {
        if name == "BEGIN" {
            let v = value.to_ascii_uppercase();
            if stack.is_empty() {
                if v != "VCALENDAR" || roots != 0 {
                    return Err(invalid());
                }
                roots += 1;
            } else if (v == "VEVENT" || v == "VTODO")
                && stack.last().map(String::as_str) != Some("VCALENDAR")
            {
                return Err(invalid());
            }
            if v == "VEVENT" || v == "VTODO" {
                fields.clear();
            }
            stack.push(v);
        } else if name == "END" {
            let kind = stack.pop().ok_or_else(invalid)?;
            if !kind.eq_ignore_ascii_case(&value) {
                return Err(invalid());
            }
            if (kind == "VEVENT" && domain == "calendar") || (kind == "VTODO" && domain == "tasks")
            {
                if fields.is_empty() {
                    return Err(invalid());
                }
                let get = |key: &str, limit| {
                    clean(
                        &unescape(fields.get(key).map(String::as_str).unwrap_or("")),
                        limit,
                    )
                };
                rows.push(json!({"native_id":format!("{domain}:{}:{}",item.id,rows.len()),"summary":get("SUMMARY",300),"start":when(&get("DTSTART",40)),"end":when(&get("DTEND",40)),"due":when(&get("DUE",40)),"location":get("LOCATION",300),"description":get("DESCRIPTION",2000),"recurrence":get("RRULE",300),"status":get("STATUS",40),"priority":get("PRIORITY",40)}));
            }
        } else {
            if stack.is_empty() {
                return Err(invalid());
            }
            if matches!(stack.last().map(String::as_str), Some("VEVENT" | "VTODO")) {
                fields.entry(name).or_insert(value);
            }
        }
    }
    if !stack.is_empty() || roots != 1 {
        return Err(invalid());
    }
    Ok(rows)
}
pub fn rows(items: &[Item], domain: &str) -> Result<Value> {
    let mut rows = Vec::new();
    for item in items {
        if domain == "contacts" {
            if !item.mime.to_ascii_lowercase().ends_with("vcard") {
                return Err(invalid());
            }
            rows.push(contact(item)?);
        } else {
            rows.extend(calendar(item, domain)?);
        }
        if rows.len() > 20000 {
            return Err(invalid());
        }
    }
    let out = Value::Array(rows);
    if serde_json::to_vec(&out).map_err(|_| invalid())?.len() > MAX_BODY {
        return Err(invalid());
    }
    Ok(out)
}
