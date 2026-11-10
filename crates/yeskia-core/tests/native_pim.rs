use std::time::Duration;
use yeskia_core::{
    pccs::{self, Channel, Item, Session},
    pim_text, Error, Result,
};
struct Replay {
    transcript: Vec<serde_json::Value>,
    pos: usize,
}
fn hex(v: &str) -> Vec<u8> {
    v.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
impl Channel for Replay {
    fn exchange(&mut self, p: &[u8], _: Duration) -> Result<Vec<u8>> {
        let row = self
            .transcript
            .get(self.pos)
            .ok_or_else(|| Error::Transport("unexpected packet".into()))?;
        if p != hex(row["request"].as_str().unwrap()) {
            return Err(Error::Transport(format!("packet mismatch at {}", self.pos)));
        }
        self.pos += 1;
        Ok(hex(row["response"].as_str().unwrap()))
    }
}
fn replay(raw: &str, domain: &str) -> serde_json::Value {
    let mut channel = Replay {
        transcript: serde_json::from_str(raw).unwrap(),
        pos: 0,
    };
    let rows;
    {
        let mut session = Session::connect(&mut channel).unwrap();
        let items = session.read_store(domain).unwrap();
        rows = pim_text::rows(&items, domain).unwrap();
    }
    assert_eq!(channel.pos, channel.transcript.len());
    rows
}
#[test]
fn matches_source_packets_and_contact_format_exactly() {
    let rows = replay(include_str!("fixtures/pccs/contacts-0.json"), "contacts");
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["phones"][0], "123");
}
#[test]
fn reassembles_obex_continuation_without_repeating_command() {
    let rows = replay(include_str!("fixtures/pccs/contacts-17.json"), "contacts");
    assert_eq!(rows[1]["name"], "Test");
}
#[test]
fn calendar_and_tasks_use_the_phone_format() {
    let raw = include_str!("fixtures/pccs/calendar-0.json");
    assert_eq!(replay(raw, "calendar")[0]["start"], "2025-01-02T10:00:00Z");
    assert_eq!(replay(raw, "tasks")[0]["summary"], "Test task");
}
#[test]
fn rejects_truncated_headers_records_duplicate_ids_and_excessive_counts() {
    for raw in [&[0xa0, 0, 4][..], &[0xa0, 0, 6, 0x49, 0, 1][..]] {
        assert!(pccs::response(raw, 0).is_err());
    }
    assert!(pccs::parse_ids(&[2, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]).is_err());
    assert!(pccs::parse_items(&[255, 255, 255, 255]).is_err());
    assert!(pccs::parse_items(&[1, 0, 0, 0]).is_err());
}
#[test]
fn preserves_quoted_printable_legacy_charset_and_folding() {
    let item=Item{id:7,mime:"text/x-vcard".into(),payload:b"BEGIN:VCARD\r\nFN;CHARSET=WINDOWS-1251;ENCODING=QUOTED-PRINTABLE:=C8=E2=\r\n=E0=ED\r\nTEL:12\r\n 3\r\nEND:VCARD\r\n".to_vec()};
    let rows = pim_text::rows(&[item], "contacts").unwrap();
    assert_eq!(rows[0]["name"], "Иван");
    assert_eq!(rows[0]["phones"][0], "123");
}
#[test]
fn alarm_fields_do_not_overwrite_event_and_truncation_is_rejected() {
    let payload=b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Meeting\nBEGIN:VALARM\nSUMMARY:Alarm\nEND:VALARM\nEND:VEVENT\nEND:VCALENDAR".to_vec();
    let item = Item {
        id: 1,
        mime: "text/x-vcalendar".into(),
        payload,
    };
    assert_eq!(
        pim_text::rows(&[item], "calendar").unwrap()[0]["summary"],
        "Meeting"
    );
    let bad = Item {
        id: 1,
        mime: "text/x-vcard".into(),
        payload: b"BEGIN:VCARD\nFN:Partial".to_vec(),
    };
    assert!(pim_text::rows(&[bad], "contacts").is_err());
}
#[test]
fn rejected_store_is_closed() {
    struct Refused {
        step: usize,
    }
    impl Channel for Refused {
        fn exchange(&mut self, p: &[u8], _: Duration) -> Result<Vec<u8>> {
            self.step += 1;
            match self.step {
                1 => {
                    let mut b = vec![0x10, 0, 255, 255];
                    b.extend(pccs::header(0x4a, pccs::TARGET)?);
                    b.extend([0xcb, 0, 0, 0, 7]);
                    pccs::packet(0xa0, &b)
                }
                2 => pccs::packet(0xa0, &pccs::header(0x49, &[0, 0, 0, 0])?),
                3 => {
                    let (_, h) = pccs::response(p, 0)?;
                    assert!(h
                        .iter()
                        .any(|(id, v)| *id == 1 && *v == pccs::command_name(b"SYNC: CloseStore")));
                    pccs::packet(0xa0, &[])
                }
                _ => pccs::packet(0xa0, &[]),
            }
        }
    }
    let mut c = Refused { step: 0 };
    {
        let mut s = Session::connect(&mut c).unwrap();
        assert!(s.read_store("contacts").is_err());
    }
    assert_eq!(c.step, 4);
}
#[test]
fn uncertain_certificate_delivery_is_not_retried() {
    struct Lost {
        puts: usize,
    }
    impl Channel for Lost {
        fn exchange(&mut self, p: &[u8], _: Duration) -> Result<Vec<u8>> {
            if p[0] == 0x80 {
                return pccs::packet(0xa0, &[0x10, 0, 0x10, 0]);
            }
            if p[0] == 0x81 {
                return pccs::packet(0xa0, &[]);
            }
            self.puts += 1;
            Err(Error::Timeout)
        }
    }
    let mut channel = Lost { puts: 0 };
    let der = yeskia_core::pkg::cert::load_public_der(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pkg/standalone-test.pem"),
    )
    .unwrap();
    assert!(yeskia_core::opp::push_certificate(&mut channel, &der)
        .unwrap_err()
        .to_string()
        .contains("CERT_TRANSFER_UNCERTAIN"));
    assert_eq!(channel.puts, 1);
}
