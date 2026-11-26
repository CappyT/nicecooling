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

/// The number of `<prefix><n>`: decimal digits only, with no sign and no leading zero. This is the
/// one form that the daemon gives, so any other text is a name that a user chose.
pub fn id_number(prefix: &str, id: &str) -> Option<u32> {
    let digits = id.strip_prefix(prefix)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) || (digits.len() > 1 && digits.starts_with('0'))
    {
        return None;
    }
    digits.parse().ok()
}

/// The next number for an identifier family: one more than the highest `<prefix><n>` in use.
/// `None` when the highest number is the largest one: no id is left.
pub fn next_number<'a>(prefix: &str, used: impl IntoIterator<Item = &'a str>) -> Option<u32> {
    used.into_iter()
        .filter_map(|id| id_number(prefix, id))
        .max()
        .map_or(Some(1), |n| n.checked_add(1))
}

/// Takes the next number from a stored counter and moves the counter on. A number that was given
/// once is never given again, also after its owner was removed. A counter that is behind an id
/// in use (a state that was edited by hand) is lifted over it, so an id never exists twice. The
/// counter never wraps: `None` when no number above the ids in use is left.
pub fn take_number<'a>(
    prefix: &str,
    counter: &mut u32,
    used: impl IntoIterator<Item = &'a str>,
) -> Option<u32> {
    let above_used = next_number(prefix, used)?;
    let n = (*counter).max(above_used);
    *counter = n.saturating_add(1);
    Some(n)
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
        assert_eq!(next_number("loop_", ["loop_1", "loop_3", "other"]), Some(4));
        assert_eq!(next_number("loop_", std::iter::empty()), Some(1));
        assert_eq!(next_number("loop_", ["loop_+9", "loop_007"]), Some(1));
        assert_eq!(next_number("loop_", ["loop_4294967294"]), Some(u32::MAX));
        assert_eq!(next_number("loop_", ["loop_4294967295", "loop_1"]), None);
    }

    #[test]
    fn a_number_is_decimal_digits_without_a_sign_or_a_leading_zero() {
        assert_eq!(id_number("graph_", "graph_1"), Some(1));
        assert_eq!(id_number("graph_", "graph_4294967295"), Some(u32::MAX));
        assert_eq!(id_number("graph_", "graph_0"), Some(0));
        for id in [
            "graph_",
            "graph_+5",
            "graph_-5",
            "graph_05",
            "graph_00",
            "graph_+4294967295",
            "graph_4294967296",
            "graph_1_2",
            "graph_ 1",
            "graph_١",
            "loop_1",
        ] {
            assert_eq!(id_number("graph_", id), None, "{id}");
        }
        // The ids of every family that the daemon has given: `<prefix><n>` for n from 1.
        for n in [1, 9, 10, 99, 100, 12345, u32::MAX] {
            for (prefix, id) in [
                ("derived_", derived_sensor_id(n)),
                ("graph_", graph_sensor_id(n)),
                ("loop_", loop_id(n)),
                ("profile_", numbered_profile(n)),
                ("group_", rule_group_id(n)),
                ("table_", table_id(n)),
            ] {
                assert_eq!(id_number(prefix, &id), Some(n), "{id}");
            }
        }
    }

    #[test]
    fn counter_never_reuses_an_id() {
        let mut counter = 1;
        let mut used = Vec::new();
        for expected in 1..=3 {
            let n = take_number("graph_", &mut counter, used.iter().map(String::as_str));
            assert_eq!(n, Some(expected));
            used.push(graph_sensor_id(expected));
        }
        // The highest sensor goes away: `next_number` would give 3 again, the counter gives 4.
        used.pop();
        assert_eq!(
            next_number("graph_", used.iter().map(String::as_str)),
            Some(3)
        );
        let n = take_number("graph_", &mut counter, used.iter().map(String::as_str));
        assert_eq!((n, counter), (Some(4), 5));
        // A counter that is behind an id in use jumps over it.
        let mut behind = 1;
        assert_eq!(take_number("graph_", &mut behind, ["graph_7"]), Some(8));
        assert_eq!(behind, 9);
        // A counter of 0 (a state edited by hand) still gives ids from 1.
        let mut zero = 0;
        assert_eq!(take_number("graph_", &mut zero, []), Some(1));
        assert_eq!(graph_sensor_id(5), "graph_5");
    }

    #[test]
    fn counter_stops_at_the_end_of_the_range() {
        let mut counter = u32::MAX;
        assert_eq!(take_number("graph_", &mut counter, []), Some(u32::MAX));
        assert_eq!(counter, u32::MAX);
        assert_eq!(
            take_number("graph_", &mut counter, ["graph_4294967295"]),
            None
        );
        assert_eq!(counter, u32::MAX);
        let mut low = 1;
        assert_eq!(take_number("graph_", &mut low, ["graph_4294967295"]), None);
        assert_eq!(low, 1);
    }
}
