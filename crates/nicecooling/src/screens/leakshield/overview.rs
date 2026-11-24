//! The Overview tab and the Action views (`docs/ui/screens/leakshield-overview.md`).

use nicecooling_core::loops::tach_sensor;

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{AppState, PressureLimits, Severity};
use crate::screens::format::{clock_minutes, compact, number, optional_value, plain, value_text};
use crate::screens::leakshield::charts::GaugeLabels;
use crate::screens::leakshield::{Device, TabState, change_text, severity_of, write_line};
use crate::screens::ui::TagKind;
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};

/// The Test pressure that the device accepts, in mbar.
pub const TEST_PRESSURE_RANGE: (f64, f64) = (80.0, 450.0);
const MODES: [&str; 3] = ["shield", "monitor", "release"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TestSource {
    #[default]
    Pressure,
    Difference,
    Change,
}

impl TestSource {
    pub const ALL: [TestSource; 3] = [Self::Pressure, Self::Difference, Self::Change];

    pub fn key(self) -> &'static str {
        match self {
            Self::Pressure => "leakshield.overview.source_of.pressure",
            Self::Difference => "leakshield.overview.source_of.difference",
            Self::Change => "leakshield.overview.source_of.change",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartRange {
    Hour,
    #[default]
    Day,
    Week,
}

impl ChartRange {
    pub const ALL: [ChartRange; 3] = [Self::Hour, Self::Day, Self::Week];

    pub fn seconds(self) -> i64 {
        match self {
            Self::Hour => 3_600,
            Self::Day => 86_400,
            Self::Week => 604_800,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Hour => "leakshield.overview.range.hour",
            Self::Day => "leakshield.overview.range.day",
            Self::Week => "leakshield.overview.range.week",
        }
    }
}

/// A question that waits for an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    Mode(String),
    Measure,
    /// "Start Refill?" while a Pump channel of the Loop runs.
    Refill,
    /// The four steps of a Refill.
    RefillSteps,
}

/// The pressure of one range of the chart, as the history database has it.
#[derive(Clone, Debug, Default)]
pub struct PressureHistory {
    /// The Leakshield and the range of `runs`.
    pub key: Option<(String, ChartRange)>,
    /// The Unix second of the last read, 0 before the first one.
    pub loaded_at: i64,
    /// A read runs.
    pub pending: bool,
    /// The runs of `(seconds, mbar)`.
    pub runs: crate::screens::leakshield::charts::Runs,
}

#[derive(Clone, Debug, Default)]
pub struct OverviewUi {
    pub confirm: Option<Confirm>,
    /// The setup of a Test shows in place of the charts.
    pub test_setup: bool,
    pub test_pressure: Option<String>,
    pub source: TestSource,
    /// The pressure that the user took as the reference of a Test.
    pub reference: Option<f64>,
    pub range: ChartRange,
    pub history: PressureHistory,
}

fn rank(mode: &str) -> u8 {
    match mode {
        "release" => 0,
        "monitor" => 1,
        _ => 2,
    }
}

impl OverviewUi {
    /// A click on a Mode button. A Mode that lowers the protection asks first.
    pub fn request_mode(&mut self, state: &AppState, device: &str, mode: &str) -> Option<Command> {
        let current = state
            .snapshot
            .leakshields
            .iter()
            .find(|d| d.id == device)
            .map_or("", |d| d.mode.as_str());
        if current == mode || !MODES.contains(&mode) {
            return None;
        }
        if rank(mode) < rank(current) {
            self.confirm = Some(Confirm::Mode(mode.to_owned()));
            return None;
        }
        Some(mode_command(device, mode))
    }

    pub fn confirm(&mut self, device: &str) -> Option<Command> {
        match self.confirm.take()? {
            Confirm::Mode(mode) => Some(mode_command(device, &mode)),
            Confirm::Measure => Some(Command::StartLeakshieldAction {
                leakshield: device.to_owned(),
                action: "measure-fill-level".to_owned(),
                test_pressure: None,
            }),
            Confirm::Refill => Some(refill_command(device)),
            Confirm::RefillSteps => None,
        }
    }

    pub fn cancel_dialog(&mut self) {
        self.confirm = None;
    }

    /// A click on an Action button. The Test opens its setup, the measurement asks first. The
    /// Refill asks only while a Pump channel of the Loop runs.
    pub fn request_action(
        &mut self,
        state: &AppState,
        device: &str,
        action: &str,
    ) -> Option<Command> {
        match action {
            "test" => {
                self.test_setup = true;
                None
            }
            "measure-fill-level" => {
                self.confirm = Some(Confirm::Measure);
                None
            }
            "deaerate" => Some(Command::StartLeakshieldAction {
                leakshield: device.to_owned(),
                action: "deaerate".to_owned(),
                test_pressure: None,
            }),
            "refill" => {
                if pump_runs(state, device) {
                    self.confirm = Some(Confirm::Refill);
                    return None;
                }
                Some(refill_command(device))
            }
            _ => None,
        }
    }

    /// "Start Test". The pressure comes from the input, or from the field of the device.
    pub fn start_test(&mut self, input: &ScreenInput<'_>, device: &str) -> Option<Command> {
        let pressure = self.test_pressure_value(input, device)?;
        self.test_setup = false;
        self.reference = None;
        Some(Command::StartLeakshieldAction {
            leakshield: device.to_owned(),
            action: "test".to_owned(),
            test_pressure: Some(pressure),
        })
    }

    fn test_pressure_value(&self, input: &ScreenInput<'_>, device: &str) -> Option<f64> {
        let value = match &self.test_pressure {
            Some(text) => text.trim().replace(',', ".").parse().ok()?,
            None => Device::new(input, device).number("test_pressure")?,
        };
        (TEST_PRESSURE_RANGE.0..=TEST_PRESSURE_RANGE.1)
            .contains(&value)
            .then_some(value)
    }

    pub fn cancel_test_setup(&mut self) {
        self.test_setup = false;
    }
}

fn refill_command(device: &str) -> Command {
    Command::StartLeakshieldAction {
        leakshield: device.to_owned(),
        action: "refill".to_owned(),
        test_pressure: None,
    }
}

/// True when a Pump channel of a Loop of this Leakshield reads above 0 rpm or has no tach value.
/// A Pump channel `<chip>_pwmN` reads as the sensor `<chip>_fanN`. Only the current sample counts:
/// after a reconnect the last one can be old.
pub fn pump_runs(state: &AppState, device: &str) -> bool {
    let sample = state.sensors.as_ref();
    state
        .snapshot
        .loops
        .iter()
        .filter(|l| l.leakshield.as_deref() == Some(device))
        .flat_map(|l| &l.pump_channels)
        .any(|channel| {
            tach_sensor(channel)
                .and_then(|tach| sample?.sensor_value(&tach))
                .is_none_or(|rpm| rpm > 0.0)
        })
}

fn mode_command(device: &str, mode: &str) -> Command {
    Command::ChangeLeakshieldMode {
        leakshield: device.to_owned(),
        mode: mode.to_owned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GaugeState {
    InRange,
    Below,
    Above,
    Offline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PressureCard {
    pub title: String,
    /// "negative pressure in the Loop".
    pub caption: String,
    pub value: Option<f64>,
    pub value_text: String,
    /// The value without its unit, or "--".
    pub number: String,
    pub unit: String,
    pub labels: GaugeLabels,
    pub limits: Option<PressureLimits>,
    pub gauge: GaugeState,
    pub tag: String,
    pub tag_kind: TagKind,
    /// The marker and the value are red.
    pub alarm: bool,
    pub tooltip: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusRow {
    pub label: String,
    pub value: String,
    /// A measured value is bold. The Feed is what the PC sent: regular weight.
    pub bold: bool,
    /// The time or the age of the value, after it.
    pub caption: Option<String>,
    pub severity: Option<Severity>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusCard {
    pub title: String,
    pub tag: String,
    pub tag_kind: TagKind,
    pub rows: Vec<StatusRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModeButton {
    pub mode: String,
    pub label: String,
    pub selected: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionButton {
    pub action: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RefillRow {
    pub text: String,
    /// The bytes of `text` that are the name of the Action: they are bold.
    pub action: std::ops::Range<usize>,
    pub link: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModeCard {
    pub title: String,
    pub buttons: Vec<ModeButton>,
    pub caption: String,
    /// "Read-only: Mode is not a Verified field yet." with a link to the Fields tab.
    pub read_only: Option<(String, String)>,
    /// The tooltip of a disabled control.
    pub tooltip: Option<String>,
    pub write_line: Option<FeedbackLine>,
    /// "Not authorized. The Mode did not change."
    pub denied: Option<String>,
    pub action_hint: String,
    pub actions: Vec<ActionButton>,
    /// While an Action runs, the bar shows only this button.
    pub cancel: Option<String>,
    pub action_line: Option<FeedbackLine>,
    pub refill: RefillRow,
    /// "Mode RELEASE: the Loop has no leak protection."
    pub release_banner: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionView {
    TestSetup {
        title: String,
        input_label: String,
        pressure: String,
        text: String,
        error: Option<String>,
        start: String,
        cancel: String,
    },
    TestRunning {
        source_label: String,
        sources: Vec<(TestSource, String, bool)>,
        reference: String,
        reset_zoom: String,
        cancel: String,
        reference_value: Option<f64>,
    },
    MeasureFillLevel {
        title: String,
        step: String,
        banner: String,
        result: Option<String>,
    },
    Deaerate {
        title: String,
        current: String,
        overall: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub title: String,
    pub text: String,
    pub steps: Vec<String>,
    pub cancel: String,
    /// `None` for the steps of a Refill: the dialog only has "Close".
    pub confirm: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartsCard {
    pub pressure_title: String,
    pub ranges: Vec<(ChartRange, String, bool)>,
    /// "Dotted: minimum 250 and maximum 350 mbar".
    pub pressure_note: Option<String>,
    /// The label of the right end of the time axis.
    pub now: String,
    pub pressure_empty: String,
    pub runtime_title: String,
    /// "vacuum pump".
    pub runtime_caption: String,
    pub runtime_empty: String,
    pub runtime_limit: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverviewModel {
    pub state: TabState,
    pub loading_text: String,
    pub pressure: PressureCard,
    pub status: StatusCard,
    pub mode: ModeCard,
    pub action_view: Option<ActionView>,
    pub dialog: Option<Dialog>,
    pub charts: ChartsCard,
}

/// The status of the device as one word of the locale: `protecting`, `building`, `monitoring`,
/// `released`, `test`, `measure`, `deaerate` or `refill`.
pub fn status_key(
    device: &Device<'_>,
    pressure: Option<f64>,
    limits: Option<PressureLimits>,
) -> &'static str {
    let Some(info) = device.info else {
        return "protecting";
    };
    match info.running_action.as_str() {
        "test" => "test",
        "measure-fill-level" => "measure",
        "deaerate" => "deaerate",
        "refill" => "refill",
        _ => match info.mode.as_str() {
            "release" => "released",
            "monitor" => "monitoring",
            _ => match (pressure, limits) {
                (Some(value), Some(limits)) if value < limits.min => "building",
                _ => "protecting",
            },
        },
    }
}

pub fn status_text(locale: Locale, key: &str) -> String {
    locale.tr(&format!("leakshield.status_value.{key}"))
}

/// The pressure limits of the current Mode.
pub fn limits_of(device: &Device<'_>) -> Option<PressureLimits> {
    let mode = device.info.map_or("shield", |info| info.mode.as_str());
    device.data?.pressure_limits(mode)
}

impl OverviewModel {
    pub fn build(input: &ScreenInput<'_>, ui: &OverviewUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let state = device.state();
        let ready = state == TabState::Ready;
        let alarms = device.alarms(input);
        let worst = severity_of(&alarms);
        let sample = input
            .state
            .sensors
            .as_ref()
            .or(input.state.last_sensors.as_ref());
        let pressure = if ready {
            device.sensor(input, "pressure")
        } else {
            None
        };
        let limits = limits_of(&device);
        let info = device.info;
        let mode = info.map_or("", |i| i.mode.as_str());
        let action = info.map_or("", |i| i.running_action.as_str());

        // The gauge.
        let gauge = match (pressure, limits) {
            (None, _) => GaugeState::Offline,
            (Some(value), Some(limits)) if value < limits.min => GaugeState::Below,
            (Some(value), Some(limits)) if value > limits.max => GaugeState::Above,
            _ => GaugeState::InRange,
        };
        let (tag, tag_kind) = match gauge {
            GaugeState::InRange => (locale.tr("leakshield.overview.in_range"), TagKind::Success),
            GaugeState::Below => (locale.tr("leakshield.overview.below"), TagKind::Warning),
            GaugeState::Above => (locale.tr("leakshield.overview.above"), TagKind::Warning),
            GaugeState::Offline => (locale.tr("leakshield.overview.no_value"), TagKind::Offline),
        };
        let pressure_card = PressureCard {
            title: locale.tr("leakshield.overview.pressure"),
            caption: locale.tr("leakshield.overview.pressure_caption"),
            value: pressure,
            value_text: optional_value(locale, pressure, "mbar"),
            number: pressure.map_or_else(|| "--".to_owned(), |v| number(locale, v, "mbar")),
            unit: "mbar".to_owned(),
            labels: limits.map_or_else(GaugeLabels::default, |l| {
                let label = |name: &str, value: f64| {
                    locale.fmt(
                        &format!("leakshield.overview.{name}"),
                        &[("value", &compact(locale, value))],
                    )
                };
                GaugeLabels {
                    minimum: label("minimum", l.min),
                    maximum: label("maximum", l.max),
                    target: label("target", l.target),
                }
            }),
            limits,
            gauge,
            tag,
            tag_kind,
            alarm: worst == Some(Severity::Alarm) && gauge != GaugeState::InRange,
            tooltip: limits.map_or_else(String::new, |l| {
                locale.fmt(
                    "leakshield.overview.gauge_tooltip",
                    &[
                        ("value", &optional_value(locale, pressure, "mbar")),
                        ("min", &compact(locale, l.min)),
                        ("target", &compact(locale, l.target)),
                        ("max", &compact(locale, l.max)),
                    ],
                )
            }),
        };

        // The status card.
        let key = status_key(&device, pressure, limits);
        let (status_tag, status_kind) = match worst {
            Some(severity) => {
                let worst_alarm = alarms
                    .iter()
                    .filter(|a| a.severity == severity)
                    .map(|a| locale.event_title(&a.event))
                    .next()
                    .unwrap_or_default();
                (
                    worst_alarm,
                    if severity == Severity::Alarm {
                        TagKind::Alarm
                    } else {
                        TagKind::Warning
                    },
                )
            }
            None if !ready => (locale.tr("leakshield.overview.no_value"), TagKind::Offline),
            None => (
                status_text(locale, key),
                if key == "protecting" {
                    TagKind::Success
                } else if key == "monitoring" {
                    TagKind::Accent
                } else {
                    TagKind::Neutral
                },
            ),
        };
        let row_severity = |needle: &str| {
            alarms
                .iter()
                .filter(|a| a.event.contains(needle))
                .map(|a| a.severity)
                .max()
        };
        let measured = sample.filter(|_| ready).map(|s| {
            locale.fmt(
                "leakshield.overview.measured",
                &[("time", &clock_minutes(s.time_us))],
            )
        });
        // The last good Feed send: its values and its age.
        let feed = info
            .map(|i| i.last_feed)
            .filter(|feed| ready && feed.time_us > 0);
        let value_of = |name: &str, unit: &str| {
            optional_value(locale, device.sensor(input, name).filter(|_| ready), unit)
        };
        let change_of = |name: &str, unit: &str| {
            change_text(locale, device.sensor(input, name).filter(|_| ready), unit)
        };
        let status = StatusCard {
            title: locale.tr("leakshield.overview.status"),
            tag: status_tag,
            tag_kind: status_kind,
            rows: vec![
                StatusRow {
                    label: locale.tr("leakshield.overview.fill_level"),
                    value: value_of("fill_level", "%"),
                    bold: true,
                    caption: measured,
                    severity: row_severity("fill-level"),
                },
                StatusRow {
                    label: locale.tr("leakshield.overview.volume_change"),
                    value: change_of("volume_change", "ml/h"),
                    bold: true,
                    caption: None,
                    severity: row_severity("leak"),
                },
                StatusRow {
                    label: locale.tr("leakshield.overview.pressure_change"),
                    value: change_of("pressure_change", "mbar/min"),
                    bold: true,
                    caption: None,
                    severity: None,
                },
                StatusRow {
                    label: locale.tr("leakshield.overview.feed"),
                    value: feed.map_or_else(
                        || "--".to_owned(),
                        |feed| {
                            format!(
                                "{}, {}",
                                value_text(locale, feed.feed_pump, "rpm"),
                                value_text(locale, feed.flow_lph, "l/h")
                            )
                        },
                    ),
                    bold: false,
                    caption: feed.map(|feed| {
                        let age = input.now_us.saturating_sub(feed.time_us) / 1_000_000;
                        locale.fmt(
                            "leakshield.overview.feed_age",
                            &[("seconds", &age.to_string())],
                        )
                    }),
                    severity: None,
                },
            ],
        };

        // The Mode and the Actions.
        let mode_access = device.access("default_mode");
        let unknown = device.unknown_firmware();
        let mode_locked = !mode_access.writable;
        let read_only = (mode_locked && ready && !unknown).then(|| {
            (
                locale.tr("leakshield.overview.mode_not_verified"),
                locale.tr("leakshield.open_fields"),
            )
        });
        let tooltip = if unknown {
            Some(locale.tr("leakshield.overview.unknown_tooltip"))
        } else {
            None
        };
        let controls = ready && !mode_locked;
        let running = !action.is_empty();
        let mode_line = write_line(input, id, "mode");
        let denied = matches!(
            input.feedback.get(&format!("ls:{id}:mode")),
            Some(WriteOutcome::NotAuthorized)
        )
        .then(|| locale.tr("leakshield.overview.denied"));
        let mode_card = ModeCard {
            title: locale.tr("leakshield.overview.mode_and_actions"),
            buttons: MODES
                .iter()
                .map(|m| ModeButton {
                    mode: (*m).to_owned(),
                    label: locale.tr(&format!("leakshield.mode.{m}")),
                    selected: mode == *m,
                    enabled: controls && !running,
                })
                .collect(),
            caption: if mode.is_empty() {
                String::new()
            } else {
                locale.tr(&format!("leakshield.overview.mode_caption.{mode}"))
            },
            read_only,
            tooltip,
            write_line: mode_line,
            denied,
            action_hint: locale.tr("leakshield.overview.action_hint"),
            actions: ["test", "measure-fill-level", "deaerate", "refill"]
                .iter()
                .map(|a| ActionButton {
                    action: (*a).to_owned(),
                    label: locale.tr(&format!("leakshield.action.{}", a.replace('-', "_"))),
                    enabled: controls && !running,
                })
                .collect(),
            cancel: (running && ready).then(|| locale.tr("leakshield.cancel")),
            action_line: write_line(input, id, "action"),
            refill: {
                let action = locale.tr("leakshield.action.refill");
                let start = locale
                    .tr("leakshield.overview.refill")
                    .find("%{action}")
                    .unwrap_or(0);
                RefillRow {
                    text: locale.fmt("leakshield.overview.refill", &[("action", &action)]),
                    action: start..start + action.len(),
                    link: locale.tr("leakshield.overview.refill_link"),
                }
            },
            release_banner: (mode == "release")
                .then(|| locale.tr("leakshield.overview.release_banner")),
        };

        // The Action view.
        let action_view = match action {
            "test" => Some(ActionView::TestRunning {
                source_label: locale.tr("leakshield.overview.source"),
                sources: TestSource::ALL
                    .iter()
                    .map(|s| (*s, locale.tr(s.key()), *s == ui.source))
                    .collect(),
                reference: locale.tr("leakshield.overview.reference"),
                reset_zoom: locale.tr("leakshield.overview.reset_zoom"),
                cancel: locale.tr("leakshield.cancel"),
                reference_value: ui.reference,
            }),
            "measure-fill-level" => Some(ActionView::MeasureFillLevel {
                title: locale.tr("leakshield.action.measure_fill_level"),
                step: locale.tr("leakshield.overview.measure_step"),
                banner: locale.tr("leakshield.overview.measure_banner"),
                result: device.sensor(input, "fill_level").map(|level| {
                    locale.fmt(
                        "leakshield.overview.measure_result",
                        &[("value", &value_text(locale, level, "%"))],
                    )
                }),
            }),
            "deaerate" => Some(ActionView::Deaerate {
                title: locale.tr("leakshield.action.deaerate"),
                current: locale.tr("leakshield.overview.cycle_current"),
                overall: locale.tr("leakshield.overview.cycle_overall"),
            }),
            _ if ui.test_setup => {
                let pressure_text = ui.test_pressure.clone().unwrap_or_else(|| {
                    device
                        .number("test_pressure")
                        .map_or_else(String::new, |v| plain(locale, v))
                });
                let valid = ui.test_pressure_value(input, id).is_some();
                Some(ActionView::TestSetup {
                    title: locale.tr("leakshield.overview.test_pressure"),
                    input_label: locale.tr("leakshield.overview.test_pressure"),
                    pressure: pressure_text,
                    text: locale.tr("leakshield.overview.test_hint"),
                    error: (!valid).then(|| {
                        locale.fmt(
                            "leakshield.range",
                            &[
                                ("min", &compact(locale, TEST_PRESSURE_RANGE.0)),
                                ("max", &compact(locale, TEST_PRESSURE_RANGE.1)),
                                ("unit", "mbar"),
                            ],
                        )
                    }),
                    start: locale.tr("leakshield.overview.start_test"),
                    cancel: locale.tr("leakshield.cancel"),
                })
            }
            _ => None,
        };

        let dialog = ui.confirm.as_ref().map(|confirm| match confirm {
            Confirm::Mode(mode) => Dialog {
                title: locale.fmt(
                    "leakshield.overview.mode_confirm_title",
                    &[("mode", &locale.tr(&format!("leakshield.mode.{mode}")))],
                ),
                text: locale.tr(&format!("leakshield.overview.mode_confirm.{mode}")),
                steps: Vec::new(),
                cancel: locale.tr("leakshield.cancel"),
                confirm: Some(locale.fmt(
                    "leakshield.overview.mode_confirm_button",
                    &[("mode", &locale.tr(&format!("leakshield.mode.{mode}")))],
                )),
            },
            Confirm::Measure => Dialog {
                title: locale.tr("leakshield.action.measure_fill_level"),
                text: locale.tr("leakshield.overview.measure_confirm"),
                steps: Vec::new(),
                cancel: locale.tr("leakshield.cancel"),
                confirm: Some(locale.tr("leakshield.overview.measure_button")),
            },
            Confirm::Refill => Dialog {
                title: locale.tr("leakshield.overview.refill_confirm_title"),
                text: locale.tr("leakshield.overview.refill_confirm"),
                steps: Vec::new(),
                cancel: locale.tr("leakshield.cancel"),
                confirm: Some(locale.tr("leakshield.overview.refill_button")),
            },
            Confirm::RefillSteps => Dialog {
                title: locale.tr("leakshield.overview.refill_title"),
                text: String::new(),
                steps: (1..=4)
                    .map(|n| locale.tr(&format!("leakshield.overview.refill_step.{n}")))
                    .collect(),
                cancel: locale.tr("leakshield.close"),
                confirm: None,
            },
        });

        Self {
            state,
            loading_text: locale.tr("leakshield.overview.loading"),
            pressure: pressure_card,
            status,
            mode: mode_card,
            action_view,
            dialog,
            charts: ChartsCard {
                pressure_title: locale.tr("leakshield.overview.pressure"),
                ranges: ChartRange::ALL
                    .iter()
                    .map(|r| (*r, locale.tr(r.key()), *r == ui.range))
                    .collect(),
                pressure_note: limits.map(|l| {
                    locale.fmt(
                        "leakshield.overview.dotted",
                        &[
                            ("min", &compact(locale, l.min)),
                            ("max", &compact(locale, l.max)),
                        ],
                    )
                }),
                now: locale.tr("leakshield.overview.now"),
                pressure_empty: locale.tr("leakshield.overview.pressure_empty"),
                runtime_title: locale.tr("leakshield.overview.runtime"),
                runtime_caption: locale.tr("leakshield.overview.runtime_caption"),
                runtime_empty: locale.tr("leakshield.overview.runtime_empty"),
                runtime_limit: locale.tr("leakshield.overview.runtime_limit"),
            },
        }
    }
}
