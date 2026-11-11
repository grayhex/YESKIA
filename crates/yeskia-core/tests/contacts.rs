use yeskia_core::{
    contacts::{self, Book, Contact, ImportOptions, Value},
    workbench::Workbench,
};
fn options(format: &str) -> ImportOptions {
    ImportOptions {
        format: format.into(),
        ..Default::default()
    }
}
fn vcf() -> Vec<u8> {
    b"BEGIN:VCARD\r\nVERSION:2.1\r\nN:Ivanov;Ivan;Middle;Dr;Jr\r\nFN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=D0=98=D0=B2=D0=B0=D0=BD\r\nTEL;TYPE=CELL:+7 (999) 111-22-33\r\nTEL;HOME:8 999 1112233\r\nEMAIL;TYPE=WORK:test@example.invalid\r\nADR;TYPE=HOME:;;Street\\; 1;City;;123;Country\r\nORG:Company;Division\r\nNOTE:one\\nline\\;semi\\,comma\r\nCATEGORIES:Friends,Work\r\nX-UNKNOWN;LANGUAGE=ru:opaque\\;value\r\nPHOTO;ENCODING=BASE64:YWJj\r\nEND:VCARD\r\n".to_vec()
}
#[test]
fn vcard_versions_multivalue_qp_unknown_photo_and_structured_address_survive() {
    for version in ["2.1", "3.0", "4.0"] {
        let s = String::from_utf8(vcf()).unwrap().replace("2.1", version);
        let imported = contacts::import(s.as_bytes(), &options("vcf")).unwrap();
        let c = &imported.book.contacts[0];
        assert_eq!(c.name, "Иван");
        assert_eq!(c.phones.len(), 2);
        assert_eq!(c.addresses[0].components[2], "Street; 1");
        assert_eq!(c.organisation, ["Company", "Division"]);
        assert_eq!(c.note, "one\nline;semi,comma");
        assert!(c.unknown.iter().any(|p| p.head.starts_with("PHOTO")));
        let bytes = contacts::export(&imported.book, "vcf").unwrap();
        let back = contacts::import(&bytes, &options("vcf")).unwrap();
        assert_eq!(back.book.contacts[0].phones, c.phones);
        assert_eq!(back.book.contacts[0].unknown, c.unknown);
        assert_eq!(back.book.contacts[0].addresses, c.addresses);
    }
}
#[test]
fn json_and_csv_are_lossless_local_roundtrips() {
    let b = contacts::import(&vcf(), &options("vcf")).unwrap().book;
    for format in ["json", "csv"] {
        let raw = contacts::export(&b, format).unwrap();
        let back = contacts::import(&raw, &options(format)).unwrap().book;
        assert_eq!(b, back);
    }
}
#[test]
fn csv_quoted_newlines_utf16_cp1251_and_manual_mapping() {
    let s = "Имя;Телефон;Notes;Custom\r\n\"Иван; И.\";+7 9991112233;\"two\nlines\";value\r\n";
    let mut utf16 = vec![0xFF, 0xFE];
    for n in s.encode_utf16() {
        utf16.extend(n.to_le_bytes());
    }
    let preview = contacts::preview_csv(&utf16, &ImportOptions::default()).unwrap();
    assert_eq!(preview.rows, 1);
    assert_eq!(preview.delimiter, ";");
    let imported = contacts::import(&utf16, &options("csv")).unwrap();
    let c = &imported.book.contacts[0];
    assert_eq!(c.name, "Иван; И.");
    assert_eq!(c.note, "two\nlines");
    assert!(!c.csv_extra.is_empty());
    let (data, _, _) = encoding_rs::WINDOWS_1251.encode(s);
    let cp = contacts::import(&data, &options("csv")).unwrap();
    assert_eq!(cp.book.contacts[0].name, c.name);
    assert!(cp.warnings.contains(&"windows-1251-review-encoding".into()));
    let mut opts = options("csv");
    opts.mapping.insert(0, "company".into());
    let custom = contacts::import(b"Unknown,Phone\nACME,1234567\n", &opts).unwrap();
    assert_eq!(custom.book.contacts[0].organisation, ["ACME"]);
}
#[test]
fn folded_utf8_partial_cards_and_invalid_data_are_explicit() {
    let long = "Я".repeat(100);
    let s = format!("BEGIN:VCARD\nVERSION:4.0\nFN:{long}\nX-TEXT:some\n folded\n");
    let result = contacts::import(s.as_bytes(), &options("vcf")).unwrap();
    assert!(result.warnings.contains(&"partial-card-preserved".into()));
    let output = contacts::export(&result.book, "vcf").unwrap();
    assert!(output.split(|b| *b == b'\n').all(|l| l.len() <= 76));
    assert_eq!(
        contacts::import(&output, &options("vcf"))
            .unwrap()
            .book
            .contacts[0]
            .name,
        long
    );
    assert!(contacts::import(b"Name,Phone\n\"unterminated", &options("csv")).is_err());
    assert!(contacts::import(b"{\"schema\":\"future\"}", &options("json")).is_err());
}
#[test]
fn duplicate_proposals_are_manual_and_merge_keeps_every_value() {
    let mut a = contacts::import(&vcf(), &options("vcf"))
        .unwrap()
        .book
        .contacts
        .remove(0);
    let mut b = a.clone();
    b.id = "b".repeat(32);
    b.name = "Different name".into();
    b.phones = vec![Value {
        value: "8 999 1112233".into(),
        ..Default::default()
    }];
    b.emails.push(Value {
        value: "second@example.invalid".into(),
        ..Default::default()
    });
    a.id = "a".repeat(32);
    let book = Book {
        contacts: vec![a.clone(), b.clone()],
    };
    assert_eq!(
        contacts::phone_key("+7 (999) 111-22-33"),
        contacts::phone_key("8 999 1112233")
    );
    assert!(!contacts::duplicates(&book).is_empty());
    let ids = vec![a.id.clone(), b.id.clone()];
    let merged = contacts::merge_preview(&book, &ids, &a.id).unwrap();
    assert!(merged
        .emails
        .iter()
        .any(|v| v.value == "second@example.invalid"));
    assert!(merged
        .unknown
        .iter()
        .any(|p| p.head == "X-YESKIA-MERGED-NAME"));
    assert_eq!(book.contacts.len(), 2, "preview does not mutate the book");
    assert!(contacts::writable_status().starts_with("unsupported"));
}
#[test]
fn private_sqlite_revision_conflicts_undo_and_restart() {
    let root = std::env::temp_dir().join(format!(
        "yeskia-contacts-{}",
        yeskia_core::workbench::local_id().unwrap()
    ));
    let store = Workbench::open(&root).unwrap();
    let b = contacts::import(&vcf(), &options("vcf")).unwrap().book;
    let first = contacts::save(&store, 0, b.clone()).unwrap();
    assert!(contacts::save(&store, 0, Book::default()).is_err());
    contacts::save(&store, first.revision, Book::default()).unwrap();
    let back: yeskia_core::workbench::Versioned<Book> = store.undo("contacts", 2).unwrap();
    assert_eq!(back.value, b);
    let reopened = Workbench::open(&root).unwrap();
    assert_eq!(contacts::load(&reopened).unwrap().value, b);
    assert!(!format!("{:?}", b.contacts[0]).contains("1112233"));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_second_phone_is_a_different_snapshot_and_never_a_writable_route() {
    let rows = vec![
        serde_json::json!({"native_id":"contacts:17","name":"Synthetic","phones":["+79990000000"]}),
    ];
    let a = contacts::native_snapshot(&rows, "aaaaaaaaaaaaaaaa", "phone").unwrap();
    let b = contacts::native_snapshot(&rows, "bbbbbbbbbbbbbbbb", "phone").unwrap();
    assert_ne!(
        a.contacts[0].source.fingerprint,
        b.contacts[0].source.fingerprint
    );
    let edit = Book {
        contacts: vec![Contact {
            name: "Updated".into(),
            ..a.contacts[0].clone()
        }],
    };
    let diff = contacts::diff(&a, &edit);
    assert_eq!(diff[0].fields, ["name"]);
    assert_eq!(a.contacts[0].name, "Synthetic");
}
#[test]
fn google_labels_unknown_columns_formula_text_and_first_save_undo_are_lossless() {
    let raw=b"Name,Phone 1 - Type,Phone 1 - Value,E-mail 1 - Type,E-mail 1 - Value,Custom\n=1+2,Mobile,+79990000000,Work,a@example.invalid,opaque\n";
    let imported = contacts::import(raw, &options("csv")).unwrap();
    let c = &imported.book.contacts[0];
    assert_eq!(c.phones[0].label, "Mobile");
    assert_eq!(c.emails[0].label, "Work");
    assert!(c.csv_extra.values().any(|v| v == "opaque"));
    let export = contacts::export(&imported.book, "csv").unwrap();
    assert!(String::from_utf8_lossy(&export).contains("'=1+2"));
    assert_eq!(
        contacts::import(&export, &options("csv")).unwrap().book,
        imported.book
    );
    let root = std::env::temp_dir().join(format!(
        "yeskia-undo-{}",
        yeskia_core::workbench::local_id().unwrap()
    ));
    let store = Workbench::open(&root).unwrap();
    let saved = contacts::save(&store, 0, imported.book).unwrap();
    let undone = store.undo::<Book>("contacts", saved.revision).unwrap();
    assert!(undone.value.contacts.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
