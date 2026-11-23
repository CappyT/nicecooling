mod common;

use common::{fw1019, fw1022};
use nicecooling_protocol::access::{Access, ReadOnlyReason, WriteRefusal};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;

const HEADER: &str = r#"
schema_version = 1
model = "LEAKSHIELD"
firmware = 1022
structure = 100
[report3]
size = 750
structure_offset = 1
crc = { algorithm = "crc16_usb", first = 1, last = 747, offset = 748 }
volatile_mask = []
"#;

/// One field for each pair of risk class and verification state.
fn registry_of_all_states() -> Registry {
    let mut text = HEADER.to_owned();
    for (i, risk) in ["low", "safety"].iter().enumerate() {
        for (j, state) in ["code-defined", "plausible-read", "verified-write", "failed"]
            .iter()
            .enumerate()
        {
            text += &format!(
                "\n[[field]]\nid = \"{risk}_{state}\"\nreport = 3\noffset = {}\ntype = \"u8\"\nrisk_class = \"{risk}\"\nverification_state = \"{state}\"\n",
                3 + i * 4 + j
            );
        }
    }
    Registry::from_toml(&text).unwrap()
}

fn report1_with(firmware: u16, structure: u16) -> Report1 {
    let mut report = Report1::decode(fw1022().report1).unwrap();
    report.firmware_version = firmware;
    report.structure_number = structure;
    report
}

#[test]
fn unknown_firmware_refuses_every_write_with_reason() {
    let registry = Registry::leakshield();
    let access = Access::new(&registry, &report1_with(1023, 100));
    assert_eq!(
        access.device_reason(),
        Some(ReadOnlyReason::UnknownFirmware)
    );
    assert_eq!(ReadOnlyReason::UnknownFirmware.as_str(), "unknown-firmware");
    for field in &registry.field {
        assert_eq!(
            access.check_write(&field.id),
            Err(WriteRefusal::ReadOnly(ReadOnlyReason::UnknownFirmware)),
            "{}",
            field.id
        );
    }
    let all = registry_of_all_states();
    let access = Access::new(&all, &report1_with(0, 100));
    for field in &all.field {
        assert!(access.check_write(&field.id).is_err());
    }
}

#[test]
fn unknown_structure_refuses_every_write_with_reason() {
    let registry = Registry::leakshield();
    let access = Access::new(&registry, &report1_with(1022, 101));
    assert_eq!(
        access.device_reason(),
        Some(ReadOnlyReason::UnknownStructure)
    );
    assert_eq!(
        access.check_write(&registry.field[0].id),
        Err(WriteRefusal::ReadOnly(ReadOnlyReason::UnknownStructure))
    );
}

#[test]
fn fw1019_is_read_only() {
    let registry = Registry::leakshield();
    let report1 = Report1::decode(fw1019().report1).unwrap();
    let access = Access::new(&registry, &report1);
    assert_eq!(
        access.device_reason(),
        Some(ReadOnlyReason::UnknownFirmware)
    );
    for field in &registry.field {
        assert!(matches!(
            access.check_write(&field.id),
            Err(WriteRefusal::ReadOnly(_))
        ));
    }
    let all = registry_of_all_states();
    let access = Access::new(&all, &report1);
    assert!(all.field.iter().all(|f| access.check_write(&f.id).is_err()));
}

#[test]
fn non_verified_field_refused_with_field_id() {
    let registry = Registry::leakshield();
    let access = Access::new(&registry, &Report1::decode(fw1022().report1).unwrap());
    assert_eq!(access.device_reason(), None);
    let id = &registry.field.iter().find(|f| !f.is_writable()).unwrap().id;
    assert_eq!(
        access.check_write(id),
        Err(WriteRefusal::NotVerified(id.clone()))
    );
    assert_eq!(
        access.check_write("no_such_field"),
        Err(WriteRefusal::UnknownField("no_such_field".into()))
    );
}

#[test]
fn low_risk_writable_at_plausible_read_and_verified_write() {
    let registry = registry_of_all_states();
    let access = Access::new(&registry, &report1_with(1022, 100));
    assert!(access.check_write("low_plausible-read").is_ok());
    assert!(access.check_write("low_verified-write").is_ok());
    for state in ["code-defined", "failed"] {
        let id = format!("low_{state}");
        assert_eq!(
            access.check_write(&id),
            Err(WriteRefusal::NotVerified(id.clone()))
        );
    }
}

#[test]
fn safety_writable_only_at_verified_write() {
    let registry = registry_of_all_states();
    let access = Access::new(&registry, &report1_with(1022, 100));
    assert!(access.check_write("safety_verified-write").is_ok());
    for state in ["code-defined", "plausible-read", "failed"] {
        let id = format!("safety_{state}");
        assert_eq!(
            access.check_write(&id),
            Err(WriteRefusal::NotVerified(id.clone()))
        );
    }
}

#[test]
fn reason_identifiers_match_the_api_document() {
    let reasons = [
        (ReadOnlyReason::UnknownFirmware, "unknown-firmware"),
        (ReadOnlyReason::UnknownStructure, "unknown-structure"),
        (ReadOnlyReason::NotVerified, "not-verified"),
        (ReadOnlyReason::Disconnected, "disconnected"),
    ];
    for (reason, text) in reasons {
        assert_eq!(reason.as_str(), text);
    }

    // The document lists exactly these identifiers.
    let api =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api.md")).unwrap();
    let section = api
        .split("## Read-only reasons")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap();
    let documented: Vec<&str> = section
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|id| id.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .collect();
    assert_eq!(documented, reasons.map(|(_, text)| text));
}
