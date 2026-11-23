use nicecooling_core::loops::Loop;
use nicecooling_core::tick::LoopSetup;
use nicecoolingd::leakshield::action::{LoopPumps, refill_blocked};

const THIS: &str = "leakshield_20417_31072";
const OTHER: &str = "leakshield_1_1";

fn loop_of(id: &str, leakshield: &str, pump: &str) -> LoopSetup {
    LoopSetup {
        config: Loop {
            id: id.to_owned(),
            name: id.to_owned(),
            pump_channels: vec![pump.to_owned()],
            flow_sensor: None,
            calibration_table: None,
            water_temperature: None,
            leakshield: Some(leakshield.to_owned()),
            pump_floor: 20.0,
        },
        table: None,
    }
}

fn blocked(loops: &[LoopSetup], rpm: &[(&str, f64)]) -> bool {
    refill_blocked(
        THIS,
        loops.iter().map(|l| LoopPumps {
            leakshield: l.config.leakshield.as_deref(),
            pump_channels: &l.config.pump_channels,
        }),
        |sensor| rpm.iter().find(|(id, _)| *id == sensor).map(|(_, v)| *v),
    )
}

#[test]
fn refill_refused_while_loop_pump_runs() {
    let loops = [loop_of("loop_1", THIS, "nct6798_pwm6")];
    assert!(blocked(&loops, &[("nct6798_fan6", 2000.0)]));
    // A pump at 0 rpm, or without a reading, does not block.
    assert!(!blocked(&loops, &[("nct6798_fan6", 0.0)]));
    assert!(!blocked(&loops, &[]));
}

#[test]
fn refill_allowed_when_only_other_loop_pump_runs() {
    let loops = [
        loop_of("loop_1", THIS, "nct6798_pwm6"),
        loop_of("loop_2", OTHER, "nct6798_pwm2"),
    ];
    let rpm = [("nct6798_fan6", 0.0), ("nct6798_fan2", 1800.0)];
    assert!(!blocked(&loops, &rpm));
    // The same reading blocks when the pump belongs to this Leakshield.
    let loops = [
        loop_of("loop_1", THIS, "nct6798_pwm6"),
        loop_of("loop_2", THIS, "nct6798_pwm2"),
    ];
    assert!(blocked(&loops, &rpm));
}
