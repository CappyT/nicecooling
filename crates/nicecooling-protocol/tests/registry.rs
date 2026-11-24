mod common;

use common::{fw1019, fw1022};
use nicecooling_protocol::registry::{Registry, RegistryError, RiskClass, VerificationState};
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

fn is_kebab_case(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

#[test]
fn enum_fields_have_unique_kebab_case_names_in_the_order_of_their_values() {
    let registry = Registry::leakshield();
    let mut enums = 0;
    for field in &registry.field {
        // A field with values and no names is a set of raw numbers, such as an index.
        assert!(
            field.names.is_none() || field.values.is_some(),
            "{}: names need values",
            field.id
        );
        let (Some(names), Some(values)) = (&field.names, &field.values) else {
            continue;
        };
        enums += 1;
        assert_eq!(names.len(), values.len(), "{}", field.id);
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "{}: duplicate name", field.id);
        for (name, &code) in names.iter().zip(values) {
            assert!(is_kebab_case(name), "{}: bad name {name:?}", field.id);
            assert_eq!(field.name_of(code), Some(name.as_str()), "{}", field.id);
            assert_eq!(field.code_of(name), Some(code), "{}", field.id);
        }
    }
    assert!(enums > 0);
}

#[test]
fn name_lookup_refuses_what_the_field_does_not_list() {
    let registry = Registry::leakshield();
    let model = registry.field("reservoir_model").unwrap();
    assert_eq!(model.name_of(255), Some("custom"));
    assert_eq!(model.code_of("ultitube-150"), Some(2));
    assert_eq!(model.name_of(12), None);
    assert_eq!(model.code_of("unknown-12"), None);
    // A limit field and a bit mask have no names.
    assert_eq!(registry.field("device_profile").unwrap().name_of(1), None);
    assert_eq!(registry.field("pump_flags").unwrap().code_of("none"), None);
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

#[test]
fn device_profile_is_a_safety_field_that_no_write_reaches() {
    let registry = Registry::leakshield();
    let field = registry.field("device_profile").unwrap();
    assert_eq!(field.risk_class, RiskClass::Safety);
    assert!(!field.is_writable());
    assert_eq!(
        registry.field("sensor_unit").unwrap().risk_class,
        RiskClass::Safety
    );
}
