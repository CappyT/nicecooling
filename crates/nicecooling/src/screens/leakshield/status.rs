//! The Status tab (`docs/ui/screens/leakshield-status.md`): the operating data, the device
//! schematic and the event list with its export.

use std::collections::BTreeMap;

use crate::i18n::{Locale, event_key};
use crate::model::{DeviceEvent, LeakshieldInfo, PressureLimits, Severity};
use crate::screens::ScreenInput;
use crate::screens::alarms::event_severity;
use crate::screens::format::{compact, date_time_seconds, optional_value};
use crate::screens::leakshield::overview::{limits_of, status_key, status_text};
use crate::screens::leakshield::{Device, TabState, change_text, severity_of};

/// The height of the chart between the tiles and the schematic, in px.
pub const CHART_HEIGHT: (f32, f32) = (84.0, 240.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tile {
    #[default]
    Status,
    Pressure,
    VolumeChange,
    PressureChange,
}

impl Tile {
    pub const ALL: [Tile; 4] = [
        Tile::Status,
        Tile::Pressure,
        Tile::VolumeChange,
        Tile::PressureChange,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Status => "leakshield.status.tile.status",
            Self::Pressure => "leakshield.status.tile.pressure",
            Self::VolumeChange => "leakshield.status.tile.volume_change",
            Self::PressureChange => "leakshield.status.tile.pressure_change",
        }
    }

    /// The name of the sensor that the tile shows.
    pub fn sensor(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Status => None,
            Self::Pressure => Some(("pressure", "mbar")),
            Self::VolumeChange => Some(("volume_change", "ml/h")),
            Self::PressureChange => Some(("pressure_change", "mbar/min")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusUi {
    pub selected: Tile,
    pub chart_height: f32,
}

impl Default for StatusUi {
    fn default() -> Self {
        Self {
            selected: Tile::Status,
            chart_height: CHART_HEIGHT.0,
        }
    }
}

impl StatusUi {
    /// Left and Right move the selection between the tiles.
    pub fn step(&mut self, delta: isize) {
        let index = Tile::ALL
            .iter()
            .position(|t| *t == self.selected)
            .unwrap_or(0);
        let next = index.saturating_add_signed(delta).min(Tile::ALL.len() - 1);
        self.selected = Tile::ALL[next];
    }

    pub fn set_chart_height(&mut self, height: f32) {
        self.chart_height = height.clamp(CHART_HEIGHT.0, CHART_HEIGHT.1);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TileModel {
    pub tile: Tile,
    /// The caption under the value: the name, and the target for the pressure.
    pub label: String,
    pub value: String,
    pub selected: bool,
    pub severity: Option<Severity>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartModel {
    pub key: &'static str,
    pub label: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub tag: &'static str,
    pub value: String,
    pub caption: String,
    pub severity: Option<Severity>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchematicModel {
    pub title: String,
    pub caption: String,
    pub parts: Vec<PartModel>,
    pub bubbles: Vec<Bubble>,
    /// The fill level in percent, for the level in the reservoir.
    pub level: Option<f64>,
    /// "Fill level", the caption of the LT bubble.
    pub level_label: String,
    /// The schematic is dim: the Leakshield does not answer.
    pub dim: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventLine {
    pub time: String,
    pub title: String,
    pub details: String,
    pub severity: Option<Severity>,
    /// A click opens the Alarms screen.
    pub opens_alarms: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventsCard {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<EventLine>,
    pub empty: Option<String>,
    pub export: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusModel {
    pub state: TabState,
    pub loading_text: String,
    pub operating_title: String,
    pub operating_caption: String,
    pub tiles: Vec<TileModel>,
    pub chart_height: f32,
    /// "Target 300 mbar", the label of the target line in the pressure chart.
    pub target_label: Option<String>,
    pub target: Option<f64>,
    pub selected: Tile,
    pub status_label: String,
    pub schematic: SchematicModel,
    pub events: EventsCard,
    /// The device status as a key, for the bands of the Status tile.
    pub status_key: &'static str,
    pub unknown_firmware: bool,
}

/// The text of an entry of the device event list, and whether the entry is a Warning or an
/// Alarm. The daemon sends `leakshield.<name>`. A device event has its text under
/// `leakshield.event`. The names of the `alarm_bits` have their title with the Alarms.
fn event_text(locale: Locale, id: &str) -> (String, bool) {
    let name = id.strip_prefix("leakshield.").unwrap_or(id);
    let key = format!("leakshield.event.{}", event_key(name));
    let text = locale.tr(&key);
    if text != key {
        return (text, false);
    }
    if name == id {
        return (id.to_owned(), false);
    }
    let key = format!("event.{}.title", event_key(id));
    (locale.event_title(id), locale.tr(&key) != key)
}

/// The text of a device event identifier.
pub fn event_title(locale: Locale, id: &str) -> String {
    event_text(locale, id).0
}

fn details_text(locale: Locale, details: &BTreeMap<String, String>) -> String {
    if let (Some(from), Some(to)) = (details.get("from"), details.get("to")) {
        let name = |mode: &str| locale.tr(&format!("leakshield.mode.{mode}"));
        return format!("{} -> {}", name(from), name(to));
    }
    if let Some(pressure) = details.get("pressure") {
        return locale.fmt("leakshield.status.detail_pressure", &[("value", pressure)]);
    }
    details
        .iter()
        .map(|(key, value)| format!("{key} {value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The text file of the event list: the serial, the firmware version and one line per event.
pub fn export_text(locale: Locale, info: &LeakshieldInfo, events: &[DeviceEvent]) -> String {
    let mut text = format!("Serial: {}\nFirmware: {}\n", info.serial, info.firmware);
    for event in events {
        text.push_str(&format!(
            "{} | {} | {}\n",
            date_time_seconds(event.time_us),
            event_title(locale, &event.id),
            details_text(locale, &event.details)
        ));
    }
    text
}

/// Write the export to `path`.
pub fn export_to(
    path: &std::path::Path,
    locale: Locale,
    info: &LeakshieldInfo,
    events: &[DeviceEvent],
) -> std::io::Result<()> {
    std::fs::write(path, export_text(locale, info, events))
}

fn parts(locale: Locale, key: &str) -> Vec<PartModel> {
    let pumping = key == "building";
    let released = key == "released";
    let part = |name: &'static str, active: bool, state: Option<&str>| PartModel {
        key: name,
        label: match state {
            Some(state) => locale.fmt(
                &format!("leakshield.status.part.{name}"),
                &[(
                    "state",
                    &locale.tr(&format!("leakshield.status.part_state.{state}")),
                )],
            ),
            None => locale.tr(&format!("leakshield.status.part.{name}")),
        },
        active,
    };
    vec![
        part("atmosphere", false, None),
        part(
            "vent_valve",
            released,
            Some(if released { "open" } else { "closed" }),
        ),
        part(
            "pump_valve",
            pumping,
            Some(if pumping { "open" } else { "closed" }),
        ),
        part(
            "vacuum_pump",
            pumping,
            Some(if pumping { "running" } else { "off" }),
        ),
        part("membrane", false, None),
        part("reservoir", false, None),
    ]
}

impl StatusModel {
    pub fn build(input: &ScreenInput<'_>, ui: &StatusUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let state = device.state();
        let ready = state == TabState::Ready;
        let alarms = device.alarms(input);
        let value = |name: &str, unit: &str| {
            optional_value(locale, device.sensor(input, name).filter(|_| ready), unit)
        };
        let pressure = device.sensor(input, "pressure").filter(|_| ready);
        let limits: Option<PressureLimits> = limits_of(&device);
        let key = status_key(&device, pressure, limits);
        let severity_for = |needle: &str| {
            alarms
                .iter()
                .filter(|a| a.event.contains(needle))
                .map(|a| a.severity)
                .max()
        };
        let change = |name: &str, unit: &str| {
            change_text(locale, device.sensor(input, name).filter(|_| ready), unit)
        };
        let target_text = limits.map(|l| {
            locale.fmt(
                "leakshield.status.target",
                &[("value", &compact(locale, l.target))],
            )
        });
        let tiles = Tile::ALL
            .iter()
            .map(|tile| TileModel {
                tile: *tile,
                label: match (tile, &target_text) {
                    (Tile::Pressure, Some(target)) => {
                        format!("{}, {target}", locale.tr(tile.key()))
                    }
                    _ => locale.tr(tile.key()),
                },
                value: match tile.sensor() {
                    None if ready => status_text(locale, key),
                    None => "--".to_owned(),
                    Some((name, unit)) if *tile == Tile::Pressure => value(name, unit),
                    Some((name, unit)) => change(name, unit),
                },
                selected: *tile == ui.selected,
                severity: match tile {
                    Tile::VolumeChange => severity_for("leak"),
                    Tile::Pressure => severity_for("pressure"),
                    _ => None,
                },
            })
            .collect();

        let fill = value("fill_level", "%");
        let schematic = SchematicModel {
            title: locale.tr("leakshield.status.schematic"),
            caption: locale.tr("leakshield.status.schematic_caption"),
            parts: parts(locale, key),
            bubbles: vec![
                Bubble {
                    tag: "PT",
                    value: value("pressure", "mbar"),
                    caption: {
                        let rate = device
                            .sensor(input, "pressure_change")
                            .filter(|_| ready)
                            .map(|rate| change_text(locale, Some(rate), "mbar/min"));
                        target_text
                            .clone()
                            .into_iter()
                            .chain(rate)
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                    severity: severity_of(&alarms),
                },
                Bubble {
                    tag: "LT",
                    value: fill,
                    caption: change("volume_change", "ml/h"),
                    severity: severity_for("fill-level"),
                },
            ],
            level: device.sensor(input, "fill_level").filter(|_| ready),
            level_label: locale.tr("leakshield.overview.fill_level"),
            dim: !ready,
        };

        let events: Vec<EventLine> = device
            .data
            .map(|data| {
                data.events
                    .iter()
                    .map(|event| {
                        let (title, is_alarm) = event_text(locale, &event.id);
                        EventLine {
                            time: date_time_seconds(event.time_us),
                            title,
                            details: details_text(locale, &event.details),
                            severity: is_alarm.then(|| event_severity(&event.id)),
                            opens_alarms: is_alarm,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let empty = events
            .is_empty()
            .then(|| locale.tr("leakshield.status.no_events"));

        Self {
            state,
            loading_text: locale.tr("leakshield.status.loading"),
            operating_title: locale.tr("leakshield.status.operating"),
            operating_caption: locale.tr("leakshield.status.operating_caption"),
            tiles,
            chart_height: ui.chart_height,
            target_label: limits.map(|l| {
                locale.fmt(
                    "leakshield.status.target_line",
                    &[("value", &compact(locale, l.target))],
                )
            }),
            target: limits.map(|l| l.target),
            selected: ui.selected,
            status_label: locale.tr("leakshield.status.tile.status"),
            schematic,
            events: EventsCard {
                title: locale.tr("leakshield.status.events"),
                columns: ["time", "event", "details"]
                    .iter()
                    .map(|name| locale.tr(&format!("leakshield.status.column.{name}")))
                    .collect(),
                rows: events,
                empty,
                export: locale.tr("leakshield.status.export"),
            },
            status_key: key,
            unknown_firmware: device.unknown_firmware(),
        }
    }
}
