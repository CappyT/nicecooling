//! The Alarms tab (`docs/ui/screens/leakshield-alarms.md`): the thresholds, the Device response
//! and the alarm response of each Leakshield event, with the recommendations of the daemon.

use crate::command::Command;
use crate::model::{AlarmEntry, FieldValue, Response, Severity};
use crate::screens::alarms::{DEFAULT_DELAY_S, default_response};
use crate::screens::format::clock;
use crate::screens::leakshield::{Device, TabState, fields_command, write_line};
use crate::screens::{FeedbackLine, ScreenInput};

/// The Device responses, as the response fields and the proposals name them.
pub const DEVICE_RESPONSES: [&str; 4] = ["none", "signal-short", "signal-long", "key-sequence"];

struct EventDef {
    event: &'static str,
    /// The field that holds the Device response of the event.
    response_field: &'static str,
    severity: Severity,
    group: &'static str,
    threshold: Option<ThresholdDef>,
    /// The bit of `alarm_buzzer` that turns the sound of a Warning on.
    sound_bit: Option<u32>,
}

#[derive(Clone, Copy)]
struct ThresholdDef {
    field: &'static str,
    min: f64,
    max: f64,
    unit: &'static str,
    /// An integer field: the daemon refuses a fraction.
    whole: bool,
}

const EVENTS: [EventDef; 6] = [
    EventDef {
        event: "leak-warning",
        response_field: "leak_rate_warning_response_action",
        severity: Severity::Warning,
        group: "leakage",
        threshold: Some(ThresholdDef {
            field: "leak_rate_warning_threshold",
            min: 50.0,
            max: 200.0,
            unit: "ml/h",
            whole: true,
        }),
        sound_bit: Some(4),
    },
    EventDef {
        event: "leak-alarm-small",
        response_field: "leak_rate_alarm_1_response_action",
        severity: Severity::Alarm,
        group: "leakage",
        threshold: None,
        sound_bit: None,
    },
    EventDef {
        event: "leak-alarm",
        response_field: "leak_rate_alarm_2_response_action",
        severity: Severity::Alarm,
        group: "leakage",
        threshold: None,
        sound_bit: None,
    },
    EventDef {
        event: "fill-level-low-warning",
        response_field: "fill_level_warning_response_action",
        severity: Severity::Warning,
        group: "fill",
        threshold: Some(ThresholdDef {
            field: "fill_level_warning_threshold",
            min: 0.0,
            max: 100.0,
            unit: "%",
            whole: false,
        }),
        sound_bit: Some(1),
    },
    EventDef {
        event: "fill-level-low-alarm",
        response_field: "fill_level_alarm_response_action",
        severity: Severity::Alarm,
        group: "fill",
        threshold: Some(ThresholdDef {
            field: "fill_level_alarm_threshold",
            min: 0.0,
            max: 100.0,
            unit: "%",
            whole: false,
        }),
        sound_bit: None,
    },
    EventDef {
        event: "system-alarm",
        response_field: "system_alarm_response_action",
        severity: Severity::Alarm,
        group: "system",
        threshold: None,
        sound_bit: None,
    },
];

/// The id of the alarm response of an event of this tab.
pub fn response_event(event: &str) -> String {
    format!("leakshield.{event}")
}

#[derive(Clone, Debug, Default)]
pub struct AlarmsUi;

impl AlarmsUi {
    /// A threshold that the user set. The Alarm threshold of the fill level must stay below
    /// the Warning threshold: a value that breaks it sends nothing.
    pub fn set_threshold(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        field: &str,
        value: f64,
    ) -> Option<Command> {
        let device = Device::new(input, id);
        let def = EVENTS
            .iter()
            .find_map(|e| e.threshold.filter(|t| t.field == field))?;
        let value = if def.whole { value.round() } else { value };
        if !(def.min..=def.max).contains(&value) || !device.access(field).writable {
            return None;
        }
        if order_error(&device, field, value) {
            return None;
        }
        Some(fields_command(
            id,
            &format!("threshold:{field}"),
            vec![(field.to_owned(), FieldValue::Number(value))],
        ))
    }

    pub fn set_device_response(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        event: &str,
        response: &str,
    ) -> Option<Command> {
        let def = EVENTS.iter().find(|e| e.event == event)?;
        let field = def.response_field.to_owned();
        if !DEVICE_RESPONSES.contains(&response) || !Device::new(input, id).access(&field).writable
        {
            return None;
        }
        Some(fields_command(
            id,
            &format!("response:{event}"),
            vec![(field, FieldValue::Text(response.to_owned()))],
        ))
    }

    pub fn set_sound(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        event: &str,
        on: bool,
    ) -> Option<Command> {
        let def = EVENTS.iter().find(|e| e.event == event)?;
        let bit = def.sound_bit?;
        let device = Device::new(input, id);
        if !device.access("alarm_buzzer").writable {
            return None;
        }
        let mask = device.number("alarm_buzzer").unwrap_or(0.0) as u32;
        let mask = if on { mask | bit } else { mask & !bit };
        Some(fields_command(
            id,
            &format!("sound:{event}"),
            vec![(
                "alarm_buzzer".to_owned(),
                FieldValue::Number(f64::from(mask)),
            )],
        ))
    }

    /// The alarm response of nicecooling. It goes to the daemon, whatever the device says.
    pub fn set_alarm_response(
        &self,
        input: &ScreenInput<'_>,
        event: &str,
        response: Response,
        delay_s: Option<u32>,
    ) -> Command {
        let delay_s = delay_s.unwrap_or_else(|| current_delay(input, event));
        Command::SetAlarmResponse {
            event: event.to_owned(),
            response,
            delay_s,
        }
    }
}

fn current_response(input: &ScreenInput<'_>, event: &str) -> (Response, u32) {
    input
        .state
        .snapshot
        .responses
        .iter()
        .find(|rule| rule.event == event)
        .map_or((default_response(event), DEFAULT_DELAY_S), |rule| {
            (rule.response, rule.delay_s)
        })
}

fn current_delay(input: &ScreenInput<'_>, event: &str) -> u32 {
    current_response(input, event).1
}

/// The fill level Alarm threshold at or above the Warning threshold.
fn order_error(device: &Device<'_>, field: &str, value: f64) -> bool {
    match field {
        "fill_level_alarm_threshold" => device
            .number("fill_level_warning_threshold")
            .is_some_and(|warning| value >= warning),
        "fill_level_warning_threshold" => device
            .number("fill_level_alarm_threshold")
            .is_some_and(|alarm| alarm >= value),
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThresholdCtl {
    pub field: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub writable: bool,
    pub reason: Option<String>,
    /// "The Alarm threshold must be below the Warning threshold."
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCtl {
    pub field: String,
    pub current: String,
    pub options: Vec<(String, String, bool)>,
    pub writable: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundCtl {
    pub on: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseCtl {
    pub event: String,
    pub response: Response,
    pub delay_s: u32,
    pub options: Vec<(Response, String, bool)>,
    pub delay_enabled: bool,
    pub delay_warning: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventRow {
    /// The event of the device: `leak-warning`. Empty for the row of nicecooling.
    pub event: String,
    pub title: String,
    pub caption: String,
    pub severity: Severity,
    pub threshold: Option<ThresholdCtl>,
    pub device: Option<DeviceCtl>,
    pub sound: Option<SoundCtl>,
    pub response: ResponseCtl,
    /// "Active since 21:46:51".
    pub active: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventGroup {
    pub title: String,
    pub rows: Vec<EventRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecommendationKind {
    Warning,
    Information,
    Success,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub kind: RecommendationKind,
    pub text: String,
    pub button: Option<(String, Command)>,
    pub write_line: Option<FeedbackLine>,
}

/// The row "Reset device alarms": shown while the Leakshield holds a latched alarm bit.
#[derive(Clone, Debug, PartialEq)]
pub struct ResetRow {
    pub text: String,
    pub label: String,
    pub enabled: bool,
    pub command: Command,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlarmsModel {
    pub state: TabState,
    pub reset: Option<ResetRow>,
    pub recommendation_title: String,
    pub recommendations: Vec<Recommendation>,
    pub columns: Vec<String>,
    pub groups: Vec<EventGroup>,
    pub device_help: String,
    pub sound_label: String,
}

fn active_text(input: &ScreenInput<'_>, alarms: &[&AlarmEntry], event: &str) -> Option<String> {
    let id = response_event(event);
    alarms.iter().find(|alarm| alarm.event == id).map(|alarm| {
        input.locale.fmt(
            "leakshield.alarms.active_since",
            &[("time", &clock(alarm.raised_us))],
        )
    })
}

impl AlarmsModel {
    pub fn build(input: &ScreenInput<'_>, _ui: &AlarmsUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let alarms = device.alarms(input);
        let all_alarms: Vec<&AlarmEntry> = input.state.snapshot.alarms.iter().collect();

        let response_ctl = |event: String| {
            let (response, delay_s) = current_response(input, &event);
            ResponseCtl {
                options: [Response::None, Response::Notify, Response::Poweroff]
                    .into_iter()
                    .map(|option| {
                        (
                            option,
                            locale.tr(&format!("alarms.option.{}", option.id())),
                            option == response,
                        )
                    })
                    .collect(),
                delay_enabled: response == Response::Poweroff && input.writes().enabled,
                delay_warning: (response == Response::Poweroff && delay_s == 0)
                    .then(|| locale.tr("alarms.delay_zero")),
                write_line: input
                    .feedback
                    .get(&format!("alarm-response:{event}"))
                    .map(|outcome| {
                        FeedbackLine::new(outcome, locale, &locale.tr("feedback.refused"), None)
                    }),
                event,
                response,
                delay_s,
            }
        };

        let mut groups: Vec<EventGroup> = Vec::new();
        for def in &EVENTS {
            let event_id = response_event(def.event);
            let response_field = def.response_field.to_owned();
            let access = device.access(&response_field);
            let threshold = def.threshold.map(|t| {
                let access = device.access(t.field);
                let value = device.number(t.field).unwrap_or(0.0);
                ThresholdCtl {
                    field: t.field.to_owned(),
                    value,
                    min: t.min,
                    max: t.max,
                    unit: crate::screens::format::unit_text(t.unit).to_owned(),
                    writable: access.writable,
                    reason: (!access.writable).then(|| device.reason_text(&access.reason)),
                    error: (t.field == "fill_level_alarm_threshold"
                        && order_error(&device, t.field, value))
                    .then(|| locale.tr("leakshield.alarms.order_error")),
                }
            });
            let device_ctl = device.field(&response_field).map(|field| {
                let current = match &field.value {
                    FieldValue::Text(name) if DEVICE_RESPONSES.contains(&name.as_str()) => {
                        name.clone()
                    }
                    _ => "none".to_owned(),
                };
                DeviceCtl {
                    field: response_field.clone(),
                    options: DEVICE_RESPONSES
                        .iter()
                        .map(|option| {
                            (
                                (*option).to_owned(),
                                locale.tr(&format!(
                                    "leakshield.alarms.device.{}",
                                    option.replace('-', "_")
                                )),
                                *option == current,
                            )
                        })
                        .collect(),
                    current,
                    writable: access.writable,
                    reason: (!access.writable).then(|| device.reason_text(&access.reason)),
                }
            });
            let sound = def.sound_bit.and_then(|bit| {
                let mask = device.number("alarm_buzzer")? as u32;
                Some(SoundCtl {
                    on: mask & bit != 0,
                    writable: device.access("alarm_buzzer").writable,
                })
            });
            let title = locale.event_title(&event_id);
            let caption_key = match def.event {
                "leak-warning" => "leakshield.alarms.caption.leak_warning",
                "leak-alarm-small" | "leak-alarm" => "leakshield.alarms.caption.leak_alarm",
                "system-alarm" => "leakshield.alarms.caption.system",
                "fill-level-low-warning" => "leakshield.alarms.caption.fill_warning",
                _ => "leakshield.alarms.caption.fill_alarm",
            };
            let row = EventRow {
                event: def.event.to_owned(),
                title,
                caption: locale.tr(caption_key),
                severity: def.severity,
                threshold,
                device: device_ctl,
                sound,
                response: response_ctl(event_id),
                active: active_text(input, &alarms, def.event),
                write_line: write_line(input, id, &format!("response:{}", def.event))
                    .or_else(|| write_line(input, id, &format!("sound:{}", def.event)))
                    .or_else(|| {
                        def.threshold
                            .and_then(|t| write_line(input, id, &format!("threshold:{}", t.field)))
                    }),
            };
            match groups.iter_mut().find(|g| g.title == def.group) {
                Some(group) => group.rows.push(row),
                None => groups.push(EventGroup {
                    title: def.group.to_owned(),
                    rows: vec![row],
                }),
            }
        }
        for group in &mut groups {
            group.title = locale.tr(&format!("leakshield.alarms.group.{}", group.title));
        }
        groups.push(EventGroup {
            title: locale.tr("leakshield.alarms.group.nicecooling"),
            rows: vec![EventRow {
                event: String::new(),
                title: locale.event_title("leakshield-unreachable"),
                caption: locale.tr("leakshield.alarms.caption.unreachable"),
                severity: Severity::Alarm,
                threshold: None,
                device: None,
                sound: None,
                response: response_ctl("leakshield-unreachable".to_owned()),
                active: all_alarms
                    .iter()
                    .find(|a| a.event == "leakshield-unreachable" && a.source == id)
                    .map(|alarm| {
                        locale.fmt(
                            "leakshield.alarms.active_since",
                            &[("time", &clock(alarm.raised_us))],
                        )
                    }),
                write_line: None,
            }],
        });

        let latched = alarms
            .iter()
            .any(|alarm| alarm.event.starts_with("leakshield.") && alarm.details.latched_bits != 0);
        let state = device.state();
        let reset = latched.then(|| ResetRow {
            text: locale.tr("leakshield.alarms.reset_text"),
            label: locale.tr("leakshield.alarms.reset_button"),
            enabled: state == TabState::Ready
                && !device.unknown_firmware()
                && input.writes().enabled,
            command: Command::ResetLeakshieldAlarms {
                leakshield: id.to_owned(),
            },
            write_line: write_line(input, id, "alarm-reset"),
        });

        Self {
            state,
            reset,
            recommendation_title: locale.tr("leakshield.alarms.recommendation"),
            recommendations: recommendations(input, &device),
            columns: ["event", "threshold", "device", "response"]
                .iter()
                .map(|name| locale.tr(&format!("leakshield.alarms.column.{name}")))
                .collect(),
            groups,
            device_help: locale.tr("leakshield.alarms.device_help"),
            sound_label: locale.tr("leakshield.alarms.sound"),
        }
    }
}

fn recommendations(input: &ScreenInput<'_>, device: &Device<'_>) -> Vec<Recommendation> {
    let locale = input.locale;
    let Some(data) = device.data.filter(|data| !data.proposals.is_empty()) else {
        return Vec::new();
    };
    let mut list = Vec::new();
    for proposal in &data.proposals {
        let Some(def) = EVENTS.iter().find(|e| e.event == proposal.alarm) else {
            continue;
        };
        let title = locale.event_title(&response_event(def.event));
        let (kind, text, button) = if proposal.overlap == "double-poweroff" {
            (
                RecommendationKind::Warning,
                locale.fmt(
                    "leakshield.alarms.rec.double_poweroff",
                    &[("event", &title)],
                ),
                locale.tr("leakshield.alarms.rec.double_poweroff_button"),
            )
        } else if proposal.overlap == "key-sequence-on" || proposal.current == "key-sequence" {
            (
                RecommendationKind::Warning,
                locale.fmt("leakshield.alarms.rec.key_sequence", &[("event", &title)]),
                locale.tr("leakshield.alarms.rec.key_sequence_button"),
            )
        } else if proposal.proposed != proposal.current {
            (
                RecommendationKind::Information,
                locale.fmt(
                    "leakshield.alarms.rec.use",
                    &[
                        ("event", &title),
                        (
                            "response",
                            &locale.tr(&format!(
                                "leakshield.alarms.device_long.{}",
                                proposal.proposed.replace('-', "_")
                            )),
                        ),
                    ],
                ),
                locale.tr("leakshield.alarms.rec.use_button"),
            )
        } else {
            continue;
        };
        if !DEVICE_RESPONSES.contains(&proposal.proposed.as_str()) {
            continue;
        }
        let field = def.response_field.to_owned();
        let control = format!("recommendation:{}", def.event);
        let writable = device.access(&field).writable;
        list.push(Recommendation {
            kind,
            text,
            button: writable.then(|| {
                (
                    button,
                    fields_command(
                        device.id,
                        &control,
                        vec![(field, FieldValue::Text(proposal.proposed.clone()))],
                    ),
                )
            }),
            write_line: write_line(input, device.id, &control),
        });
    }
    if list.is_empty() {
        list.push(Recommendation {
            kind: RecommendationKind::Success,
            text: locale.tr("leakshield.alarms.rec.ok"),
            button: None,
            write_line: None,
        });
    }
    list
}
