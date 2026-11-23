//! The acceptance checks of an image against a device (design D4). A pure function: it reads no
//! file and touches no device.

use nicecooling_protocol::firmware::bootloader::{Identity, Language};

use super::package::ImageRow;

/// The result of the checks. `Ok` and `Downgrade` come from the version checks, the rest are the
/// refusal reasons of the error `ImageRefused` (D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Downgrade,
    SameVersion,
    HardwareMismatch,
    BootloaderMismatch,
    TargetUnknown,
    TargetConflict,
}

impl Status {
    pub fn id(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Downgrade => "downgrade",
            Self::SameVersion => "same-version",
            Self::HardwareMismatch => "hardware-mismatch",
            Self::BootloaderMismatch => "bootloader-mismatch",
            Self::TargetUnknown => "target-unknown",
            Self::TargetConflict => "target-conflict",
        }
    }
}

/// The confirmations of the call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Lifts `downgrade`.
    pub downgrade: bool,
    /// Lifts `target-unknown`. A field that is known is still checked.
    pub unverified_target: bool,
}

/// What the update record says about the device (`record.rs`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordIdentity {
    pub hardware: Option<u16>,
    pub bootloader: Option<u16>,
    pub firmware_before: Option<u16>,
}

/// The device that the image goes to. A field that no source holds is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    hardware: Option<u16>,
    bootloader: Option<u16>,
    /// The installed firmware version. Absent in the bootloader.
    firmware: Option<u16>,
    /// The display language of the device. Absent in the bootloader.
    language: Option<Language>,
    /// The device is in the bootloader: the update is a resume.
    resume: bool,
    /// The firmware version before the interrupted update, from the record.
    firmware_before: Option<u16>,
    /// The record and the bootloader report 1 name different hardware or bootloader values.
    conflict: bool,
}

impl Target {
    /// A device in normal operation: every field comes from its report 1.
    pub fn running(identity: &Identity) -> Self {
        Self {
            hardware: identity.hardware,
            bootloader: identity.bootloader,
            firmware: identity.firmware,
            language: identity.language,
            resume: false,
            firmware_before: None,
            conflict: false,
        }
    }

    /// A device in the bootloader. The record is the first source of hardware and bootloader, the
    /// bootloader report 1 the second. A nonzero report value that differs from the record is a
    /// conflict.
    pub fn in_bootloader(record: Option<RecordIdentity>, report: Option<&Identity>) -> Self {
        let record = record.unwrap_or_default();
        let reported = |field: fn(&Identity) -> Option<u16>| report.and_then(field);
        let merge = |recorded: Option<u16>, reported: Option<u16>| match (recorded, reported) {
            (Some(r), Some(x)) if r != x => (Some(r), true),
            (Some(r), _) => (Some(r), false),
            (None, x) => (x, false),
        };
        let (hardware, hardware_conflict) = merge(record.hardware, reported(|i| i.hardware));
        let (bootloader, bootloader_conflict) =
            merge(record.bootloader, reported(|i| i.bootloader));
        Self {
            hardware,
            bootloader,
            firmware: None,
            language: None,
            resume: true,
            firmware_before: record.firmware_before,
            conflict: hardware_conflict || bootloader_conflict,
        }
    }
}

/// The status of `image` for `target`: the identity rules first, then the checks in order.
pub fn check(target: &Target, image: &ImageRow, options: Options) -> Status {
    if target.conflict {
        return Status::TargetConflict;
    }
    let unknown = target.hardware.is_none() || target.bootloader.is_none();
    if unknown && !options.unverified_target {
        return Status::TargetUnknown;
    }
    if target.hardware.is_some_and(|h| h != image.hardware) {
        return Status::HardwareMismatch;
    }
    if target.bootloader.is_some_and(|b| b != image.bootloader) {
        return Status::BootloaderMismatch;
    }
    // A resume repeats a flash, so the same version is valid. The version of the record is the
    // installed one for the downgrade check.
    let installed = if target.resume {
        target.firmware_before
    } else {
        let same_language = target.language.is_none_or(|l| l == image.language);
        if target.firmware == Some(image.version) && same_language {
            return Status::SameVersion;
        }
        target.firmware
    };
    if installed.is_some_and(|v| image.version < v) && !options.downgrade {
        return Status::Downgrade;
    }
    Status::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(version: u16, language: Language) -> ImageRow {
        ImageRow {
            id: 1,
            language,
            version,
            bootloader: 100,
            hardware: 1000,
        }
    }

    fn running(firmware: u16, language: Option<Language>) -> Target {
        Target {
            hardware: Some(1000),
            bootloader: Some(100),
            firmware: Some(firmware),
            language,
            resume: false,
            firmware_before: None,
            conflict: false,
        }
    }

    fn report(hardware: Option<u16>, bootloader: Option<u16>) -> Identity {
        let mut bytes = vec![0u8; 64];
        bytes[0] = 1;
        bytes[14] = 1;
        if let Some(h) = hardware {
            bytes[7..9].copy_from_slice(&h.to_be_bytes());
        }
        if let Some(b) = bootloader {
            bytes[11..13].copy_from_slice(&b.to_be_bytes());
        }
        Identity::from_report1(&bytes).unwrap()
    }

    fn record(
        hardware: Option<u16>,
        bootloader: Option<u16>,
        firmware_before: Option<u16>,
    ) -> RecordIdentity {
        RecordIdentity {
            hardware,
            bootloader,
            firmware_before,
        }
    }

    const NONE: Options = Options {
        downgrade: false,
        unverified_target: false,
    };

    #[test]
    fn ok_for_newer_version() {
        let target = running(1022, Some(Language::English));
        assert_eq!(
            check(&target, &image(1030, Language::English), NONE),
            Status::Ok
        );
    }

    #[test]
    fn same_version_refused() {
        let target = running(1022, Some(Language::English));
        assert_eq!(
            check(&target, &image(1022, Language::English), NONE),
            Status::SameVersion
        );
    }

    #[test]
    fn same_version_other_language_ok() {
        let target = running(1022, Some(Language::English));
        assert_eq!(
            check(&target, &image(1022, Language::German), NONE),
            Status::Ok
        );
    }

    #[test]
    fn same_version_unknown_language_refused() {
        let target = running(1022, None);
        for language in [Language::English, Language::German] {
            assert_eq!(
                check(&target, &image(1022, language), NONE),
                Status::SameVersion
            );
        }
    }

    #[test]
    fn equal_version_is_not_a_downgrade() {
        let target = running(1022, Some(Language::English));
        assert_ne!(
            check(&target, &image(1022, Language::German), NONE),
            Status::Downgrade
        );
    }

    #[test]
    fn hardware_mismatch_refused() {
        let target = running(1022, Some(Language::English));
        let mut other = image(1030, Language::English);
        other.hardware = 2000;
        assert_eq!(check(&target, &other, NONE), Status::HardwareMismatch);
    }

    #[test]
    fn bootloader_mismatch_refused() {
        let target = running(1022, Some(Language::English));
        let mut other = image(1030, Language::English);
        other.bootloader = 101;
        assert_eq!(check(&target, &other, NONE), Status::BootloaderMismatch);
        other.hardware = 2000;
        assert_eq!(check(&target, &other, NONE), Status::HardwareMismatch);
    }

    #[test]
    fn downgrade_needs_option() {
        let target = running(1022, Some(Language::English));
        assert_eq!(
            check(&target, &image(1020, Language::English), NONE),
            Status::Downgrade
        );
    }

    #[test]
    fn downgrade_with_option_ok() {
        let target = running(1022, Some(Language::English));
        let options = Options {
            downgrade: true,
            ..NONE
        };
        assert_eq!(
            check(&target, &image(1020, Language::English), options),
            Status::Ok
        );
    }

    #[test]
    fn bootloader_device_uses_record_identity() {
        let target = Target::in_bootloader(
            Some(record(Some(1000), Some(100), Some(1022))),
            Some(&report(None, None)),
        );
        assert_eq!(
            check(&target, &image(1030, Language::English), NONE),
            Status::Ok
        );
        let mut other = image(1030, Language::English);
        other.hardware = 2000;
        assert_eq!(check(&target, &other, NONE), Status::HardwareMismatch);
    }

    #[test]
    fn bootloader_device_uses_report_without_record() {
        let target = Target::in_bootloader(None, Some(&report(Some(1000), Some(100))));
        assert_eq!(
            check(&target, &image(1022, Language::English), NONE),
            Status::Ok
        );
        let mut other = image(1022, Language::English);
        other.bootloader = 99;
        assert_eq!(check(&target, &other, NONE), Status::BootloaderMismatch);
    }

    #[test]
    fn target_conflict_before_hardware_mismatch() {
        let target = Target::in_bootloader(
            Some(record(Some(1000), Some(100), None)),
            Some(&report(Some(2000), Some(100))),
        );
        let mut other = image(1030, Language::English);
        other.hardware = 3000;
        assert_eq!(check(&target, &other, NONE), Status::TargetConflict);
    }

    #[test]
    fn target_conflict_not_lifted_by_any_option() {
        let target = Target::in_bootloader(
            Some(record(Some(1000), Some(100), None)),
            Some(&report(Some(1000), Some(101))),
        );
        let options = Options {
            downgrade: true,
            unverified_target: true,
        };
        assert_eq!(
            check(&target, &image(1030, Language::English), options),
            Status::TargetConflict
        );
    }

    #[test]
    fn target_unknown_without_record() {
        let target = Target::in_bootloader(None, Some(&report(None, None)));
        assert_eq!(
            check(&target, &image(1030, Language::English), NONE),
            Status::TargetUnknown
        );
        let partial = Target::in_bootloader(None, Some(&report(Some(1000), None)));
        assert_eq!(
            check(&partial, &image(1030, Language::English), NONE),
            Status::TargetUnknown
        );
        let none = Target::in_bootloader(None, None);
        assert_eq!(
            check(&none, &image(1030, Language::English), NONE),
            Status::TargetUnknown
        );
    }

    #[test]
    fn unverified_target_lifts_target_unknown() {
        let target = Target::in_bootloader(None, Some(&report(None, None)));
        let options = Options {
            unverified_target: true,
            ..NONE
        };
        assert_eq!(
            check(&target, &image(1030, Language::English), options),
            Status::Ok
        );
    }

    #[test]
    fn unverified_target_still_checks_known_field() {
        let target = Target::in_bootloader(None, Some(&report(Some(1000), None)));
        let options = Options {
            unverified_target: true,
            ..NONE
        };
        let mut other = image(1030, Language::English);
        other.hardware = 2000;
        assert_eq!(check(&target, &other, options), Status::HardwareMismatch);
        assert_eq!(
            check(&target, &image(1030, Language::English), options),
            Status::Ok
        );
    }

    #[test]
    fn resume_skips_same_version() {
        let target = Target::in_bootloader(Some(record(Some(1000), Some(100), Some(1022))), None);
        for language in [Language::English, Language::German] {
            assert_eq!(check(&target, &image(1022, language), NONE), Status::Ok);
        }
    }

    #[test]
    fn resume_downgrade_from_record_needs_option() {
        let target = Target::in_bootloader(Some(record(Some(1000), Some(100), Some(1022))), None);
        assert_eq!(
            check(&target, &image(1020, Language::English), NONE),
            Status::Downgrade
        );
        let options = Options {
            downgrade: true,
            ..NONE
        };
        assert_eq!(
            check(&target, &image(1020, Language::English), options),
            Status::Ok
        );
    }

    #[test]
    fn resume_without_record_skips_version_checks() {
        let target = Target::in_bootloader(None, Some(&report(Some(1000), Some(100))));
        for version in [900, 1022, 1030] {
            assert_eq!(
                check(&target, &image(version, Language::English), NONE),
                Status::Ok
            );
        }
        let record_without_before =
            Target::in_bootloader(Some(record(Some(1000), Some(100), None)), None);
        assert_eq!(
            check(&record_without_before, &image(900, Language::German), NONE),
            Status::Ok
        );
    }

    #[test]
    fn status_ids_follow_the_contract() {
        let ids: Vec<&str> = [
            Status::Ok,
            Status::Downgrade,
            Status::SameVersion,
            Status::HardwareMismatch,
            Status::BootloaderMismatch,
            Status::TargetUnknown,
            Status::TargetConflict,
        ]
        .map(Status::id)
        .to_vec();
        assert_eq!(
            ids,
            [
                "ok",
                "downgrade",
                "same-version",
                "hardware-mismatch",
                "bootloader-mismatch",
                "target-unknown",
                "target-conflict"
            ]
        );
    }
}
