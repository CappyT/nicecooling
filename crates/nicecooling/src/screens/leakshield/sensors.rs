//! The Sensors tab (`docs/ui/screens/leakshield-sensors.md`): the names of the device sensors
//! and the 12 Software sensors.

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{AppState, SensorInfo};
use crate::screens::format::optional_value;
use crate::screens::leakshield::{Device, TabState, write_line};
use crate::screens::{FeedbackLine, ScreenInput};

/// The device sensors in the order of the device (`docs/protocol/leakshield.md`).
const DEVICE_ORDER: [&str; 6] = [
    "pressure",
    "fill_level",
    "volume_change",
    "pressure_change",
    "feed_rpm",
    "flow",
];
/// The sensors that the Feed gives to the Leakshield.
const FEED_SENSORS: [&str; 2] = ["feed_rpm", "flow"];

/// The index of a Software sensor slot among the device sensors: slot 1 is sensor 8.
pub fn software_index(slot: u16) -> f64 {
    f64::from(slot) + 7.0
}

#[derive(Clone, Debug, Default)]
pub struct SensorsUi {
    /// The slot whose clearing waits for the confirmation.
    pub clear: Option<u16>,
}

impl SensorsUi {
    /// A click on "Clear" of a slot. A slot that a page or a controller uses asks first.
    pub fn request_clear(&mut self, state: &AppState, device: &str, slot: u16) -> Option<Command> {
        let used = state
            .snapshot
            .leakshield_data
            .get(device)
            .is_some_and(|data| !used_by(data, slot).is_empty());
        if used {
            self.clear = Some(slot);
            None
        } else {
            Some(set_source(device, slot, ""))
        }
    }

    pub fn confirm_clear(&mut self, device: &str) -> Option<Command> {
        self.clear.take().map(|slot| set_source(device, slot, ""))
    }

    pub fn cancel(&mut self) {
        self.clear = None;
    }
}

pub fn set_source(device: &str, slot: u16, sensor: &str) -> Command {
    Command::SetSoftwareSensor {
        leakshield: device.to_owned(),
        slot,
        sensor: sensor.to_owned(),
    }
}

/// The display pages and the lighting controllers that use a Software sensor slot.
pub fn used_by(data: &crate::model::LeakshieldData, slot: u16) -> Vec<UsedBy> {
    let index = software_index(slot);
    let mut users = Vec::new();
    if let Some(sources) = data.list("page_sources") {
        for (page, row) in sources.chunks(8).enumerate() {
            if row.contains(&index) {
                users.push(UsedBy::Page(page + 1));
            }
        }
    }
    if let Some(sources) = data.list("led_controller_source") {
        for (controller, source) in sources.iter().enumerate() {
            if *source == index {
                users.push(UsedBy::Controller(controller + 1));
            }
        }
    }
    users
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsedBy {
    Page(usize),
    Controller(usize),
}

impl UsedBy {
    pub fn text(self, locale: Locale) -> String {
        match self {
            Self::Page(n) => locale.fmt("leakshield.sensors.used_page", &[("n", &n.to_string())]),
            Self::Controller(n) => locale.fmt(
                "leakshield.sensors.used_controller",
                &[("n", &n.to_string())],
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceSensorRow {
    pub number: usize,
    pub sensor: String,
    pub name: String,
    pub value: String,
    pub kind: String,
    pub dim: bool,
    pub enabled: bool,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoftwareRow {
    pub slot: u16,
    pub name: String,
    pub source: Option<String>,
    /// The name of the source, or "No source".
    pub source_text: String,
    pub value: String,
    pub unit: String,
    pub used_by: Vec<String>,
    /// The Warning icon: the source has no value.
    pub warning: Option<String>,
    pub enabled: bool,
    pub write_line: Option<FeedbackLine>,
    /// The sensors that the picker lists: id, name, selected.
    pub choices: Vec<(String, String, bool)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClearDialog {
    pub title: String,
    pub text: String,
    pub cancel: String,
    pub confirm: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorsModel {
    pub state: TabState,
    pub device_title: String,
    pub device_columns: Vec<String>,
    pub device_rows: Vec<DeviceSensorRow>,
    pub software_title: String,
    pub software_columns: Vec<String>,
    pub software_rows: Vec<SoftwareRow>,
    pub caption: String,
    pub clear_label: String,
    pub dialog: Option<ClearDialog>,
}

fn suffix<'a>(device: &str, sensor: &'a SensorInfo) -> &'a str {
    sensor
        .id
        .strip_prefix(device)
        .and_then(|rest| rest.strip_prefix('_'))
        .unwrap_or(&sensor.id)
}

impl SensorsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &SensorsUi, id: &str) -> Self {
        let locale = input.locale;
        let state = input.state;
        let device = Device::new(input, id);
        let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
        let enabled = device.can_write();
        let dim = device.state() != TabState::Ready;

        let mut own: Vec<&SensorInfo> = state
            .snapshot
            .sensors
            .values()
            .filter(|sensor| sensor.origin == "leakshield" && sensor.id.starts_with(id))
            .collect();
        own.sort_by_key(|sensor| {
            let name = suffix(id, sensor);
            (
                DEVICE_ORDER
                    .iter()
                    .position(|known| *known == name)
                    .unwrap_or(DEVICE_ORDER.len()),
                sensor.id.clone(),
            )
        });
        let device_rows = own
            .iter()
            .enumerate()
            .map(|(index, sensor)| {
                let value = sample.and_then(|s| s.sensor_value(&sensor.id));
                DeviceSensorRow {
                    number: index + 1,
                    sensor: sensor.id.clone(),
                    name: sensor.name.clone(),
                    value: optional_value(locale, value, &sensor.unit),
                    kind: locale.tr(if FEED_SENSORS.contains(&suffix(id, sensor)) {
                        "leakshield.sensors.type_feed"
                    } else {
                        "leakshield.sensors.type_local"
                    }),
                    dim,
                    enabled,
                    write_line: sensor_write_line(input, &sensor.id),
                }
            })
            .collect();

        let mut all: Vec<&SensorInfo> = state.snapshot.sensors.values().collect();
        all.sort_by_key(|sensor| sensor.name.to_lowercase());
        let software_rows = (1..=device
            .data
            .map_or(12, |data| data.software_sensor_slots.max(12)))
            .map(|slot| {
                let source = device
                    .data
                    .and_then(|data| {
                        data.software_sensors
                            .iter()
                            .find(|(number, _)| *number == slot)
                    })
                    .and_then(|(_, sensor)| sensor.clone());
                let info = source.as_ref().and_then(|s| state.snapshot.sensors.get(s));
                let value = source
                    .as_ref()
                    .and_then(|s| sample.and_then(|sample| sample.sensor_value(s)));
                let warning = match (&source, value) {
                    (Some(_), None) => Some(locale.fmt(
                        "leakshield.sensors.no_value",
                        &[("name", &info.map_or_else(String::new, |s| s.name.clone()))],
                    )),
                    _ => None,
                };
                SoftwareRow {
                    slot,
                    name: locale.fmt(
                        "leakshield.sensors.software_name",
                        &[("n", &slot.to_string())],
                    ),
                    source: source.clone(),
                    source_text: info.map_or_else(
                        || locale.tr("leakshield.sensors.no_source"),
                        |sensor| sensor.name.clone(),
                    ),
                    value: if source.is_some() {
                        optional_value(locale, value, info.map_or("", |s| s.unit.as_str()))
                    } else {
                        "--".to_owned()
                    },
                    unit: info.map_or_else(String::new, |s| {
                        crate::screens::format::unit_text(&s.unit).to_owned()
                    }),
                    used_by: device
                        .data
                        .map(|data| {
                            used_by(data, slot)
                                .into_iter()
                                .map(|user| user.text(locale))
                                .collect()
                        })
                        .unwrap_or_default(),
                    warning,
                    enabled,
                    write_line: write_line(input, id, &format!("software-sensor:{slot}")),
                    choices: all
                        .iter()
                        .map(|sensor| {
                            (
                                sensor.id.clone(),
                                format!(
                                    "{}  {}",
                                    sensor.name,
                                    optional_value(
                                        locale,
                                        sample.and_then(|s| s.sensor_value(&sensor.id)),
                                        &sensor.unit
                                    )
                                ),
                                source.as_deref() == Some(sensor.id.as_str()),
                            )
                        })
                        .collect(),
                }
            })
            .collect();

        let dialog = ui.clear.map(|slot| {
            let users = device
                .data
                .map(|data| used_by(data, slot))
                .unwrap_or_default();
            let users = users
                .iter()
                .map(|user| match user {
                    UsedBy::Page(n) => {
                        locale.fmt("leakshield.sensors.page_n", &[("n", &n.to_string())])
                    }
                    UsedBy::Controller(n) => {
                        locale.fmt("leakshield.sensors.controller_n", &[("n", &n.to_string())])
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            ClearDialog {
                title: locale.fmt(
                    "leakshield.sensors.clear_title",
                    &[("n", &slot.to_string())],
                ),
                text: locale.fmt(
                    "leakshield.sensors.clear_text",
                    &[("n", &slot.to_string()), ("users", &users)],
                ),
                cancel: locale.tr("leakshield.cancel"),
                confirm: locale.tr("leakshield.sensors.clear_confirm"),
            }
        });

        Self {
            state: device.state(),
            device_title: locale.tr("leakshield.sensors.device"),
            device_columns: ["number", "name", "value", "type"]
                .iter()
                .map(|name| locale.tr(&format!("leakshield.sensors.column.{name}")))
                .collect(),
            device_rows,
            software_title: locale.tr("leakshield.sensors.software"),
            software_columns: ["slot", "name", "source", "unit", "value"]
                .iter()
                .map(|name| locale.tr(&format!("leakshield.sensors.column.{name}")))
                .collect(),
            software_rows,
            caption: locale.tr("leakshield.sensors.caption"),
            clear_label: locale.tr("leakshield.sensors.clear"),
            dialog,
        }
    }
}

fn sensor_write_line(input: &ScreenInput<'_>, sensor: &str) -> Option<FeedbackLine> {
    let outcome = input.feedback.get(&format!("sensor-name:{sensor}"))?;
    Some(FeedbackLine::new(
        outcome,
        input.locale,
        &input.locale.tr("feedback.refused"),
        None,
    ))
}
