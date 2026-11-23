mod common;

use common::{Fixtures, fw1019, fw1022};
use nicecooling_protocol::ReportError;
use nicecooling_protocol::crc::crc16_usb;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::report5::Report5;
use nicecooling_protocol::report8::Report8;

type Decode = fn(&[u8]) -> Result<(), ReportError>;

fn roundtrip(f: &Fixtures, name: &str) {
    assert_eq!(
        Report1::decode(f.report1).unwrap().encode(),
        f.report1,
        "{name} report 1"
    );
    assert_eq!(
        Report3::decode(f.report3).unwrap().encode(),
        f.report3,
        "{name} report 3"
    );
    assert_eq!(
        Report5::decode(f.report5).unwrap().encode(),
        f.report5,
        "{name} report 5"
    );
    assert_eq!(
        Report8::decode(f.report8).unwrap().encode(),
        f.report8,
        "{name} report 8"
    );
}

#[test]
fn fixtures_decode_and_reencode_to_same_bytes() {
    roundtrip(&fw1019(), "fw1019");
    roundtrip(&fw1022(), "fw1022");
}

#[test]
fn crc_report3_fw1019_is_0xecf7() {
    let f = fw1019();
    assert_eq!(crc16_usb(&f.report3[1..748]), 0xECF7);
    assert_eq!(Report3::decode(f.report3).unwrap().crc(), 0xECF7);
    assert_eq!(u16::from_be_bytes([f.report3[748], f.report3[749]]), 0xECF7);
}

#[test]
fn crc_report8_fw1019_is_0x7e32() {
    let f = fw1019();
    assert_eq!(crc16_usb(&f.report8[1..3089]), 0x7E32);
    assert_eq!(
        u16::from_be_bytes([f.report8[3089], f.report8[3090]]),
        0x7E32
    );
    Report8::decode(f.report8).unwrap();
}

#[test]
fn wrong_size_report_rejected() {
    let f = fw1022();
    let cases: [(&[u8], usize, Decode); 4] = [
        (f.report1, 837, |b| Report1::decode(b).map(drop)),
        (f.report3, 750, |b| Report3::decode(b).map(drop)),
        (f.report5, 1028, |b| Report5::decode(b).map(drop)),
        (f.report8, 3091, |b| Report8::decode(b).map(drop)),
    ];
    for (good, size, decode) in cases {
        assert_eq!(decode(good), Ok(()));
        let mut long = good.to_vec();
        long.push(0);
        for bad in [&good[..size - 1], &long[..], &[][..]] {
            assert_eq!(
                decode(bad),
                Err(ReportError::WrongSize {
                    expected: size,
                    actual: bad.len()
                })
            );
        }
    }
}

#[test]
fn wrong_report_id_rejected() {
    let f = fw1022();
    let mut b = f.report3.to_vec();
    b[0] = 1;
    assert_eq!(
        Report3::decode(&b),
        Err(ReportError::WrongId {
            expected: 3,
            actual: 1
        })
    );
    let mut b = f.report1.to_vec();
    b[0] = 3;
    assert_eq!(
        Report1::decode(&b),
        Err(ReportError::WrongId {
            expected: 1,
            actual: 3
        })
    );
}

#[test]
fn bad_crc_rejected_for_reports_3_and_8() {
    let f = fw1022();
    let mut b = f.report3.to_vec();
    b[100] ^= 1;
    assert!(matches!(
        Report3::decode(&b),
        Err(ReportError::BadCrc { stored: 0xECF7, .. })
    ));
    let mut b = f.report8.to_vec();
    b[2000] ^= 1;
    assert!(matches!(
        Report8::decode(&b),
        Err(ReportError::BadCrc { stored: 0x7E32, .. })
    ));
}

#[test]
fn report3_set_bytes_encodes_a_valid_crc() {
    let f = fw1022();
    let mut report = Report3::decode(f.report3).unwrap();
    report.set_bytes(15, &[0x13, 0x88]);
    let encoded = report.encode();
    assert_eq!(&encoded[15..17], &[0x13, 0x88]);
    assert_ne!(&encoded[748..750], &f.report3[748..750]);
    assert_eq!(Report3::decode(&encoded).unwrap(), report);
}

#[test]
fn report1_fields_sit_at_the_documented_offsets() {
    let f = fw1022();
    let r = Report1::decode(f.report1).unwrap();
    let be16 = |o: usize| u16::from_be_bytes([f.report1[o], f.report1[o + 1]]);
    let be32 = |o: usize| u32::from_be_bytes(f.report1[o..o + 4].try_into().unwrap());
    assert_eq!(r.structure_number, be16(1));
    assert_eq!((r.structure_number, r.firmware_version), (100, 1022));
    assert_eq!(r.serial_text(), "20417-31072");
    assert_eq!(r.device_clock, be32(35));
    assert_eq!(r.sensors[19].average as u16, be16(59 + 190 + 8));
    assert_eq!(r.pump_speed_reading as u16, be16(261));
    assert_eq!(r.pressure as u16, be16(285));
    assert_eq!(r.mode as u16, be16(297));
    assert_eq!(r.fill_level as u16, be16(315));
    assert_eq!(r.pump_runtime, be32(321));
    assert_eq!(r.refill_history[6].volume as u32, be32(353 + 6 * 16 + 12));
    assert_eq!(r.event_list[14].value_2 as u16, be16(465 + 14 * 20 + 18));
    assert_eq!(r.alarm_bits, be32(765));
    assert_eq!(r.debug[31] as u16, be16(835));
}

#[test]
fn fixtures_report_their_firmware() {
    assert_eq!(
        Report1::decode(fw1019().report1).unwrap().firmware_version,
        1019
    );
    assert_eq!(
        Report1::decode(fw1022().report1).unwrap().firmware_version,
        1022
    );
}

#[test]
fn report1_active_profile_reads_offset_50() {
    let mut bytes = fw1022().report1.to_vec();
    // `feature_bits` sits at offset 19, `active_profile` at offset 50.
    bytes[19] = 1;
    bytes[50] = 3;
    let report = Report1::decode(&bytes).unwrap();
    assert_eq!(report.active_profile, 3);
    assert_eq!(report.feature_bits, 1);
}
