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
