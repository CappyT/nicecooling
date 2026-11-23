//! The alarm reset: report 2 command `reset_alarm` with parameter 0. It is the only command that
//! the code of aquasuite defines to clear `alarm_bits_latched` (`docs/protocol/leakshield.md`,
//! section Warnings and Alarms). Whether it clears every latched bit or only those that are clear
//! in `alarm_bits` is open: the hardware check of task 5.11 measures it.

use std::time::Duration;

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;

/// aquasuite waits this long before it sends command 11.
pub const COMMAND_DELAY: Duration = Duration::from_millis(5);

/// `None` when the registry of the firmware has no `reset_alarm` command.
pub fn command(registry: &Registry) -> Option<Report2> {
    registry
        .command("reset_alarm")
        .map(|c| Report2::new(c.code, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_alarm_is_command_11_with_parameter_0() {
        let command = command(&Registry::leakshield()).unwrap();
        assert_eq!((command.command, command.parameter), (11, 0));
    }
}
