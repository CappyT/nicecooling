use nicecooling_protocol::access::WriteRefusal;
use nicecooling_protocol::lighting::{CONTROLLERS, LedController, LedStrip, read_lighting};
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::lighting::{controller_edits, strip_edits};
use nicecoolingd::leakshield::write::WriteError;

use crate::Rig;

fn feature_writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

#[test]
fn write_lighting_fields_through_write_path() {
    let rig = Rig::new("lighting-write");
    let before = rig.sim.report3();
    let report3 = rig.io.read_report3().unwrap();
    let original = read_lighting(&rig.registry, &report3).unwrap();

    // The strip: brightness and flags are `plausible-read`.
    let strip = LedStrip {
        brightness: 128,
        flags: 6,
    };
    let edits = strip_edits(&rig.registry, &report3, &strip).unwrap();
    assert_eq!(edits.len(), 2);
    rig.write(&edits).unwrap();
    let after_strip = rig.io.read_report3().unwrap();
    assert_eq!(rig.sim.report3()[571], 128);
    assert_eq!(&rig.sim.report3()[572..574], &6u16.to_be_bytes());

    // The second controller: the effect code and the hue of the first color are `plausible-read`.
    let mut second: LedController = original.controllers[1].clone();
    second.mode = 1;
    second.colors[0].hue = 120;
    let edits = controller_edits(&rig.registry, &after_strip, 1, &second).unwrap();
    assert_eq!(
        edits.iter().map(|e| e.field.as_str()).collect::<Vec<_>>(),
        ["led_controller_mode", "led_controller_color_hue"]
    );
    let written = rig.write(&edits).unwrap();

    let lighting = read_lighting(&rig.registry, &written.report3).unwrap();
    assert_eq!(lighting.strip, strip);
    assert_eq!(lighting.controllers.len(), CONTROLLERS);
    assert_eq!(lighting.controllers[0], original.controllers[0]);
    assert_eq!(lighting.controllers[1], second);
    // Nothing else in report 3 changed: the strip bytes, controller 2 and the CRC.
    let after = rig.sim.report3();
    let changed: Vec<usize> = (0..750).filter(|&i| after[i] != before[i]).collect();
    let hue = 620 + 70;
    // The two u16 values only change their low byte.
    assert_eq!(changed, [571, 573, 574 + 70 + 3, hue + 1, 748, 749]);
    assert_eq!(&after[hue..hue + 2], &120u16.to_be_bytes());
    assert_eq!(after[574 + 70 + 3], 1);
    assert_eq!(rig.backups().list().unwrap().len(), 2);
}

#[test]
fn a_lighting_field_that_is_not_verified_is_refused() {
    let rig = Rig::new("lighting-refused");
    let before = rig.sim.report3();
    let report3 = Report3::decode(&before).unwrap();
    let mut first = read_lighting(&rig.registry, &report3).unwrap().controllers[0].clone();
    // `led_controller_source` is at `code-defined`.
    first.source = 2;

    let edits = controller_edits(&rig.registry, &report3, 0, &first).unwrap();
    let error = rig.write(&edits).unwrap_err();

    assert!(
        matches!(&error, WriteError::Refused(WriteRefusal::NotVerified(id)) if id == "led_controller_source"),
        "{error:?}"
    );
    assert_eq!(rig.sim.report3(), before);
    assert_eq!(feature_writes(&rig), 0);
}

#[test]
fn a_value_out_of_range_is_refused_before_any_write() {
    let rig = Rig::new("lighting-range");
    let before = rig.sim.report3();
    let report3 = Report3::decode(&before).unwrap();
    let mut second = read_lighting(&rig.registry, &report3).unwrap().controllers[1].clone();
    // Effect 6 is not one of the codes of the registry.
    second.mode = 6;

    let edits = controller_edits(&rig.registry, &report3, 1, &second).unwrap();
    let error = rig.write(&edits).unwrap_err();

    assert!(
        matches!(&error, WriteError::OutOfRange { field } if field == "led_controller_mode"),
        "{error:?}"
    );
    assert_eq!(feature_writes(&rig), 0);
    assert!(controller_edits(&rig.registry, &report3, CONTROLLERS, &second).is_err());
}
