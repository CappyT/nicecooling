//! The Calibration table editor (`docs/ui/widgets.md`): a table and a plot of one Loop's flow
//! sensor. The Sensors screen and the Loops screen both use it.

use nicecooling_core::calibration::{CalibrationError, CalibrationTable};

use crate::command::Command;
use crate::model::{AppState, CalibrationTableInfo, LoopInfo};
use crate::screens::format::number;
use crate::screens::{FeedbackLine, ScreenInput};

/// Why a draft is not a valid table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    TooFewPoints,
    NotSorted,
}

/// The state of the editor that the user changes.
#[derive(Clone, Debug, Default)]
pub struct CalibrationUi {
    /// The rows while the user edits them. `None` shows the table that the daemon holds.
    pub draft: Option<Vec<(f64, f64)>>,
    pub selected: Option<usize>,
    /// The preset that waits for the confirmation of "Load preset".
    pub preset_confirm: Option<String>,
    pub error: Option<Fault>,
}

fn table_of<'a>(state: &'a AppState, info: &LoopInfo) -> Option<&'a CalibrationTableInfo> {
    state
        .snapshot
        .calibration_tables
        .iter()
        .find(|table| table.id == info.calibration_table)
}

fn loop_info<'a>(state: &'a AppState, id: &str) -> Option<&'a LoopInfo> {
    state.snapshot.loops.iter().find(|info| info.id == id)
}

impl CalibrationUi {
    /// The rows that the editor shows.
    pub fn rows(&self, state: &AppState, loop_id: &str) -> Vec<(f64, f64)> {
        if let Some(draft) = &self.draft {
            return draft.clone();
        }
        loop_info(state, loop_id)
            .and_then(|info| table_of(state, info))
            .map(|table| table.points.clone())
            .unwrap_or_default()
    }

    /// Drop the draft when the daemon already holds it.
    pub fn sync(&mut self, state: &AppState, loop_id: &str) {
        let held = loop_info(state, loop_id)
            .and_then(|info| table_of(state, info))
            .map(|table| &table.points);
        if held.is_some() && held == self.draft.as_ref() {
            self.draft = None;
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Check the draft, and give the command that saves it when it is valid.
    fn save(&mut self, state: &AppState, loop_id: &str, name: &str) -> Option<Command> {
        let points = self.draft.clone()?;
        if let Err(error) = CalibrationTable::new(points.clone()) {
            self.error = Some(match error {
                CalibrationError::TooFewPoints => Fault::TooFewPoints,
                _ => Fault::NotSorted,
            });
            return None;
        }
        self.error = None;
        let info = loop_info(state, loop_id)?;
        let preset = table_of(state, info).is_none_or(|table| table.preset);
        Some(Command::SaveCalibration {
            loop_id: loop_id.to_owned(),
            sensor: info.flow_sensor.clone()?,
            table: info.calibration_table.clone(),
            preset,
            name: name.to_owned(),
            points,
        })
    }

    fn edit(&mut self, state: &AppState, loop_id: &str) -> &mut Vec<(f64, f64)> {
        let rows = self.rows(state, loop_id);
        self.draft.get_or_insert(rows)
    }

    /// A cell changed. Rows sort by raw rate.
    pub fn set_row(
        &mut self,
        input: &ScreenInput<'_>,
        loop_id: &str,
        index: usize,
        rpm: f64,
        flow: f64,
    ) -> Option<Command> {
        let rows = self.edit(input.state, loop_id);
        let row = rows.get_mut(index)?;
        let edited = (rpm.max(0.0), flow.max(0.0));
        if *row == edited {
            return None;
        }
        *row = edited;
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.selected = self.draft.as_ref()?.iter().position(|p| *p == edited);
        self.save(input.state, loop_id, &table_name(input, loop_id))
    }

    /// "Add point": a row after the last one.
    pub fn add_row(&mut self, input: &ScreenInput<'_>, loop_id: &str) -> Option<Command> {
        let rows = self.edit(input.state, loop_id);
        let next = match rows.as_slice() {
            [] => (100.0, 10.0),
            [.., last] => (last.0 + 50.0, last.1 + 10.0),
        };
        rows.push(next);
        self.selected = Some(rows.len() - 1);
        self.save(input.state, loop_id, &table_name(input, loop_id))
    }

    pub fn remove_row(
        &mut self,
        input: &ScreenInput<'_>,
        loop_id: &str,
        index: usize,
    ) -> Option<Command> {
        let rows = self.edit(input.state, loop_id);
        if index >= rows.len() {
            return None;
        }
        rows.remove(index);
        self.selected = None;
        self.save(input.state, loop_id, &table_name(input, loop_id))
    }

    pub fn request_preset(&mut self, preset: &str) {
        self.preset_confirm = Some(preset.to_owned());
    }

    pub fn cancel_preset(&mut self) {
        self.preset_confirm = None;
    }

    /// "Load preset" replaces the table after the confirmation.
    pub fn confirm_preset(&mut self, input: &ScreenInput<'_>, loop_id: &str) -> Option<Command> {
        let id = self.preset_confirm.take()?;
        let preset = input
            .state
            .snapshot
            .calibration_tables
            .iter()
            .find(|table| table.preset && table.id == id)?;
        self.draft = Some(preset.points.clone());
        self.selected = None;
        self.save(input.state, loop_id, &table_name(input, loop_id))
    }
}

fn table_name(input: &ScreenInput<'_>, loop_id: &str) -> String {
    let name = loop_info(input.state, loop_id).map_or("", |info| info.name.as_str());
    input
        .locale
        .fmt("calibration.table_name", &[("loop", name)])
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresetItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmDialog {
    pub title: String,
    pub text: String,
    pub confirm: String,
    pub cancel: String,
}

/// The live marker: a vertical line at the raw rate, with the computed flow.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub raw: f64,
    pub flow: f64,
    pub out_of_range: bool,
    /// "671 rpm, 115 L/h".
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationPlot {
    pub points: Vec<(f64, f64)>,
    pub x_max: f64,
    pub y_max: f64,
    /// The dashed segment from 0,0 to the first point.
    pub origin: Option<((f64, f64), (f64, f64))>,
    /// The dashed extension after the last point, with the label "Out of range".
    pub extension: Option<((f64, f64), (f64, f64))>,
    pub extension_label: String,
    pub marker: Option<Marker>,
    pub selected: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationEditorModel {
    pub loop_id: String,
    pub title: String,
    pub table_id: String,
    pub table_name: String,
    pub preset: bool,
    pub x_header: String,
    pub y_header: String,
    pub rows: Vec<(f64, f64)>,
    pub plot: CalibrationPlot,
    /// "671 rpm is above the last point. The flow is an estimate."
    pub out_of_range: Option<String>,
    pub add_label: String,
    pub preset_label: String,
    pub presets: Vec<PresetItem>,
    pub confirm: Option<ConfirmDialog>,
    pub error: Option<String>,
    pub editable: bool,
    pub tooltip: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

impl CalibrationEditorModel {
    /// `None` when the Loop has no flow sensor.
    pub fn build(input: &ScreenInput<'_>, ui: &CalibrationUi, info: &LoopInfo) -> Option<Self> {
        let flow_sensor = info.flow_sensor.as_ref()?;
        let locale = input.locale;
        let state = input.state;
        let writes = input.writes();
        let table = table_of(state, info);
        let rows = ui.rows(state, &info.id);
        let valid = CalibrationTable::new(rows.clone()).ok();
        let sample = state.sensors.as_ref().filter(|_| state.is_online());
        let raw = sample.and_then(|s| s.sensor_value(flow_sensor));

        let marker = raw.zip(valid.as_ref()).map(|(raw, table)| {
            let flow = table.flow(raw);
            Marker {
                raw,
                flow: flow.lph,
                out_of_range: flow.out_of_range,
                label: locale.fmt(
                    "calibration.marker",
                    &[
                        ("raw", &number(locale, raw, "rpm")),
                        ("flow", &number(locale, flow.lph, "l/h")),
                    ],
                ),
            }
        });
        let last = rows.last().copied();
        let x_max = last
            .map_or(100.0, |p| p.0 * 1.2)
            .max(raw.map_or(0.0, |r| r * 1.1))
            .max(10.0);
        let extension = valid
            .as_ref()
            .zip(last)
            .map(|(table, last)| ((last.0, last.1), (x_max, table.flow(x_max).lph)));
        let y_max = rows
            .iter()
            .map(|p| p.1)
            .chain(extension.map(|e| e.1.1))
            .fold(10.0, f64::max)
            * 1.1;
        let plot = CalibrationPlot {
            origin: rows.first().map(|first| ((0.0, 0.0), *first)),
            extension,
            extension_label: locale.tr("calibration.out_of_range"),
            marker: marker.clone(),
            selected: ui.selected,
            points: rows.clone(),
            x_max,
            y_max,
        };
        let out_of_range = marker.as_ref().filter(|m| m.out_of_range).map(|m| {
            locale.fmt(
                "calibration.above_last",
                &[("raw", &number(locale, m.raw, "rpm"))],
            )
        });
        let preset = table.is_none_or(|t| t.preset);
        let write_line = input
            .feedback
            .get(&format!("table:{}", info.id))
            .map(|outcome| {
                FeedbackLine::new(outcome, locale, &locale.tr("calibration.refused"), None)
            });
        let confirm = ui.preset_confirm.as_ref().and_then(|id| {
            state
                .snapshot
                .calibration_tables
                .iter()
                .find(|t| &t.id == id)
                .map(|preset| ConfirmDialog {
                    title: locale.fmt("calibration.confirm.title", &[("name", &preset.name)]),
                    text: locale.tr("calibration.confirm.text"),
                    confirm: locale.tr("calibration.confirm.confirm"),
                    cancel: locale.tr("calibration.confirm.cancel"),
                })
        });
        Some(Self {
            loop_id: info.id.clone(),
            title: locale.tr("calibration.title"),
            table_id: info.calibration_table.clone(),
            table_name: table.map_or_else(String::new, |t| t.name.clone()),
            preset,
            x_header: locale.tr("calibration.x_header"),
            y_header: locale.tr("calibration.y_header"),
            plot,
            rows,
            out_of_range,
            add_label: locale.tr("calibration.add_point"),
            preset_label: locale.tr("calibration.load_preset"),
            presets: state
                .snapshot
                .calibration_tables
                .iter()
                .filter(|t| t.preset)
                .map(|t| PresetItem {
                    id: t.id.clone(),
                    name: t.name.clone(),
                })
                .collect(),
            confirm,
            error: ui.error.as_ref().map(|error| {
                locale.tr(match error {
                    Fault::TooFewPoints => "calibration.error.too_few",
                    Fault::NotSorted => "calibration.error.not_sorted",
                })
            }),
            editable: writes.enabled,
            tooltip: writes.tooltip,
            write_line,
        })
    }
}
