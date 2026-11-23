//! The two counter resets of the device: the refill history and the service counter. The code
//! does not say which values of report 1 they clear (`docs/protocol/leakshield.md`, open
//! questions). The device has no command for the pump runtime.

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;

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
    fn service_counter_parameter_is_the_serial_inverted() {
        let command = reset_service_counter(&Registry::leakshield(), [20417, 31072]).unwrap();
        assert_eq!(command.command, 25000);
        assert_eq!(command.parameter, !((20417u32 << 16) | 31072));
    }
}
