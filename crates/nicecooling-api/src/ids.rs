//! The identifier rules of `docs/api.md`. An identifier contains only `[a-z0-9_]`, so the object
//! path element is the identifier itself.

pub const BUS_NAME: &str = "io.github.cappyt.NiceCooling1";
pub const ROOT_PATH: &str = "/io/github/cappyt/NiceCooling1";
pub const DEFAULT_PROFILE: &str = "profile_default";

/// The child objects below the root path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sensor,
    Channel,
    Loop,
    Leakshield,
    Profile,
}

impl Kind {
    pub const fn segment(self) -> &'static str {
        match self {
            Self::Sensor => "sensor",
            Self::Channel => "channel",
            Self::Loop => "loop",
            Self::Leakshield => "leakshield",
            Self::Profile => "profile",
        }
    }
}

pub fn is_identifier(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `<hwmon prefix>_<attribute prefix>`, for example `nct6798_temp1`.
pub fn hwmon_sensor_id(hwmon_prefix: &str, attribute: &str) -> String {
    format!("{hwmon_prefix}_{attribute}")
}

/// `<hwmon prefix>_pwm<N>`, for example `nct6798_pwm6`.
pub fn channel_id(hwmon_prefix: &str, number: u32) -> String {
    format!("{hwmon_prefix}_pwm{number}")
}

/// `leakshield_<serial with - replaced by _>`.
pub fn leakshield_id(serial: &str) -> String {
    format!(
        "leakshield_{}",
        serial.replace('-', "_").to_ascii_lowercase()
    )
}

pub fn leakshield_sensor_id(leakshield: &str, field: &str) -> String {
    format!("{leakshield}_{field}")
}

pub fn derived_sensor_id(n: u32) -> String {
    format!("derived_{n}")
}

/// `graph_<n>`.
pub fn graph_sensor_id(n: u32) -> String {
    format!("graph_{n}")
}

/// The calibrated flow sensor of a Loop.
pub fn flow_sensor_id(loop_id: &str) -> String {
    format!("{loop_id}_flow")
}

pub fn loop_id(n: u32) -> String {
    format!("loop_{n}")
}

pub fn numbered_profile(n: u32) -> String {
    format!("profile_{n}")
}

pub fn rule_group_id(n: u32) -> String {
    format!("group_{n}")
}

/// The id of a user Calibration table. A preset keeps the id of its file.
pub fn table_id(n: u32) -> String {
    format!("table_{n}")
}

pub fn object_path(kind: Kind, id: &str) -> String {
    format!("{ROOT_PATH}/{}/{id}", kind.segment())
}

pub const BOARD_PATH: &str = "/io/github/cappyt/NiceCooling1/board";

/// The identifier of an object path of this kind. `None` for any other path.
pub fn id_from_path(kind: Kind, path: &str) -> Option<&str> {
    let id = path
        .strip_prefix(ROOT_PATH)?
        .strip_prefix('/')?
        .strip_prefix(kind.segment())?
        .strip_prefix('/')?;
    is_identifier(id).then_some(id)
}

/// The next number for an identifier family: one more than the highest `<prefix><n>` in use.
pub fn next_number<'a>(prefix: &str, used: impl IntoIterator<Item = &'a str>) -> u32 {
    used.into_iter()
        .filter_map(|id| id.strip_prefix(prefix)?.parse::<u32>().ok())
        .max()
        .map_or(1, |n| n + 1)
}

/// Takes the next number from a stored counter and moves the counter on. A number that was given
/// once is never given again, also after its owner was removed. A counter that is behind an id
/// in use (a state that was edited by hand) is lifted over it, so an id never exists twice.
pub fn take_number<'a>(
    prefix: &str,
    counter: &mut u32,
    used: impl IntoIterator<Item = &'a str>,
) -> u32 {
    let n = (*counter).max(next_number(prefix, used));
    *counter = n.saturating_add(1);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_follow_api_md() {
        assert_eq!(hwmon_sensor_id("nct6798", "temp1"), "nct6798_temp1");
        assert_eq!(hwmon_sensor_id("asusec", "fan1"), "asusec_fan1");
        assert_eq!(channel_id("nct6798", 6), "nct6798_pwm6");
        assert_eq!(leakshield_id("20417-31072"), "leakshield_20417_31072");
        assert_eq!(
            leakshield_sensor_id("leakshield_20417_31072", "pressure"),
            "leakshield_20417_31072_pressure"
        );
        assert_eq!(derived_sensor_id(1), "derived_1");
        assert_eq!(flow_sensor_id("loop_1"), "loop_1_flow");
        assert_eq!(loop_id(1), "loop_1");
        assert_eq!(numbered_profile(2), "profile_2");
        assert_eq!(rule_group_id(3), "group_3");
        assert_eq!(table_id(1), "table_1");
        for id in [
            "nct6798_temp1",
            "leakshield_20417_31072_pressure",
            "alphacool_17558",
            DEFAULT_PROFILE,
        ] {
            assert!(is_identifier(id), "{id}");
        }
        for id in ["", "Nct6798", "leakshield-1", "a b", "a/b", "é"] {
            assert!(!is_identifier(id), "{id}");
        }
    }

    #[test]
    fn object_paths_hold_the_identifier() {
        let path = object_path(Kind::Channel, "nct6798_pwm6");
        assert_eq!(path, "/io/github/cappyt/NiceCooling1/channel/nct6798_pwm6");
        assert_eq!(id_from_path(Kind::Channel, &path), Some("nct6798_pwm6"));
        assert_eq!(id_from_path(Kind::Sensor, &path), None);
        assert_eq!(id_from_path(Kind::Channel, ROOT_PATH), None);
        assert_eq!(id_from_path(Kind::Channel, "/"), None);
        assert_eq!(
            id_from_path(Kind::Channel, &format!("{ROOT_PATH}/channel/A-b")),
            None
        );
    }

    #[test]
    fn next_number_follows_the_highest() {
        assert_eq!(next_number("loop_", ["loop_1", "loop_3", "other"]), 4);
        assert_eq!(next_number("loop_", std::iter::empty()), 1);
    }

    #[test]
    fn counter_never_reuses_an_id() {
        let mut counter = 1;
        let mut used = Vec::new();
        for expected in 1..=3 {
            let n = take_number("graph_", &mut counter, used.iter().map(String::as_str));
            assert_eq!(n, expected);
            used.push(graph_sensor_id(n));
        }
        // The highest sensor goes away: `next_number` would give 3 again, the counter gives 4.
        used.pop();
        assert_eq!(next_number("graph_", used.iter().map(String::as_str)), 3);
        let n = take_number("graph_", &mut counter, used.iter().map(String::as_str));
        assert_eq!((n, counter), (4, 5));
        // A counter that is behind an id in use jumps over it.
        let mut behind = 1;
        assert_eq!(take_number("graph_", &mut behind, ["graph_7"]), 8);
        assert_eq!(behind, 9);
        // A counter of 0 (a state edited by hand) still gives ids from 1.
        let mut zero = 0;
        assert_eq!(take_number("graph_", &mut zero, []), 1);
        assert_eq!(graph_sensor_id(5), "graph_5");
    }
}
