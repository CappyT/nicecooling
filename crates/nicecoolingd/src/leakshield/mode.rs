//! Mode and Action. They share one transport: report 2 command `set_mode` with a code as the
//! parameter (`docs/protocol/leakshield.md`). Code 0 is `cancel_action`.

use std::time::Duration;

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;

/// aquasuite waits this long before it sends command 12.
pub const COMMAND_DELAY: Duration = Duration::from_millis(25);

/// The persistent state of the Leakshield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Shield,
    Monitor,
    Release,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Shield, Self::Monitor, Self::Release];

    pub fn code(self) -> u32 {
        match self {
            Self::Shield => 1,
            Self::Monitor => 2,
            Self::Release => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shield => "shield",
            Self::Monitor => "monitor",
            Self::Release => "release",
        }
    }

    /// Lowercase only: `Shield` is not a Mode.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == text)
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() as i16 == code)
    }
}

/// `None` when the registry of the firmware has no `set_mode` command.
pub fn set_mode_command(registry: &Registry, code: u32) -> Option<Report2> {
    registry
        .command("set_mode")
        .map(|c| Report2::new(c.code, code))
}

/// `cancel_action`: command 12 with code 0. `None` when the registry lacks it.
pub fn cancel_command(registry: &Registry) -> Option<Report2> {
    registry
        .command("cancel_action")
        .map(|c| Report2::new(c.code, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_are_lowercase_only() {
        for mode in Mode::ALL {
            assert_eq!(Mode::parse(mode.as_str()), Some(mode));
        }
        for text in ["Shield", "SHIELD", "", " shield", "test"] {
            assert_eq!(Mode::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn codes_follow_the_protocol() {
        assert_eq!(Mode::Shield.code(), 1);
        assert_eq!(Mode::Release.code(), 3);
        assert_eq!(Mode::from_code(2), Some(Mode::Monitor));
        assert_eq!(Mode::from_code(4), None);
    }
}
