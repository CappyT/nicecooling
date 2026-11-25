//! The units of a Graph sensor and the limit on their number. The daemon checks a unit with this
//! table, and the app offers the same units in the editor.

/// The most Graph sensors that exist at one time.
pub const MAX_GRAPH_SENSORS: usize = 64;

/// One unit that a Graph sensor can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphSensorUnit {
    /// The unit identifier of `docs/api.md`.
    pub code: &'static str,
    /// The `Kind` of a sensor with this unit.
    pub kind: &'static str,
    /// The text that the app shows for the unit.
    pub display: &'static str,
}

/// The 8 units of a Graph sensor.
pub const GRAPH_SENSOR_UNITS: [GraphSensorUnit; 8] = [
    GraphSensorUnit {
        code: "degC",
        kind: "temperature",
        display: "°C",
    },
    GraphSensorUnit {
        code: "rpm",
        kind: "fan",
        display: "rpm",
    },
    GraphSensorUnit {
        code: "%",
        kind: "other",
        display: "%",
    },
    GraphSensorUnit {
        code: "V",
        kind: "voltage",
        display: "V",
    },
    GraphSensorUnit {
        code: "A",
        kind: "other",
        display: "A",
    },
    GraphSensorUnit {
        code: "W",
        kind: "other",
        display: "W",
    },
    GraphSensorUnit {
        code: "l/h",
        kind: "flow",
        display: "L/h",
    },
    GraphSensorUnit {
        code: "mbar",
        kind: "pressure",
        display: "mbar",
    },
];

/// The row of a unit, or `None` for a unit that a Graph sensor cannot have.
pub fn graph_sensor_unit(code: &str) -> Option<&'static GraphSensorUnit> {
    GRAPH_SENSOR_UNITS.iter().find(|u| u.code == code)
}

/// The `Kind` of a Graph sensor with this unit.
pub fn graph_sensor_kind(code: &str) -> Option<&'static str> {
    graph_sensor_unit(code).map(|u| u.kind)
}

/// The text that the app shows for this unit.
pub fn graph_sensor_display(code: &str) -> Option<&'static str> {
    graph_sensor_unit(code).map(|u| u.display)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_to_kind_table() {
        let table = [
            ("degC", "temperature"),
            ("rpm", "fan"),
            ("%", "other"),
            ("V", "voltage"),
            ("A", "other"),
            ("W", "other"),
            ("l/h", "flow"),
            ("mbar", "pressure"),
        ];
        assert_eq!(GRAPH_SENSOR_UNITS.len(), table.len());
        for (unit, kind) in table {
            assert_eq!(graph_sensor_kind(unit), Some(kind), "{unit}");
        }
    }

    #[test]
    fn unit_display_text() {
        assert_eq!(graph_sensor_display("degC"), Some("°C"));
        assert_eq!(graph_sensor_display("l/h"), Some("L/h"));
        for unit in ["rpm", "%", "V", "A", "W", "mbar"] {
            assert_eq!(graph_sensor_display(unit), Some(unit));
        }
    }

    #[test]
    fn unit_not_in_the_table_is_none() {
        for unit in ["bar", "", "DEGC", "°C", "mbar/min", "L/h", " V"] {
            assert_eq!(graph_sensor_unit(unit), None, "{unit:?}");
            assert_eq!(graph_sensor_kind(unit), None);
            assert_eq!(graph_sensor_display(unit), None);
        }
    }

    #[test]
    fn max_graph_sensors_is_64() {
        assert_eq!(MAX_GRAPH_SENSORS, 64);
    }
}
