//! The Sensors screen (`docs/ui/screens/sensors.md`): every sensor by device, with rename,
//! hide and the Derived sensor editor.

use std::collections::BTreeSet;

use nicecooling_core::derived::{DerivedKind, compute};

use crate::command::Command;
use crate::model::{AppState, Control, DerivedDef, Link, SensorInfo, Severity};
use crate::screens::format::{optional_value, unit_text, value_text};
use crate::screens::{ScreenInput, WriteOutcome};

pub const FUNCTIONS: [&str; 3] = ["maximum", "mean", "difference"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensorUse {
    /// The Curve of a Channel reads the sensor.
    Curve {
        channel: String,
        name: String,
    },
    Loop {
        id: String,
        name: String,
    },
    Derived {
        id: String,
        name: String,
    },
}

/// What reads a sensor: Curves, Loops and Derived sensors.
pub fn sensor_users(state: &AppState, sensor: &str) -> Vec<SensorUse> {
    let snapshot = &state.snapshot;
    let mut users = Vec::new();
    for channel in snapshot.channels.values() {
        if channel.control == Control::Curve && channel.curve.input.as_deref() == Some(sensor) {
            users.push(SensorUse::Curve {
                channel: channel.id.clone(),
                name: channel.name.clone(),
            });
        }
    }
    for info in &snapshot.loops {
        let reads = |id: &Option<String>| id.as_deref() == Some(sensor);
        if reads(&info.flow_sensor) || reads(&info.flow) || reads(&info.water_temperature) {
            users.push(SensorUse::Loop {
                id: info.id.clone(),
                name: info.name.clone(),
            });
        }
    }
    for other in snapshot.sensors.values() {
        if other
            .derived
            .as_ref()
            .is_some_and(|def| def.sources.iter().any(|source| source == sensor))
        {
            users.push(SensorUse::Derived {
                id: other.id.clone(),
                name: other.name.clone(),
            });
        }
    }
    users
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorRow {
    pub id: String,
    pub name: String,
    /// The Board file name, shown when the user gave another name.
    pub caption: Option<String>,
    pub value: String,
    pub no_value: bool,
    pub dim: bool,
    /// The severity icon before the name.
    pub severity: Option<Severity>,
    pub used_by: String,
    pub hidden: bool,
    pub selected: bool,
    pub rename_enabled: bool,
    pub hide_enabled: bool,
    /// The tooltip of a disabled button: the reason.
    pub hide_reason: Option<String>,
    /// "Source without value: GPU temperature".
    pub warning: Option<String>,
    pub derived: Option<DerivedDef>,
    pub delete_reason: Option<String>,
    /// The Loop that reads this raw flow sensor: the row opens its Calibration table editor.
    pub flow_loop: Option<String>,
    pub is_flow: bool,
    pub unit: String,
    pub feedback: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorSection {
    pub id: String,
    pub title: String,
    pub caption: Option<String>,
    pub folded: bool,
    pub rows: Vec<SensorRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorsState {
    Loading,
    Empty,
    Ready,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorsModel {
    pub state: SensorsState,
    pub empty_text: String,
    pub groups: Vec<SensorSection>,
    pub writes_enabled: bool,
    pub writes_tooltip: Option<String>,
    pub show_hidden: bool,
    pub search: String,
}

/// The state of the screen that the user changes: search, hidden sensors, selection.
#[derive(Clone, Debug, Default)]
pub struct SensorsUi {
    pub search: String,
    pub show_hidden: bool,
    /// The app keeps this set: the daemon contract has no hidden flag.
    pub hidden: BTreeSet<String>,
    pub folded: BTreeSet<String>,
    pub selected: Option<String>,
    /// The sensor whose rename popover is open, with the text of its input.
    pub rename: Option<(String, String)>,
    pub editor: Option<DerivedEditor>,
}

impl SensorsUi {
    pub fn open_rename(&mut self, sensor: &str, current: &str) {
        self.rename = Some((sensor.to_owned(), current.to_owned()));
    }

    /// Enter in the rename popover. An empty name restores the Board file name.
    pub fn rename_command(&mut self) -> Option<Command> {
        let (sensor, name) = self.rename.take()?;
        Some(Command::SetSensorName {
            sensor,
            name: name.trim().to_owned(),
        })
    }

    pub fn toggle_hidden(&mut self, sensor: &str) {
        if !self.hidden.remove(sensor) {
            self.hidden.insert(sensor.to_owned());
        }
    }

    pub fn toggle_fold(&mut self, group: &str) {
        if !self.folded.remove(group) {
            self.folded.insert(group.to_owned());
        }
    }
}

impl SensorsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &SensorsUi) -> Self {
        let locale = input.locale;
        let state = input.state;
        let writes = input.writes();
        let online = state.is_online();
        let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
        let search = ui.search.trim().to_lowercase();

        let group_of = |sensor: &SensorInfo| -> (String, String, Option<String>) {
            match sensor.origin.as_str() {
                "leakshield" => (
                    "leakshield".into(),
                    locale.tr("sensors.group.leakshield"),
                    None,
                ),
                "derived" => ("derived".into(), locale.tr("sensors.group.derived"), None),
                "calibrated" => (
                    "calibrated".into(),
                    locale.tr("sensors.group.calibrated"),
                    None,
                ),
                _ => match sensor.driver.as_str() {
                    "amdgpu" | "nouveau" | "nvidia" | "radeon" => (
                        "gpu".into(),
                        locale.tr("sensors.group.gpu"),
                        Some(sensor.driver.clone()),
                    ),
                    "nvme" => (
                        "nvme".into(),
                        locale.tr("sensors.group.nvme"),
                        Some(sensor.driver.clone()),
                    ),
                    driver => (
                        format!("chip:{driver}"),
                        locale.tr("sensors.group.chip"),
                        Some(driver.to_owned()),
                    ),
                },
            }
        };
        let order = |id: &str| match id {
            id if id.starts_with("chip:") => 0,
            "leakshield" => 1,
            "gpu" => 2,
            "nvme" => 3,
            "calibrated" => 4,
            _ => 5,
        };

        let mut groups: Vec<SensorSection> = Vec::new();
        for sensor in state.snapshot.sensors.values() {
            let hidden = ui.hidden.contains(&sensor.id);
            if hidden && !ui.show_hidden {
                continue;
            }
            let (group_id, title, caption) = group_of(sensor);
            if !search.is_empty() {
                let haystack = format!(
                    "{} {} {} {}",
                    sensor.name,
                    sensor.default_name,
                    title,
                    caption.clone().unwrap_or_default()
                )
                .to_lowercase();
                if !haystack.contains(&search) {
                    continue;
                }
            }
            let row = row_of(input, ui, sensor, sample, hidden, online);
            match groups.iter_mut().find(|g| g.id == group_id) {
                Some(group) => group.rows.push(row),
                None => groups.push(SensorSection {
                    folded: ui.folded.contains(&group_id),
                    id: group_id,
                    title,
                    caption,
                    rows: vec![row],
                }),
            }
        }
        groups.sort_by(|a, b| order(&a.id).cmp(&order(&b.id)).then(a.id.cmp(&b.id)));

        let ui_state = if state.link == Link::Connecting {
            SensorsState::Loading
        } else if state.snapshot.sensors.is_empty() {
            SensorsState::Empty
        } else {
            SensorsState::Ready
        };
        Self {
            state: ui_state,
            empty_text: locale.tr("sensors.empty"),
            groups,
            writes_enabled: writes.enabled,
            writes_tooltip: writes.tooltip,
            show_hidden: ui.show_hidden,
            search: ui.search.clone(),
        }
    }
}

fn row_of(
    input: &ScreenInput<'_>,
    ui: &SensorsUi,
    sensor: &SensorInfo,
    sample: Option<&crate::model::SensorSample>,
    hidden: bool,
    online: bool,
) -> SensorRow {
    let locale = input.locale;
    let state = input.state;
    let writes = input.writes();
    let value = sample.and_then(|s| s.sensor_value(&sensor.id));
    let users = sensor_users(state, &sensor.id);
    let used_by = users
        .iter()
        .map(|user| match user {
            SensorUse::Curve { name, .. } => locale.fmt("sensors.used.curve", &[("name", name)]),
            SensorUse::Loop { name, .. } | SensorUse::Derived { name, .. } => name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let hide_reason = users.first().map(|user| {
        locale.tr(match user {
            SensorUse::Curve { .. } => "sensors.in_use.curve",
            SensorUse::Loop { .. } => "sensors.in_use.loop",
            SensorUse::Derived { .. } => "sensors.in_use.derived",
        })
    });
    let severity = state
        .snapshot
        .alarms
        .iter()
        .filter(|alarm| {
            alarm.source == sensor.id || alarm.details.sensor.as_deref() == Some(&sensor.id)
        })
        .map(|alarm| alarm.severity)
        .max();
    let warning = sensor.derived.as_ref().and_then(|def| {
        if value.is_some() {
            return None;
        }
        def.sources
            .iter()
            .find(|source| sample.and_then(|s| s.sensor_value(source)).is_none())
            .map(|source| {
                let name = state
                    .snapshot
                    .sensors
                    .get(source)
                    .map_or(source.clone(), |s| s.name.clone());
                locale.fmt("sensors.source_without_value", &[("name", &name)])
            })
    });
    let delete_reason = sensor.derived.as_ref().and_then(|_| {
        users.first().map(|user| match user {
            SensorUse::Curve { name, .. } => {
                locale.fmt("sensors.delete_refused.curve", &[("name", name)])
            }
            SensorUse::Loop { name, .. } => {
                locale.fmt("sensors.delete_refused.loop", &[("name", name)])
            }
            SensorUse::Derived { name, .. } => {
                locale.fmt("sensors.delete_refused.derived", &[("name", name)])
            }
        })
    });
    let feedback = input
        .feedback
        .get(&format!("sensor-name:{}", sensor.id))
        .filter(|outcome| outcome.is_failure())
        .map(|outcome| outcome.text(locale, &locale.tr("sensors.rename_refused")));
    let flow_loop = (sensor.kind == "flow" && sensor.origin == "hwmon")
        .then(|| {
            state
                .snapshot
                .loops
                .iter()
                .find(|info| info.flow_sensor.as_deref() == Some(&sensor.id))
                .map(|info| info.id.clone())
        })
        .flatten();
    let value_shown = value.filter(|_| online || sample.is_some());
    SensorRow {
        id: sensor.id.clone(),
        name: sensor.name.clone(),
        caption: (sensor.name != sensor.default_name && !sensor.default_name.is_empty())
            .then(|| sensor.default_name.clone()),
        value: optional_value(locale, value_shown, &sensor.unit),
        no_value: value_shown.is_none(),
        dim: !online || hidden,
        severity,
        used_by,
        hidden,
        selected: ui.selected.as_deref() == Some(&sensor.id),
        rename_enabled: writes.enabled,
        hide_enabled: writes.enabled && hide_reason.is_none(),
        hide_reason: hide_reason.or_else(|| writes.tooltip.clone()),
        warning,
        derived: sensor.derived.clone(),
        delete_reason,
        flow_loop,
        is_flow: sensor.kind == "flow" && sensor.origin == "hwmon",
        unit: sensor.unit.clone(),
        feedback,
    }
}

/// The text next to a failed write of the editor.
pub fn editor_feedback(input: &ScreenInput<'_>, key: &str) -> Option<String> {
    let locale = input.locale;
    input.feedback.get(key).and_then(|outcome| match outcome {
        WriteOutcome::Refused { error } if error == "InUse" => {
            Some(locale.tr("sensors.editor.in_use"))
        }
        other if other.is_failure() => {
            Some(other.text(locale, &locale.tr("sensors.editor.refused")))
        }
        _ => None,
    })
}

// ---- The Derived sensor editor ----

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedEditor {
    /// The Derived sensor that the editor changes. `None` for a new one.
    pub editing: Option<String>,
    pub name: String,
    pub function: String,
    pub sources: Vec<Option<String>>,
}

impl DerivedEditor {
    pub fn new_sensor() -> Self {
        Self {
            editing: None,
            name: String::new(),
            function: "maximum".to_owned(),
            sources: vec![None, None],
        }
    }

    pub fn edit(sensor: &SensorInfo) -> Option<Self> {
        let def = sensor.derived.as_ref()?;
        Some(Self {
            editing: Some(sensor.id.clone()),
            name: sensor.name.clone(),
            function: def.function.clone(),
            sources: def.sources.iter().cloned().map(Some).collect(),
        })
    }

    /// "Difference" has exactly two sources: the other functions keep what the user chose.
    pub fn set_function(&mut self, function: &str) {
        self.function = function.to_owned();
        if function == "difference" {
            self.sources.resize(2, None);
        }
    }

    pub fn set_source(&mut self, slot: usize, sensor: Option<String>) {
        if let Some(entry) = self.sources.get_mut(slot) {
            *entry = sensor;
        }
    }

    pub fn add_source(&mut self) {
        if self.function != "difference" {
            self.sources.push(None);
        }
    }

    pub fn remove_source(&mut self, slot: usize) {
        if self.sources.len() > 2 && slot < self.sources.len() {
            self.sources.remove(slot);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceSlot {
    /// "Minuend" and "Subtrahend" for a difference, "Source 1" and so on otherwise.
    pub label: String,
    pub sensor: Option<String>,
    pub sensor_name: Option<String>,
    pub value: Option<String>,
    pub removable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedEditorModel {
    pub title: String,
    pub name: String,
    /// A saved Derived sensor changes its name with "Rename", not here.
    pub name_editable: bool,
    pub function: String,
    pub function_help: String,
    pub slots: Vec<SourceSlot>,
    pub can_add_source: bool,
    pub unit_mismatch: Option<String>,
    /// "Now 46.2 °C", or "Now --".
    pub live_result: String,
    pub can_save: bool,
    pub save_reason: Option<String>,
    pub feedback: Option<String>,
}

impl DerivedEditorModel {
    pub fn build(input: &ScreenInput<'_>, editor: &DerivedEditor) -> Self {
        let locale = input.locale;
        let state = input.state;
        let sample = state.sensors.as_ref();
        let sensor_of =
            |id: &Option<String>| id.as_ref().and_then(|id| state.snapshot.sensors.get(id));
        let slots: Vec<SourceSlot> = editor
            .sources
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let label = if editor.function == "difference" {
                    locale.tr(if index == 0 {
                        "sensors.editor.minuend"
                    } else {
                        "sensors.editor.subtrahend"
                    })
                } else {
                    locale.fmt("sensors.editor.source", &[("n", &(index + 1).to_string())])
                };
                let sensor = sensor_of(id);
                SourceSlot {
                    label,
                    sensor: id.clone(),
                    sensor_name: sensor.map(|s| s.name.clone()),
                    value: sensor.map(|s| {
                        optional_value(
                            locale,
                            sample.and_then(|sm| sm.sensor_value(&s.id)),
                            &s.unit,
                        )
                    }),
                    removable: editor.function != "difference" && editor.sources.len() > 2,
                }
            })
            .collect();

        let units: BTreeSet<&str> = editor
            .sources
            .iter()
            .filter_map(sensor_of)
            .map(|s| s.unit.as_str())
            .collect();
        let mismatch = units.len() > 1;
        let complete = editor.sources.iter().all(Option::is_some);
        let kind = match editor.function.as_str() {
            "mean" => DerivedKind::Mean,
            "difference" => DerivedKind::Difference,
            _ => DerivedKind::Maximum,
        };
        let values: Vec<Option<f64>> = editor
            .sources
            .iter()
            .map(|id| {
                id.as_ref()
                    .and_then(|id| sample.and_then(|s| s.sensor_value(id)))
            })
            .collect();
        let result = compute(kind, &values);
        let unit = units.iter().next().copied().unwrap_or("");
        let live_result = locale.fmt(
            "sensors.editor.now",
            &[(
                "value",
                &match (result, mismatch) {
                    (Some(value), false) => value_text(locale, value, unit),
                    _ => "--".to_owned(),
                },
            )],
        );
        let save_reason = if editor.name.trim().is_empty() {
            Some(locale.tr("sensors.editor.name_missing"))
        } else if mismatch {
            Some(locale.tr("sensors.editor.mixed_units"))
        } else if !complete || editor.sources.len() < 2 {
            Some(locale.tr("sensors.editor.sources_missing"))
        } else {
            None
        };
        let key = editor
            .editing
            .as_ref()
            .map_or_else(|| "derived:new".to_owned(), |id| format!("derived:{id}"));
        Self {
            title: locale.tr(if editor.editing.is_some() {
                "sensors.editor.title_edit"
            } else {
                "sensors.editor.title_new"
            }),
            name: editor.name.clone(),
            name_editable: editor.editing.is_none(),
            function: editor.function.clone(),
            function_help: locale.tr(&format!("sensors.help.{}", editor.function)),
            can_add_source: editor.function != "difference",
            slots,
            unit_mismatch: mismatch.then(|| locale.tr("sensors.editor.mixed_units")),
            live_result,
            can_save: save_reason.is_none() && input.writes().enabled,
            save_reason,
            feedback: editor_feedback(input, &key),
        }
    }

    /// The command of "Save". `None` while the editor is not valid.
    pub fn save_command(editor: &DerivedEditor) -> Option<Command> {
        if editor.name.trim().is_empty() || editor.sources.len() < 2 {
            return None;
        }
        let sources: Option<Vec<String>> = editor.sources.iter().cloned().collect();
        let sources = sources?;
        Some(match &editor.editing {
            Some(sensor) => Command::SetDerivedDefinition {
                sensor: sensor.clone(),
                function: editor.function.clone(),
                sources,
            },
            None => Command::CreateDerivedSensor {
                name: editor.name.trim().to_owned(),
                function: editor.function.clone(),
                sources,
            },
        })
    }
}

/// The unit text for a chart axis of a group.
pub fn axis_unit(unit: &str) -> &str {
    unit_text(unit)
}
