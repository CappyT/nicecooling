use std::fmt;

use serde::Deserialize;

/// A converted flow value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flow {
    pub lph: f64,
    /// The rate is above the last point of the table.
    pub out_of_range: bool,
}

/// Points of `[raw rpm, flow L/h]`, sorted by rpm.
#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationTable {
    points: Vec<(f64, f64)>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CalibrationError {
    TooFewPoints,
    NotFinite,
    NotSorted,
    Parse(String),
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewPoints => write!(f, "a Calibration table needs at least 2 points"),
            Self::NotFinite => write!(f, "the points must be finite numbers"),
            Self::NotSorted => write!(f, "the points must increase in rpm"),
            Self::Parse(m) => write!(f, "invalid preset: {m}"),
        }
    }
}

impl std::error::Error for CalibrationError {}

/// A preset file of `data/calibration/` (`docs/storage.md`).
#[derive(Clone, Debug)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub table: CalibrationTable,
}

#[derive(Deserialize)]
struct PresetFile {
    schema_version: u32,
    id: String,
    name: String,
    points: Vec<(f64, f64)>,
}

impl CalibrationTable {
    pub fn new(points: Vec<(f64, f64)>) -> Result<Self, CalibrationError> {
        if points.len() < 2 {
            return Err(CalibrationError::TooFewPoints);
        }
        if points.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
            return Err(CalibrationError::NotFinite);
        }
        if points.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(CalibrationError::NotSorted);
        }
        Ok(Self { points })
    }

    pub fn points(&self) -> &[(f64, f64)] {
        &self.points
    }

    /// Linear interpolation. Below the first point the line runs from 0 rpm = 0 L/h.
    /// Above the last point the last segment continues and the flow carries the out-of-range flag.
    pub fn flow(&self, rpm: f64) -> Flow {
        let rpm = rpm.max(0.0);
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];
        if rpm < first.0 {
            return Flow {
                lph: rpm * first.1 / first.0,
                out_of_range: false,
            };
        }
        let segment = self
            .points
            .windows(2)
            .find(|w| rpm <= w[1].0)
            .unwrap_or(&self.points[self.points.len() - 2..]);
        let (a, b) = (segment[0], segment[1]);
        Flow {
            lph: a.1 + (rpm - a.0) * (b.1 - a.1) / (b.0 - a.0),
            out_of_range: rpm > last.0,
        }
    }
}

impl Preset {
    pub fn from_toml(text: &str) -> Result<Self, CalibrationError> {
        let file: PresetFile =
            toml::from_str(text).map_err(|e| CalibrationError::Parse(e.to_string()))?;
        if file.schema_version != 1 {
            return Err(CalibrationError::Parse(format!(
                "unsupported schema_version {}",
                file.schema_version
            )));
        }
        Ok(Self {
            id: file.id,
            name: file.name,
            table: CalibrationTable::new(file.points)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> CalibrationTable {
        CalibrationTable::new(vec![(100.0, 10.0), (200.0, 30.0), (300.0, 40.0)]).unwrap()
    }

    fn preset() -> Preset {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/calibration/alphacool_17558.toml"
        );
        Preset::from_toml(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn between_points() {
        let flow = table().flow(150.0);
        assert_eq!(
            flow,
            Flow {
                lph: 20.0,
                out_of_range: false
            }
        );
        assert_eq!(table().flow(200.0).lph, 30.0);
    }

    #[test]
    fn below_first_point_line_from_zero() {
        let flow = table().flow(50.0);
        assert_eq!(
            flow,
            Flow {
                lph: 5.0,
                out_of_range: false
            }
        );
        assert_eq!(table().flow(0.0).lph, 0.0);
    }

    #[test]
    fn above_last_point_sets_out_of_range_flag() {
        let flow = table().flow(400.0);
        assert_eq!(
            flow,
            Flow {
                lph: 50.0,
                out_of_range: true
            }
        );
        assert!(!table().flow(300.0).out_of_range);
    }

    #[test]
    fn preset_has_27_points() {
        let preset = preset();
        assert_eq!(preset.id, "alphacool_17558");
        assert_eq!(preset.table.points().len(), 27);
    }

    #[test]
    fn alphacool_17558_671rpm() {
        let flow = preset().table.flow(671.0);
        assert!((flow.lph - 115.2).abs() < 0.1, "got {}", flow.lph);
        assert!(!flow.out_of_range);
    }

    #[test]
    fn table_rejects_non_finite_points() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for points in [
                vec![(bad, 1.0), (200.0, 2.0)],
                vec![(100.0, 1.0), (bad, 2.0)],
                vec![(100.0, bad), (200.0, 2.0)],
                vec![(100.0, 1.0), (200.0, bad)],
            ] {
                assert_eq!(
                    CalibrationTable::new(points.clone()),
                    Err(CalibrationError::NotFinite),
                    "{points:?}"
                );
            }
        }
    }

    #[test]
    fn table_rejects_bad_points() {
        assert_eq!(
            CalibrationTable::new(vec![(1.0, 1.0)]),
            Err(CalibrationError::TooFewPoints)
        );
        assert_eq!(
            CalibrationTable::new(vec![(2.0, 1.0), (1.0, 2.0)]),
            Err(CalibrationError::NotSorted)
        );
    }
}
