//! The Settings tab, a wizard with seven steps (`docs/ui/screens/leakshield-settings.md`).
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
const BIT_STANDBY_PRESSURE: u32 = 32;
/// The pump pressure above which the Leakshield can only work as a leak monitor, in mbar.
const MONITOR_ONLY_PRESSURE: f64 = 470.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Step {
    #[default]
    Reservoir,
    Pump,
    Flow,
    Pressure,
    Standby,
    Mode,
    Summary,
}

impl Step {
    pub const ALL: [Step; 7] = [
        Step::Reservoir,
        Step::Pump,
        Step::Flow,
        Step::Pressure,
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
        "shield_pressure_low" | "shield_pressure_low_standby" => (30.0, 430.0),
        "shield_pressure_working_range" => (0.0, 420.0),
        "shield_pressure_max_range" => (0.0, 450.0),
        _ => return None,
    })
}

/// The fields of `range_of` without a scale. The daemon refuses a fraction for them.
fn is_whole(field: &str) -> bool {
    range_of(field) == Some((0.0, 32767.0))
}

fn parse(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok()
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

    /// The minimum, the target and the maximum of the expert values.
    pub fn expert(&self, data: &LeakshieldData) -> Option<(f64, f64, f64)> {
        let low = self.number(data, "shield_pressure_low")?;
        let working = self.number(data, "shield_pressure_working_range")?;
        let upper = self.number(data, "shield_pressure_max_range")?;
        Some((low, low + working, low + working + upper))
    }

    fn set_expert(&mut self, data: &LeakshieldData, which: usize, value: f64) {
        let Some((mut min, mut target, mut max)) = self.expert(data) else {
            return;
        };
        match which {
            0 => min = value,
            1 => target = value,
            _ => max = value,
        }
        self.set(data, "shield_pressure_low", FieldValue::Number(min));
        self.set(
            data,
            "shield_pressure_working_range",
            FieldValue::Number(target - min),
        );
        self.set(
            data,
            "shield_pressure_max_range",
            FieldValue::Number(max - target),
        );
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
            "expert_min" | "expert_target" | "expert_max" => {
                if let Some(value) = parse(text) {
                    let which = ["expert_min", "expert_target", "expert_max"]
                        .iter()
                        .position(|k| *k == key)
                        .unwrap_or(0);
                    self.set_expert(data, which, value);
                }
            }
            "timer_1" | "timer_2" => self.set_timer(data, usize::from(key == "timer_2"), text),
            "reservoir_model" | "pump_model" | "default_mode" => {
                self.set(data, key, FieldValue::Text(text.to_owned()));
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
    }

    /// "Use the current value" of the Standby step.
    pub fn use_current(&mut self, input: &ScreenInput<'_>, id: &str, field: &str) {
        let Some(data) = input.state.snapshot.leakshield_data.get(id) else {
            return;
        };
        let sample = input
            .state
            .sensors
            .as_ref()
            .or(input.state.last_sensors.as_ref());
        let loop_info = input
            .state
            .snapshot
            .loops
            .iter()
            .find(|l| l.leakshield.as_deref() == Some(id));
        let value = match field {
            "pump_start_speed" => loop_info
                .and_then(|l| l.pump_channels.first())
                .and_then(|channel| input.state.fan_sensor(channel))
                .and_then(|fan| sample.and_then(|s| s.sensor_value(&fan))),
            _ => loop_info
                .and_then(|l| l.flow.as_ref())
                .and_then(|flow| sample.and_then(|s| s.sensor_value(flow))),
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
    /// The three Modes: identifier, text, selected.
    Radio(Vec<(String, String, bool)>),
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
    /// Dim: the setting does not apply now.
    pub dim: bool,
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
    pub write_line: Option<FeedbackLine>,
}

fn field_value_text(locale: Locale, field: &str, value: Option<&FieldValue>) -> String {
    let unit = unit_of(field);
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
            Step::Pressure => pressure_rows(&row_ctx),
            Step::Standby => standby_rows(&row_ctx),
            Step::Mode => mode_rows(&row_ctx),
            Step::Summary => Vec::new(),
        };
        let summary = (step == Step::Summary).then(|| summary(input, ui, &device, data));

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
            dim: false,
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
                unit: crate::screens::format::unit_text(unit_of(field)).to_owned(),
            },
        )
    }

    fn caption(mut row: Row, text: Option<String>) -> Row {
        row.caption = text;
        row
    }

    fn switch(&self, key: &str, field: &str, bit: u32, label_key: &str) -> Row {
        let on = self.ui.number(self.data, field).unwrap_or(0.0) as u32 & bit != 0;
        let mut row = self.row(key, field, label_key, Control::Switch(on));
        row.changed = self.ui.pending.contains_key(field);
        row
    }

    fn select(&self, key: &str, field: &str, label_key: &str) -> Row {
        let current = self
            .ui
            .text(self.data, field)
            .unwrap_or("custom")
            .to_owned();
        let mut options = vec![(
            "custom".to_owned(),
            self.locale.tr("leakshield.settings.custom"),
            current == "custom",
        )];
        if current != "custom" {
            options.insert(0, (current.clone(), humanize(&current), true));
        }
        let shown = options
            .iter()
            .find(|(_, _, selected)| *selected)
            .map_or_else(String::new, |(_, text, _)| text.clone());
        self.row(key, field, label_key, Control::Select { shown, options })
    }
}

fn reservoir_rows(ctx: &RowCtx<'_>) -> Vec<Row> {
    let mut rows = vec![ctx.select(
        "reservoir_model",
        "reservoir_model",
        "leakshield.settings.reservoir",
    )];
    if ctx.ui.text(ctx.data, "reservoir_model").unwrap_or("custom") == "custom" {
        rows.push(ctx.number(
            "reservoir_total_volume",
            "reservoir_total_volume",
            "leakshield.settings.volume_sum",
        ));
        rows.push(ctx.number(
            "reservoir_air_volume_full",
            "reservoir_air_volume_full",
            "leakshield.settings.air_full",
        ));
        rows.push(ctx.number(
            "reservoir_air_volume_empty",
            "reservoir_air_volume_empty",
            "leakshield.settings.air_empty",
        ));
    }
    rows
}

fn pump_rows(ctx: &RowCtx<'_>, input: &ScreenInput<'_>, id: &str) -> Vec<Row> {
    let locale = ctx.locale;
    let mut rows = vec![ctx.select("pump_model", "pump_model", "leakshield.settings.pump")];
    if ctx.ui.text(ctx.data, "pump_model").unwrap_or("custom") == "custom" {
        rows.push(ctx.number(
            "pump_max_pressure",
            "pump_max_pressure",
            "leakshield.settings.max_pressure",
        ));
        rows.push(ctx.number(
            "pump_max_speed",
            "pump_max_speed",
            "leakshield.settings.max_speed",
        ));
    }
    let source = input
        .state
        .snapshot
        .loops
        .iter()
        .find(|l| l.leakshield.as_deref() == Some(id))
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
        dim: false,
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

fn pressure_rows(ctx: &RowCtx<'_>) -> Vec<Row> {
    let locale = ctx.locale;
    let adaptive = ctx.ui.number(ctx.data, SHIELD_FLAGS).unwrap_or(0.0) as u32 & BIT_ADAPTIVE != 0;
    let mut rows = vec![RowCtx::caption(
        ctx.switch(
            "adaptive",
            SHIELD_FLAGS,
            BIT_ADAPTIVE,
            "leakshield.settings.adaptive",
        ),
        Some(locale.tr("leakshield.settings.adaptive_text")),
    )];
    let expert = ctx.ui.expert(ctx.data);
    for (index, (key, field, label)) in [
        (
            "expert_min",
            "shield_pressure_low",
            "leakshield.settings.expert_min",
        ),
        (
            "expert_target",
            "shield_pressure_working_range",
            "leakshield.settings.expert_target",
        ),
        (
            "expert_max",
            "shield_pressure_max_range",
            "leakshield.settings.expert_max",
        ),
    ]
    .iter()
    .enumerate()
    {
        let value = expert.map(|(min, target, max)| [min, target, max][index]);
        let mut row = ctx.row(
            key,
            field,
            label,
            Control::Number {
                text: value.map_or_else(String::new, |v| plain(locale, v)),
                unit: "mbar".to_owned(),
            },
        );
        row.dim = adaptive;
        if index == 0 {
            row.caption = Some(locale.tr("leakshield.settings.expert_text"));
        }
        rows.push(row);
    }
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
    if ui
        .pending
        .get("pump_max_pressure")
        .is_some_and(|v| matches!(v, FieldValue::Number(n) if *n > MONITOR_ONLY_PRESSURE))
    {
        warnings.push(locale.tr("leakshield.settings.warn_pressure"));
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
