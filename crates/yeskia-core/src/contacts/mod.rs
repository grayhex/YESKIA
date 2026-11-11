//! Local address book. No unverified PCCS write commands are constructed here.
mod text;
use crate::{
    workbench::{Versioned, Workbench},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use text::{decode, export, import, preview_csv, CsvPreview, ImportOptions, ImportResult};
pub const MAX_CONTACTS: usize = 10_000;
pub const MAX_IMPORT: usize = 16 * 1024 * 1024;
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Value {
    pub value: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub head: String,
    pub value: String,
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub kind: String,
    pub native_id: Option<String>,
    pub fingerprint: Option<String>,
    pub original_id: Option<String>,
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub given: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub middle: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    #[serde(default)]
    pub phones: Vec<Value>,
    #[serde(default)]
    pub emails: Vec<Value>,
    #[serde(default)]
    pub addresses: Vec<Value>,
    #[serde(default)]
    pub organisation: Vec<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub birthday: String,
    #[serde(default)]
    pub websites: Vec<Value>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub unknown: Vec<Property>,
    #[serde(default)]
    pub csv_extra: BTreeMap<String, String>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub status: String,
}
impl std::fmt::Debug for Contact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Contact([private])")
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    pub contacts: Vec<Contact>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Duplicate {
    pub ids: Vec<String>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub kind: String,
    pub fields: Vec<String>,
}
fn bad() -> Error {
    Error::InvalidArgument("CONTACT_DOCUMENT_INVALID".into())
}
pub fn validate(book: &Book) -> Result<()> {
    if book.contacts.len() > MAX_CONTACTS {
        return Err(Error::LimitExceeded("CONTACT_BOOK_LIMIT".into()));
    }
    let mut ids = BTreeSet::new();
    for c in &book.contacts {
        if c.id.len() != 32
            || !c.id.bytes().all(|b| b.is_ascii_hexdigit())
            || !ids.insert(&c.id)
            || c.name.len() > 2000
            || c.note.len() > 128 * 1024
            || c.unknown.len() > 512
            || c.csv_extra.len() > 512
        {
            return Err(bad());
        }
        for fields in [&c.phones, &c.emails, &c.addresses, &c.websites] {
            if fields.len() > 100
                || fields.iter().any(|v| {
                    v.params.len() > 50
                        || v.components.len() > 20
                        || v.components.iter().any(|s| s.len() > 64 * 1024)
                        || v.value.len() > 64 * 1024
                        || v.label.len() > 300
                        || v.params
                            .iter()
                            .any(|p| p.len() > 1000 || p.contains(['\r', '\n']))
                })
            {
                return Err(bad());
            }
        }
        if c.unknown.iter().any(|p| {
            p.head.len() > 2000
                || p.head.contains(['\r', '\n'])
                || p.value.len() > 1024 * 1024
                || p.value.contains(['\r', '\n'])
        }) {
            return Err(bad());
        }
    }
    if serde_json::to_vec(book).map_err(|_| bad())?.len() > crate::workbench::MAX_DOCUMENT {
        return Err(Error::LimitExceeded("CONTACT_BOOK_LIMIT".into()));
    }
    Ok(())
}
pub fn load(store: &Workbench) -> Result<Versioned<Book>> {
    store.load("contacts")
}
pub fn save(store: &Workbench, revision: i64, book: Book) -> Result<Versioned<Book>> {
    validate(&book)?;
    store.save("contacts", revision, book)
}
pub fn phone_key(raw: &str) -> String {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 11 && digits.starts_with('8') {
        format!("7{}", &digits[1..])
    } else {
        digits
    }
}
pub fn duplicates(book: &Book) -> Vec<Duplicate> {
    let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, c) in book.contacts.iter().enumerate() {
        for p in &c.phones {
            let k = phone_key(&p.value);
            if k.len() >= 7 {
                index.entry(format!("phone:{k}")).or_default().push(i);
            }
        }
        for e in &c.emails {
            let k = e.value.trim().to_lowercase();
            if !k.is_empty() {
                index.entry(format!("email:{k}")).or_default().push(i);
            }
        }
        let n = c
            .name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if !n.is_empty() {
            index.entry(format!("name:{n}")).or_default().push(i);
        }
    }
    let mut groups: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
    for (key, indices) in index {
        let ids: Vec<String> = indices
            .into_iter()
            .map(|i| book.contacts[i].id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if ids.len() > 1 {
            groups
                .entry(ids)
                .or_default()
                .insert(key.split(':').next().unwrap().into());
        }
    }
    groups
        .into_iter()
        .map(|(ids, reasons)| Duplicate {
            ids,
            reasons: reasons.into_iter().collect(),
        })
        .collect()
}
pub fn diff(before: &Book, after: &Book) -> Vec<Change> {
    let mut changes = Vec::new();
    for next in &after.contacts {
        match before.contacts.iter().find(|c| c.id == next.id) {
            None => changes.push(Change {
                id: next.id.clone(),
                kind: "add".into(),
                fields: vec!["record".into()],
            }),
            Some(old) if old != next => {
                let a = serde_json::to_value(old).unwrap_or_default();
                let b = serde_json::to_value(next).unwrap_or_default();
                let fields = b
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .filter(|(k, v)| a.get(*k) != Some(*v))
                            .map(|(k, _)| k.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                changes.push(Change {
                    id: next.id.clone(),
                    kind: "update".into(),
                    fields,
                });
            }
            _ => {}
        }
    }
    for old in &before.contacts {
        if !after.contacts.iter().any(|c| c.id == old.id) {
            changes.push(Change {
                id: old.id.clone(),
                kind: "delete".into(),
                fields: vec!["record".into()],
            });
        }
    }
    changes
}
fn union<T: PartialEq + Clone>(to: &mut Vec<T>, from: &[T]) {
    for v in from {
        if !to.contains(v) {
            to.push(v.clone());
        }
    }
}
pub fn merge_preview(book: &Book, ids: &[String], primary: &str) -> Result<Contact> {
    if ids.len() < 2 || ids.len() > 200 || !ids.iter().any(|id| id == primary) {
        return Err(bad());
    }
    let mut out = book
        .contacts
        .iter()
        .find(|c| c.id == primary)
        .cloned()
        .ok_or_else(bad)?;
    for id in ids.iter().filter(|id| id.as_str() != primary) {
        let other = book.contacts.iter().find(|c| &c.id == id).ok_or_else(bad)?;
        union(&mut out.phones, &other.phones);
        union(&mut out.emails, &other.emails);
        union(&mut out.addresses, &other.addresses);
        union(&mut out.websites, &other.websites);
        union(&mut out.groups, &other.groups);
        union(&mut out.unknown, &other.unknown);
        let a = serde_json::to_value(&out).map_err(|_| bad())?;
        let b = serde_json::to_value(other).map_err(|_| bad())?;
        for field in [
            "name",
            "given",
            "family",
            "middle",
            "prefix",
            "suffix",
            "organisation",
            "title",
            "note",
            "birthday",
            "source",
        ] {
            if a.get(field) != b.get(field) {
                out.unknown.push(Property {
                    head: format!("X-YESKIA-MERGED-{}", field.to_ascii_uppercase()),
                    value: text::escape(
                        &serde_json::to_string(b.get(field).unwrap_or(&serde_json::Value::Null))
                            .map_err(|_| bad())?,
                    ),
                });
            }
        }
        for (k, v) in &other.csv_extra {
            if out.csv_extra.get(k).is_some_and(|old| old != v) {
                out.csv_extra
                    .insert(format!("{k} ({})", other.id), v.clone());
            } else {
                out.csv_extra.insert(k.clone(), v.clone());
            }
        }
    }
    out.status = "local".into();
    validate(&Book {
        contacts: vec![out.clone()],
    })?;
    Ok(out)
}
pub fn native_snapshot(rows: &[serde_json::Value], fingerprint: &str, kind: &str) -> Result<Book> {
    if rows.len() > MAX_CONTACTS {
        return Err(Error::LimitExceeded("CONTACT_BOOK_LIMIT".into()));
    }
    let mut contacts = Vec::new();
    for row in rows {
        let text = |key: &str| {
            row.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let values = |key: &str| {
            row.get(key)
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(|v| Value {
                            value: v.into(),
                            ..Default::default()
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        contacts.push(Contact {
            id: crate::fsutil::sha256_hex(
                format!(
                    "{}|{}",
                    fingerprint,
                    row.get("native_id")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| contacts.len().to_string())
                )
                .as_bytes(),
            )[..32]
                .into(),
            name: text("name"),
            given: text("given"),
            family: text("family"),
            phones: values("phones"),
            emails: values("emails"),
            source: Source {
                kind: kind.into(),
                native_id: Some(text("native_id")),
                fingerprint: Some(fingerprint.into()),
                original_id: None,
            },
            unknown: vec![Property {
                head: "X-YESKIA-NATIVE-RECORD".into(),
                value: text::escape(&serde_json::to_string(row).map_err(|_| bad())?),
            }],
            status: "phone-snapshot".into(),
            ..Default::default()
        });
    }
    let book = Book { contacts };
    validate(&book)?;
    Ok(book)
}
/// Real Nokia writes remain unavailable until a provider has independent firmware evidence.
pub fn writable_status() -> &'static str {
    "unsupported-no-verified-pim-write-provider"
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContactValue([private])")
    }
}
impl std::fmt::Debug for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContactProperty([private])")
    }
}
impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContactSource([private])")
    }
}
