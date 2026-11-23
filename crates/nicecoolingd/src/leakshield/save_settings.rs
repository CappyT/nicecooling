//! The save-settings command: report 2 command `save_settings`, sent 4 s after the last report 3 write.

use std::time::Duration;

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;

pub const SAVE_SETTINGS_DELAY: Duration = Duration::from_secs(4);

/// `None` when the registry of the firmware has no `save_settings` command.
pub fn command(registry: &Registry) -> Option<Report2> {
    registry
        .command("save_settings")
        .map(|c| Report2::new(c.code, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_settings_bytes_match_golden() {
        let bytes = command(&Registry::leakshield()).unwrap().encode();
        assert_eq!(
            bytes,
            [
                0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x34, 0xC6
            ]
        );
    }
}
