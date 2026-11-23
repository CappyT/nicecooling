use std::fmt;

use serde::{Deserialize, Serialize};

/// One water circuit (`docs/storage.md`, `loops`). An absent link is `null`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loop {
    pub id: String,
    pub name: String,
    pub pump_channels: Vec<String>,
    pub flow_sensor: Option<String>,
    pub calibration_table: Option<String>,
    pub water_temperature: Option<String>,
    pub leakshield: Option<String>,
    /// Minimum Duty of the Pump channels, in percent.
    pub pump_floor: f64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoopError {
    BadId,
    BadPumpFloor,
}

impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadId => write!(f, "a Loop id contains only [a-z0-9_]"),
            Self::BadPumpFloor => write!(f, "the Pump floor is a percent from 0 to 100"),
        }
    }
}

impl std::error::Error for LoopError {}

impl Loop {
    /// A Loop needs no Leakshield, no flow sensor and no water temperature sensor.
    pub fn validate(&self) -> Result<(), LoopError> {
        let id_ok = !self.id.is_empty()
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !id_ok {
            return Err(LoopError::BadId);
        }
        if !(0.0..=100.0).contains(&self.pump_floor) {
            return Err(LoopError::BadPumpFloor);
        }
        Ok(())
    }

    /// The id of the calibrated flow sensor of this Loop.
    pub fn flow_id(&self) -> String {
        format!("{}_flow", self.id)
    }
}

/// The id of the tachometer sensor of a Pump channel: `nct6798_pwm6` -> `nct6798_fan6`.
pub fn tach_sensor(channel: &str) -> Option<String> {
    let (chip, number) = channel.rsplit_once("_pwm")?;
    Some(format!("{chip}_fan{number}"))
}
