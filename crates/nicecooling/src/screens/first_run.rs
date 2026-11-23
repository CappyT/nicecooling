//! The First run screen (`docs/ui/screens/first-run.md`): five steps that detect the Board,
//! set up the first Loop and explain the BIOS snapshot and the Leakshield.

use nicecooling_core::calibration::CalibrationTable;

use crate::command::Command;
use crate::model::{AppState, ChannelInfo, LoopInfo, SensorSample};
use crate::screens::ScreenInput;
use crate::screens::format::{date_time, optional_value, value_text};
use crate::screens::loops::{Choice, PartRow};
use crate::shell::Page;
use crate::widgets::loop_diagram::LoopDiagram;

pub const DEFAULT_FLOOR: f64 = 40.0;
/// The id of the Loop that the live preview draws.
const DRAFT_FLOW: &str = "first_run_flow";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    #[default]
    Board,
    Loop,
    Bios,
    Leakshield,
    Done,
}

impl Step {
    pub const ALL: [Step; 5] = [
        Step::Board,
        Step::Loop,
        Step::Bios,
        Step::Leakshield,
        Step::Done,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Board => "first_run.step.board",
            Self::Loop => "first_run.step.loop",
            Self::Bios => "first_run.step.bios",
            Self::Leakshield => "first_run.step.leakshield",
            Self::Done => "first_run.step.done",
        }
    }
}

/// The picks of the Loop step. `None` is "not picked yet": the proposal of the Board applies.
/// An empty text is the choice "None".
#[derive(Clone, Debug, Default)]
pub struct FirstRunUi {
    pub step: Step,
    pub pump: Option<String>,
    pub flow: Option<String>,
    pub preset: Option<String>,
    pub water: Option<String>,
    pub leakshield: Option<String>,
    pub floor: Option<f64>,
    /// "Next" in the Loop step sent the command: the flow moves on when the Loop exists.
    pub creating: bool,
}

impl FirstRunUi {
    /// The Pump channel: the pick, or the Channel that the Board file marks as a pump.
    pub fn pump(&self, state: &AppState) -> Option<String> {
        match &self.pump {
            Some(pick) => Some(pick.clone()).filter(|id| !id.is_empty()),
            None => state
                .snapshot
                .channels
                .values()
                .find(|c| c.is_pump)
                .map(|c| c.id.clone()),
        }
    }

    pub fn flow_sensor(&self, state: &AppState) -> Option<String> {
        match &self.flow {
            Some(pick) => Some(pick.clone()).filter(|id| !id.is_empty()),
            None => state
                .snapshot
                .sensors
                .values()
                .find(|s| s.origin == "hwmon" && s.kind == "flow" && s.unit == "rpm")
                .map(|s| s.id.clone()),
        }
    }

    pub fn water(&self, state: &AppState) -> Option<String> {
        match &self.water {
            Some(pick) => Some(pick.clone()).filter(|id| !id.is_empty()),
            None => state
                .snapshot
                .sensors
                .values()
                .find(|s| s.kind == "temperature" && s.name.to_lowercase().contains("water"))
                .map(|s| s.id.clone()),
        }
    }

    pub fn leakshield(&self, state: &AppState) -> Option<String> {
        match &self.leakshield {
            Some(pick) => Some(pick.clone()).filter(|id| !id.is_empty()),
            None => state
                .snapshot
                .leakshields
                .iter()
                .find(|d| d.connected)
                .map(|d| d.id.clone()),
        }
    }

    /// The Calibration table preset: the pick, or the first preset.
    pub fn table(&self, state: &AppState) -> String {
        match &self.preset {
            Some(pick) => pick.clone(),
            None => state
                .snapshot
                .calibration_tables
                .iter()
                .find(|t| t.preset)
                .map(|t| t.id.clone())
                .unwrap_or_default(),
        }
    }

    pub fn floor(&self) -> f64 {
        self.floor.unwrap_or(DEFAULT_FLOOR)
    }

    fn has_leakshield_step(&self, state: &AppState) -> bool {
        self.leakshield(state).is_some()
    }

    /// The Loop that the flow draws now, from the picks.
    fn draft(&self, state: &AppState, name: String) -> LoopInfo {
        let flow_sensor = self.flow_sensor(state);
        LoopInfo {
            id: "first_run".to_owned(),
            name,
            leakshield: self.leakshield(state),
            flow: flow_sensor.as_ref().map(|_| DRAFT_FLOW.to_owned()),
            water_temperature: self.water(state),
            pump_channels: self.pump(state).into_iter().collect(),
            flow_sensor,
            calibration_table: self.table(state),
            pump_floor: self.floor(),
        }
    }

    /// The Next button. The Loop step creates the Loop and waits for the daemon.
    pub fn next(&mut self, input: &ScreenInput<'_>) -> Option<Command> {
        let state = input.state;
        match self.step {
            Step::Board => self.step = Step::Loop,
            Step::Loop => {
                let pump = self.pump(state)?;
                self.creating = true;
                return Some(Command::SetupLoop {
                    name: input.locale.tr("first_run.loop_name"),
                    pump_channels: vec![pump],
                    flow_sensor: self.flow_sensor(state).unwrap_or_default(),
                    table: self.table(state),
                    water: self.water(state).unwrap_or_default(),
                    leakshield: self.leakshield(state).unwrap_or_default(),
                    floor: self.floor(),
                });
            }
            Step::Bios => {
                self.step = if self.has_leakshield_step(state) {
                    Step::Leakshield
                } else {
                    Step::Done
                };
            }
            Step::Leakshield => self.step = Step::Done,
            Step::Done => {}
        }
        None
    }

    pub fn back(&mut self, state: &AppState) {
        self.step = match self.step {
            Step::Board | Step::Loop => Step::Board,
            Step::Bios => Step::Loop,
            Step::Leakshield => Step::Bios,
            Step::Done => {
                if self.has_leakshield_step(state) {
                    Step::Leakshield
                } else {
                    Step::Bios
                }
            }
        };
    }

    /// Move on after the Loop exists.
    pub fn sync(&mut self, state: &AppState) {
        if self.creating && !state.snapshot.loops.is_empty() {
            self.creating = false;
            self.step = Step::Bios;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StepLabel {
    pub step: Step,
    pub label: String,
    pub current: bool,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelRow {
    pub name: String,
    pub rpm: String,
    pub duty: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardStep {
    /// "Reading the Board" in the Loading state.
    pub loading: Option<String>,
    pub name: String,
    pub text: String,
    pub counts: String,
    pub channels: Vec<ChannelRow>,
    pub warning: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopStep {
    pub pump: PartRow,
    pub flow: PartRow,
    pub preset: Option<PartRow>,
    /// "671 rpm = 115 L/h".
    pub live_flow: Option<String>,
    pub water: PartRow,
    pub leakshield: PartRow,
    pub diagram: LoopDiagram,
    pub floor_label: String,
    pub floor: f64,
    pub floor_text: String,
    pub no_leakshield: Option<String>,
    /// With a flow sensor and "Empty table": the sensor has no value until the table has 2 points.
    pub empty_table_note: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BiosRow {
    pub name: String,
    /// "30 °C: 40 %, 60 °C: 100 %", or "--" when the snapshot has no auto points.
    pub curve: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BiosStep {
    pub text: String,
    /// "Saved 2026-11-20 08:14". `None` when the daemon holds no snapshot.
    pub saved: Option<String>,
    pub no_curve: String,
    pub columns: Vec<String>,
    pub rows: Vec<BiosRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeakshieldStep {
    pub id: String,
    pub serial: String,
    pub firmware: String,
    /// Known firmware: the counts of the fields. Unknown firmware: the warning.
    pub status: String,
    pub unknown: bool,
    pub text: String,
    pub open_fields: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DoneStep {
    pub summary: Vec<String>,
    pub text: String,
    pub overview: String,
    pub wizard: Option<String>,
    pub wizard_page: Option<Page>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footer {
    pub skip: String,
    pub back: Option<String>,
    pub next: Option<String>,
    pub next_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OfflineCard {
    pub title: String,
    pub try_again: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FirstRunModel {
    pub state: FirstRunState,
    pub title: String,
    pub steps: Vec<StepLabel>,
    pub step: Step,
    pub offline: Option<OfflineCard>,
    pub board: BoardStep,
    pub loop_step: LoopStep,
    pub bios: BiosStep,
    pub leakshield: Option<LeakshieldStep>,
    pub done: DoneStep,
    pub footer: Footer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstRunState {
    Offline,
    Loading,
    NoChannel,
    Ready,
}

impl FirstRunModel {
    pub fn build(input: &ScreenInput<'_>, ui: &FirstRunUi) -> Self {
        let locale = input.locale;
        let state = input.state;
        let snapshot = &state.snapshot;
        let state_kind = match state.link {
            crate::model::Link::Connecting => FirstRunState::Loading,
            crate::model::Link::Online if snapshot.channels.is_empty() => FirstRunState::NoChannel,
            crate::model::Link::Online => FirstRunState::Ready,
            _ => FirstRunState::Offline,
        };
        let leakshield_step = leakshield_step(input, ui);
        let steps = Step::ALL
            .into_iter()
            .filter(|step| *step != Step::Leakshield || leakshield_step.is_some())
            .map(|step| StepLabel {
                step,
                label: locale.tr(step.key()),
                current: step == ui.step,
                done: step < ui.step,
            })
            .collect();
        let loop_enabled = state_kind == FirstRunState::Ready;
        let next_enabled = match ui.step {
            Step::Board => loop_enabled,
            Step::Loop => loop_enabled && ui.pump(state).is_some() && !ui.creating,
            Step::Done => false,
            _ => true,
        };
        Self {
            state: state_kind,
            title: locale.tr("first_run.title"),
            steps,
            step: ui.step,
            offline: (state_kind == FirstRunState::Offline).then(|| OfflineCard {
                title: locale.tr("first_run.offline"),
                try_again: locale.tr("first_run.try_again"),
            }),
            board: board_step(input, state_kind),
            loop_step: loop_step(input, ui, loop_enabled),
            bios: bios_step(input),
            leakshield: leakshield_step,
            done: done_step(input, ui),
            footer: Footer {
                skip: locale.tr("first_run.skip"),
                back: (ui.step != Step::Board).then(|| locale.tr("first_run.back")),
                next: (ui.step != Step::Done).then(|| locale.tr("first_run.next")),
                next_enabled,
            },
        }
    }
}

fn sample_of(state: &AppState) -> Option<&SensorSample> {
    state.sensors.as_ref().or(state.last_sensors.as_ref())
}

fn channel_row(input: &ScreenInput<'_>, channel: &ChannelInfo) -> ChannelRow {
    let locale = input.locale;
    let sample = sample_of(input.state);
    ChannelRow {
        name: channel.name.clone(),
        rpm: optional_value(
            locale,
            channel
                .fan_sensor
                .as_ref()
                .and_then(|fan| sample.and_then(|s| s.sensor_value(fan))),
            "rpm",
        ),
        duty: optional_value(
            locale,
            sample.and_then(|s| s.channel_duty(&channel.id)),
            "%",
        ),
    }
}

fn board_step(input: &ScreenInput<'_>, kind: FirstRunState) -> BoardStep {
    let locale = input.locale;
    let snapshot = &input.state.snapshot;
    let board = snapshot.board.as_ref();
    let name = board.map_or_else(String::new, |b| {
        if b.vendor.is_empty() {
            b.model.clone()
        } else {
            format!("{} {}", b.vendor, b.model)
        }
    });
    let known = board.is_some_and(|b| !b.board_file.is_empty());
    let text = if known {
        locale.fmt("first_run.board.known", &[("board", &name)])
    } else {
        locale.tr("first_run.board.unknown")
    };
    BoardStep {
        loading: (kind == FirstRunState::Loading).then(|| locale.tr("first_run.reading")),
        name,
        text,
        counts: locale.fmt(
            "first_run.board.counts",
            &[
                ("channels", &snapshot.channels.len().to_string()),
                ("sensors", &snapshot.sensors.len().to_string()),
            ],
        ),
        channels: snapshot
            .channels
            .values()
            .map(|c| channel_row(input, c))
            .collect(),
        warning: (kind == FirstRunState::NoChannel)
            .then(|| locale.tr("first_run.board.no_channel")),
        columns: ["channel", "rpm", "duty"]
            .iter()
            .map(|key| locale.tr(&format!("first_run.column.{key}")))
            .collect(),
    }
}

fn loop_step(input: &ScreenInput<'_>, ui: &FirstRunUi, enabled: bool) -> LoopStep {
    let locale = input.locale;
    let state = input.state;
    let snapshot = &state.snapshot;
    let none_choice = |selected: bool| Choice {
        id: String::new(),
        name: locale.tr("loops.none"),
        value: None,
        selected,
    };
    let sample = sample_of(state);
    let value_of = |id: &str, unit: &str| {
        optional_value(locale, sample.and_then(|s| s.sensor_value(id)), unit)
    };

    let pump_id = ui.pump(state);
    let pump = PartRow {
        label: locale.tr("first_run.loop.pump"),
        shown: pump_id
            .as_ref()
            .and_then(|id| snapshot.channels.get(id))
            .map_or_else(|| locale.tr("loops.none"), |c| c.name.clone()),
        choices: snapshot
            .channels
            .values()
            .map(|c| Choice {
                id: c.id.clone(),
                name: c.name.clone(),
                value: None,
                selected: pump_id.as_deref() == Some(c.id.as_str()),
            })
            .collect(),
        multiple: false,
        warning: pump_id
            .is_none()
            .then(|| locale.tr("first_run.loop.pump_needed")),
        note: None,
        write_line: None,
    };

    let sensor_row = |label: &str,
                      current: Option<&String>,
                      keep: &dyn Fn(&crate::model::SensorInfo) -> bool| {
        let mut choices = vec![none_choice(current.is_none())];
        choices.extend(
            snapshot
                .sensors
                .values()
                .filter(|s| keep(s))
                .map(|s| Choice {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    value: Some(value_of(&s.id, &s.unit)),
                    selected: current == Some(&s.id),
                }),
        );
        PartRow {
            label: locale.tr(label),
            shown: choices
                .iter()
                .find(|c| c.selected)
                .map_or_else(|| locale.tr("loops.none"), |c| c.name.clone()),
            choices,
            multiple: false,
            warning: None,
            note: None,
            write_line: None,
        }
    };
    let flow_id = ui.flow_sensor(state);
    let flow = sensor_row("first_run.loop.flow", flow_id.as_ref(), &|s| {
        s.unit == "rpm" && s.origin == "hwmon"
    });
    let water_id = ui.water(state);
    let water = sensor_row("first_run.loop.water", water_id.as_ref(), &|s| {
        s.kind == "temperature"
    });

    // The preset select, and the flow that the live rate gives with it.
    let table_id = ui.table(state);
    let preset = flow_id.as_ref().map(|_| {
        let mut choices: Vec<Choice> = snapshot
            .calibration_tables
            .iter()
            .filter(|t| t.preset)
            .map(|t| Choice {
                id: t.id.clone(),
                name: t.name.clone(),
                value: None,
                selected: t.id == table_id,
            })
            .collect();
        choices.push(Choice {
            id: String::new(),
            name: locale.tr("first_run.loop.empty_table"),
            value: None,
            selected: table_id.is_empty(),
        });
        PartRow {
            label: locale.tr("first_run.loop.preset"),
            shown: choices
                .iter()
                .find(|c| c.selected)
                .map_or_else(String::new, |c| c.name.clone()),
            choices,
            multiple: false,
            warning: None,
            note: None,
            write_line: None,
        }
    });
    let rate = flow_id
        .as_ref()
        .and_then(|id| sample.and_then(|s| s.sensor_value(id)));
    let live_flow = rate.and_then(|rpm| {
        let table = snapshot
            .calibration_tables
            .iter()
            .find(|t| t.id == table_id)?;
        let flow = CalibrationTable::new(table.points.clone()).ok()?.flow(rpm);
        Some(locale.fmt(
            "first_run.loop.live_flow",
            &[
                ("rpm", &value_text(locale, rpm, "rpm")),
                ("flow", &value_text(locale, flow.lph, "l/h")),
            ],
        ))
    });

    let leakshield_id = ui.leakshield(state);
    let mut leakshield_choices = vec![none_choice(leakshield_id.is_none())];
    leakshield_choices.extend(
        snapshot
            .leakshields
            .iter()
            .filter(|d| d.connected)
            .map(|d| Choice {
                id: d.id.clone(),
                name: format!("{} {}", locale.tr("nav.leakshield"), d.serial),
                value: None,
                selected: leakshield_id.as_deref() == Some(d.id.as_str()),
            }),
    );
    let has_devices = snapshot.leakshields.iter().any(|d| d.connected);
    let leakshield = PartRow {
        label: locale.tr("first_run.loop.leakshield"),
        shown: leakshield_choices
            .iter()
            .find(|c| c.selected)
            .map_or_else(|| locale.tr("loops.none"), |c| c.name.clone()),
        choices: leakshield_choices,
        multiple: false,
        warning: None,
        note: None,
        write_line: None,
    };

    // The live diagram: a copy of the state that holds the flow of the picks as a sensor.
    let mut preview = state.clone();
    if let (Some(rpm), Some(table)) = (
        rate,
        snapshot
            .calibration_tables
            .iter()
            .find(|t| t.id == table_id),
    ) && let Ok(table) = CalibrationTable::new(table.points.clone())
    {
        let lph = table.flow(rpm).lph;
        let mut sample = preview
            .sensors
            .clone()
            .or(preview.last_sensors.clone())
            .unwrap_or_default();
        sample.sensors.push((DRAFT_FLOW.to_owned(), lph, 0));
        preview.sensors = Some(sample);
        preview.snapshot.sensors.insert(
            DRAFT_FLOW.to_owned(),
            crate::model::SensorInfo {
                id: DRAFT_FLOW.to_owned(),
                name: locale.tr("first_run.loop.flow_name"),
                kind: "flow".to_owned(),
                unit: "l/h".to_owned(),
                origin: "calibrated".to_owned(),
                ..Default::default()
            },
        );
    }
    let preview_input = ScreenInput {
        state: &preview,
        now_us: input.now_us,
        locale,
        down_since_us: input.down_since_us,
        feedback: input.feedback,
    };
    let draft = ui.draft(state, locale.tr("first_run.loop_name"));
    LoopStep {
        pump,
        flow,
        preset,
        live_flow,
        water,
        leakshield,
        diagram: LoopDiagram::build(&preview_input, &draft),
        floor_label: locale.tr("loops.floor.title"),
        floor: ui.floor(),
        floor_text: value_text(locale, ui.floor(), "%"),
        no_leakshield: (!has_devices || leakshield_id.is_none())
            .then(|| locale.tr("first_run.loop.no_leakshield")),
        empty_table_note: (flow_id.is_some() && table_id.is_empty())
            .then(|| locale.tr("first_run.loop.empty_table_note")),
        enabled,
    }
}

fn bios_step(input: &ScreenInput<'_>) -> BiosStep {
    let locale = input.locale;
    let channels = &input.state.snapshot.channels;
    let time_us = channels
        .values()
        .map(|c| c.bios_snapshot.time_us)
        .max()
        .unwrap_or_default();
    BiosStep {
        text: locale.tr("first_run.bios.text"),
        saved: (time_us != 0)
            .then(|| locale.fmt("channels.bios.saved", &[("time", &date_time(time_us))])),
        no_curve: locale.tr("first_run.bios.no_curve"),
        columns: vec![
            locale.tr("first_run.column.channel"),
            locale.tr("channels.control.hardware"),
        ],
        rows: channels
            .values()
            .map(|channel| BiosRow {
                name: channel.name.clone(),
                curve: hardware_curve_text(locale, channel),
            })
            .collect(),
    }
}

fn hardware_curve_text(locale: crate::i18n::Locale, channel: &ChannelInfo) -> String {
    let points = channel.hardware_curve();
    if points.is_empty() {
        return "--".to_owned();
    }
    points
        .iter()
        .map(|(temp, duty)| {
            format!(
                "{}: {}",
                value_text(locale, *temp, "degC"),
                value_text(locale, *duty, "%")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn leakshield_step(input: &ScreenInput<'_>, ui: &FirstRunUi) -> Option<LeakshieldStep> {
    let locale = input.locale;
    let state = input.state;
    let id = ui.leakshield(state)?;
    let device = state.snapshot.leakshields.iter().find(|d| d.id == id)?;
    let unknown = device.read_only_reason == "unknown-firmware";
    let status = if unknown {
        locale.fmt(
            "first_run.leakshield.unknown",
            &[("firmware", &device.firmware.to_string())],
        )
    } else if let Some((total, read_only)) = device.field_counts {
        locale.fmt(
            "first_run.leakshield.known",
            &[
                ("firmware", &device.firmware.to_string()),
                ("verified", &(total - read_only).to_string()),
                ("total", &total.to_string()),
                ("read_only", &read_only.to_string()),
            ],
        )
    } else {
        locale.fmt(
            "first_run.leakshield.known_short",
            &[("firmware", &device.firmware.to_string())],
        )
    };
    Some(LeakshieldStep {
        id: device.id.clone(),
        serial: device.serial.clone(),
        firmware: device.firmware.to_string(),
        status,
        unknown,
        text: locale.tr("first_run.leakshield.text"),
        open_fields: (!unknown).then(|| locale.tr("first_run.leakshield.open_fields")),
    })
}

fn done_step(input: &ScreenInput<'_>, ui: &FirstRunUi) -> DoneStep {
    let locale = input.locale;
    let state = input.state;
    let snapshot = &state.snapshot;
    let found = snapshot.loops.first();
    let name_of = |id: &str| {
        snapshot
            .channels
            .get(id)
            .map_or_else(|| id.to_owned(), |c| c.name.clone())
    };
    let sensor_name = |id: &str| {
        snapshot
            .sensors
            .get(id)
            .map_or_else(|| id.to_owned(), |s| s.name.clone())
    };
    let mut summary = Vec::new();
    if let Some(info) = found {
        summary.push(
            locale.fmt(
                "first_run.done.pump",
                &[(
                    "channels",
                    &info
                        .pump_channels
                        .iter()
                        .map(|id| name_of(id))
                        .collect::<Vec<_>>()
                        .join(", "),
                )],
            ),
        );
        if let Some(flow) = &info.flow_sensor {
            summary.push(locale.fmt("first_run.done.flow", &[("sensor", &sensor_name(flow))]));
        }
        if let Some(water) = &info.water_temperature {
            summary.push(locale.fmt("first_run.done.water", &[("sensor", &sensor_name(water))]));
        }
        summary.push(locale.fmt(
            "first_run.done.floor",
            &[("value", &value_text(locale, info.pump_floor, "%"))],
        ));
    }
    let device = ui
        .leakshield(state)
        .or_else(|| found.and_then(|l| l.leakshield.clone()));
    DoneStep {
        summary,
        text: locale.tr("first_run.done.text"),
        overview: locale.tr("first_run.done.overview"),
        wizard: device.as_ref().map(|_| locale.tr("first_run.done.wizard")),
        wizard_page: device.map(Page::Leakshield),
    }
}
