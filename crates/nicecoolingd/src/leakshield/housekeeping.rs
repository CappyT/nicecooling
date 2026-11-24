//! Three small commands of report 2: the event log reset, the Refill pause and the alarm
//! suppression for the sleep of the PC. Each sends its code and a fixed parameter.

use std::time::Duration;

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;

/// aquasuite waits this long before it sends command 14.
pub const RESET_EVENT_LOG_DELAY: Duration = Duration::from_millis(5);

/// The parameter of `suppress_alarm`: 90000 in the unit of the command, divided by 25.
pub const SUPPRESS_ALARM_PARAMETER: u32 = 90_000 / 25;

/// `reset_event_log`: command 14 with parameter 0. `None` when the registry lacks it.
pub fn reset_event_log(registry: &Registry) -> Option<Report2> {
    registry
        .command("reset_event_log")
        .map(|c| Report2::new(c.code, 0))
}

/// `set_fill_mode` with parameter 0: the Refill view of aquasuite sends it for its button that
/// pauses and resumes the Refill. `None` when the registry lacks it.
pub fn toggle_fill_mode(registry: &Registry) -> Option<Report2> {
    registry
        .command("set_fill_mode")
        .map(|c| Report2::new(c.code, 0))
}

/// `suppress_alarm`: command 16. aquasuite sends it when the PC goes to sleep. `None` when the
/// registry lacks it.
pub fn suppress_alarm(registry: &Registry) -> Option<Report2> {
    registry
        .command("suppress_alarm")
        .map(|c| Report2::new(c.code, SUPPRESS_ALARM_PARAMETER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(command: Option<Report2>) -> (u32, u32) {
        let command = command.unwrap();
        (command.command, command.parameter)
    }

    #[test]
    fn the_commands_carry_their_codes_and_parameters() {
        let registry = Registry::leakshield();
        assert_eq!(pair(reset_event_log(&registry)), (14, 0));
        assert_eq!(pair(toggle_fill_mode(&registry)), (15, 0));
        assert_eq!(pair(suppress_alarm(&registry)), (16, 3600));
    }
}
