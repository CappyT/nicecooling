//! The Alarms screen (`docs/ui/screens/alarms.md`): the active Alarms and Warnings, the alarm
//! response of every event type, and the history. The countdown dialog of `widgets.md` is
//! built here too: it shows over every page.

use crate::command::Command;
use crate::command::response_feedback_key;
use crate::i18n::Locale;
use crate::model::{
    AlarmEntry, AlarmRecord, AppState, Link, Outcome, Response, ResponseRule, Severity,
    format_countdown,
};
use crate::screens::format::{clock, optional_value, value_text};
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};
use crate::shell::Page;

/// The delay that the app shows while the daemon has sent no response row. The built-in defaults
/// are the daemon's: a row with override true differs from them.
pub const DEFAULT_DELAY_S: u32 = 30;

/// The severity of an event type.
pub fn event_severity(event: &str) -> Severity {
    if event.ends_with("warning") || event == "lost-input" {
        Severity::Warning
    } else {
        Severity::Alarm
    }
}

/// The events that the nicecooling group of the response table lists.
fn is_nicecooling_event(event: &str) -> bool {
    !event.starts_with("leakshield.")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryFilter {
    #[default]
    All,
    Alarms,
    Warnings,
}

#[derive(Clone, Debug, Default)]
pub struct AlarmsUi {
    pub filter: HistoryFilter,
    /// The filter select of the History card is open.
    pub filter_open: bool,
}

impl AlarmsUi {
    pub fn set_filter(&mut self, filter: HistoryFilter) {
        self.filter = filter;
        self.filter_open = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmsState {
    Loading,
    Ready,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveRow {
    pub instance: String,
    pub severity: Severity,
    pub event: String,
    pub source: String,
    pub since: String,
    pub value: String,
    pub response: String,
    /// "Poweroff in 0:21".
    pub countdown: Option<String>,
    pub cancel_label: String,
    pub cancel: Option<Command>,
    /// "Poweroff cancelled 21:47:02. The Alarm is still active."
    pub cancelled: Option<String>,
    pub caption: Option<String>,
    /// A click on the row opens this page.
    pub open: Option<Page>,
    /// The app makes this row itself: "Daemon down".
    pub synthetic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveCard {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<ActiveRow>,
    pub empty: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseOption {
    pub response: Response,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseRow {
    pub event: String,
    /// `""` for the default of the event, or the Leakshield id of an override row.
    pub source: String,
    /// `event` or `event@source`: the key of the delay input.
    pub key: String,
    pub title: String,
    pub severity: Severity,
    pub response: Response,
    pub delay_s: u32,
    pub options: Vec<ResponseOption>,
    /// The delay applies to the poweroff only.
    pub delay_enabled: bool,
    pub differs: bool,
    pub reset: Option<Command>,
    /// The warning icon: the Leakshield also reacts to this Alarm.
    pub overlap: Option<String>,
    pub delay_warning: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseGroup {
    pub title: String,
    pub rows: Vec<ResponseRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponsesCard {
    pub title: String,
    pub columns: Vec<String>,
    pub groups: Vec<ResponseGroup>,
    /// The table is off while the daemon is away.
    pub enabled: bool,
    pub disabled_text: Option<String>,
    pub reset_label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryRow {
    pub time: String,
    pub event: String,
    pub severity: Severity,
    pub source: String,
    pub duration: String,
    pub response: String,
    pub sensor: Option<String>,
    pub at_s: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryCard {
    pub title: String,
    pub columns: Vec<String>,
    pub filter: HistoryFilter,
    pub filter_label: String,
    pub filters: Vec<(HistoryFilter, String)>,
    pub rows: Vec<HistoryRow>,
    pub empty: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlarmsModel {
    pub state: AlarmsState,
    pub active: ActiveCard,
    pub responses: ResponsesCard,
    pub history: HistoryCard,
}

impl AlarmsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &AlarmsUi) -> Self {
        let locale = input.locale;
        let state = input.state;
        let ui_state = if state.link == Link::Connecting {
            AlarmsState::Loading
        } else {
            AlarmsState::Ready
        };
        let _ = locale;
        Self {
            state: ui_state,
            active: active_card(input),
            responses: responses_card(input),
            history: history_card(input, ui),
        }
    }
}

fn source_text(state: &AppState, locale: Locale, alarm: &AlarmEntry) -> String {
    let leakshield = locale.tr("nav.leakshield");
    if alarm.event == "pump-stop"
        && let Some(info) = state.snapshot.loops.iter().find(|l| l.id == alarm.source)
    {
        let pump = info
            .pump_channels
            .first()
            .map(|id| state.source_name(id, &leakshield))
            .unwrap_or_default();
        return format!("{}: {pump}", info.name);
    }
    if alarm.event == "leakshield-unreachable"
        && let Some(info) = state.source_loop(&alarm.source)
    {
        return format!("{}: {leakshield}", info.name);
    }
    state.source_with_loop(&alarm.source, &leakshield)
}

fn value_of(input: &ScreenInput<'_>, alarm: &AlarmEntry) -> String {
    let locale = input.locale;
    let state = input.state;
    let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
    if alarm.event == "pump-stop"
        && let Some(info) = state.snapshot.loops.iter().find(|l| l.id == alarm.source)
        && let Some(channel) = info.pump_channels.first()
    {
        let rpm = state
            .fan_sensor(channel)
            .and_then(|fan| sample.and_then(|s| s.sensor_value(&fan)));
        let duty = sample.and_then(|s| s.channel_duty(channel));
        return locale.fmt(
            "alarms.value.pump_stop",
            &[
                ("rpm", &optional_value(locale, rpm, "rpm")),
                ("duty", &optional_value(locale, duty, "%")),
            ],
        );
    }
    if alarm.event == "leakshield-unreachable" {
        let seconds = input.now_us.saturating_sub(alarm.raised_us) / 1_000_000;
        return locale.fmt("alarms.value.no_data", &[("seconds", &seconds.to_string())]);
    }
    match (alarm.details.value, alarm.details.limit) {
        (Some(value), Some(limit)) => locale.fmt(
            "alarms.value.limit",
            &[
                ("value", &value_text(locale, value, &alarm.details.unit)),
                ("limit", &value_text(locale, limit, &alarm.details.unit)),
            ],
        ),
        (Some(value), None) => value_text(locale, value, &alarm.details.unit),
        _ => "--".to_owned(),
    }
}

/// The text of an alarm response: "Poweroff after 30 s", "Notify", "None".
pub fn response_text(locale: Locale, response: Response, delay_s: u32) -> String {
    match response {
        Response::None => locale.tr("alarms.response.none"),
        Response::Notify => locale.tr("alarms.response.notify"),
        Response::Poweroff => locale.fmt(
            "alarms.response.poweroff_after",
            &[("seconds", &delay_s.to_string())],
        ),
    }
}

/// The delay of the response of an event for a source: the row of the Leakshield, else the
/// default of the event.
fn delay_of(state: &AppState, event: &str, source: &str) -> u32 {
    state
        .snapshot
        .responses
        .iter()
        .find(|r| r.event == event && r.source == source)
        .or_else(|| state.default_response(event))
        .map_or(DEFAULT_DELAY_S, |r| r.delay_s)
}

fn active_card(input: &ScreenInput<'_>) -> ActiveCard {
    let locale = input.locale;
    let state = input.state;
    let mut alarms: Vec<&AlarmEntry> = state.snapshot.alarms.iter().collect();
    alarms.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.raised_us.cmp(&b.raised_us))
    });
    let mut rows: Vec<ActiveRow> = alarms
        .into_iter()
        .map(|alarm| {
            let remaining = alarm.poweroff_in_s(input.now_us);
            let cancelled = state
                .cancelled
                .get(&alarm.instance)
                .filter(|_| alarm.poweroff_at_us == 0 && alarm.response == Response::Poweroff)
                .map(|at| locale.fmt("alarms.cancelled", &[("time", &clock(*at))]))
                .or_else(|| {
                    let too_late = matches!(
                        input.feedback.get(&format!("poweroff:{}", alarm.instance)),
                        Some(WriteOutcome::Refused { error }) if error == "PoweroffStarted"
                    );
                    too_late.then(|| locale.tr("alarms.poweroff_started"))
                });
            let first_pump = alarm.event == "pump-stop";
            ActiveRow {
                instance: alarm.instance.clone(),
                severity: alarm.severity,
                event: locale.event_title(&alarm.event),
                source: source_text(state, locale, alarm),
                since: clock(alarm.raised_us),
                value: value_of(input, alarm),
                response: response_text(
                    locale,
                    alarm.response,
                    delay_of(state, &alarm.event, &alarm.source),
                ),
                countdown: remaining.map(|seconds| {
                    locale.fmt("alarms.countdown", &[("time", &format_countdown(seconds))])
                }),
                cancel_label: locale.tr("alarms.cancel_poweroff"),
                cancel: remaining.map(|_| Command::CancelPoweroff {
                    instance: alarm.instance.clone(),
                }),
                cancelled,
                caption: first_pump.then(|| locale.tr("alarms.caption.pump_stop")),
                open: open_page(state, alarm),
                synthetic: false,
            }
        })
        .collect();
    // The daemon cannot tell that it is down: the app adds the row itself.
    if state.link == Link::Down {
        rows.insert(
            0,
            ActiveRow {
                instance: "daemon-down".to_owned(),
                severity: Severity::Alarm,
                event: locale.tr("alarms.daemon_down.title"),
                source: locale.tr("alarms.daemon_down.source"),
                since: input.down_since_us.map_or_else(|| "--".to_owned(), clock),
                value: locale.tr("alarms.daemon_down.text"),
                response: String::new(),
                countdown: None,
                cancel_label: String::new(),
                cancel: None,
                cancelled: None,
                caption: None,
                open: Some(Page::Settings),
                synthetic: true,
            },
        );
    }
    ActiveCard {
        title: locale.tr("alarms.active.title"),
        columns: ["event", "source", "since", "value", "response"]
            .iter()
            .map(|key| locale.tr(&format!("alarms.column.{key}")))
            .collect(),
        empty: rows.is_empty().then(|| locale.tr("alarms.active.empty")),
        rows,
    }
}

/// A click on an active row opens its source.
fn open_page(state: &AppState, alarm: &AlarmEntry) -> Option<Page> {
    let snapshot = &state.snapshot;
    if snapshot.leakshields.iter().any(|l| l.id == alarm.source) {
        return Some(Page::Leakshield(alarm.source.clone()));
    }
    if snapshot.loops.iter().any(|l| l.id == alarm.source) {
        return Some(Page::Loops);
    }
    if snapshot.channels.contains_key(&alarm.source) {
        return Some(Page::Channels);
    }
    snapshot
        .sensors
        .contains_key(&alarm.source)
        .then_some(Page::Sensors)
}

fn response_row(input: &ScreenInput<'_>, rule: &ResponseRule, enabled: bool) -> ResponseRow {
    let locale = input.locale;
    // The daemon says whether a row differs from the built-in default or has an override.
    let differs = rule.overridden;
    let key = response_feedback_key(&rule.event, &rule.source);
    // A Leakshield row has the overlap of its device. A default row has the overlap of any device.
    let overlap = input
        .state
        .snapshot
        .leakshield_data
        .iter()
        .filter(|(id, _)| rule.source.is_empty() || **id == rule.source)
        .flat_map(|(_, data)| &data.proposals)
        .any(|proposal| {
            rule.event.strip_prefix("leakshield.") == Some(proposal.alarm.as_str())
                && !proposal.overlap.is_empty()
        })
        .then(|| locale.tr("alarms.overlap"));
    ResponseRow {
        event: rule.event.clone(),
        source: rule.source.clone(),
        key: if rule.source.is_empty() {
            rule.event.clone()
        } else {
            format!("{}@{}", rule.event, rule.source)
        },
        title: locale.event_title(&rule.event),
        severity: event_severity(&rule.event),
        response: rule.response,
        delay_s: rule.delay_s,
        options: [Response::None, Response::Notify, Response::Poweroff]
            .into_iter()
            .map(|response| ResponseOption {
                response,
                label: locale.tr(match response {
                    Response::None => "alarms.option.none",
                    Response::Notify => "alarms.option.notify",
                    Response::Poweroff => "alarms.option.poweroff",
                }),
                selected: response == rule.response,
            })
            .collect(),
        delay_enabled: enabled && rule.response == Response::Poweroff,
        differs,
        reset: (differs && enabled).then(|| Command::ResetResponse {
            event: rule.event.clone(),
            source: rule.source.clone(),
        }),
        overlap,
        delay_warning: (rule.response == Response::Poweroff && rule.delay_s == 0)
            .then(|| locale.tr("alarms.delay_zero")),
        write_line: input.feedback.get(&key).map(|outcome| {
            FeedbackLine::new(outcome, locale, &locale.tr("feedback.refused"), None)
        }),
    }
}

fn responses_card(input: &ScreenInput<'_>) -> ResponsesCard {
    let locale = input.locale;
    let state = input.state;
    let enabled = input.writes().enabled;
    let rules = &state.snapshot.responses;
    let defaults = || rules.iter().filter(|rule| rule.source.is_empty());
    let mut groups = Vec::new();
    // The default of each Leakshield event, then the rows of each known Leakshield.
    let device_defaults: Vec<ResponseRow> = defaults()
        .filter(|rule| !is_nicecooling_event(&rule.event))
        .map(|rule| response_row(input, rule, enabled))
        .collect();
    if !device_defaults.is_empty() {
        groups.push(ResponseGroup {
            title: locale.tr("nav.leakshield"),
            rows: device_defaults,
        });
    }
    for device in &state.snapshot.leakshields {
        let rows: Vec<ResponseRow> = rules
            .iter()
            .filter(|rule| rule.source == device.id)
            .map(|rule| response_row(input, rule, enabled))
            .collect();
        if !rows.is_empty() {
            groups.push(ResponseGroup {
                title: format!("{} {}", locale.tr("nav.leakshield"), device.serial),
                rows,
            });
        }
    }
    let own: Vec<ResponseRow> = defaults()
        .filter(|rule| is_nicecooling_event(&rule.event))
        .map(|rule| response_row(input, rule, enabled))
        .collect();
    if !own.is_empty() {
        groups.push(ResponseGroup {
            title: locale.tr("alarms.group.nicecooling"),
            rows: own,
        });
    }
    ResponsesCard {
        title: locale.tr("alarms.responses.title"),
        columns: ["event", "source", "response", "delay"]
            .iter()
            .map(|key| locale.tr(&format!("alarms.column.{key}")))
            .collect(),
        groups,
        enabled,
        disabled_text: (!matches!(state.link, Link::Online | Link::Connecting))
            .then(|| locale.tr("alarms.responses.offline")),
        reset_label: locale.tr("alarms.reset"),
    }
}

fn duration_text(locale: Locale, from_us: u64, to_us: Option<u64>) -> String {
    let Some(to_us) = to_us else {
        return locale.tr("alarms.history.active");
    };
    let seconds = to_us.saturating_sub(from_us) / 1_000_000;
    let (minutes, rest) = (seconds / 60, seconds % 60);
    if minutes == 0 {
        locale.fmt("profiles.duration.s", &[("n", &rest.to_string())])
    } else {
        format!(
            "{} {}",
            locale.fmt("profiles.duration.min", &[("n", &minutes.to_string())]),
            locale.fmt("profiles.duration.s", &[("n", &rest.to_string())])
        )
    }
}

fn outcome_key(record: &AlarmRecord) -> &'static str {
    match record.outcome {
        Outcome::None => "alarms.history.none",
        Outcome::Notified => "alarms.history.notified",
        Outcome::PoweroffPending => "alarms.history.poweroff_pending",
        Outcome::PoweroffCancelled => "alarms.history.cancelled",
        Outcome::PoweroffStopped => "alarms.history.poweroff_stopped",
        Outcome::PoweroffRequested => "alarms.history.poweroff_requested",
        Outcome::PoweroffFailed => "alarms.history.poweroff_failed",
    }
}

fn history_card(input: &ScreenInput<'_>, ui: &AlarmsUi) -> HistoryCard {
    let locale = input.locale;
    let state = input.state;
    // The daemon returns the newest raise first.
    let rows: Vec<HistoryRow> = state
        .snapshot
        .alarm_history
        .iter()
        .filter(|record| match ui.filter {
            HistoryFilter::All => true,
            HistoryFilter::Alarms => record.severity == Severity::Alarm,
            HistoryFilter::Warnings => record.severity == Severity::Warning,
        })
        .map(|record| HistoryRow {
            time: crate::screens::format::date_time(record.raised_us),
            event: locale.event_title(&record.event),
            severity: record.severity,
            source: state.source_with_loop(&record.source, &locale.tr("nav.leakshield")),
            duration: if record.closed_by_restart {
                locale.tr("alarms.history.closed_by_restart")
            } else {
                duration_text(locale, record.raised_us, record.cleared_us)
            },
            response: locale.tr(outcome_key(record)),
            sensor: state
                .snapshot
                .sensors
                .contains_key(&record.source)
                .then(|| record.source.clone())
                .or_else(|| record.details.sensor.clone()),
            at_s: (record.raised_us / 1_000_000) as i64,
        })
        .collect();
    HistoryCard {
        title: locale.tr("alarms.history.title"),
        columns: ["time", "event", "source", "duration", "response"]
            .iter()
            .map(|key| locale.tr(&format!("alarms.history.column.{key}")))
            .collect(),
        filter: ui.filter,
        filter_label: locale.tr(match ui.filter {
            HistoryFilter::All => "alarms.filter.all",
            HistoryFilter::Alarms => "alarms.filter.alarms",
            HistoryFilter::Warnings => "alarms.filter.warnings",
        }),
        filters: [
            (HistoryFilter::All, "alarms.filter.all"),
            (HistoryFilter::Alarms, "alarms.filter.alarms"),
            (HistoryFilter::Warnings, "alarms.filter.warnings"),
        ]
        .into_iter()
        .map(|(filter, key)| (filter, locale.tr(key)))
        .collect(),
        empty: rows.is_empty().then(|| locale.tr("alarms.history.empty")),
        rows,
    }
}

// --- The countdown dialog ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogButton {
    Cancel,
    Hide,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CountdownDialog {
    pub instance: String,
    pub title: String,
    pub text: String,
    pub seconds: u64,
    /// "seconds left of 30".
    pub seconds_label: String,
    /// The share of the countdown that is left, 0 to 1.
    pub fraction: f32,
    /// Source, Raised, the value that caused the Alarm and the alarm response.
    pub rows: Vec<(String, String)>,
    /// "2 Alarms" when another Alarm has a countdown too.
    pub more: Option<String>,
    pub cancel: String,
    pub hide: String,
    pub hint: String,
    /// The countdown ended: no buttons.
    pub ended: bool,
    /// The button that has the focus when the dialog opens.
    pub default_focus: DialogButton,
}

impl CountdownDialog {
    /// The dialog of the Alarm with the shortest countdown, or `None` without a countdown.
    pub fn build(input: &ScreenInput<'_>) -> Option<Self> {
        let locale = input.locale;
        let state = input.state;
        let alarm = state.next_poweroff()?;
        let seconds = alarm.poweroff_in_s(input.now_us).unwrap_or(0);
        let total = u64::from(delay_of(state, &alarm.event, &alarm.source))
            .max(seconds)
            .max(1);
        let pending = state
            .snapshot
            .alarms
            .iter()
            .filter(|a| a.poweroff_at_us != 0)
            .count();
        let ended = seconds == 0;
        Some(Self {
            instance: alarm.instance.clone(),
            title: locale.event_title(&alarm.event),
            text: locale.tr(if ended {
                "alarms.dialog.ended"
            } else {
                "alarms.dialog.text"
            }),
            seconds,
            seconds_label: locale.fmt("alarms.dialog.seconds_of", &[("total", &total.to_string())]),
            fraction: (seconds as f32 / total as f32).clamp(0.0, 1.0),
            rows: vec![
                (
                    locale.tr("alarms.dialog.source"),
                    source_text(state, locale, alarm),
                ),
                (locale.tr("alarms.dialog.raised"), clock(alarm.raised_us)),
                (locale.tr("alarms.dialog.value"), value_of(input, alarm)),
                (
                    locale.tr("alarms.dialog.response"),
                    response_text(
                        locale,
                        alarm.response,
                        delay_of(state, &alarm.event, &alarm.source),
                    ),
                ),
            ],
            more: (pending > 1).then(|| locale.plural("status.alarms", pending)),
            cancel: locale.tr("alarms.cancel_poweroff"),
            hide: locale.tr("alarms.dialog.hide"),
            hint: locale.tr("alarms.dialog.hint"),
            ended,
            default_focus: DialogButton::Cancel,
        })
    }

    pub fn cancel_command(&self) -> Command {
        Command::CancelPoweroff {
            instance: self.instance.clone(),
        }
    }
}
