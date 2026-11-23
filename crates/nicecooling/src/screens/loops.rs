//! The Loops screen (`docs/ui/screens/loops.md`): the list of Loops and the parts of one Loop,
//! with its diagram, its Pump floor and its Calibration table.

use crate::command::Command;
use crate::model::{AppState, FeedState, Link, LoopInfo};
use crate::screens::format::{optional_value, value_text};
use crate::screens::{FeedbackLine, ScreenInput};
use crate::widgets::calibration_editor::{CalibrationEditorModel, CalibrationUi};
use crate::widgets::loop_diagram::LoopDiagram;

/// After 5 s without a Feed the age shows in the Warning color.
const FEED_STALE_S: u64 = 5;

#[derive(Clone, Debug, Default)]
pub struct LoopsUi {
    pub selected: Option<String>,
    /// The rename popover: the Loop and the text of its input.
    pub rename: Option<(String, String)>,
    pub delete_dialog: bool,
    pub calibration: CalibrationUi,
}

impl LoopsUi {
    /// The Loop that the detail column shows: the selected one, or the first.
    pub fn current<'a>(&self, state: &'a AppState) -> Option<&'a LoopInfo> {
        let loops = &state.snapshot.loops;
        self.selected
            .as_ref()
            .and_then(|id| loops.iter().find(|info| &info.id == id))
            .or_else(|| loops.first())
    }

    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
        self.rename = None;
        self.delete_dialog = false;
        self.calibration.reset();
    }

    /// "New": a Loop with a free name.
    pub fn new_loop(&self, input: &ScreenInput<'_>) -> Command {
        let taken = input.state.snapshot.loops.len();
        Command::CreateLoop {
            name: input
                .locale
                .fmt("loops.new_name", &[("n", &(taken + 1).to_string())]),
        }
    }

    pub fn open_rename(&mut self, state: &AppState) {
        if let Some(info) = self.current(state) {
            self.rename = Some((info.id.clone(), info.name.clone()));
        }
    }

    pub fn rename_command(&mut self) -> Option<Command> {
        let (loop_id, name) = self.rename.take()?;
        let name = name.trim().to_owned();
        (!name.is_empty()).then_some(Command::SetLoopName { loop_id, name })
    }

    /// "Delete Loop" in the menu opens the dialog. The button of the dialog deletes.
    pub fn confirm_delete(&mut self, state: &AppState) -> Option<Command> {
        self.delete_dialog = false;
        let loop_id = self.current(state)?.id.clone();
        self.selected = None;
        Some(Command::DeleteLoop { loop_id })
    }

    /// A Channel joins or leaves the Pump channels of the Loop.
    pub fn toggle_pump_channel(&self, state: &AppState, channel: &str) -> Option<Command> {
        let info = self.current(state)?;
        let mut channels = info.pump_channels.clone();
        match channels.iter().position(|id| id == channel) {
            Some(index) => {
                channels.remove(index);
            }
            None => channels.push(channel.to_owned()),
        }
        Some(Command::SetPumpChannels {
            loop_id: info.id.clone(),
            channels,
        })
    }

    /// `None` clears the flow sensor.
    pub fn set_flow_sensor(&self, state: &AppState, sensor: Option<&str>) -> Option<Command> {
        let info = self.current(state)?;
        Some(Command::SetFlowSensor {
            loop_id: info.id.clone(),
            sensor: sensor.unwrap_or_default().to_owned(),
            // A new sensor keeps the table while the Loop has one: the user changes it in the editor.
            table: info.calibration_table.clone(),
        })
    }

    pub fn set_water_temperature(&self, state: &AppState, sensor: Option<&str>) -> Option<Command> {
        Some(Command::SetWaterTemperature {
            loop_id: self.current(state)?.id.clone(),
            sensor: sensor.unwrap_or_default().to_owned(),
        })
    }

    pub fn set_leakshield(&self, state: &AppState, leakshield: Option<&str>) -> Option<Command> {
        Some(Command::SetLoopLeakshield {
            loop_id: self.current(state)?.id.clone(),
            leakshield: leakshield.unwrap_or_default().to_owned(),
        })
    }

    /// The slider and the number input: steps of 1 %, from 0 to 100.
    pub fn set_pump_floor(&self, state: &AppState, duty: f64) -> Option<Command> {
        Some(Command::SetPumpFloor {
            loop_id: self.current(state)?.id.clone(),
            duty: duty.round().clamp(0.0, 100.0),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopsState {
    Loading,
    Empty,
    Ready,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopRow {
    pub id: String,
    pub name: String,
    pub caption: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    /// An empty identifier is "None".
    pub id: String,
    pub name: String,
    /// The live value of a sensor.
    pub value: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartRow {
    pub label: String,
    /// The text of the closed control.
    pub shown: String,
    pub choices: Vec<Choice>,
    /// The row allows several choices: the Pump channels.
    pub multiple: bool,
    pub warning: Option<String>,
    pub note: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloorCard {
    pub title: String,
    pub value: f64,
    pub text: String,
    pub warning: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeedCard {
    pub title: String,
    pub feed_pump: String,
    pub flow: String,
    /// "Sent 1 s ago", or "Last Feed 12 s ago" when stale.
    pub sent: String,
    pub stale: bool,
    pub caption: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeleteDialog {
    pub title: String,
    pub text: String,
    pub cancel: String,
    pub confirm: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopDetail {
    pub id: String,
    pub name: String,
    /// The red row at the top: "Pump stopped: Waterpump reads 0 rpm. Every Channel is at 100 %."
    pub alarm: Option<String>,
    pub diagram: LoopDiagram,
    pub parts_title: String,
    pub pumps: PartRow,
    pub flow_sensor: PartRow,
    pub water: PartRow,
    pub leakshield: PartRow,
    pub floor: FloorCard,
    pub calibration: Option<CalibrationEditorModel>,
    pub feed: Option<FeedCard>,
    pub editable: bool,
    pub disabled_reason: Option<String>,
    pub delete_dialog: Option<DeleteDialog>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopsModel {
    pub state: LoopsState,
    pub empty_title: String,
    pub empty_button: String,
    pub rows: Vec<LoopRow>,
    pub detail: Option<LoopDetail>,
}

impl LoopsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &LoopsUi) -> Self {
        let locale = input.locale;
        let state = input.state;
        let current = ui.current(state);
        let ui_state = if state.link == Link::Connecting {
            LoopsState::Loading
        } else if state.snapshot.loops.is_empty() {
            LoopsState::Empty
        } else {
            LoopsState::Ready
        };
        let rows = state
            .snapshot
            .loops
            .iter()
            .map(|info| LoopRow {
                id: info.id.clone(),
                name: info.name.clone(),
                caption: caption(input, info),
                selected: current.is_some_and(|c| c.id == info.id),
            })
            .collect();
        Self {
            state: ui_state,
            empty_title: locale.tr("overview.empty.title"),
            empty_button: locale.tr("overview.empty.button"),
            rows,
            detail: current.map(|info| detail_of(input, ui, info)),
        }
    }
}

/// "Leakshield, 1 Pump channel".
pub fn caption(input: &ScreenInput<'_>, info: &LoopInfo) -> String {
    let locale = input.locale;
    let has_device = info.leakshield.is_some();
    locale.fmt(
        "loop.caption",
        &[
            (
                "leakshield",
                &locale.tr(if has_device {
                    "loop.with_leakshield"
                } else {
                    "loop.without_leakshield"
                }),
            ),
            (
                "pumps",
                &locale.plural("loop.pumps", info.pump_channels.len()),
            ),
        ],
    )
}

fn write_line(input: &ScreenInput<'_>, key: String) -> Option<FeedbackLine> {
    let locale = input.locale;
    input
        .feedback
        .get(&key)
        .map(|outcome| FeedbackLine::new(outcome, locale, &locale.tr("feedback.refused"), None))
}

fn detail_of(input: &ScreenInput<'_>, ui: &LoopsUi, info: &LoopInfo) -> LoopDetail {
    let locale = input.locale;
    let state = input.state;
    let writes = input.writes();
    let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
    let live = state.sensors.as_ref();
    let value_of =
        |id: &str, unit: &str| optional_value(locale, live.and_then(|s| s.sensor_value(id)), unit);

    // Pump channels: one or more Channels that no other Loop uses.
    let taken: Vec<&String> = state
        .snapshot
        .loops
        .iter()
        .filter(|other| other.id != info.id)
        .flat_map(|other| &other.pump_channels)
        .collect();
    let channel_name = |id: &str| {
        state
            .snapshot
            .channels
            .get(id)
            .map_or_else(|| id.to_owned(), |c| c.name.clone())
    };
    let pumps = PartRow {
        label: locale.tr("loops.part.pumps"),
        shown: if info.pump_channels.is_empty() {
            locale.tr("loops.none")
        } else {
            info.pump_channels
                .iter()
                .map(|id| channel_name(id))
                .collect::<Vec<_>>()
                .join(", ")
        },
        choices: state
            .snapshot
            .channels
            .values()
            .filter(|channel| !taken.contains(&&channel.id))
            .map(|channel| Choice {
                id: channel.id.clone(),
                name: channel.name.clone(),
                value: None,
                selected: info.pump_channels.contains(&channel.id),
            })
            .collect(),
        multiple: true,
        warning: info
            .pump_channels
            .is_empty()
            .then(|| locale.tr("loops.no_pump_warning")),
        note: None,
        write_line: write_line(input, format!("loop-pumps:{}", info.id)),
    };

    let sensor_row = |label: &str,
                      key: String,
                      current: Option<&String>,
                      keep: &dyn Fn(&crate::model::SensorInfo) -> bool| {
        let name_of = |id: &String| {
            state
                .snapshot
                .sensors
                .get(id)
                .map_or_else(|| id.clone(), |s| s.name.clone())
        };
        let mut choices = vec![Choice {
            id: String::new(),
            name: locale.tr("loops.none"),
            value: None,
            selected: current.is_none(),
        }];
        choices.extend(
            state
                .snapshot
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
            shown: current.map_or_else(|| locale.tr("loops.none"), name_of),
            choices,
            multiple: false,
            warning: None,
            note: None,
            write_line: write_line(input, key),
        }
    };
    // A raw flow sensor reads a pulse rate in rpm.
    let flow_sensor = sensor_row(
        "loops.part.flow",
        format!("loop-flow:{}", info.id),
        info.flow_sensor.as_ref(),
        &|s| s.unit == "rpm" && s.origin == "hwmon",
    );
    let water = sensor_row(
        "loops.part.water",
        format!("loop-water:{}", info.id),
        info.water_temperature.as_ref(),
        &|s| s.kind == "temperature",
    );

    // Leakshield: the connected devices that no other Loop uses.
    let used_elsewhere: Vec<&String> = state
        .snapshot
        .loops
        .iter()
        .filter(|other| other.id != info.id)
        .filter_map(|other| other.leakshield.as_ref())
        .collect();
    let mut leakshield_choices = vec![Choice {
        id: String::new(),
        name: locale.tr("loops.none"),
        value: None,
        selected: info.leakshield.is_none(),
    }];
    leakshield_choices.extend(
        state
            .snapshot
            .leakshields
            .iter()
            .filter(|device| device.connected && !used_elsewhere.contains(&&device.id))
            .map(|device| Choice {
                id: device.id.clone(),
                name: format!("{} {}", locale.tr("nav.leakshield"), device.serial),
                value: None,
                selected: info.leakshield.as_ref() == Some(&device.id),
            }),
    );
    let leakshield = PartRow {
        label: locale.tr("loops.part.leakshield"),
        shown: leakshield_choices
            .iter()
            .find(|c| c.selected)
            .map_or_else(|| locale.tr("loops.none"), |c| c.name.clone()),
        choices: leakshield_choices,
        multiple: false,
        warning: None,
        note: info
            .leakshield
            .is_none()
            .then(|| locale.tr("loops.leakshield_none")),
        write_line: write_line(input, format!("loop-leakshield:{}", info.id)),
    };

    let floor = FloorCard {
        title: locale.tr("loops.floor.title"),
        value: info.pump_floor,
        text: locale.tr("loops.floor.text"),
        warning: (info.pump_floor == 0.0).then(|| locale.tr("loops.floor.zero")),
        write_line: write_line(input, format!("loop-floor:{}", info.id)),
    };

    let pump_stopped = state
        .snapshot
        .alarms
        .iter()
        .any(|alarm| alarm.event == "pump-stop" && alarm.source == info.id);
    let alarm = pump_stopped.then(|| {
        let stopped = info
            .pump_channels
            .iter()
            .find(|id| {
                state
                    .fan_sensor(id)
                    .and_then(|fan| sample.and_then(|s| s.sensor_value(&fan)))
                    .is_some_and(|rpm| rpm == 0.0)
            })
            .or(info.pump_channels.first());
        locale.fmt(
            "loops.pump_stopped",
            &[(
                "channel",
                &stopped.map_or_else(String::new, |id| channel_name(id)),
            )],
        )
    });

    let feed = info.leakshield.as_ref().map(|_| feed_card(input, info));
    let delete_dialog = ui.delete_dialog.then(|| DeleteDialog {
        title: locale.fmt("loops.delete.title", &[("name", &info.name)]),
        text: locale.tr("loops.delete.text"),
        cancel: locale.tr("loops.delete.cancel"),
        confirm: locale.tr("loops.delete.confirm"),
    });
    LoopDetail {
        id: info.id.clone(),
        name: info.name.clone(),
        alarm,
        diagram: LoopDiagram::build(input, info),
        parts_title: locale.tr("loops.parts"),
        pumps,
        flow_sensor,
        water,
        leakshield,
        floor,
        calibration: CalibrationEditorModel::build(input, &ui.calibration, info),
        feed,
        editable: writes.enabled,
        disabled_reason: writes.tooltip,
        delete_dialog,
    }
}

/// The Feed card shows what the daemon reports: the time and the values of the last good send.
/// Before the first send it shows "Not sent yet". After a send it shows "Sent N s ago", and
/// "Last Feed N s ago" in the Warning color when the Feed is not `ok` or the send is older than 5 s.
fn feed_card(input: &ScreenInput<'_>, info: &LoopInfo) -> FeedCard {
    let locale = input.locale;
    let state = input.state;
    let device = info
        .leakshield
        .as_ref()
        .and_then(|id| state.snapshot.leakshields.iter().find(|d| &d.id == id));
    let feed = device.map(|d| d.last_feed).unwrap_or_default();
    let feed_state = device.map_or(FeedState::Off, |d| d.feed_state);
    let sent_once = feed.time_us != 0;
    let (feed_pump, flow) = if sent_once {
        (Some(feed.feed_pump), Some(feed.flow_lph))
    } else {
        (None, None)
    };
    let age = sent_once.then(|| input.now_us.saturating_sub(feed.time_us) / 1_000_000);
    let stale = sent_once
        && (!state.is_online()
            || !matches!(feed_state, FeedState::Ok)
            || age.is_some_and(|age| age > FEED_STALE_S));
    FeedCard {
        title: locale.tr("loops.feed.title"),
        feed_pump: locale.fmt(
            "loops.feed.feed_pump",
            &[("value", &optional_value(locale, feed_pump, "rpm"))],
        ),
        flow: locale.fmt(
            "loops.feed.flow",
            &[("value", &optional_value(locale, flow, "l/h"))],
        ),
        sent: match age {
            Some(age) if stale => locale.fmt("loops.feed.stale", &[("seconds", &age.to_string())]),
            Some(age) => locale.fmt("loops.feed.sent", &[("seconds", &age.to_string())]),
            None => locale.tr("loops.feed.never"),
        },
        stale,
        caption: locale.tr("loops.feed.caption"),
    }
}

/// The slider and the number input show the same text.
pub fn floor_text(locale: crate::i18n::Locale, duty: f64) -> String {
    value_text(locale, duty, "%")
}
