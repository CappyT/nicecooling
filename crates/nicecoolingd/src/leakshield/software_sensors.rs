//! The 12 Software sensors of a Leakshield: Feed message slots 4 to 15. A slot links a sensor of
//! the daemon, and each Feed message carries its value times the scale of the slot plus its
//! offset, as aquasuite computes it. The API numbers the slots from 1.

use std::collections::BTreeMap;
use std::sync::Mutex;

use nicecooling_protocol::feed::{FEED_SLOTS, FeedMessage, NO_VALUE};

pub const SLOT_COUNT: usize = 12;
/// Feed message slot of Software sensor 1.
pub const FIRST_FEED_SLOT: usize = FEED_SLOTS - SLOT_COUNT;

/// A sensor in a slot, with the unit code that tells the device how to scale the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub sensor: String,
    pub unit_code: u8,
    /// Decimal places of the raw value for this unit code.
    pub decimals: u8,
}

/// The device unit code and its decimal places for a unit of `docs/api.md`
/// (`docs/protocol/leakshield.md`, section Unit codes). `None` when the device has no such unit.
pub fn device_unit(unit: &str) -> Option<(u8, u8)> {
    Some(match unit {
        "rpm" => (3, 0),
        "%" => (4, 2),
        "A" => (7, 2),
        "V" => (9, 2),
        "degC" => (10, 2),
        "l/h" => (12, 1),
        "W" => (21, 0),
        "mbar" => (32, 1),
        _ => return None,
    })
}

/// The scale and the offset of a slot. The default leaves the value as it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scaling {
    pub scale: f64,
    pub offset: f64,
}

impl Default for Scaling {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: 0.0,
        }
    }
}

impl Scaling {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn apply(&self, value: f64) -> f64 {
        value * self.scale + self.offset
    }
}

#[derive(Debug, Default)]
struct Slots {
    sensors: [Option<Assignment>; SLOT_COUNT],
    scalings: [Scaling; SLOT_COUNT],
}

impl Assignment {
    /// `None` when the unit of the sensor has no device unit code.
    pub fn new(sensor: &str, unit: &str) -> Option<Self> {
        let (unit_code, decimals) = device_unit(unit)?;
        Some(Self {
            sensor: sensor.to_owned(),
            unit_code,
            decimals,
        })
    }

    /// The raw value in the scale of the unit. `None` when the value does not fit the slot.
    fn raw(&self, value: f64) -> Option<i16> {
        let scaled = (value * 10f64.powi(i32::from(self.decimals))).round();
        (scaled.is_finite() && scaled.abs() < f64::from(NO_VALUE)).then_some(scaled as i16)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct NoSuchSlot;

#[derive(Debug, Default)]
pub struct SoftwareSlots(Mutex<Slots>);

fn index(slot: u16) -> Result<usize, NoSuchSlot> {
    match usize::from(slot) {
        1..=SLOT_COUNT => Ok(usize::from(slot) - 1),
        _ => Err(NoSuchSlot),
    }
}

impl SoftwareSlots {
    /// `None` clears the slot. Its scale and offset stay.
    pub fn assign(&self, slot: u16, assignment: Option<Assignment>) -> Result<(), NoSuchSlot> {
        self.0.lock().unwrap().sensors[index(slot)?] = assignment;
        Ok(())
    }

    /// Sets the scale and the offset of a slot. The caller checks that both are finite.
    pub fn set_scale(&self, slot: u16, scale: f64, offset: f64) -> Result<(), NoSuchSlot> {
        self.0.lock().unwrap().scalings[index(slot)?] = Scaling { scale, offset };
        Ok(())
    }

    /// Slot number and the sensor id in it, for all 12 slots.
    pub fn list(&self) -> Vec<(u16, Option<String>)> {
        self.0
            .lock()
            .unwrap()
            .sensors
            .iter()
            .enumerate()
            .map(|(i, a)| (i as u16 + 1, a.as_ref().map(|a| a.sensor.clone())))
            .collect()
    }

    /// Slot number, scale and offset, for all 12 slots.
    pub fn scales(&self) -> Vec<(u16, f64, f64)> {
        self.0
            .lock()
            .unwrap()
            .scalings
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u16 + 1, s.scale, s.offset))
            .collect()
    }

    /// Puts the value of each assigned sensor into its slot. A sensor without a value leaves the
    /// slot at "no value".
    pub fn fill(&self, message: &mut FeedMessage, values: &BTreeMap<String, f64>) {
        let slots = self.0.lock().unwrap();
        for (i, assignment) in slots.sensors.iter().enumerate() {
            let slot = FIRST_FEED_SLOT + i;
            match assignment {
                Some(a) => match values
                    .get(&a.sensor)
                    .and_then(|&v| a.raw(slots.scalings[i].apply(v)))
                {
                    Some(raw) => message.set(slot, raw, a.unit_code),
                    None => message.clear(slot),
                },
                None => message.clear(slot),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_scale_by_their_decimals() {
        let (unit_code, decimals) = device_unit("degC").unwrap();
        let a = Assignment {
            sensor: "t".into(),
            unit_code,
            decimals,
        };
        assert_eq!(a.raw(31.456), Some(3146));
        assert_eq!(a.raw(f64::NAN), None);
        assert_eq!(a.raw(400.0), None);
        assert_eq!(device_unit("furlong"), None);
    }
}
