mod common;

use common::{fw1019, fw1022};
use nicecooling_protocol::registry::{Registry, RegistryError, VerificationState};
use nicecooling_protocol::report3::Report3;

#[test]
fn registry_every_entry_has_risk_class_and_verification_state_and_in_range_on_fixtures() {
    let registry = Registry::leakshield();
    assert!(!registry.field.is_empty());
    for fixture in [fw1022().report3, fw1019().report3] {
        let report = Report3::decode(fixture).unwrap();
        assert_eq!(report.structure_number(), registry.structure);
        for field in &registry.field {
            // risk_class and verification_state are required keys, so the load already proves them.
            for offset in field.element_offsets() {
                let raw = field.read_raw(&report, offset);
                assert!(
                    field.raw_in_range(raw),
                    "{} at offset {offset}: raw {raw}",
                    field.id
                );
            }
        }
    }
}

#[test]
fn plausible_read_entries_in_range_on_fixtures() {
    let registry = Registry::leakshield();
    let report = Report3::decode(fw1022().report3).unwrap();
    let plausible: Vec<_> = registry
        .field
        .iter()
        .filter(|f| f.verification_state == VerificationState::PlausibleRead)
        .collect();
    assert!(!plausible.is_empty());
    for field in plausible {
        assert!(field.in_range_in(&report), "{}", field.id);
    }
}

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

fn field(state: &str, risk: &str) -> String {
    format!(
        "{HEADER}\n[[field]]\nid = \"x\"\nreport = 3\noffset = 3\ntype = \"u8\"\nrisk_class = \"{risk}\"\nverification_state = \"{state}\"\n"
    )
}

#[test]
fn registry_rejects_unknown_verification_state() {
    for state in ["code-defined", "plausible-read", "verified-write", "failed"] {
        Registry::from_toml(&field(state, "low")).unwrap();
    }
    for state in ["verified", "plausible_read", "Failed", ""] {
        assert!(
            matches!(
                Registry::from_toml(&field(state, "low")),
                Err(RegistryError::Parse(_))
            ),
            "{state:?}"
        );
    }
}

#[test]
fn registry_rejects_missing_or_unknown_risk_class_and_state() {
    let no_risk = format!(
        "{HEADER}\n[[field]]\nid = \"x\"\nreport = 3\noffset = 3\ntype = \"u8\"\nverification_state = \"failed\"\n"
    );
    let no_state = format!(
        "{HEADER}\n[[field]]\nid = \"x\"\nreport = 3\noffset = 3\ntype = \"u8\"\nrisk_class = \"low\"\n"
    );
    assert!(Registry::from_toml(&no_risk).is_err());
    assert!(Registry::from_toml(&no_state).is_err());
    assert!(Registry::from_toml(&field("failed", "medium")).is_err());
}

#[test]
fn registry_rejects_a_field_outside_the_struct_and_duplicate_ids() {
    let outside = field("failed", "low").replace("offset = 3", "offset = 748");
    assert!(matches!(
        Registry::from_toml(&outside),
        Err(RegistryError::Invalid(_))
    ));
    let twice = format!(
        "{}{}",
        field("failed", "low"),
        "\n[[field]]\nid = \"x\"\nreport = 3\noffset = 4\ntype = \"u8\"\nrisk_class = \"low\"\nverification_state = \"failed\"\n"
    );
    assert!(matches!(
        Registry::from_toml(&twice),
        Err(RegistryError::Invalid(_))
    ));
}

#[test]
fn registry_keys_are_firmware_1022_structure_100() {
    let registry = Registry::leakshield();
    assert_eq!((registry.firmware, registry.structure), (1022, 100));
    assert_eq!(registry.command("save_settings").map(|c| c.code), Some(2));
}
