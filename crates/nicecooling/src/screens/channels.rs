//! The Channels screen (`docs/ui/screens/channels.md`): the list of Channels and the control
//! of one Channel, with the Curve editor.

use nicecooling_core::curve::{DEFAULT_SMOOTHING_S, DEFAULT_STEP_LIMIT};

use crate::command::Command;
use crate::model::{AppState, ChannelInfo, ChannelOverride, Control, Link, SensorSample};
use crate::screens::format::{date_time, number, optional_value, unit_text, value_text};
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};
use crate::widgets::curve_editor::{
    MAX_POINTS, MIN_POINTS, PlotScale, add_point_after, add_point_at, move_point, remove_point,
};

const DEFAULT_POINTS: [(f64, f64); 3] = [(30.0, 40.0), (45.0, 70.0), (60.0, 100.0)];

/// The Curve that the user edits, before the daemon answers.
#[derive(Clone, Debug, PartialEq)]
pub struct CurveDraft {
    pub channel: String,
    pub input: String,
    pub points: Vec<(f64, f64)>,
    pub smoothing_s: f64,
    pub step_limit: f64,
}

impl CurveDraft {
    fn command(&self) -> Command {
        Command::SetCurve {
            channel: self.channel.clone(),
            input: self.input.clone(),
            points: self.points.clone(),
            smoothing_s: self.smoothing_s,
            step_limit: self.step_limit,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChannelsUi {
    pub selected: Option<String>,
    pub selected_point: usize,
    pub draft: Option<CurveDraft>,
    /// The dialog "Give this Channel back to the Board?" is open.
    pub release_dialog: bool,
    /// The rename popover: the Channel and the text of its input.
    pub rename: Option<(String, String)>,
    pub bios_open: bool,
    /// The refusal of a Fixed Duty below the Pump floor.
    pub fixed_error: Option<String>,
    /// What "Try again" sends.
    pub last_command: Option<Command>,
}

/// The Pump floor of a Pump channel: the floor of the Loop that has it.
pub fn pump_floor(state: &AppState, channel: &str) -> Option<f64> {
    state
        .snapshot
        .loops
        .iter()
        .find(|info| info.pump_channels.iter().any(|id| id == channel))
        .map(|info| info.pump_floor)
}

fn default_input(state: &AppState, channel: &str) -> Option<String> {
    state
        .snapshot
        .loops
        .iter()
        .find(|info| info.pump_channels.iter().any(|id| id == channel))
        .and_then(|info| info.water_temperature.clone())
        .or_else(|| {
            state
                .snapshot
                .sensors
                .values()
                .find(|sensor| sensor.kind == "temperature")
                .map(|sensor| sensor.id.clone())
        })
}

impl ChannelsUi {
    /// The Channel that the detail column shows: the selected one, or the first.
    pub fn current<'a>(&self, state: &'a AppState) -> Option<&'a ChannelInfo> {
        let channels = &state.snapshot.channels;
        self.selected
            .as_ref()
            .and_then(|id| channels.get(id))
            .or_else(|| channels.values().next())
    }

    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
        self.selected_point = 0;
        self.draft = None;
        self.release_dialog = false;
        self.rename = None;
        self.fixed_error = None;
        self.last_command = None;
    }

    /// Drop the draft when the daemon already holds it.
    pub fn sync(&mut self, state: &AppState) {
        let Some(draft) = &self.draft else { return };
        let Some(channel) = state.snapshot.channels.get(&draft.channel) else {
            self.draft = None;
            return;
        };
        let held = channel.control == Control::Curve
            && channel.curve.input.as_deref() == Some(&draft.input)
            && channel.curve.points == draft.points
            && channel.curve.smoothing_s == draft.smoothing_s
            && channel.curve.step_limit == draft.step_limit;
        if held {
            self.draft = None;
        }
    }

    /// The Curve of the selected Channel: the draft, or the one that the daemon holds.
    fn working(&mut self, state: &AppState) -> Option<&mut CurveDraft> {
        let channel = self.current(state)?;
        if self.draft.as_ref().is_none_or(|d| d.channel != channel.id) {
            if channel.control != Control::Curve {
                return None;
            }
            self.draft = Some(CurveDraft {
                channel: channel.id.clone(),
                input: channel.curve.input.clone()?,
                points: channel.curve.points.clone(),
                smoothing_s: channel.curve.smoothing_s,
                step_limit: channel.curve.step_limit,
            });
        }
        self.draft.as_mut()
    }

    /// The draft goes to the daemon when a drag ends or an input loses focus.
    pub fn commit(&mut self) -> Option<Command> {
        let command = self.draft.as_ref()?.command();
        self.last_command = Some(command.clone());
        Some(command)
    }

    /// A drag step: the point moves, and nothing goes to the daemon yet.
    pub fn drag_point(&mut self, state: &AppState, index: usize, x: f64, y: f64) -> bool {
        let floor = self.current(state).and_then(|c| pump_floor(state, &c.id));
        let changed = self
            .working(state)
            .is_some_and(|draft| move_point(&mut draft.points, index, x, y, floor));
        self.selected_point = index;
        changed
    }

    /// A value typed in a number input. A value that changes nothing sends nothing.
    pub fn set_point(&mut self, state: &AppState, index: usize, x: f64, y: f64) -> Option<Command> {
        self.drag_point(state, index, x, y)
            .then(|| self.commit())
            .flatten()
    }

    /// A double click on the line, or "Add point".
    pub fn add_point(&mut self, state: &AppState, at: Option<f64>) -> Option<Command> {
        let floor = self.current(state).and_then(|c| pump_floor(state, &c.id));
        let selected = self.selected_point;
        let draft = self.working(state)?;
        let index = match at {
            Some(x) => add_point_at(&mut draft.points, x, floor),
            None => add_point_after(&mut draft.points, selected, floor),
        }?;
        self.selected_point = index;
        self.commit()
    }

    /// A double click on a point, Delete, or "Remove point".
    pub fn remove_point(&mut self, state: &AppState, index: usize) -> Option<Command> {
        let draft = self.working(state)?;
        if !remove_point(&mut draft.points, index) {
            return None;
        }
        self.selected_point = self.selected_point.min(
            self.draft
                .as_ref()
                .map_or(0, |d| d.points.len().saturating_sub(1)),
        );
        self.commit()
    }

    pub fn set_input(&mut self, state: &AppState, sensor: &str) -> Option<Command> {
        self.working(state)?.input = sensor.to_owned();
        self.commit()
    }

    pub fn set_smoothing(&mut self, state: &AppState, seconds: f64) -> Option<Command> {
        self.working(state)?.smoothing_s = seconds.clamp(0.0, 60.0);
        self.commit()
    }

    pub fn set_step_limit(&mut self, state: &AppState, percent: f64) -> Option<Command> {
        self.working(state)?.step_limit = percent.clamp(0.1, 100.0);
        self.commit()
    }

    /// A click on "Curve", "Fixed Duty" or "Hardware curve". A change applies at once, except the
    /// way back to the Board: the dialog asks first.
    pub fn request_control(&mut self, state: &AppState, control: Control) -> Option<Command> {
        let channel = self.current(state)?.clone();
        self.fixed_error = None;
        let command = match (channel.control, control) {
            (current, wanted) if current == wanted => return None,
            (_, Control::Hardware) => {
                self.release_dialog = true;
                return None;
            }
            (_, Control::Curve) => {
                let floor = pump_floor(state, &channel.id);
                let input = channel
                    .curve
                    .input
                    .clone()
                    .or_else(|| default_input(state, &channel.id))?;
                let mut points = if channel.curve.points.len() >= MIN_POINTS {
                    channel.curve.points.clone()
                } else {
                    DEFAULT_POINTS.to_vec()
                };
                for point in &mut points {
                    point.1 = point.1.max(floor.unwrap_or(0.0).ceil());
                }
                let spec = &channel.curve;
                self.draft = Some(CurveDraft {
                    channel: channel.id.clone(),
                    input,
                    points,
                    smoothing_s: if spec.smoothing_s > 0.0 {
                        spec.smoothing_s
                    } else {
                        DEFAULT_SMOOTHING_S
                    },
                    step_limit: if spec.step_limit > 0.0 {
                        spec.step_limit
                    } else {
                        DEFAULT_STEP_LIMIT
                    },
                });
                self.draft.as_ref()?.command()
            }
            (_, Control::Fixed) => {
                let floor = pump_floor(state, &channel.id).unwrap_or(0.0);
                Command::SetFixedDuty {
                    channel: channel.id.clone(),
                    duty: channel.fixed_duty.max(floor),
                }
            }
        };
        self.last_command = Some(command.clone());
        Some(command)
    }

    /// "Use the Hardware curve" in the dialog.
    pub fn confirm_release(&mut self, state: &AppState) -> Option<Command> {
        self.release_dialog = false;
        let channel = self.current(state)?.id.clone();
        self.draft = None;
        let command = Command::ReleaseToHardware { channel };
        self.last_command = Some(command.clone());
        Some(command)
    }

    pub fn cancel_release(&mut self) {
        self.release_dialog = false;
    }

    /// A Fixed Duty under the Pump floor is refused here: "The Pump floor is 40 %.".
    pub fn set_fixed_duty(&mut self, input: &ScreenInput<'_>, duty: f64) -> Option<Command> {
        let channel = self.current(input.state)?.id.clone();
        let duty = duty.round().clamp(0.0, 100.0);
        if let Some(floor) = pump_floor(input.state, &channel)
            && duty < floor
        {
            self.fixed_error = Some(input.locale.fmt(
                "channels.pump_floor_refused",
                &[("value", &value_text(input.locale, floor, "%"))],
            ));
            return None;
        }
        self.fixed_error = None;
        let command = Command::SetFixedDuty { channel, duty };
        self.last_command = Some(command.clone());
        Some(command)
    }

    pub fn open_rename(&mut self, state: &AppState) {
        if let Some(channel) = self.current(state) {
            self.rename = Some((channel.id.clone(), channel.name.clone()));
        }
    }

    /// Enter in the rename popover. An empty name restores the Board file name.
    pub fn rename_command(&mut self) -> Option<Command> {
        let (channel, name) = self.rename.take()?;
        Some(Command::SetChannelName {
            channel,
            name: name.trim().to_owned(),
        })
    }
}

// ---- The model ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelsState {
    Loading,
    Empty,
    Ready,
}

/// The Channel that Up or Down selects in the list: the next row, or `None` at the end.
pub fn neighbor(rows: &[ChannelRow], down: bool) -> Option<String> {
    let at = rows.iter().position(|row| row.selected)?;
    let next = if down {
        at.checked_add(1)?
    } else {
        at.checked_sub(1)?
    };
    rows.get(next).map(|row| row.id.clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelRow {
    pub id: String,
    pub name: String,
    /// "Curve on Water temperature", "Fixed Duty", "Hardware curve, not controlled".
    pub line2: String,
    /// The triangle: the Input of the Curve is lost.
    pub line2_warning: bool,
    pub pump_tag: Option<String>,
    /// The speed without its unit: the column title has it.
    pub rpm: String,
    /// "no speed signal", with the dashed-circle icon.
    pub no_signal: Option<String>,
    pub duty: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlButton {
    pub control: Control,
    pub label: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputOption {
    pub id: String,
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputField {
    pub label: String,
    pub sensor: Option<String>,
    pub name: String,
    pub value: String,
    pub options: Vec<InputOption>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Operating {
    pub x: f64,
    pub y: f64,
    /// "Now 31.4 °C, 62 %".
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurvePlot {
    pub points: Vec<(f64, f64)>,
    pub selected: usize,
    pub scale: PlotScale,
    pub x_title: String,
    pub y_title: String,
    pub floor: Option<(f64, String)>,
    pub operating: Option<Operating>,
    /// The drag handles show only while the editor is writable.
    pub handles: bool,
    /// The text equivalent of the state, for the accessible name.
    pub accessible: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Readout {
    pub input: String,
    pub smoothing: String,
    pub curve: String,
    pub duty: String,
    /// The values of `input`, `curve` and `duty`: the view shows them in bold.
    pub values: [String; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveDetail {
    pub input: InputField,
    /// "°C": the unit of the Input, for the point inputs.
    pub input_unit: String,
    /// The text under the fields while the Input is a Graph sensor. A Graph sensor has no value
    /// while the app does not run.
    pub input_note: Option<String>,
    pub smoothing_label: String,
    pub smoothing_unit: String,
    pub smoothing_s: f64,
    pub smoothing_help: String,
    pub step_label: String,
    pub step_unit: String,
    pub step_limit: f64,
    pub step_help: String,
    pub plot: CurvePlot,
    pub readout: Readout,
    pub points: Vec<(f64, f64)>,
    pub can_add: bool,
    pub can_remove: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FixedDetail {
    pub duty: f64,
    pub floor: Option<f64>,
    pub floor_text: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BiosPanel {
    pub title: String,
    pub text: String,
    /// "Saved 2026-11-20 08:14". `None` when the daemon holds no snapshot.
    pub saved: Option<String>,
    /// The Hardware curve of the snapshot: the two column titles. Empty without auto points.
    pub columns: Option<(String, String)>,
    /// One row per auto point: the temperature and the Duty.
    pub rows: Vec<(String, String)>,
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseDialog {
    pub title: String,
    pub text: String,
    pub cancel: String,
    pub confirm: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelDetail {
    pub id: String,
    pub name: String,
    pub pump_tag: Option<String>,
    pub controls: Vec<ControlButton>,
    pub control: Control,
    pub editable: bool,
    /// Why the controls are disabled.
    pub disabled_reason: Option<String>,
    /// The Warning strip above the plot.
    pub lost_input: Option<String>,
    pub curve: Option<CurveDetail>,
    pub fixed: Option<FixedDetail>,
    /// The text of the Hardware curve state.
    pub hardware_text: Option<String>,
    pub write_line: Option<FeedbackLine>,
    pub bios: BiosPanel,
    pub release_dialog: Option<ReleaseDialog>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelsModel {
    pub state: ChannelsState,
    /// The title of the list card, and the Board as its caption.
    pub title: String,
    pub board: String,
    pub empty_text: String,
    pub empty_link: String,
    pub rows: Vec<ChannelRow>,
    /// The red row at the top of the detail while a pump-stop Alarm is active.
    pub pump_stop: Option<String>,
    pub detail: Option<ChannelDetail>,
}

impl ChannelsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &ChannelsUi) -> Self {
        let locale = input.locale;
        let state = input.state;
        let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
        let pump_stopped = state
            .snapshot
            .alarms
            .iter()
            .any(|alarm| alarm.event == "pump-stop");
        let current = ui.current(state).map(|c| c.id.clone());

        let rows = state
            .snapshot
            .channels
            .values()
            .map(|channel| row_of(input, channel, sample, pump_stopped, current.as_deref()))
            .collect();

        let ui_state = if state.link == Link::Connecting {
            ChannelsState::Loading
        } else if state.snapshot.channels.is_empty() {
            ChannelsState::Empty
        } else {
            ChannelsState::Ready
        };
        Self {
            state: ui_state,
            title: locale.tr("nav.channels"),
            board: state
                .snapshot
                .board
                .as_ref()
                .map_or_else(String::new, |board| board.model.clone()),
            empty_text: locale.tr("channels.empty.text"),
            empty_link: locale.tr("channels.empty.link"),
            rows,
            pump_stop: pump_stopped.then(|| locale.tr("channels.pump_stop")),
            detail: ui
                .current(state)
                .map(|channel| detail_of(input, ui, channel, sample, pump_stopped)),
        }
    }
}

fn input_name(state: &AppState, id: Option<&str>) -> String {
    id.and_then(|id| state.snapshot.sensors.get(id))
        .map_or_else(String::new, |sensor| sensor.name.clone())
}

fn row_of(
    input: &ScreenInput<'_>,
    channel: &ChannelInfo,
    sample: Option<&SensorSample>,
    pump_stopped: bool,
    current: Option<&str>,
) -> ChannelRow {
    let locale = input.locale;
    let state = input.state;
    let lost = channel.override_state == ChannelOverride::LostInput;
    let name = input_name(state, channel.curve.input.as_deref());
    let (line2, warning) = match channel.control {
        Control::Curve if lost => (
            locale.fmt("channels.line.curve_lost", &[("input", &name)]),
            true,
        ),
        Control::Curve => (
            locale.fmt("channels.line.curve", &[("input", &name)]),
            false,
        ),
        Control::Fixed => (locale.tr("channels.line.fixed"), false),
        Control::Hardware => (locale.tr("channels.line.hardware"), false),
    };
    let rpm = channel
        .fan_sensor
        .as_ref()
        .filter(|id| state.snapshot.sensors.contains_key(*id))
        .and_then(|id| sample.and_then(|s| s.sensor_value(id)));
    let duty = if pump_stopped {
        Some(100.0)
    } else {
        sample.and_then(|s| s.channel_duty(&channel.id))
    };
    ChannelRow {
        id: channel.id.clone(),
        name: channel.name.clone(),
        line2,
        line2_warning: warning,
        pump_tag: channel.is_pump.then(|| locale.tr("channels.pump_tag")),
        rpm: rpm.map_or_else(|| "--".to_owned(), |v| number(locale, v, "rpm")),
        no_signal: rpm.is_none().then(|| locale.tr("channels.no_signal")),
        duty: optional_value(locale, duty, "%"),
        selected: current == Some(&channel.id),
    }
}

fn detail_of(
    input: &ScreenInput<'_>,
    ui: &ChannelsUi,
    channel: &ChannelInfo,
    sample: Option<&SensorSample>,
    pump_stopped: bool,
) -> ChannelDetail {
    let locale = input.locale;
    let state = input.state;
    let writes = input.writes();
    let editable = writes.enabled && !pump_stopped;
    let disabled_reason = if !writes.enabled {
        writes.tooltip.clone()
    } else if pump_stopped {
        Some(locale.tr("channels.pump_stop_readonly"))
    } else {
        None
    };
    let floor = channel
        .is_pump
        .then(|| pump_floor(state, &channel.id))
        .flatten();
    let lost =
        channel.override_state == ChannelOverride::LostInput && channel.control == Control::Curve;

    let controls = [
        (Control::Curve, "channels.control.curve"),
        (Control::Fixed, "channels.control.fixed"),
        (Control::Hardware, "channels.control.hardware"),
    ]
    .into_iter()
    .map(|(control, key)| ControlButton {
        control,
        label: locale.tr(key),
        active: channel.control == control,
    })
    .collect();

    let key = format!("channel:{}", channel.id);
    let write_line = input.feedback.get(&key).map(|outcome| {
        let refused = match (&ui.last_command, outcome) {
            (_, WriteOutcome::Refused { error }) if error == "InvalidArgument" => {
                locale.tr("channels.refused_curve")
            }
            (Some(Command::SetFixedDuty { .. }), _) => locale.tr("channels.refused_fixed"),
            (Some(Command::ReleaseToHardware { .. }), _) => locale.tr("channels.refused_release"),
            _ => locale.tr("channels.refused_curve"),
        };
        FeedbackLine::new(
            outcome,
            locale,
            &refused,
            Some(locale.tr("channels.try_again")),
        )
    });

    let (curve, fixed, hardware_text) = match channel.control {
        Control::Curve => (
            Some(curve_detail(
                input, ui, channel, sample, floor, editable, lost,
            )),
            None,
            None,
        ),
        Control::Fixed => (
            None,
            Some(FixedDetail {
                duty: channel.fixed_duty,
                floor,
                floor_text: floor.map(|f| {
                    locale.fmt(
                        "channels.pump_floor_note",
                        &[("value", &value_text(locale, f, "%"))],
                    )
                }),
                error: ui.fixed_error.clone(),
            }),
            None,
        ),
        Control::Hardware => (None, None, Some(locale.tr("channels.hardware_text"))),
    };

    ChannelDetail {
        id: channel.id.clone(),
        name: channel.name.clone(),
        pump_tag: channel.is_pump.then(|| locale.tr("channels.pump_tag")),
        controls,
        control: channel.control,
        editable,
        disabled_reason,
        lost_input: lost.then(|| {
            locale.tr("channels.lost_input_strip").replace(
                "%{input}",
                &input_name(state, channel.curve.input.as_deref()),
            )
        }),
        curve,
        fixed,
        hardware_text,
        write_line,
        bios: bios_panel(input, ui, channel),
        release_dialog: ui.release_dialog.then(|| ReleaseDialog {
            title: locale.tr("channels.release.title"),
            text: locale.tr("channels.release.text"),
            cancel: locale.tr("channels.release.cancel"),
            confirm: locale.tr("channels.release.confirm"),
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn curve_detail(
    input: &ScreenInput<'_>,
    ui: &ChannelsUi,
    channel: &ChannelInfo,
    sample: Option<&SensorSample>,
    floor: Option<f64>,
    editable: bool,
    lost: bool,
) -> CurveDetail {
    let locale = input.locale;
    let state = input.state;
    let draft = ui.draft.as_ref().filter(|d| d.channel == channel.id);
    let (input_id, points, smoothing, step) = match draft {
        Some(d) => (
            Some(d.input.clone()),
            d.points.clone(),
            d.smoothing_s,
            d.step_limit,
        ),
        None => (
            channel.curve.input.clone(),
            channel.curve.points.clone(),
            channel.curve.smoothing_s,
            channel.curve.step_limit,
        ),
    };
    let sensor = input_id
        .as_ref()
        .and_then(|id| state.snapshot.sensors.get(id));
    let unit = sensor.map_or("degC", |s| s.unit.as_str());
    let live_input = input_id
        .as_ref()
        .and_then(|id| sample.and_then(|s| s.sensor_value(id)))
        .filter(|_| state.is_online() && !lost);
    let duty_now = sample.and_then(|s| s.channel_duty(&channel.id));
    let readout = sample
        .and_then(|s| s.channel_readout(&channel.id))
        .filter(|_| state.is_online());
    let scale = PlotScale::for_points(&points, live_input);
    let operating = live_input.zip(duty_now).map(|(x, y)| Operating {
        x,
        y,
        label: locale.fmt(
            "channels.plot.now",
            &[
                ("input", &value_text(locale, x, unit)),
                ("duty", &number(locale, y, "%")),
            ],
        ),
    });
    let options = state
        .snapshot
        .sensors
        .values()
        .filter(|s| s.kind == "temperature")
        .map(|s| InputOption {
            id: s.id.clone(),
            name: s.name.clone(),
            value: optional_value(locale, sample.and_then(|m| m.sensor_value(&s.id)), &s.unit),
        })
        .collect();
    let unit_label = crate::screens::format::unit_text(unit);
    let readout_values = [
        optional_value(locale, readout.map(|(input, _)| input), unit),
        optional_value(locale, readout.map(|(_, target)| target), "%"),
        optional_value(locale, duty_now, "%"),
    ];
    let plot = CurvePlot {
        accessible: locale.fmt(
            "channels.plot.accessible",
            &[
                ("count", &points.len().to_string()),
                ("name", &channel.name),
            ],
        ),
        points: points.clone(),
        selected: ui.selected_point.min(points.len().saturating_sub(1)),
        scale,
        x_title: locale.fmt("channels.plot.x_title", &[("unit", unit_label)]),
        y_title: locale.tr("channels.plot.y_title"),
        floor: floor.map(|f| {
            (
                f,
                locale.fmt(
                    "channels.plot.floor",
                    &[("value", &value_text(locale, f, "%"))],
                ),
            )
        }),
        operating: operating.filter(|_| state.is_online()),
        handles: editable,
    };
    CurveDetail {
        input: InputField {
            label: locale.tr("channels.field.input"),
            sensor: input_id.clone(),
            name: sensor.map_or_else(String::new, |s| s.name.clone()),
            value: optional_value(
                locale,
                input_id
                    .as_ref()
                    .and_then(|id| sample.and_then(|s| s.sensor_value(id))),
                unit,
            ),
            options,
        },
        smoothing_label: locale.tr("channels.field.smoothing"),
        smoothing_unit: locale.tr("channels.field.smoothing_unit"),
        smoothing_s: smoothing,
        smoothing_help: locale.fmt(
            "channels.help.smoothing",
            &[("value", &number(locale, smoothing, "%"))],
        ),
        step_label: locale.tr("channels.field.step"),
        step_unit: locale.tr("channels.field.step_unit"),
        step_limit: step,
        step_help: locale.fmt(
            "channels.help.step",
            &[("value", &number(locale, step, "%"))],
        ),
        input_unit: unit_label.to_owned(),
        input_note: sensor
            .filter(|s| s.origin == "graph")
            .map(|_| locale.tr("channels.graph_input_note")),
        readout: Readout {
            values: readout_values.clone(),
            input: locale.fmt("channels.readout.input", &[("value", &readout_values[0])]),
            smoothing: locale.fmt(
                "channels.readout.smoothing",
                &[("value", &number(locale, smoothing, "%"))],
            ),
            curve: locale.fmt("channels.readout.curve", &[("value", &readout_values[1])]),
            duty: locale.fmt("channels.readout.duty", &[("value", &readout_values[2])]),
        },
        can_add: points.len() < MAX_POINTS,
        can_remove: points.len() > MIN_POINTS,
        points,
        plot,
    }
}

fn bios_panel(input: &ScreenInput<'_>, ui: &ChannelsUi, channel: &ChannelInfo) -> BiosPanel {
    let locale = input.locale;
    let snapshot = &channel.bios_snapshot;
    let curve = channel.hardware_curve();
    BiosPanel {
        title: locale.tr("channels.bios.title"),
        text: locale.tr("channels.bios.text"),
        saved: (snapshot.time_us != 0).then(|| {
            locale.fmt(
                "channels.bios.saved",
                &[("time", &date_time(snapshot.time_us))],
            )
        }),
        columns: (!curve.is_empty()).then(|| {
            (
                locale.fmt("channels.plot.x_title", &[("unit", unit_text("degC"))]),
                locale.tr("channels.plot.y_title"),
            )
        }),
        rows: curve
            .iter()
            .map(|(temp, duty)| {
                (
                    value_text(locale, *temp, "degC"),
                    value_text(locale, *duty, "%"),
                )
            })
            .collect(),
        // With the Hardware curve the snapshot is what the Channel runs: it always shows.
        open: ui.bios_open || channel.control == Control::Hardware,
    }
}
