use nicecooling_protocol::ReportError;
use nicecooling_protocol::crc::crc16_usb;
use nicecooling_protocol::feed::{
    FEED_SIZE, FeedMessage, NO_VALUE, UNIT_LITERS_PER_HOUR, UNIT_SPEED_RPM,
};

/// Golden bytes: the message that `aqc_leakshield_send_report` of the out-of-tree driver builds
/// for 2000 rpm in channel 1 and 115.0 L/h (raw 1150) in channel 2.
/// Source: github.com/aleksamagicka/aquacomputer_d5next-hwmon, file `aquacomputer_d5next.c`,
/// commit 9ae7fd514d635f2d8bc5cdfefbbeecd4198641e9 (also the last commit of that file).
/// The driver starts from `leakshield_usb_report_template`, writes the speed at byte 1 with unit
/// 3 at byte 33, writes the flow at byte 3 with unit 12 at byte 34 and appends the CRC-16/USB.
const GOLDEN_2000_RPM_115_0_LPH: [u8; FEED_SIZE] = [
    0x04, 0x07, 0xD0, 0x04, 0x7E, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F,
    0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F, 0xFF, 0x7F,
    0xFF, 0x03, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x2E, 0x77,
];

fn message_2000_rpm_115_0_lph() -> FeedMessage {
    let mut message = FeedMessage::default();
    message.set(0, 2000, UNIT_SPEED_RPM);
    message.set(1, 1150, UNIT_LITERS_PER_HOUR);
    message
}

#[test]
fn feed_golden_2000rpm_115_0_lph() {
    assert_eq!(
        message_2000_rpm_115_0_lph().encode(),
        GOLDEN_2000_RPM_115_0_LPH
    );
    assert_eq!(
        FeedMessage::decode(&GOLDEN_2000_RPM_115_0_LPH),
        Ok(message_2000_rpm_115_0_lph())
    );
}

#[test]
fn feed_is_51_bytes_49_from_0x04_then_be_crc() {
    let bytes = message_2000_rpm_115_0_lph().encode();
    assert_eq!(bytes.len(), 51);
    assert_eq!(bytes[0], 0x04);
    let crc = crc16_usb(&bytes[..49]);
    assert_eq!(&bytes[49..], &crc.to_be_bytes());
    assert_eq!(crc, 0x2E77);
}

#[test]
fn empty_feed_has_no_value_in_every_slot() {
    let bytes = FeedMessage::default().encode();
    for slot in 0..16 {
        assert_eq!(&bytes[1 + 2 * slot..3 + 2 * slot], &NO_VALUE.to_be_bytes());
        assert_eq!(bytes[33 + slot], 0);
    }
}

#[test]
fn cleared_slot_goes_back_to_no_value_and_unit_0() {
    let mut message = message_2000_rpm_115_0_lph();
    message.clear(0);
    assert_eq!((message.value(0), message.unit(0)), (NO_VALUE, 0));
    assert_eq!(
        (message.value(1), message.unit(1)),
        (1150, UNIT_LITERS_PER_HOUR)
    );
}

#[test]
fn feed_decode_rejects_wrong_size_lead_byte_and_crc() {
    let good = GOLDEN_2000_RPM_115_0_LPH;
    assert_eq!(
        FeedMessage::decode(&good[..50]),
        Err(ReportError::WrongSize {
            expected: 51,
            actual: 50
        })
    );
    let mut bad = good;
    bad[0] = 0x02;
    assert!(matches!(
        FeedMessage::decode(&bad),
        Err(ReportError::WrongId { .. })
    ));
    let mut bad = good;
    bad[10] ^= 1;
    assert!(matches!(
        FeedMessage::decode(&bad),
        Err(ReportError::BadCrc { .. })
    ));
}
