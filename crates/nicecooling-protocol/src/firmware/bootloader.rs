//! The bootloader test, its report 1 and the restart commands (`docs/re/firmware-update.md`).

use crate::registry::Registry;
use crate::report1::REPORT1_ID;
use crate::report2::{REPORT2_SIZE, Report2};

/// The identity fields end at offset 14 (the firmware version). The display language is byte 34.
const IDENTITY_LEN: usize = 35;
/// The firmware version ends at offset 14, so a report that holds it has 15 bytes.
const FIRMWARE_END: usize = 15;

/// The bootloader reports a firmware version from 1 to 99. The application reports 100 or more.
pub fn is_bootloader_firmware(firmware: u16) -> bool {
    (1..=99).contains(&firmware)
}

fn restart(parameter: u32) -> [u8; REPORT2_SIZE] {
    let code = Registry::leakshield()
        .command("restart")
        .expect("the registry has the restart command")
        .code;
    Report2::new(code, parameter).encode()
}

/// The report 2 that restarts the device into the bootloader (parameter 1).
pub fn restart_into_bootloader() -> [u8; REPORT2_SIZE] {
    restart(1)
}

/// The report 2 that restarts the device into the application (parameter 0).
pub fn restart_plain() -> [u8; REPORT2_SIZE] {
    restart(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    German,
}

impl Language {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::English),
            1 => Some(Self::German),
            _ => None,
        }
    }
}

/// The fields of report 1 that name a device. A field that the report does not hold, or holds as
/// 0, is absent. In the bootloader the firmware field holds 1 to 99, which is no firmware version:
/// `firmware` and `language` are absent then (design D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub serial: Option<[u16; 2]>,
    pub hardware: Option<u16>,
    pub bootloader: Option<u16>,
    pub firmware: Option<u16>,
    pub language: Option<Language>,
    in_bootloader: bool,
}

impl Identity {
    /// Reads report 1 of any length, or returns `None` when the first byte is not the report ID.
    /// aquasuite pads a short report with zeros before it reads the fields, and so does this
    /// function. The bootloader sends a short one. A field that lies beyond the end is absent.
    pub fn from_report1(bytes: &[u8]) -> Option<Self> {
        if bytes.first() != Some(&REPORT1_ID) {
            return None;
        }
        let mut padded = [0u8; IDENTITY_LEN];
        let n = bytes.len().min(IDENTITY_LEN);
        padded[..n].copy_from_slice(&bytes[..n]);
        let word = |at: usize| u16::from_be_bytes([padded[at], padded[at + 1]]);
        let nonzero = |value: u16| (value != 0).then_some(value);
        let serial = [word(3), word(5)];
        let firmware = (bytes.len() >= FIRMWARE_END).then(|| word(13));
        let in_bootloader = firmware.is_some_and(is_bootloader_firmware);
        let application = firmware.filter(|_| !in_bootloader);
        Some(Self {
            serial: (serial != [0, 0]).then_some(serial),
            hardware: nonzero(word(7)),
            bootloader: nonzero(word(11)),
            firmware: application,
            language: application
                .filter(|_| bytes.len() >= IDENTITY_LEN)
                .and_then(|_| Language::from_byte(padded[34])),
            in_bootloader,
        })
    }

    pub fn is_bootloader(&self) -> bool {
        self.in_bootloader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report1::Report1;

    const FW1022: &[u8] =
        include_bytes!("../../../../tests/fixtures/leakshield/fw1022/report1.bin");

    /// The bootloader report of the reference device: a short report 1 with byte 2 = 1 and the
    /// firmware 1 (`docs/hardware-checks/firmware-update.md`).
    fn short_report() -> Vec<u8> {
        let mut report = vec![0; 64];
        report[0] = 1;
        report[2] = 1;
        report[14] = 1;
        report
    }

    #[test]
    fn restart_into_bootloader_bytes() {
        let expected = [0x02, 0, 0, 0, 1, 0, 0, 0, 1, 0xF4, 0x43];
        assert_eq!(restart_into_bootloader(), expected);
    }

    #[test]
    fn restart_plain_bytes() {
        let expected = [0x02, 0, 0, 0, 1, 0, 0, 0, 0, 0x34, 0x82];
        assert_eq!(restart_plain(), expected);
    }

    fn ident(bytes: &[u8]) -> Identity {
        Identity::from_report1(bytes).expect("a report 1")
    }

    #[test]
    fn firmware_1_to_99_is_bootloader() {
        for firmware in [1, 50, 99] {
            assert!(is_bootloader_firmware(firmware), "{firmware}");
        }
        assert!(!is_bootloader_firmware(0));
        assert!(!is_bootloader_firmware(100));
        assert!(ident(&short_report()).is_bootloader());
    }

    #[test]
    fn firmware_1022_is_not() {
        assert!(!ident(FW1022).is_bootloader());
        assert_eq!(ident(FW1022).firmware, Some(1022));
    }

    #[test]
    fn short_report_padded() {
        let identity = ident(&short_report());
        assert!(identity.is_bootloader());
        assert_eq!(identity.firmware, None);
        assert_eq!(identity.hardware, None);
        assert_eq!(identity.bootloader, None);
        assert_eq!(identity.serial, None);
        assert_eq!(identity.language, None);
        let mut with_ids = short_report();
        with_ids[7..9].copy_from_slice(&1000u16.to_be_bytes());
        with_ids[11..13].copy_from_slice(&100u16.to_be_bytes());
        assert_eq!(ident(&with_ids).hardware, Some(1000));
        assert_eq!(ident(&with_ids).bootloader, Some(100));
        let shorter = ident(&short_report()[..9]);
        assert_eq!(shorter.firmware, None);
        assert!(!shorter.is_bootloader());
    }

    #[test]
    fn report_without_id_1_gives_no_identity() {
        assert_eq!(Identity::from_report1(&[]), None);
        let mut report = FW1022.to_vec();
        report[0] = 3;
        assert_eq!(Identity::from_report1(&report), None);
        report[0] = 0;
        assert_eq!(Identity::from_report1(&report), None);
    }

    #[test]
    fn report_shorter_than_35_bytes_has_no_language() {
        let mut report = FW1022[..20].to_vec();
        assert_eq!(ident(&report).firmware, Some(1022));
        assert_eq!(ident(&report).language, None);
        report.resize(35, 0);
        assert_eq!(ident(&report).language, Some(Language::English));
        assert_eq!(ident(&FW1022[..34]).language, None);
    }

    #[test]
    fn bootloader_report_has_no_firmware_and_no_language() {
        let mut long = FW1022.to_vec();
        long[13..15].copy_from_slice(&1u16.to_be_bytes());
        let identity = ident(&long);
        assert!(identity.is_bootloader());
        assert_eq!(identity.firmware, None);
        assert_eq!(identity.language, None);
        assert_eq!(identity.hardware, Some(1000));
    }

    #[test]
    fn long_report_reads_identity() {
        let report = Report1::decode(FW1022).unwrap();
        let identity = ident(FW1022);
        assert_eq!(identity.hardware, Some(1000));
        assert_eq!(identity.bootloader, Some(100));
        assert_eq!(identity.firmware, Some(1022));
        assert_eq!(identity.serial, Some(report.serial));
        assert_eq!(
            identity.language,
            Language::from_byte(report.display_language)
        );
        assert!(identity.language.is_some());
    }

    #[test]
    fn zero_ids_are_absent() {
        let mut report = FW1022.to_vec();
        report[7..9].fill(0);
        report[11..13].fill(0);
        let identity = ident(&report);
        assert_eq!(identity.hardware, None);
        assert_eq!(identity.bootloader, None);
        assert_eq!(identity.firmware, Some(1022));
    }

    #[test]
    fn zero_serial_is_absent() {
        let mut report = FW1022.to_vec();
        report[3..7].fill(0);
        assert_eq!(ident(&report).serial, None);
        report[6] = 1;
        assert_eq!(ident(&report).serial, Some([0, 1]));
    }

    #[test]
    fn language_2_is_absent() {
        let mut report = FW1022.to_vec();
        report[34] = 2;
        assert_eq!(ident(&report).language, None);
        report[34] = 1;
        assert_eq!(ident(&report).language, Some(Language::German));
    }
}
