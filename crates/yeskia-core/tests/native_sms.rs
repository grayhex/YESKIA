use yeskia_core::sms;
#[test]
fn decodes_ucs2_incoming() {
    // SMSC=none, deliver, +123, PID, UCS2, timestamp, two UCS2 characters.
    let pdu = [
        0, 0, 3, 0x91, 0x21, 0xf3, 0, 8, 0x52, 0x10, 0x20, 0x01, 0, 0, 0, 4, 0x04, 0x18, 0x04, 0x32,
    ];
    let row = sms::decode(&pdu).unwrap();
    assert_eq!(row["number"], "+123");
    assert_eq!(row["text"], "Ив");
    assert_eq!(row["direction"], "incoming");
}
#[test]
fn decodes_gsm7_submit() {
    let pdu = [
        0, 1, 0, 3, 0x91, 0x21, 0xf3, 0, 0, 5, 0xe8, 0x32, 0x9b, 0xfd, 0x06,
    ];
    assert_eq!(sms::decode(&pdu).unwrap()["text"], "hello");
}
#[test]
fn rejects_truncated_and_invalid_pdu() {
    for p in [&[][..], &[0, 0, 3, 0x91, 0x21][..]] {
        assert!(sms::decode(p).is_err());
    }
    assert!(sms::parse_list("+CMGL: 1,0,,12\r\n00\r\nOK\r\n").is_err());
}

#[test]
fn preserves_multipart_metadata_and_gsm_extension() {
    let pdu = [
        0, 0x40, 3, 0x91, 0x21, 0xf3, 0, 8, 0x52, 0x10, 0x20, 0x01, 0, 0, 0, 8, 5, 0, 3, 99, 2, 1,
        0x04, 0x18,
    ];
    let row = sms::decode(&pdu).unwrap();
    assert_eq!(row["part"], 1);
    assert_eq!(row["parts"], 2);
    assert_eq!(row["text"], "И");
    let euro = [0, 1, 0, 3, 0x91, 0x21, 0xf3, 0, 0, 2, 0x9b, 0x32];
    assert_eq!(sms::decode(&euro).unwrap()["text"], "€");
}
#[test]
fn requests_only_non_marking_pdu_read_and_honors_cancel() {
    use std::{io, sync::atomic::AtomicBool, time::Duration};
    use yeskia_core::at::SerialPort;
    struct Port {
        commands: Vec<String>,
        reply: Vec<u8>,
    }
    impl SerialPort for Port {
        fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
            let text = String::from_utf8_lossy(data).into_owned();
            self.reply = if text == "AT+CMGF?\r" {
                b"\r\n+CMGF:0\r\nOK\r\n".to_vec()
            } else if text == "AT+CMGL=4,1\r" {
                b"\r\nOK\r\n".to_vec()
            } else {
                panic!("unexpected modem write")
            };
            self.commands.push(text);
            Ok(())
        }
        fn read(&mut self, b: &mut [u8], _: Duration) -> io::Result<usize> {
            let n = self.reply.len().min(b.len());
            b[..n].copy_from_slice(&self.reply[..n]);
            self.reply.drain(..n);
            Ok(n)
        }
    }
    let mut p = Port {
        commands: Vec::new(),
        reply: Vec::new(),
    };
    assert_eq!(
        sms::read(&mut p, &AtomicBool::new(false)).unwrap(),
        serde_json::json!([])
    );
    assert_eq!(p.commands.len(), 2);
    let n = p.commands.len();
    assert!(matches!(
        sms::read(&mut p, &AtomicBool::new(true)),
        Err(yeskia_core::Error::Cancelled)
    ));
    assert_eq!(p.commands.len(), n);
}
