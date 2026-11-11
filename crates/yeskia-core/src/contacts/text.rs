use super::*;
use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1251};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImportOptions {
    pub format: String,
    pub encoding: String,
    pub delimiter: Option<String>,
    pub mapping: BTreeMap<usize, String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CsvPreview {
    pub columns: Vec<String>,
    pub sample: Vec<Vec<String>>,
    pub rows: usize,
    pub delimiter: String,
    pub encoding: String,
    pub mapping: BTreeMap<usize, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportResult {
    pub book: Book,
    pub warnings: Vec<String>,
    pub encoding: String,
}
fn invalid() -> Error {
    Error::InvalidArgument("CONTACT_IMPORT_INVALID".into())
}
pub fn decode(bytes: &[u8], encoding: &str) -> Result<(String, String)> {
    if bytes.len() > MAX_IMPORT {
        return Err(Error::LimitExceeded("CONTACT_IMPORT_LIMIT".into()));
    }
    let (data, enc) = if bytes.starts_with(&[0xFF, 0xFE]) {
        (&bytes[2..], "utf-16le")
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (&bytes[2..], "utf-16be")
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (&bytes[3..], "utf-8")
    } else {
        (bytes, encoding)
    };
    let selected = if enc.is_empty() || enc == "auto" {
        if std::str::from_utf8(data).is_ok() {
            "utf-8"
        } else {
            "windows-1251"
        }
    } else {
        enc
    };
    let text = match selected {
        "utf-8" => std::str::from_utf8(data).map_err(|_| invalid())?.to_owned(),
        "utf-16le" | "utf-16be" | "windows-1251" => {
            let codec = match selected {
                "utf-16le" => UTF_16LE,
                "utf-16be" => UTF_16BE,
                _ => WINDOWS_1251,
            };
            let (s, errors) = codec.decode_without_bom_handling(data);
            if errors {
                return Err(invalid());
            }
            s.into_owned()
        }
        _ => return Err(invalid()),
    };
    if text.contains('\0') {
        return Err(invalid());
    }
    Ok((text, selected.into()))
}
pub(super) fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace(';', "\\;")
        .replace(',', "\\,")
}
fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut it = value.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(n) = it.next() {
                out.push(if n == 'n' || n == 'N' { '\n' } else { n });
            }
        } else {
            out.push(c);
        }
    }
    out
}
fn components(value: &str, delimiter: char) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut escaped = false;
    for c in value.chars() {
        if c == delimiter && !escaped {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
        escaped = c == '\\' && !escaped;
    }
    out.into_iter().map(|v| unescape(&v)).collect()
}
fn colon(line: &str) -> Option<usize> {
    let (mut quote, mut escaped) = (false, false);
    for (i, c) in line.char_indices() {
        if c == '"' && !escaped {
            quote = !quote;
        }
        if c == ':' && !quote {
            return Some(i);
        }
        escaped = c == '\\' && !escaped;
    }
    None
}
fn qp(value: &str, charset: Option<&str>) -> Result<String> {
    let mut data = Vec::new();
    let b = value.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3).ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            data.push(u8::from_str_radix(hex, 16).map_err(|_| invalid())?);
            i += 3;
        } else {
            data.push(b[i]);
            i += 1;
        }
    }
    let enc = encoding_rs::Encoding::for_label(charset.unwrap_or("utf-8").as_bytes())
        .ok_or_else(invalid)?;
    let (s, errors) = enc.decode_without_bom_handling(&data);
    if errors {
        return Err(invalid());
    }
    Ok(s.into_owned())
}
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if raw.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.to_ascii_uppercase().contains("QUOTED-PRINTABLE") && last.ends_with('=') {
                last.pop();
                last.push_str(raw);
                continue;
            }
            if raw.starts_with([' ', '\t']) {
                last.push_str(&raw[1..]);
                continue;
            }
            if last.to_ascii_uppercase().contains("ENCODING=BASE64") && !raw.contains(':') {
                last.push_str(raw);
                continue;
            }
        }
        out.push(raw.into());
    }
    out
}
fn vcard(text: &str) -> Result<ImportResult> {
    let mut records = Vec::new();
    let mut current: Option<Contact> = None;
    let mut warnings = Vec::new();
    for line in unfold(text) {
        if line.eq_ignore_ascii_case("BEGIN:VCARD") {
            if let Some(c) = current.take() {
                records.push(c);
                warnings.push("partial-card-preserved".into());
            }
            current = Some(Contact {
                id: crate::workbench::local_id()?,
                source: Source {
                    kind: "vcard".into(),
                    ..Default::default()
                },
                status: "local".into(),
                ..Default::default()
            });
            continue;
        }
        if line.eq_ignore_ascii_case("END:VCARD") {
            if let Some(c) = current.take() {
                records.push(c);
            } else {
                warnings.push("unmatched-end".into());
            }
            continue;
        }
        let Some(c) = current.as_mut() else {
            continue;
        };
        let Some(at) = colon(&line) else {
            warnings.push("malformed-property-preserved-in-note".into());
            c.note.push_str(&format!("\n{line}"));
            continue;
        };
        let head = &line[..at];
        let raw = &line[at + 1..];
        let mut headparts = head.split(';');
        let name = headparts
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let params: Vec<String> = headparts.map(str::to_owned).collect();
        let upper: Vec<String> = params.iter().map(|v| v.to_ascii_uppercase()).collect();
        if upper
            .iter()
            .any(|p| p == "ENCODING=B" || p == "ENCODING=BASE64")
        {
            c.unknown.push(Property {
                head: head.into(),
                value: raw.into(),
            });
            warnings.push("binary-field-preserved-not-writable".into());
            continue;
        }
        let charset = params.iter().find_map(|p| {
            p.split_once('=')
                .filter(|(k, _)| k.eq_ignore_ascii_case("CHARSET"))
                .map(|(_, v)| v.trim_matches('"'))
        });
        let value = if upper.iter().any(|p| p == "ENCODING=QUOTED-PRINTABLE") {
            qp(raw, charset)?
        } else {
            raw.into()
        };
        let label = params
            .iter()
            .find_map(|p| {
                p.split_once('=')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("TYPE"))
                    .map(|(_, v)| v.trim_matches('"').to_string())
            })
            .or_else(|| params.iter().find(|p| !p.contains('=')).cloned())
            .unwrap_or_default();
        let field = || Value {
            value: unescape(&value),
            label: label.clone(),
            params: params
                .iter()
                .filter(|p| {
                    !p.to_ascii_uppercase().starts_with("ENCODING=")
                        && !p.to_ascii_uppercase().starts_with("CHARSET=")
                })
                .map(|p| {
                    if p.contains('=') {
                        p.clone()
                    } else {
                        format!("TYPE={p}")
                    }
                })
                .collect(),
            components: Vec::new(),
        };
        match name.as_str() {
            "VERSION" => {
                if !["2.1", "3.0", "4.0"].contains(&value.as_str()) {
                    warnings.push("unknown-vcard-version".into());
                }
            }
            "FN" if c.name.is_empty() => c.name = unescape(&value),
            "N" if c.given.is_empty() && c.family.is_empty() => {
                let mut n = components(&value, ';');
                n.resize(n.len().max(5), String::new());
                c.family = n[0].clone();
                c.given = n[1].clone();
                c.middle = n[2].clone();
                c.prefix = n[3].clone();
                c.suffix = n[4].clone();
                if n.len() > 5 {
                    c.unknown.push(Property {
                        head: head.into(),
                        value: raw.into(),
                    });
                    warnings.push("extended-name-preserved".into());
                }
            }
            "TEL" => {
                let mut f = field();
                f.value = f.value.strip_prefix("tel:").unwrap_or(&f.value).into();
                c.phones.push(f);
            }
            "EMAIL" => c.emails.push(field()),
            "ADR" => {
                let mut f = field();
                f.components = components(&value, ';');
                f.value = f
                    .components
                    .iter()
                    .filter(|v| !v.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                c.addresses.push(f);
            }
            "ORG" if c.organisation.is_empty() => c.organisation = components(&value, ';'),
            "TITLE" if c.title.is_empty() => c.title = unescape(&value),
            "NOTE" if c.note.is_empty() => c.note = unescape(&value),
            "BDAY" if c.birthday.is_empty() => c.birthday = unescape(&value),
            "URL" => c.websites.push(field()),
            "CATEGORIES" => c.groups.extend(components(&value, ',')),
            _ => {
                c.unknown.push(Property {
                    head: head.into(),
                    value: raw.into(),
                });
                warnings.push("unknown-properties-preserved".into());
            }
        }
    }
    if let Some(c) = current {
        records.push(c);
        warnings.push("partial-card-preserved".into());
    }
    if records.is_empty() {
        return Err(invalid());
    }
    for (i, c) in records.iter_mut().enumerate() {
        if c.name.is_empty() {
            c.name = format!("{} {}", c.given, c.family).trim().into();
            if c.name.is_empty() {
                c.name = c
                    .phones
                    .first()
                    .map(|v| v.value.clone())
                    .unwrap_or_else(|| format!("Contact {}", i + 1));
            }
        }
    }
    let book = Book { contacts: records };
    validate(&book)?;
    warnings.sort();
    warnings.dedup();
    Ok(ImportResult {
        book,
        warnings,
        encoding: String::new(),
    })
}
fn delimiter(text: &str, provided: Option<&str>) -> Result<char> {
    if let Some(v) = provided {
        let mut chars = v.chars();
        let c = chars.next().ok_or_else(invalid)?;
        if ![',', ';', '\t'].contains(&c) || chars.next().is_some() {
            return Err(invalid());
        }
        return Ok(c);
    }
    let candidates = [',', ';', '\t'];
    let mut counts = [0usize; 3];
    let mut quoted = false;
    for c in text.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            if c == '\n' || c == '\r' {
                break;
            }
            for (i, d) in candidates.iter().enumerate() {
                if c == *d {
                    counts[i] += 1;
                }
            }
        }
    }
    Ok((0..3)
        .max_by_key(|i| counts[*i])
        .filter(|i| counts[*i] > 0)
        .map(|i| candidates[i])
        .unwrap_or(','))
}
fn csv_rows(text: &str, delimiter: char) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            }
            '"' if field.is_empty() => quoted = true,
            c if c == delimiter && !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                if row.iter().any(|v| !v.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                if rows.len() > MAX_CONTACTS + 1 {
                    return Err(Error::LimitExceeded("CONTACT_BOOK_LIMIT".into()));
                }
            }
            '\r' if !quoted => {}
            c => field.push(c),
        }
    }
    if quoted {
        return Err(invalid());
    }
    row.push(field);
    if row.iter().any(|v| !v.is_empty()) {
        rows.push(row);
    }
    if rows.iter().any(|r| r.len() > 512) {
        return Err(invalid());
    }
    Ok(rows)
}
fn suggested(header: &str) -> String {
    let h = header.to_lowercase();
    if h == "_yeskia_contact" {
        "json"
    } else if ["name", "full name", "display name", "имя", "фио"].contains(&h.as_str()) {
        "name"
    } else if h == "given name" || h == "first name" || h == "имя (личное)" {
        "given"
    } else if h == "family name" || h == "last name" || h == "фамилия" {
        "family"
    } else if h.contains("phone") || h.contains("телефон") {
        if h.ends_with("type") {
            "phone-label"
        } else {
            "phone"
        }
    } else if h.contains("email") || h.contains("e-mail") {
        if h.ends_with("type") {
            "email-label"
        } else {
            "email"
        }
    } else if h.contains("company") || h == "organization 1 - name" || h == "компания" {
        "company"
    } else if h == "notes" || h == "note" || h == "заметки" {
        "note"
    } else if h.contains("birthday") || h == "день рождения" {
        "birthday"
    } else {
        "preserve"
    }
    .into()
}
pub fn preview_csv(bytes: &[u8], options: &ImportOptions) -> Result<CsvPreview> {
    let (text, encoding) = decode(bytes, &options.encoding)?;
    let sep = delimiter(&text, options.delimiter.as_deref())?;
    let mut rows = csv_rows(&text, sep)?;
    if rows.is_empty() {
        return Err(invalid());
    }
    let columns = rows.remove(0);
    let mapping = columns
        .iter()
        .enumerate()
        .map(|(i, h)| {
            (
                i,
                options
                    .mapping
                    .get(&i)
                    .cloned()
                    .unwrap_or_else(|| suggested(h)),
            )
        })
        .collect();
    Ok(CsvPreview {
        columns,
        sample: rows.iter().take(5).cloned().collect(),
        rows: rows.len(),
        delimiter: sep.to_string(),
        encoding,
        mapping,
    })
}
fn csv(text: &str, options: &ImportOptions) -> Result<ImportResult> {
    let sep = delimiter(text, options.delimiter.as_deref())?;
    let mut rows = csv_rows(text, sep)?;
    if rows.is_empty() {
        return Err(invalid());
    }
    let headers = rows.remove(0);
    let mapping: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            options
                .mapping
                .get(&i)
                .cloned()
                .unwrap_or_else(|| suggested(h))
        })
        .collect();
    let mut contacts = Vec::new();
    let mut warnings = Vec::new();
    for row in rows {
        if row.len() > headers.len() {
            return Err(invalid());
        }
        if let Some(i) = mapping.iter().position(|m| m == "json") {
            if let Some(raw) = row.get(i).filter(|v| !v.is_empty()) {
                let c: Contact = serde_json::from_str(raw).map_err(|_| invalid())?;
                contacts.push(c);
                continue;
            }
        }
        let mut c = Contact {
            id: crate::workbench::local_id()?,
            source: Source {
                kind: "csv".into(),
                ..Default::default()
            },
            status: "local".into(),
            ..Default::default()
        };
        for (i, value) in row.iter().enumerate() {
            if value.is_empty() {
                continue;
            }
            match mapping.get(i).map(String::as_str).unwrap_or("preserve") {
                "name" => c.name = value.clone(),
                "given" => c.given = value.clone(),
                "family" => c.family = value.clone(),
                "middle" => c.middle = value.clone(),
                "company" => c.organisation.push(value.clone()),
                "prefix" => c.prefix = value.clone(),
                "suffix" => c.suffix = value.clone(),
                "address" => c.addresses.push(Value {
                    value: value.clone(),
                    ..Default::default()
                }),
                "website" => c.websites.push(Value {
                    value: value.clone(),
                    ..Default::default()
                }),
                "title" => c.title = value.clone(),
                "note" => c.note = value.clone(),
                "birthday" => c.birthday = value.clone(),
                "phone" => c.phones.push(Value {
                    value: value.clone(),
                    ..Default::default()
                }),
                "email" => c.emails.push(Value {
                    value: value.clone(),
                    ..Default::default()
                }),
                "phone-label" | "email-label" => {
                    c.csv_extra.insert(headers[i].clone(), value.clone());
                }
                "groups" => c.groups.extend(value.split(';').map(str::to_owned)),
                "ignore" => {
                    warnings.push("explicitly-ignored-columns".into());
                }
                _ => {
                    c.csv_extra
                        .insert(format!("{} [column {}]", headers[i], i + 1), value.clone());
                    warnings.push("unmapped-columns-preserved".into());
                }
            }
        }
        // Keep every original column, including overlapping/manual mappings.
        for (i, value) in row.iter().enumerate() {
            c.csv_extra
                .entry(format!("{} [column {}]", headers[i], i + 1))
                .or_insert_with(|| value.clone());
        }
        for (field, values) in [("phone", &mut c.phones), ("email", &mut c.emails)] {
            for (index, value) in mapping
                .iter()
                .enumerate()
                .filter(|(_, m)| m.as_str() == field)
                .zip(values.iter_mut())
            {
                let i = index.0;
                let h = headers[i].to_lowercase();
                let type_header = h.replace("value", "type");
                let label = headers
                    .iter()
                    .position(|s| s.to_lowercase() == type_header && type_header != h)
                    .and_then(|n| row.get(n))
                    .cloned()
                    .unwrap_or_else(|| {
                        if h.contains("mobile") {
                            "CELL".into()
                        } else if h.contains("home") {
                            "HOME".into()
                        } else if h.contains("business") || h.contains("work") {
                            "WORK".into()
                        } else {
                            String::new()
                        }
                    });
                if !label.is_empty() {
                    value.params.push(format!("TYPE={label}"));
                    value.label = label;
                }
            }
        }
        if c.name.is_empty() {
            c.name = format!("{} {}", c.given, c.family).trim().into();
        }
        if c.name.is_empty() {
            c.name = c
                .phones
                .first()
                .map(|v| v.value.clone())
                .unwrap_or_else(|| "Contact".into());
        }
        contacts.push(c);
    }
    let book = Book { contacts };
    validate(&book)?;
    warnings.sort();
    warnings.dedup();
    Ok(ImportResult {
        book,
        warnings,
        encoding: String::new(),
    })
}
pub fn import(bytes: &[u8], options: &ImportOptions) -> Result<ImportResult> {
    let (text, encoding) = decode(bytes, &options.encoding)?;
    let format = if options.format.is_empty() || options.format == "auto" {
        if text.trim_start().starts_with(['{', '[']) {
            "json"
        } else if text.to_ascii_uppercase().contains("BEGIN:VCARD") {
            "vcf"
        } else {
            "csv"
        }
    } else {
        &options.format
    };
    let mut result = match format {
        "vcf" => vcard(&text)?,
        "csv" => csv(&text, options)?,
        "json" => {
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| invalid())?;
            if v.get("schema").and_then(|v| v.as_str()) != Some("yeskia.contacts/1") {
                return Err(invalid());
            }
            let book: Book = serde_json::from_value(v.get("book").cloned().ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            validate(&book)?;
            ImportResult {
                book,
                warnings: Vec::new(),
                encoding: String::new(),
            }
        }
        _ => return Err(invalid()),
    };
    result.encoding = encoding;
    if result.encoding == "windows-1251" {
        result.warnings.push("windows-1251-review-encoding".into());
    }
    Ok(result)
}
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut bytes = 0;
    for c in line.chars() {
        if bytes + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            bytes = 1;
        }
        out.push(c);
        bytes += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}
fn typed(name: &str, field: &Value, value: String) -> String {
    let mut params = field
        .params
        .iter()
        .filter(|p| {
            !p.to_ascii_uppercase().starts_with("ENCODING=")
                && !p.to_ascii_uppercase().starts_with("CHARSET=")
        })
        .cloned()
        .collect::<Vec<_>>();
    if !field.label.is_empty()
        && !params
            .iter()
            .any(|p| p.to_ascii_uppercase().starts_with("TYPE="))
    {
        params.push(format!(
            "TYPE={}",
            field.label.replace(['\r', '\n', ';', ':'], " ")
        ));
    }
    let value = if name == "TEL"
        && params.iter().any(|p| p.eq_ignore_ascii_case("VALUE=uri"))
        && !value.starts_with("tel:")
    {
        format!("tel:{value}")
    } else {
        value
    };
    fold(&format!(
        "{name}{}:{value}",
        params.iter().map(|p| format!(";{p}")).collect::<String>()
    ))
}
pub fn export(book: &Book, format: &str) -> Result<Vec<u8>> {
    validate(book)?;
    match format {
        "json" => serde_json::to_vec_pretty(
            &serde_json::json!({"schema":"yeskia.contacts/1","book":book}),
        )
        .map_err(|_| invalid()),
        "vcf" => {
            let mut out = String::new();
            for c in &book.contacts {
                out.push_str("BEGIN:VCARD\r\nVERSION:3.0\r\n");
                out.push_str(&fold(&format!("FN:{}", escape(&c.name))));
                out.push_str(&fold(&format!(
                    "N:{}",
                    [&c.family, &c.given, &c.middle, &c.prefix, &c.suffix]
                        .into_iter()
                        .map(|v| escape(v))
                        .collect::<Vec<_>>()
                        .join(";")
                )));
                for (name, fields) in [
                    ("TEL", &c.phones),
                    ("EMAIL", &c.emails),
                    ("URL", &c.websites),
                ] {
                    for field in fields {
                        out.push_str(&typed(name, field, escape(&field.value)));
                    }
                }
                for field in &c.addresses {
                    let components = if field.components.is_empty() {
                        vec![
                            String::new(),
                            String::new(),
                            field.value.clone(),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]
                    } else {
                        field.components.clone()
                    };
                    out.push_str(&typed(
                        "ADR",
                        field,
                        components
                            .iter()
                            .map(|v| escape(v))
                            .collect::<Vec<_>>()
                            .join(";"),
                    ));
                }
                if !c.organisation.is_empty() {
                    out.push_str(&fold(&format!(
                        "ORG:{}",
                        c.organisation
                            .iter()
                            .map(|v| escape(v))
                            .collect::<Vec<_>>()
                            .join(";")
                    )));
                }
                for (key, value) in [
                    ("TITLE", &c.title),
                    ("NOTE", &c.note),
                    ("BDAY", &c.birthday),
                ] {
                    if !value.is_empty() {
                        out.push_str(&fold(&format!("{key}:{}", escape(value))));
                    }
                }
                if !c.groups.is_empty() {
                    out.push_str(&fold(&format!(
                        "CATEGORIES:{}",
                        c.groups
                            .iter()
                            .map(|v| escape(v))
                            .collect::<Vec<_>>()
                            .join(",")
                    )));
                }
                for p in &c.unknown {
                    out.push_str(&fold(&format!("{}:{}", p.head, p.value)));
                }
                for (index, (key, value)) in c.csv_extra.iter().enumerate() {
                    out.push_str(&fold(&format!(
                        "X-YESKIA-CSV-{index}:{}",
                        escape(&format!("{key}\n{value}"))
                    )));
                }
                out.push_str("END:VCARD\r\n");
            }
            Ok(out.into_bytes())
        }
        "csv" => {
            let quote = |s: &str| {
                let escaped = s.replace('"', "\"\"");
                format!(
                    "\"{}{}\"",
                    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                        "'"
                    } else {
                        ""
                    },
                    escaped
                )
            };
            let mut out = String::from("\u{feff}Name,Phones,Emails,_YESKIA_CONTACT\r\n");
            for c in &book.contacts {
                out.push_str(
                    &[
                        quote(&c.name),
                        quote(
                            &c.phones
                                .iter()
                                .map(|v| v.value.clone())
                                .collect::<Vec<_>>()
                                .join("\n"),
                        ),
                        quote(
                            &c.emails
                                .iter()
                                .map(|v| v.value.clone())
                                .collect::<Vec<_>>()
                                .join("\n"),
                        ),
                        quote(&serde_json::to_string(c).map_err(|_| invalid())?),
                    ]
                    .join(","),
                );
                out.push_str("\r\n");
            }
            Ok(out.into_bytes())
        }
        _ => Err(invalid()),
    }
}

impl std::fmt::Debug for CsvPreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CsvPreview([private])")
    }
}
