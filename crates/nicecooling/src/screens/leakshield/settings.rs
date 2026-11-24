//! The Settings tab, a wizard with eight steps (`docs/ui/screens/leakshield-settings.md`).
//! The wizard writes nothing before "Apply". Every edit goes into a list of pending changes.

use std::collections::BTreeMap;

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{FieldValue, LeakshieldData, WriteFailure};
use crate::screens::format::{compact, plain};
use crate::screens::leakshield::fields::{humanize, unit_of};
use crate::screens::leakshield::{Device, TabState, fields_command, write_line};
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};

const SHIELD_FLAGS: &str = "shield_flags";
const BIT_TIMER: u32 = 2;
const BIT_ADAPTIVE: u32 = 8;
/// Set: the Leakshield holds the recommended pressure values. Clear: the values of the user.
const BIT_RECOMMENDED: u32 = 16;
const BIT_STANDBY_PRESSURE: u32 = 32;
const PUMP_FLAGS: &str = "pump_flags";
/// The Loop has more than one pump in a row: their pressures add up.
const BIT_TWO_PUMPS: u32 = 2;
/// The range of the height difference of the Loop in cm.
const LOOP_HEIGHT_RANGE: (f64, f64) = (0.0, 200.0);
/// Above this height in cm the recommended pressure values can be too low.
const LOOP_HEIGHT_WARNING: f64 = 100.0;
/// The highest pump pressure that the Leakshield can protect against, in mbar: the highest
/// minimum of the control band.
const PROTECT_LIMIT: f64 = 430.0;

/// A type that the Leakshield knows: its identifier, its name and the values of its fields.
type Preset = (&'static str, &'static str, [f64; 3]);

const PUMP_FIELDS: [&str; 3] = ["pump_max_speed", "pump_start_speed", "pump_max_pressure"];
/// The pump types: maximum speed in rpm, start speed in rpm, maximum pressure in mbar.
const PUMPS: [Preset; 8] = [
    ("d5", "D5", [4800.0, 4800.0, 375.0]),
    ("aquastream", "aquastream", [5500.0, 5500.0, 400.0]),
    ("ddc-1t", "DDC-1T", [3900.0, 3900.0, 350.0]),
    ("ddc-1t-plus", "DDC-1T Plus", [4500.0, 4500.0, 450.0]),
    ("ddc-325", "DDC 3.25", [4850.0, 5500.0, 530.0]),
    ("eheim-1046", "Eheim 1046", [3000.0, 3000.0, 120.0]),
    ("eheim-1048", "Eheim 1048", [3000.0, 3000.0, 150.0]),
    ("eheim-1250", "Eheim 1250", [3000.0, 3000.0, 200.0]),
];

const RESERVOIR_FIELDS: [&str; 3] = [
    "reservoir_total_volume",
    "reservoir_air_volume_full",
    "reservoir_air_volume_empty",
];
/// The reservoir types: total volume, air volume at the maximum fill level and air volume at
/// the minimum fill level, in ml.
const RESERVOIRS: [Preset; 11] = [
    ("ultitube-100", "ULTITUBE 100", [240.0, 50.0, 200.0]),
    ("ultitube-150", "ULTITUBE 150", [360.0, 50.0, 300.0]),
    ("ultitube-200", "ULTITUBE 200", [470.0, 70.0, 400.0]),
    ("aqualis-100", "aqualis 100", [100.0, 40.0, 80.0]),
    ("aqualis-150", "aqualis 150", [150.0, 50.0, 100.0]),
    ("aqualis-450", "aqualis 450", [450.0, 70.0, 350.0]),
    ("aqualis-880", "aqualis 880", [880.0, 100.0, 750.0]),
    ("aquainlet-100", "aquainlet 100", [100.0, 40.0, 80.0]),
    ("aquainlet-150", "aquainlet 150", [150.0, 50.0, 100.0]),
    ("aquatube", "aquatube", [160.0, 50.0, 100.0]),
    ("aquabox", "aquabox", [114.0, 50.0, 80.0]),
];

/// The presets and the fields of a type field.
fn presets_of(field: &str) -> (&'static [Preset], [&'static str; 3]) {
    if field == "pump_model" {
        (&PUMPS, PUMP_FIELDS)
    } else {
        (&RESERVOIRS, RESERVOIR_FIELDS)
    }
}

/// The pressure of a pump at `speed` in mbar: it grows with the square of the speed.
pub fn pump_pressure_at(speed: f64, max_speed: f64, max_pressure: f64) -> f64 {
    if max_speed > 0.0 {
        ((speed / max_speed).powi(2) * max_pressure).round()
    } else {
        0.0
    }
}

/// The speed in rpm at which a pump makes `pressure`.
pub fn pump_speed_for(pressure: f64, max_speed: f64, max_pressure: f64) -> f64 {
    if max_pressure > 0.0 {
        ((pressure / max_pressure).sqrt() * max_speed).round()
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Step {
    #[default]
    Reservoir,
    Pump,
    Flow,
    Pressure,
    Loop,
    Standby,
    Mode,
    Summary,
}

impl Step {
    pub const ALL: [Step; 8] = [
        Step::Reservoir,
        Step::Pump,
        Step::Flow,
        Step::Pressure,
        Step::Loop,
        Step::Standby,
        Step::Mode,
        Step::Summary,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Reservoir => "leakshield.settings.step.reservoir",
            Self::Pump => "leakshield.settings.step.pump",
            Self::Flow => "leakshield.settings.step.flow",
            Self::Pressure => "leakshield.settings.step.pressure",
            Self::Loop => "leakshield.settings.step.loop",
            Self::Standby => "leakshield.settings.step.standby",
            Self::Mode => "leakshield.settings.step.mode",
            Self::Summary => "leakshield.settings.step.summary",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SettingsUi {
    pub step: Step,
    /// The changes that wait for "Apply", by field id.
    pub pending: BTreeMap<String, FieldValue>,
    /// "Cancel" asks "Discard 2 changes?".
    pub confirm_discard: bool,
    pub show_unchanged: bool,
    /// The number of changes that the last "Apply" sent, until its result is known.
    pub applying: Option<usize>,
    /// The number of changes that the last "Apply" wrote.
    pub applied: Option<usize>,
    /// The user chose "Keep the current state" after a failed write.
    pub kept: bool,
}

/// The range of a number field, in the unit of the field.
pub fn range_of(field: &str) -> Option<(f64, f64)> {
    Some(match field {
        "reservoir_total_volume"
        | "reservoir_air_volume_full"
        | "reservoir_air_volume_empty"
        | "pump_max_speed"
        | "pump_start_speed" => (0.0, 32767.0),
        "pump_max_pressure" | "pump_start_flow" => (0.0, 3276.7),
        "shield_pressure_low_standby" => (30.0, 430.0),
        _ => return None,
    })
}

/// The unit of a field of the wizard.
fn unit(field: &str) -> &'static str {
    if field == "loop_height" {
        "cm"
    } else {
        unit_of(field)
    }
}

/// The fields of `range_of` without a scale. The daemon refuses a fraction for them.
fn is_whole(field: &str) -> bool {
    range_of(field) == Some((0.0, 32767.0))
}

fn parse(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse()
        .ok()
        .filter(|value: &f64| value.is_finite())
}

/// The control band of SHIELD in mbar, with the rules of the Leakshield for its values. A
/// setter puts its value in the range and moves the neighbors that are too close.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub min: f64,
    pub target: f64,
    pub max: f64,
    /// The minimum in standby. It is never above the minimum.
    pub standby: f64,
}

impl Band {
    pub const MIN_RANGE: (f64, f64) = (30.0, 430.0);
    pub const TARGET_RANGE: (f64, f64) = (50.0, 450.0);
    pub const MAX_RANGE: (f64, f64) = (80.0, 500.0);
    /// The least distance from the minimum to the target.
    pub const TARGET_DISTANCE: f64 = 20.0;
    /// The least distance from the target to the maximum.
    pub const MAX_DISTANCE: f64 = 30.0;

    /// A higher minimum raises the target. A lower minimum lowers the standby minimum.
    pub fn set_min(&mut self, value: f64) {
        self.min = value.clamp(Self::MIN_RANGE.0, Self::MIN_RANGE.1);
        if self.target < self.min + Self::TARGET_DISTANCE {
            self.set_target(self.min + Self::TARGET_DISTANCE);
        }
        if self.standby > self.min {
            self.standby = self.min;
        }
    }

    /// A lower target lowers the minimum. A higher target raises the maximum.
    pub fn set_target(&mut self, value: f64) {
        self.target = value.clamp(Self::TARGET_RANGE.0, Self::TARGET_RANGE.1);
        if self.target < self.min + Self::TARGET_DISTANCE {
            self.set_min(self.target - Self::TARGET_DISTANCE);
        }
        if self.max < self.target + Self::MAX_DISTANCE {
            self.set_max(self.target + Self::MAX_DISTANCE);
        }
    }

    /// A lower maximum lowers the target.
    pub fn set_max(&mut self, value: f64) {
        self.max = value.clamp(Self::MAX_RANGE.0, Self::MAX_RANGE.1);
        if self.max < self.target + Self::MAX_DISTANCE {
            self.set_target(self.max - Self::MAX_DISTANCE);
        }
    }

    pub fn set_standby(&mut self, value: f64) {
        self.standby = value.max(Self::MIN_RANGE.0).min(self.min);
    }

    /// The values that the wizard recommends for a pump: the minimum is the maximum pressure
    /// of the pump, or twice that with more than one pump. The target is 50 mbar above the
    /// pump pressure and the maximum 40 mbar above the target. The standby minimum is the
    /// pump pressure, 80 mbar at most.
    pub fn recommended(pump_max_pressure: f64, two_pumps: bool) -> Self {
        let pressure = if two_pumps {
            2.0 * pump_max_pressure
        } else {
            pump_max_pressure
        };
        let mut band = Self {
            min: Self::MIN_RANGE.0,
            target: Self::TARGET_RANGE.0,
            max: Self::MAX_RANGE.0,
            standby: Self::MIN_RANGE.0,
        };
        band.set_min(pressure);
        band.set_target(pressure + 50.0);
        band.set_max(band.target + 40.0);
        band.set_standby(pressure.min(80.0));
        band
    }
}

/// A difference of two values with one decimal, as the fields of the band hold it.
fn tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

impl SettingsUi {
    pub fn go(&mut self, step: Step) {
        self.step = step;
    }

    /// Alt+Right and Alt+Left.
    pub fn step_by(&mut self, delta: isize) {
        let index = self
            .step
            .index()
            .saturating_add_signed(delta)
            .min(Step::ALL.len() - 1);
        self.step = Step::ALL[index];
    }

    fn effective<'a>(&'a self, data: &'a LeakshieldData, field: &str) -> Option<&'a FieldValue> {
        self.pending
            .get(field)
            .or_else(|| data.field(field).map(|f| &f.value))
    }

    pub fn number(&self, data: &LeakshieldData, field: &str) -> Option<f64> {
        match self.effective(data, field)? {
            FieldValue::Number(value) => Some(*value),
            _ => None,
        }
    }

    fn text<'a>(&'a self, data: &'a LeakshieldData, field: &str) -> Option<&'a str> {
        match self.effective(data, field)? {
            FieldValue::Text(text) => Some(text),
            _ => None,
        }
    }

    fn list(&self, data: &LeakshieldData, field: &str) -> Vec<f64> {
        match self.effective(data, field) {
            Some(FieldValue::List(list)) => list.clone(),
            _ => Vec::new(),
        }
    }

    fn set(&mut self, data: &LeakshieldData, field: &str, value: FieldValue) {
        self.applied = None;
        // A value that equals the device value is no change.
        let same = data.field(field).is_some_and(|f| f.value == value);
        if same {
            self.pending.remove(field);
        } else {
            self.pending.insert(field.to_owned(), value);
        }
    }

    fn set_bit(&mut self, data: &LeakshieldData, field: &str, bit: u32, on: bool) {
        let mask = self.number(data, field).unwrap_or(0.0) as u32;
        let mask = if on { mask | bit } else { mask & !bit };
        self.set(data, field, FieldValue::Number(f64::from(mask)));
    }

    /// The control band of SHIELD: the device values with the pending changes.
    pub fn band(&self, data: &LeakshieldData) -> Option<Band> {
        let min = self.number(data, "shield_pressure_low")?;
        let working = self.number(data, "shield_pressure_working_range")?;
        let upper = self.number(data, "shield_pressure_max_range")?;
        Some(Band {
            min,
            target: tenth(min + working),
            max: tenth(min + working + upper),
            standby: self
                .number(data, "shield_pressure_low_standby")
                .unwrap_or(min),
        })
    }

    /// Put a band into the pending changes. The device holds the minimum, the distance from
    /// the minimum to the target and the distance from the target to the maximum.
    fn store_band(&mut self, data: &LeakshieldData, band: Band) {
        self.set(data, "shield_pressure_low", FieldValue::Number(band.min));
        self.set(
            data,
            "shield_pressure_working_range",
            FieldValue::Number(tenth(band.target - band.min)),
        );
        self.set(
            data,
            "shield_pressure_max_range",
            FieldValue::Number(tenth(band.max - band.target)),
        );
        if data.field("shield_pressure_low_standby").is_some() {
            self.set(
                data,
                "shield_pressure_low_standby",
                FieldValue::Number(band.standby),
            );
        }
    }

    fn two_pumps(&self, data: &LeakshieldData) -> bool {
        self.number(data, PUMP_FLAGS).unwrap_or(0.0) as u32 & BIT_TWO_PUMPS != 0
    }

    /// The maximum pressure of the pumps of the Loop in mbar: twice the pressure of one pump
    /// with more than one.
    pub fn pumps_pressure(&self, data: &LeakshieldData) -> Option<f64> {
        let pressure = self.number(data, "pump_max_pressure")?;
        Some(if self.two_pumps(data) {
            2.0 * pressure
        } else {
            pressure
        })
    }

    /// The recommended band for the pump of the wizard.
    pub fn recommended(&self, data: &LeakshieldData) -> Option<Band> {
        let pressure = self.number(data, "pump_max_pressure")?;
        Some(Band::recommended(pressure, self.two_pumps(data)))
    }

    fn store_recommended(&mut self, data: &LeakshieldData) {
        if let Some(band) = self.recommended(data) {
            self.store_band(data, band);
        }
    }

    /// An edit of one value of the band, in whole mbar. `which` is 0 for the minimum, 1 for
    /// the target and 2 for the maximum.
    fn set_band(&mut self, data: &LeakshieldData, which: usize, value: f64) {
        let Some(mut band) = self.band(data) else {
            return;
        };
        let value = value.round();
        match which {
            0 => band.set_min(value),
            1 => band.set_target(value),
            _ => band.set_max(value),
        }
        // Device values that break a rule get their correction with the first edit.
        band.set_min(band.min);
        band.set_target(band.target);
        band.set_max(band.max);
        self.store_band(data, band);
    }

    /// The time of a vacuum window as `HH:MM`.
    pub fn timer(&self, data: &LeakshieldData, window: usize) -> Option<String> {
        let hour = *self.list(data, "vacuum_timer_start_hour").get(window)?;
        let minute = *self.list(data, "vacuum_timer_start_minute").get(window)?;
        Some(format!("{:02}:{:02}", hour as u32, minute as u32))
    }

    fn set_timer(&mut self, data: &LeakshieldData, window: usize, text: &str) {
        let Some((hour, minute)) = text.trim().split_once(':') else {
            return;
        };
        let (Ok(hour), Ok(minute)) = (hour.parse::<u32>(), minute.parse::<u32>()) else {
            return;
        };
        if hour > 23 || minute > 59 {
            return;
        }
        let mut hours = self.list(data, "vacuum_timer_start_hour");
        let mut minutes = self.list(data, "vacuum_timer_start_minute");
        let mut end_hours = self.list(data, "vacuum_timer_end_hour");
        let mut end_minutes = self.list(data, "vacuum_timer_end_minute");
        if window >= hours.len() || window >= minutes.len() {
            return;
        }
        hours[window] = f64::from(hour);
        minutes[window] = f64::from(minute);
        // aquasuite writes the end one minute after the start.
        let end = (hour * 60 + minute + 1) % 1440;
        if window < end_hours.len() && window < end_minutes.len() {
            end_hours[window] = f64::from(end / 60);
            end_minutes[window] = f64::from(end % 60);
        }
        self.set(data, "vacuum_timer_start_hour", FieldValue::List(hours));
        self.set(data, "vacuum_timer_start_minute", FieldValue::List(minutes));
        self.set(data, "vacuum_timer_end_hour", FieldValue::List(end_hours));
        self.set(
            data,
            "vacuum_timer_end_minute",
            FieldValue::List(end_minutes),
        );
    }

    /// An edit of a control: a number or a time as text, a choice by identifier, a switch as
    /// "1" or "0". `key` is the identifier of the row.
    pub fn edit(&mut self, data: &LeakshieldData, key: &str, text: &str) {
        match key {
            "adaptive" => self.set_bit(data, SHIELD_FLAGS, BIT_ADAPTIVE, text == "1"),
            "vacuum_timer" => self.set_bit(data, SHIELD_FLAGS, BIT_TIMER, text == "1"),
            "standby_pressure" => {
                self.set_bit(data, SHIELD_FLAGS, BIT_STANDBY_PRESSURE, text == "1")
            }
            "pressure_values" => {
                self.set_bit(data, SHIELD_FLAGS, BIT_RECOMMENDED, text == "recommended");
                if text == "recommended" {
                    self.store_recommended(data);
                }
            }
            "use_recommended" => self.store_recommended(data),
            "band_min" | "band_target" | "band_max" => {
                if let Some(value) = parse(text) {
                    let which = ["band_min", "band_target", "band_max"]
                        .iter()
                        .position(|k| *k == key)
                        .unwrap_or(0);
                    self.set_band(data, which, value);
                }
            }
            "timer_1" | "timer_2" => self.set_timer(data, usize::from(key == "timer_2"), text),
            "default_mode" => self.set(data, key, FieldValue::Text(text.to_owned())),
            // A known type writes its numbers. "Custom" keeps them, for the user to edit.
            "reservoir_model" | "pump_model" => {
                self.set(data, key, FieldValue::Text(text.to_owned()));
                let (presets, fields) = presets_of(key);
                if let Some((_, _, values)) = presets.iter().find(|preset| preset.0 == text) {
                    for (field, value) in fields.iter().zip(values) {
                        self.set(data, field, FieldValue::Number(*value));
                    }
                }
            }
            "two_pumps" => self.set_bit(data, PUMP_FLAGS, BIT_TWO_PUMPS, text == "1"),
            "loop_height" => {
                if let Some(value) = parse(text) {
                    let (min, max) = LOOP_HEIGHT_RANGE;
                    self.set(data, key, FieldValue::Number(value.round().clamp(min, max)));
                }
            }
            field if range_of(field).is_some() => {
                if let Some(value) = parse(text) {
                    let value = if is_whole(field) {
                        value.round()
                    } else {
                        value
                    };
                    self.set(data, field, FieldValue::Number(value));
                }
            }
            _ => {}
        }
        // The recommended values follow the pump.
        let recommended =
            self.number(data, SHIELD_FLAGS).unwrap_or(0.0) as u32 & BIT_RECOMMENDED != 0;
        if recommended && matches!(key, "pump_model" | "pump_max_pressure" | "two_pumps") {
            self.store_recommended(data);
        }
    }

    /// "Use the current value" of the Standby step.
    pub fn use_current(&mut self, input: &ScreenInput<'_>, id: &str, field: &str) {
        let Some(data) = input.state.snapshot.leakshield_data.get(id) else {
            return;
        };
        let value = match field {
            "pump_start_speed" => loop_pump_rpm(input, id),
            _ => loop_of(input, id)
                .and_then(|l| l.flow.as_ref())
                .and_then(|flow| {
                    let state = input.state;
                    let sample = state.sensors.as_ref().or(state.last_sensors.as_ref())?;
                    sample.sensor_value(flow)
                }),
        };
        if let Some(value) = value {
            self.set(data, field, FieldValue::Number(value.round()));
        }
    }

    pub fn request_discard(&mut self) {
        if !self.pending.is_empty() {
            self.confirm_discard = true;
        }
    }

    pub fn discard(&mut self) {
        self.pending.clear();
        self.confirm_discard = false;
        self.applied = None;
        self.step = Step::Reservoir;
    }

    pub fn keep_editing(&mut self) {
        self.confirm_discard = false;
    }

    /// "Apply": one write of every change that the device accepts.
    pub fn apply(&mut self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        let device = Device::new(input, id);
        device.data?;
        if !device.can_write() {
            return None;
        }
        let fields: Vec<(String, FieldValue)> = self
            .pending
            .iter()
            .filter(|(field, _)| device.access(field).writable)
            .map(|(field, value)| (field.clone(), value.clone()))
            .collect();
        if fields.is_empty() || self.errors().next().is_some() {
            return None;
        }
        self.applying = Some(fields.len());
        self.kept = false;
        Some(fields_command(id, "wizard", fields))
    }

    /// The pending numbers that are out of range.
    fn errors(&self) -> impl Iterator<Item = &str> {
        self.pending.iter().filter_map(|(field, value)| {
            let (min, max) = range_of(field)?;
            match value {
                FieldValue::Number(n) if *n < min || *n > max => Some(field.as_str()),
                _ => None,
            }
        })
    }

    /// After a write: clear the changes that the device took.
    pub fn reconcile(&mut self, input: &ScreenInput<'_>, id: &str) {
        let Some(count) = self.applying else { return };
        match input.feedback.get(&format!("ls:{id}:wizard")) {
            Some(WriteOutcome::Applied { .. }) => {
                self.pending.clear();
                self.applying = None;
                self.applied = Some(count);
            }
            Some(outcome) if outcome.is_failure() => self.applying = None,
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Select {
        shown: String,
        options: Vec<(String, String, bool)>,
    },
    Number {
        text: String,
        unit: String,
    },
    Time {
        text: String,
    },
    Switch(bool),
    /// A read-only line.
    Info(String),
    /// The choices of a setting as radio rows: identifier, text, selected.
    Radio(Vec<(String, String, bool)>),
    /// The band diagram of the Pressure step.
    Band(BandDiagram),
}

/// The lines of the band diagram, as parts of its height from the top: the maximum, the
/// target and the minimum. The diagram is a scheme: the lines keep their place.
pub const BAND_LINES: [f32; 3] = [0.25, 0.5, 0.75];

/// One line of the band diagram.
#[derive(Clone, Debug, PartialEq)]
pub struct BandLine {
    /// The value of the wizard: "Maximum 465 mbar".
    pub label: String,
    /// The value that the Leakshield uses now, when it differs: "now 215 mbar".
    pub now: Option<String>,
}

/// The band diagram: what the Leakshield does above, in and below the control band.
#[derive(Clone, Debug, PartialEq)]
pub struct BandDiagram {
    /// The zone above the maximum, the zone between the limits and the zone below the minimum.
    pub zones: [String; 3],
    /// The maximum, the target and the minimum, from the top.
    pub lines: [BandLine; 3],
    /// The live pressure: its place from the top of the diagram (0 to 1) and its text.
    pub marker: Option<(f32, String)>,
}

/// The place of the live pressure in the band diagram, from the top (0 to 1). Between two
/// lines the place follows the pressure. Outside the band it stays in the zone.
pub fn marker_place(pressure: f64, min: f64, target: f64, max: f64) -> f32 {
    let [top, middle, bottom] = BAND_LINES.map(f64::from);
    // The part of `from..to` that `value` covers. A span without a width counts as 1 mbar.
    let part = |value: f64, from: f64, to: f64| (value - from) / (to - from).max(1.0);
    let place = if pressure >= max {
        top - (middle - top) * part(pressure, max, 2.0 * max - target)
    } else if pressure >= target {
        middle - (middle - top) * part(pressure, target, max)
    } else if pressure >= min {
        bottom - (bottom - middle) * part(pressure, min, target)
    } else {
        bottom + (bottom - middle) * part(min, pressure, pressure + target - min)
    };
    place.clamp(0.06, 0.94) as f32
}

/// A line under a setting: an information, or a warning.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub text: String,
    pub warning: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Loops,
    Fields(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub caption: Option<String>,
    /// "Verified field", or "Read-only".
    pub badge: String,
    pub control: Control,
    pub writable: bool,
    pub reason: Option<String>,
    pub error: Option<String>,
    pub changed: bool,
    pub link: Option<(String, Link)>,
    /// The button "Use the current value".
    pub use_current: Option<String>,
    /// A button of the row: its label and the key of its edit.
    pub action: Option<(String, String)>,
    /// Dim: the setting does not apply now.
    pub dim: bool,
    /// Another setting gives this value: the control takes no edit, and the badge says why.
    pub fixed: bool,
    pub note: Option<Note>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StepperItem {
    pub step: Step,
    pub label: String,
    pub done: bool,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiffRow {
    pub field: String,
    pub name: String,
    pub now: String,
    pub new: String,
    pub note: Option<String>,
    pub padlock: bool,
    /// The write counts in "Apply N changes".
    pub counted: bool,
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub rows: Vec<DiffRow>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub nothing: Option<String>,
    pub apply: String,
    pub apply_enabled: bool,
    pub apply_command: Option<Command>,
    pub applying: bool,
    pub success: Option<String>,
    pub failed: Option<FailedWrite>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FailedWrite {
    pub text: String,
    pub restore: String,
    pub restore_command: Option<Command>,
    pub keep: String,
}

/// One pending change: the setting, its value on the device and its new value.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingChange {
    pub name: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsModel {
    pub state: TabState,
    pub loading_text: String,
    pub stepper: Vec<StepperItem>,
    pub step: Step,
    pub title: String,
    pub counter: String,
    pub rows: Vec<Row>,
    pub summary: Option<Summary>,
    pub back: Option<(Step, String)>,
    pub next: Option<(Step, String)>,
    pub cancel: String,
    pub discard: Option<(String, String, String)>,
    pub pending_title: String,
    pub pending_note: String,
    /// The four steps of a write, as a numbered list under the note.
    pub pending_steps: Vec<String>,
    pub pending: Vec<PendingChange>,
    pub no_changes: Option<String>,
    pub unknown_firmware: bool,
    /// The warnings of the step, above its rows.
    pub notices: Vec<String>,
    pub write_line: Option<FeedbackLine>,
}

fn field_value_text(locale: Locale, field: &str, value: Option<&FieldValue>) -> String {
    let unit = unit(field);
    match value {
        Some(FieldValue::Number(n)) if unit.is_empty() => compact(locale, *n),
        Some(FieldValue::Number(n)) => format!("{} {unit}", compact(locale, *n)),
        Some(FieldValue::Text(text)) => text.clone(),
        Some(FieldValue::Flag(on)) => locale.tr(if *on {
            "leakshield.yes"
        } else {
            "leakshield.no"
        }),
        Some(FieldValue::List(list)) => format!(
            "[{}]",
            list.iter()
                .map(|v| compact(locale, *v))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Some(FieldValue::TextList(names)) => format!("[{}]", names.join(", ")),
        None => "--".to_owned(),
    }
}

impl SettingsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &SettingsUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let state = device.state();
        let empty = LeakshieldData::default();
        let data = device.data.unwrap_or(&empty);
        let unknown = device.unknown_firmware();
        let step = ui.step;
        let row_ctx = RowCtx {
            locale,
            device: &device,
            ui,
            data,
            unknown,
        };

        let rows = match step {
            Step::Reservoir => reservoir_rows(&row_ctx),
            Step::Pump => pump_rows(&row_ctx, input, id),
            Step::Flow => flow_rows(&row_ctx, input, id),
            Step::Pressure => pressure_rows(&row_ctx, input),
            Step::Loop => loop_rows(&row_ctx),
            Step::Standby => standby_rows(&row_ctx),
            Step::Mode => mode_rows(&row_ctx),
            Step::Summary => Vec::new(),
        };
        let summary = (step == Step::Summary).then(|| summary(input, ui, &device, data));
        let notices = match step {
            Step::Pump | Step::Pressure | Step::Mode => pump_pressure_warning(locale, ui, data)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };

        let neighbor = |delta: isize| {
            let index = step.index().checked_add_signed(delta)?;
            Step::ALL.get(index).map(|s| (*s, locale.tr(s.key())))
        };
        let changes = ui.pending.len();
        let pending: Vec<PendingChange> = ui
            .pending
            .iter()
            .map(|(field, value)| PendingChange {
                name: humanize(field),
                old: field_value_text(locale, field, data.field(field).map(|f| &f.value)),
                new: field_value_text(locale, field, Some(value)),
            })
            .collect();
        Self {
            state,
            loading_text: locale.tr("leakshield.settings.loading"),
            stepper: Step::ALL
                .iter()
                .map(|s| StepperItem {
                    step: *s,
                    label: locale.tr(s.key()),
                    done: s.index() < step.index(),
                    current: *s == step,
                })
                .collect(),
            step,
            title: locale.tr(step.key()),
            counter: locale.fmt(
                "leakshield.settings.step_of",
                &[
                    ("n", &(step.index() + 1).to_string()),
                    ("total", &Step::ALL.len().to_string()),
                ],
            ),
            rows,
            summary,
            back: neighbor(-1),
            next: neighbor(1),
            cancel: locale.tr("leakshield.cancel"),
            discard: ui.confirm_discard.then(|| {
                (
                    locale.plural("leakshield.settings.discard", changes),
                    locale.tr("leakshield.settings.keep_editing"),
                    locale.tr("leakshield.settings.discard_button"),
                )
            }),
            pending_title: locale.tr("leakshield.settings.pending"),
            pending_note: locale.tr("leakshield.settings.pending_note"),
            pending_steps: (1..=4)
                .map(|n| locale.tr(&format!("leakshield.settings.pending_step.{n}")))
                .collect(),
            no_changes: pending
                .is_empty()
                .then(|| locale.tr("leakshield.settings.no_change")),
            pending,
            unknown_firmware: unknown,
            notices,
            write_line: write_line(input, id, "wizard"),
        }
    }
}

struct RowCtx<'a> {
    locale: Locale,
    device: &'a Device<'a>,
    ui: &'a SettingsUi,
    data: &'a LeakshieldData,
    unknown: bool,
}

impl RowCtx<'_> {
    /// A row for one field. `control` builds its control from the effective value.
    fn row(&self, key: &str, field: &str, label_key: &str, control: Control) -> Row {
        let access = self.device.access(field);
        let writable = access.writable;
        let error = match (self.ui.pending.get(field), range_of(field)) {
            (Some(FieldValue::Number(n)), Some((min, max))) if *n < min || *n > max => {
                Some(self.locale.fmt(
                    "leakshield.range",
                    &[
                        ("min", &compact(self.locale, min)),
                        ("max", &compact(self.locale, max)),
                        ("unit", unit_of(field)),
                    ],
                ))
            }
            _ => None,
        };
        Row {
            key: key.to_owned(),
            label: self.locale.tr(label_key),
            caption: None,
            badge: self.locale.tr(if writable {
                "leakshield.settings.verified"
            } else {
                "leakshield.fields.read_only"
            }),
            control,
            writable,
            // The per-field reasons are hidden with an unknown firmware: one banner says it.
            reason: (!writable && !self.unknown && self.device.can_write())
                .then(|| self.device.reason_text(&access.reason)),
            error,
            changed: self.ui.pending.contains_key(field),
            link: (!writable && !self.unknown).then(|| {
                (
                    self.locale.tr("leakshield.open_fields"),
                    Link::Fields(field.to_owned()),
                )
            }),
            use_current: None,
            action: None,
            dim: false,
            fixed: false,
            note: None,
        }
    }

    fn number(&self, key: &str, field: &str, label_key: &str) -> Row {
        let value = self.ui.number(self.data, field);
        self.row(
            key,
            field,
            label_key,
            Control::Number {
                text: value.map_or_else(String::new, |v| plain(self.locale, v)),
                unit: crate::screens::format::unit_text(unit(field)).to_owned(),
            },
        )
    }

    fn caption(mut row: Row, text: Option<String>) -> Row {
        row.caption = text;
        row
    }

    /// True when the pending value of a flags field differs from the device in `bits`.
    fn bits_changed(&self, field: &str, bits: u32) -> bool {
        let device = self.data.number(field).unwrap_or(0.0) as u32;
        let new = self.ui.number(self.data, field).unwrap_or(0.0) as u32;
        (device ^ new) & bits != 0
    }

    fn switch(&self, key: &str, field: &str, bit: u32, label_key: &str) -> Row {
        let on = self.ui.number(self.data, field).unwrap_or(0.0) as u32 & bit != 0;
        let mut row = self.row(key, field, label_key, Control::Switch(on));
        row.changed = self.bits_changed(field, bit);
        row
    }

    /// The select of a type field: the known types, then "Custom".
    fn select(&self, key: &str, field: &str, label_key: &str) -> Row {
        let current = self
            .ui
            .text(self.data, field)
            .unwrap_or("custom")
            .to_owned();
        let (presets, _) = presets_of(field);
        let mut options: Vec<(String, String, bool)> = presets
            .iter()
            .map(|(id, name, _)| ((*id).to_owned(), (*name).to_owned(), *id == current))
            .collect();
        options.push((
            "custom".to_owned(),
            self.locale.tr("leakshield.settings.custom"),
            current == "custom",
        ));
        // A value that is no known type, as "undefined" after a factory reset.
        if !options.iter().any(|(_, _, selected)| *selected) {
            options.insert(0, (current.clone(), humanize(&current), true));
        }
        let shown = options
            .iter()
            .find(|(_, _, selected)| *selected)
            .map_or_else(String::new, |(_, text, _)| text.clone());
        self.row(key, field, label_key, Control::Select { shown, options })
    }

    /// The number rows of a type field. With a known type they take no edit.
    fn preset_numbers(&self, field: &str, rows: &[(&str, &str)]) -> Vec<Row> {
        let (presets, _) = presets_of(field);
        let current = self.ui.text(self.data, field).unwrap_or("custom");
        let preset = presets.iter().any(|preset| preset.0 == current);
        rows.iter()
            .map(|(field, label_key)| {
                let mut row = self.number(field, field, label_key);
                if preset && row.writable {
                    row.fixed = true;
                    row.badge = self.locale.tr("leakshield.settings.preset_value");
                }
                row
            })
            .collect()
    }
}

/// The warning of a pump that the Leakshield cannot fully protect against.
fn pump_pressure_warning(locale: Locale, ui: &SettingsUi, data: &LeakshieldData) -> Option<String> {
    let pressure = ui.pumps_pressure(data)?;
    (pressure > PROTECT_LIMIT).then(|| {
        locale.fmt(
            "leakshield.settings.warn_pump_pressure",
            &[
                ("value", &compact(locale, pressure)),
                ("limit", &compact(locale, PROTECT_LIMIT)),
            ],
        )
    })
}

fn reservoir_rows(ctx: &RowCtx<'_>) -> Vec<Row> {
    let mut rows = vec![RowCtx::caption(
        ctx.select(
            "reservoir_model",
            "reservoir_model",
            "leakshield.settings.reservoir",
        ),
        Some(ctx.locale.tr("leakshield.settings.reservoir_text")),
    )];
    rows.extend(ctx.preset_numbers(
        "reservoir_model",
        &[
            ("reservoir_total_volume", "leakshield.settings.volume_sum"),
            ("reservoir_air_volume_full", "leakshield.settings.air_full"),
            (
                "reservoir_air_volume_empty",
                "leakshield.settings.air_empty",
            ),
        ],
    ));
    rows
}

/// The Loop that this Leakshield belongs to.
fn loop_of<'a>(input: &'a ScreenInput<'_>, id: &str) -> Option<&'a crate::model::LoopInfo> {
    input
        .state
        .snapshot
        .loops
        .iter()
        .find(|l| l.leakshield.as_deref() == Some(id))
}

/// The speed of the first Pump channel of the Loop in rpm.
fn loop_pump_rpm(input: &ScreenInput<'_>, id: &str) -> Option<f64> {
    let fan = input
        .state
        .fan_sensor(loop_of(input, id)?.pump_channels.first()?)?;
    let sample = input
        .state
        .sensors
        .as_ref()
        .or(input.state.last_sensors.as_ref())?;
    sample.sensor_value(&fan)
}

fn pump_rows(ctx: &RowCtx<'_>, input: &ScreenInput<'_>, id: &str) -> Vec<Row> {
    let locale = ctx.locale;
    let mut rows = vec![RowCtx::caption(
        ctx.select("pump_model", "pump_model", "leakshield.settings.pump"),
        Some(locale.tr("leakshield.settings.pump_text")),
    )];
    rows.extend(ctx.preset_numbers(
        "pump_model",
        &[
            ("pump_max_pressure", "leakshield.settings.max_pressure"),
            ("pump_max_speed", "leakshield.settings.max_speed"),
        ],
    ));
    let mut two = RowCtx::caption(
        ctx.switch(
            "two_pumps",
            PUMP_FLAGS,
            BIT_TWO_PUMPS,
            "leakshield.settings.two_pumps",
        ),
        Some(locale.tr("leakshield.settings.two_pumps_text")),
    );
    if two.control == Control::Switch(true) {
        two.note = Some(Note {
            text: locale.tr("leakshield.settings.two_pumps_warning"),
            warning: true,
        });
    }
    rows.push(two);

    // The helper: the pressure of the pump at its speed now, and the speed at which it
    // reaches the highest target of the Leakshield.
    let max_speed = ctx.ui.number(ctx.data, "pump_max_speed");
    let pressure = ctx.ui.pumps_pressure(ctx.data);
    let now = match (loop_pump_rpm(input, id), max_speed, pressure) {
        (Some(speed), Some(max_speed), Some(pressure)) if max_speed > 0.0 => locale.fmt(
            "leakshield.settings.pump_now_text",
            &[
                ("speed", &compact(locale, speed.round())),
                (
                    "pressure",
                    &compact(locale, pump_pressure_at(speed, max_speed, pressure)),
                ),
            ],
        ),
        _ => locale.tr("leakshield.settings.pump_now_none"),
    };
    let mut row = info_row(
        ctx,
        "pump_pressure_now",
        "leakshield.settings.pump_now",
        now,
    );
    row.link = None;
    rows.push(row);
    if let (Some(max_speed), Some(pressure)) = (max_speed, pressure)
        && max_speed > 0.0
        && pressure > 0.0
    {
        let highest = Band::TARGET_RANGE.1;
        let speed = pump_speed_for(highest, max_speed, pressure);
        let text = if speed < max_speed {
            locale.fmt(
                "leakshield.settings.pump_limit_text",
                &[
                    ("speed", &compact(locale, speed)),
                    ("pressure", &compact(locale, highest)),
                ],
            )
        } else {
            locale.fmt(
                "leakshield.settings.pump_limit_none",
                &[("pressure", &compact(locale, highest))],
            )
        };
        let mut row = info_row(
            ctx,
            "pump_speed_limit",
            "leakshield.settings.pump_limit",
            text,
        );
        row.link = None;
        rows.push(row);
    }

    let source = loop_of(input, id)
        .and_then(|l| {
            let channel = l.pump_channels.first()?;
            let name = input.state.snapshot.channels.get(channel)?.name.clone();
            Some(locale.fmt(
                "leakshield.settings.pump_source",
                &[("loop", &l.name), ("channel", &name)],
            ))
        })
        .unwrap_or_else(|| locale.tr("leakshield.settings.no_source"));
    rows.push(info_row(
        ctx,
        "pump_source",
        "leakshield.settings.speed_source",
        source,
    ));
    rows
}

fn info_row(ctx: &RowCtx<'_>, key: &str, label_key: &str, text: String) -> Row {
    Row {
        key: key.to_owned(),
        label: ctx.locale.tr(label_key),
        caption: None,
        badge: String::new(),
        control: Control::Info(text),
        writable: false,
        reason: None,
        error: None,
        changed: false,
        link: Some((ctx.locale.tr("leakshield.settings.open_loop"), Link::Loops)),
        use_current: None,
        action: None,
        dim: false,
        fixed: false,
        note: None,
    }
}

fn flow_rows(ctx: &RowCtx<'_>, input: &ScreenInput<'_>, id: &str) -> Vec<Row> {
    let locale = ctx.locale;
    let text = input
        .state
        .snapshot
        .loops
        .iter()
        .find(|l| l.leakshield.as_deref() == Some(id))
        .and_then(|l| {
            let flow = l.flow.as_ref()?;
            let name = input.state.snapshot.sensors.get(flow)?.name.clone();
            let sample = input
                .state
                .sensors
                .as_ref()
                .or(input.state.last_sensors.as_ref());
            let value = sample.and_then(|s| s.sensor_value(flow));
            Some(locale.fmt(
                "leakshield.settings.flow_source",
                &[
                    ("loop", &l.name),
                    ("name", &name),
                    (
                        "value",
                        &crate::screens::format::optional_value(locale, value, "l/h"),
                    ),
                ],
            ))
        })
        .unwrap_or_else(|| locale.tr("leakshield.settings.flow_none"));
    vec![info_row(
        ctx,
        "flow_source",
        "leakshield.settings.flow_label",
        text,
    )]
}

/// The band diagram: the values of the wizard on the lines, and the live values of the
/// Leakshield next to them.
fn band_diagram(ctx: &RowCtx<'_>, input: &ScreenInput<'_>) -> (BandDiagram, Option<String>) {
    let locale = ctx.locale;
    let ready = ctx.device.can_write();
    let live = |name: &str| ctx.device.sensor(input, name).filter(|_| ready);
    let set = ctx.ui.band(ctx.data);
    let now = match (
        live("pressure_low"),
        live("pressure_setpoint"),
        live("pressure_high"),
    ) {
        (Some(min), Some(target), Some(max)) => Some((min, target, max)),
        _ => None,
    };
    let line = |name: &str, index: usize| {
        let value = set.map(|band| [band.max, band.target, band.min][index]);
        BandLine {
            label: locale.fmt(
                &format!("leakshield.settings.diagram.{name}"),
                &[(
                    "value",
                    &value.map_or_else(|| "--".to_owned(), |v| compact(locale, v)),
                )],
            ),
            // A difference under half a mbar is the rounding of the device.
            now: now
                .map(|(min, target, max)| [max, target, min][index])
                .filter(|now| value.is_none_or(|value| (now - value).abs() >= 0.5))
                .map(|now| {
                    locale.fmt(
                        "leakshield.settings.diagram.now_value",
                        &[("value", &compact(locale, now))],
                    )
                }),
        }
    };
    let marker = live("pressure").and_then(|pressure| {
        let (min, target, max) = now.or(set.map(|band| (band.min, band.target, band.max)))?;
        Some((
            marker_place(pressure, min, target, max),
            locale.fmt(
                "leakshield.settings.diagram.marker",
                &[("value", &compact(locale, pressure.round()))],
            ),
        ))
    });
    let now_line = now.map(|(min, target, max)| {
        locale.fmt(
            "leakshield.settings.diagram.now",
            &[
                ("min", &compact(locale, min)),
                ("target", &compact(locale, target)),
                ("max", &compact(locale, max)),
            ],
        )
    });
    let diagram = BandDiagram {
        zones: ["above", "inside", "below"]
            .map(|zone| locale.tr(&format!("leakshield.settings.diagram.{zone}"))),
        lines: [line("maximum", 0), line("target", 1), line("minimum", 2)],
        marker,
    };
    (diagram, now_line)
}

/// The information line of a value of the band that is at a limit.
fn band_note(locale: Locale, key: &str, band: Band) -> Option<Note> {
    let name = match key {
        "band_max" if band.max <= band.target + Band::MAX_DISTANCE => "max_close",
        "band_target" if band.target <= band.min + Band::TARGET_DISTANCE => "target_close",
        "band_min" if band.min <= Band::MIN_RANGE.0 => "min_lowest",
        "band_min" if band.min >= Band::MIN_RANGE.1 => "min_highest",
        _ => return None,
    };
    Some(Note {
        text: locale.tr(&format!("leakshield.settings.band_note.{name}")),
        warning: false,
    })
}

fn pressure_rows(ctx: &RowCtx<'_>, input: &ScreenInput<'_>) -> Vec<Row> {
    let locale = ctx.locale;
    let flags = ctx.ui.number(ctx.data, SHIELD_FLAGS).unwrap_or(0.0) as u32;
    let own = flags & BIT_RECOMMENDED == 0;

    // The two choices.
    let mut values = ctx.row(
        "pressure_values",
        SHIELD_FLAGS,
        "leakshield.settings.values",
        Control::Radio(
            [("recommended", !own), ("own", own)]
                .iter()
                .map(|(choice, selected)| {
                    (
                        (*choice).to_owned(),
                        locale.tr(&format!("leakshield.settings.values_{choice}")),
                        *selected,
                    )
                })
                .collect(),
        ),
    );
    values.caption = Some(locale.tr("leakshield.settings.values_text"));
    values.changed = ctx.bits_changed(SHIELD_FLAGS, BIT_RECOMMENDED);
    let band = ctx.ui.band(ctx.data);
    let recommended = ctx.ui.recommended(ctx.data);
    let differs = match (band, recommended) {
        (Some(band), Some(recommended)) => {
            (band.min, band.target, band.max)
                != (recommended.min, recommended.target, recommended.max)
        }
        _ => false,
    };
    // The recommended values as numbers: with own values as the reference, and with the
    // recommended choice when the Leakshield holds other values.
    if own || differs {
        let number =
            |value: Option<f64>| value.map_or_else(|| "--".to_owned(), |v| compact(locale, v));
        values.note = Some(Note {
            text: locale.fmt(
                if own {
                    "leakshield.settings.values_own_warning"
                } else {
                    "leakshield.settings.values_differ"
                },
                &[
                    ("min", &number(recommended.map(|b| b.min))),
                    ("target", &number(recommended.map(|b| b.target))),
                    ("max", &number(recommended.map(|b| b.max))),
                ],
            ),
            warning: own,
        });
        values.action = Some((
            locale.tr("leakshield.settings.use_recommended"),
            "use_recommended".to_owned(),
        ));
    }
    let mut rows = vec![values];

    // The diagram, then the three values from its top line to its bottom line.
    let (diagram, now_line) = band_diagram(ctx, input);
    rows.push(Row {
        key: "band".to_owned(),
        label: locale.tr("leakshield.settings.diagram.title"),
        caption: now_line,
        badge: String::new(),
        control: Control::Band(diagram),
        writable: false,
        reason: None,
        error: None,
        changed: false,
        link: None,
        use_current: None,
        action: None,
        dim: false,
        fixed: false,
        note: None,
    });
    for (key, field, value, range) in [
        (
            "band_max",
            "shield_pressure_max_range",
            band.map(|b| b.max),
            Band::MAX_RANGE,
        ),
        (
            "band_target",
            "shield_pressure_working_range",
            band.map(|b| b.target),
            Band::TARGET_RANGE,
        ),
        (
            "band_min",
            "shield_pressure_low",
            band.map(|b| b.min),
            Band::MIN_RANGE,
        ),
    ] {
        let mut row = ctx.row(
            key,
            field,
            &format!("leakshield.settings.{key}"),
            Control::Number {
                text: value.map_or_else(String::new, |v| plain(locale, v)),
                unit: "mbar".to_owned(),
            },
        );
        // The meaning of the value, then its range: the wizard corrects a value outside it.
        row.caption = Some(format!(
            "{} {}",
            locale.tr(&format!("leakshield.settings.{key}_text")),
            range_text(locale, range, "mbar")
        ));
        row.note = band.and_then(|band| band_note(locale, key, band));
        // The recommended values are not for the user to edit.
        if !own && row.writable {
            row.fixed = true;
            row.badge = locale.tr("leakshield.settings.recommended_value");
        }
        rows.push(row);
    }
    rows.push(RowCtx::caption(
        ctx.switch(
            "adaptive",
            SHIELD_FLAGS,
            BIT_ADAPTIVE,
            "leakshield.settings.adaptive",
        ),
        Some(locale.tr("leakshield.settings.adaptive_text")),
    ));
    let timer = ctx.ui.number(ctx.data, SHIELD_FLAGS).unwrap_or(0.0) as u32 & BIT_TIMER != 0;
    rows.push(RowCtx::caption(
        ctx.switch(
            "vacuum_timer",
            SHIELD_FLAGS,
            BIT_TIMER,
            "leakshield.settings.timer",
        ),
        Some(locale.tr("leakshield.settings.timer_text")),
    ));
    for (index, key) in ["timer_1", "timer_2"].iter().enumerate() {
        let mut row = ctx.row(
            key,
            "vacuum_timer_start_hour",
            if index == 0 {
                "leakshield.settings.timer_1"
            } else {
                "leakshield.settings.timer_2"
            },
            Control::Time {
                text: ctx.ui.timer(ctx.data, index).unwrap_or_default(),
            },
        );
        row.dim = !timer;
        row.changed = ctx.ui.pending.contains_key("vacuum_timer_start_hour");
        rows.push(row);
    }
    rows
}

/// The text of a range with its unit: "0 to 200 cm."
fn range_text(locale: Locale, range: (f64, f64), unit: &str) -> String {
    locale.fmt(
        "leakshield.range",
        &[
            ("min", &compact(locale, range.0)),
            ("max", &compact(locale, range.1)),
            ("unit", unit),
        ],
    )
}

fn loop_rows(ctx: &RowCtx<'_>) -> Vec<Row> {
    let locale = ctx.locale;
    let mut height = ctx.number(
        "loop_height",
        "loop_height",
        "leakshield.settings.loop_height",
    );
    height.caption = Some(format!(
        "{} {}",
        locale.tr("leakshield.settings.loop_height_text"),
        range_text(locale, LOOP_HEIGHT_RANGE, "cm")
    ));
    if ctx
        .ui
        .number(ctx.data, "loop_height")
        .is_some_and(|height| height > LOOP_HEIGHT_WARNING)
    {
        height.note = Some(Note {
            text: locale.tr("leakshield.settings.loop_height_warning"),
            warning: true,
        });
    }
    vec![height]
}

fn standby_rows(ctx: &RowCtx<'_>) -> Vec<Row> {
    let locale = ctx.locale;
    let mut speed = ctx.number(
        "pump_start_speed",
        "pump_start_speed",
        "leakshield.settings.default_speed",
    );
    speed.use_current = Some(locale.tr("leakshield.settings.use_current"));
    speed.caption = Some(locale.tr("leakshield.settings.standby_text"));
    let mut flow = ctx.number(
        "pump_start_flow",
        "pump_start_flow",
        "leakshield.settings.default_flow",
    );
    flow.use_current = Some(locale.tr("leakshield.settings.use_current"));
    let reduced = ctx.switch(
        "standby_pressure",
        SHIELD_FLAGS,
        BIT_STANDBY_PRESSURE,
        "leakshield.settings.reduced_vacuum",
    );
    let on = matches!(reduced.control, Control::Switch(true));
    let mut value = ctx.number(
        "shield_pressure_low_standby",
        "shield_pressure_low_standby",
        "leakshield.settings.reduced_value",
    );
    value.dim = !on;
    vec![speed, flow, reduced, value]
}

fn mode_rows(ctx: &RowCtx<'_>) -> Vec<Row> {
    let locale = ctx.locale;
    let current = ctx
        .ui
        .text(ctx.data, "default_mode")
        .unwrap_or("shield")
        .to_owned();
    let options = ["shield", "monitor", "release"]
        .iter()
        .map(|mode| {
            (
                (*mode).to_owned(),
                locale.tr(&format!("leakshield.overview.mode_caption.{mode}")),
                *mode == current,
            )
        })
        .collect();
    vec![ctx.row(
        "default_mode",
        "default_mode",
        "leakshield.settings.mode",
        Control::Radio(options),
    )]
}

fn summary(
    input: &ScreenInput<'_>,
    ui: &SettingsUi,
    device: &Device<'_>,
    data: &LeakshieldData,
) -> Summary {
    let locale = input.locale;
    let applying = matches!(
        input.feedback.get(&format!("ls:{}:wizard", device.id)),
        Some(WriteOutcome::Pending)
    );
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (field, value) in &ui.pending {
        let access = device.access(field);
        let out_of_range = matches!(
            (value, range_of(field)),
            (FieldValue::Number(n), Some((min, max))) if *n < min || *n > max
        );
        if out_of_range {
            errors.push(locale.fmt(
                "leakshield.settings.error_row",
                &[("name", &humanize(field))],
            ));
        }
        rows.push(DiffRow {
            field: field.clone(),
            name: humanize(field),
            now: field_value_text(locale, field, data.field(field).map(|f| &f.value)),
            new: field_value_text(locale, field, Some(value)),
            note: (!access.writable).then(|| device.reason_text(&access.reason)),
            padlock: !access.writable,
            counted: access.writable,
            status: applying.then(|| locale.tr("feedback.pending")),
        });
    }
    let counted = rows.iter().filter(|r| r.counted).count();
    let mut warnings = Vec::new();
    if ui.pending.contains_key("default_mode") && ui.text(data, "default_mode") == Some("monitor") {
        warnings.push(locale.tr("leakshield.settings.warn_monitor"));
    }
    if (ui.pending.contains_key("pump_max_pressure") || ui.pending.contains_key(PUMP_FLAGS))
        && let Some(text) = pump_pressure_warning(locale, ui, data)
    {
        warnings.push(text);
    }
    let apply_command = {
        let mut copy = ui.clone();
        copy.apply(input, device.id)
    };
    let failure: Option<&WriteFailure> = input
        .state
        .write_failure
        .as_ref()
        .filter(|f| f.leakshield == device.id && !ui.kept);
    Summary {
        nothing: ui
            .pending
            .is_empty()
            .then(|| locale.tr("leakshield.settings.nothing")),
        apply: if applying {
            locale.plural("leakshield.settings.applying", counted)
        } else {
            locale.plural("leakshield.settings.apply", counted)
        },
        apply_enabled: apply_command.is_some() && !applying,
        apply_command,
        applying,
        success: ui
            .applied
            .map(|n| locale.plural("leakshield.settings.applied", n)),
        failed: failure.map(|failure| FailedWrite {
            text: locale.tr("leakshield.settings.write_failed"),
            restore: locale.tr("leakshield.settings.restore"),
            restore_command: Some(Command::RestoreBackup {
                leakshield: device.id.to_owned(),
                backup: failure.backup.clone(),
            }),
            keep: locale.tr("leakshield.settings.keep_state"),
        }),
        rows,
        warnings,
        errors,
    }
}
