//! The two counter resets of the device: the refill history and the service counter. The code
//! does not say which values of report 1 they clear (`docs/protocol/leakshield.md`, open
//! questions). The device has no command for the pump runtime.

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report2::Report2;

/// The counters of the System page that report 1 holds, as `(name, value)` for `GetCounters`
/// (`docs/api.md`). The times are in seconds.
pub fn counters(report1: &Report1) -> Vec<(&'static str, u64)> {
    [
        ("pump_runtime_s", report1.pump_runtime),
        ("pump_short_cycle_runtime_s", report1.pump_short_cycles),
        ("pump_revolution_count", report1.pump_revolution_count),
        ("fill_valve_cycles", report1.fill_valve_cycles),
        ("release_valve_cycles", report1.release_valve_cycles),
        ("total_runtime_s", report1.total_runtime),
        ("power_up_count", report1.power_up_count),
    ]
    .into_iter()
    .map(|(name, value)| (name, u64::from(value)))
    .collect()
}

/// `None` when the registry of the firmware has no `reset_refill_history` command.
pub fn reset_refill_history(registry: &Registry) -> Option<Report2> {
    registry
        .command("reset_refill_history")
        .map(|c| Report2::new(c.code, 0))
}

/// The parameter is the serial (report 1 bytes 3-6, big-endian) XOR 0xFFFFFFFF. `None` when the
/// registry of the firmware has no `reset_service_counter` command.
pub fn reset_service_counter(registry: &Registry, serial: [u16; 2]) -> Option<Report2> {
    let serial = u32::from(serial[0]) << 16 | u32::from(serial[1]);
    registry
        .command("reset_service_counter")
        .map(|c| Report2::new(c.code, serial ^ 0xFFFF_FFFF))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_of_the_fw1022_dump() {
        let report1 = Report1::decode(include_bytes!(
            "../../../../tests/fixtures/leakshield/fw1022/report1.bin"
        ))
        .unwrap();
        assert_eq!(
            counters(&report1),
            [
                ("pump_runtime_s", 11431),
                ("pump_short_cycle_runtime_s", 21),
                ("pump_revolution_count", 633_745),
                ("fill_valve_cycles", 3074),
                ("release_valve_cycles", 952),
                ("total_runtime_s", 145_500_674),
                ("power_up_count", 142),
            ]
        );
    }

    #[test]
    fn service_counter_parameter_is_the_serial_inverted() {
        let command = reset_service_counter(&Registry::leakshield(), [20417, 31072]).unwrap();
        assert_eq!(command.command, 25000);
        assert_eq!(command.parameter, !((20417u32 << 16) | 31072));
    }
}
