//! The Overview screen (`docs/ui/screens/overview.md`): one card per Loop with its diagram,
//! the pressure card while a Leakshield event is active, the active Alarms, the key
//! temperatures and the active Profile.

use crate::model::{AlarmEntry, Cause, Link, Response, Severity};
use crate::screens::ScreenInput;
use crate::screens::format::{clock, optional_value, value_text};
use crate::shell::Page;
use crate::widgets::loop_diagram::{LoopDiagram, last_value_time};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverviewState {
    /// The daemon did not answer yet: card frames with grey bars.
    Loading,
    /// No Loop exists.
    Empty,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagTone {
    Neutral,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModeTag {
    pub text: String,
    pub tone: TagTone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopCard {
    pub id: String,
    pub name: String,
    pub mode_tag: Option<ModeTag>,
    pub caption: String,
    /// The tag "Not live" of a daemon that is down.
    pub not_live: Option<String>,
    /// "Last value 21:40:12" while the daemon is away.
    pub last_values: Option<String>,
    pub diagram: LoopDiagram,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PressureCard {
    pub title: String,
    pub subtitle: String,
    /// The history series of the value that caused the event.
    pub sensor: String,
    pub unit: String,
    /// The limit as a dashed line, with its label.
    pub limit: Option<(f64, String)>,
    /// The time of the vertical line, in Unix microseconds, with its label.
    pub raised: Option<(u64, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTone {
    Warning,
    Alarm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlarmRow {
    pub title: String,
    pub detail: String,
    pub tone: RowTone,
    /// The page of the source of the event.
    pub opens: Option<Page>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyTemperature {
    pub label: String,
    pub sensor: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileCard {
    /// "None" while the daemon is down.
    pub name: String,
    pub tag: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmptyCard {
    pub title: String,
    pub text: String,
    pub button: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverviewModel {
    pub state: OverviewState,
    pub empty: Option<EmptyCard>,
    pub loops: Vec<LoopCard>,
    pub pressure: Option<PressureCard>,
    pub alarms: Vec<AlarmRow>,
    pub no_alarms: String,
    pub temperatures: Vec<KeyTemperature>,
    pub profile: ProfileCard,
}

impl OverviewModel {
    pub fn build(input: &ScreenInput<'_>) -> Self {
        let locale = input.locale;
        let state = input.state;
        let link = state.link;
        let ui_state = if link == Link::Connecting {
            OverviewState::Loading
        } else if state.snapshot.loops.is_empty() {
            OverviewState::Empty
        } else {
            OverviewState::Ready
        };

        let last_time = last_value_time(input);
        let loops = state
            .snapshot
            .loops
            .iter()
            .map(|info| {
                let device = info
                    .leakshield
                    .as_ref()
                    .and_then(|id| state.snapshot.leakshields.iter().find(|d| &d.id == id));
                let mode_tag = device.and_then(|device| mode_tag(input, device));
                let leakshield = locale.tr(if device.is_some() {
                    "loop.with_leakshield"
                } else {
                    "loop.without_leakshield"
                });
                LoopCard {
                    id: info.id.clone(),
                    name: info.name.clone(),
                    mode_tag,
                    caption: locale.fmt(
                        "loop.caption",
                        &[
                            ("leakshield", &leakshield),
                            (
                                "pumps",
                                &locale.plural("loop.pumps", info.pump_channels.len()),
                            ),
                        ],
                    ),
                    not_live: (link == Link::Down).then(|| locale.tr("overview.not_live")),
                    last_values: match (link, &last_time) {
                        (Link::Offline, Some(time)) => {
                            Some(locale.fmt("overview.last_value", &[("time", time)]))
                        }
                        (Link::Down, Some(time)) => {
                            Some(locale.fmt("overview.last_values", &[("time", time)]))
                        }
                        _ => None,
                    },
                    diagram: LoopDiagram::build(input, info),
                }
            })
            .collect();

        let empty = (ui_state == OverviewState::Empty).then(|| EmptyCard {
            title: locale.tr("overview.empty.title"),
            text: locale.tr("overview.empty.text"),
            button: locale.tr("overview.empty.button"),
        });

        let mut alarms: Vec<AlarmRow> = Vec::new();
        if link == Link::Down {
            let since = input.down_since_us.map_or_else(String::new, clock);
            alarms.push(AlarmRow {
                title: locale.tr("overview.daemon_down.title"),
                detail: locale.fmt("overview.daemon_down.detail", &[("time", &since)]),
                tone: RowTone::Alarm,
                opens: Some(Page::Settings),
            });
        }
        let mut active: Vec<&AlarmEntry> = state.snapshot.alarms.iter().collect();
        active.sort_by_key(|alarm| (std::cmp::Reverse(alarm.severity), alarm.raised_us));
        for alarm in active {
            alarms.push(alarm_row(input, alarm));
        }

        Self {
            state: ui_state,
            empty,
            loops,
            pressure: pressure_card(input),
            alarms,
            no_alarms: locale.tr("overview.no_alarms"),
            temperatures: key_temperatures(input),
            profile: profile_card(input),
        }
    }
}

fn mode_tag(input: &ScreenInput<'_>, device: &crate::model::LeakshieldInfo) -> Option<ModeTag> {
    let locale = input.locale;
    if !device.running_action.is_empty() {
        let key = format!(
            "overview.action_running.{}",
            device.running_action.replace('-', "_")
        );
        return Some(ModeTag {
            text: locale.tr(&key),
            tone: TagTone::Neutral,
        });
    }
    match device.mode.as_str() {
        "shield" => Some(ModeTag {
            text: "SHIELD".to_owned(),
            tone: TagTone::Neutral,
        }),
        "monitor" => Some(ModeTag {
            text: "MONITOR".to_owned(),
            tone: TagTone::Neutral,
        }),
        "release" => Some(ModeTag {
            text: "RELEASE".to_owned(),
            tone: TagTone::Warning,
        }),
        _ => None,
    }
}

/// The page that shows the source of an event.
pub fn source_page(input: &ScreenInput<'_>, source: &str) -> Option<Page> {
    let snapshot = &input.state.snapshot;
    if snapshot
        .leakshields
        .iter()
        .any(|device| device.id == source)
    {
        Some(Page::Leakshield(source.to_owned()))
    } else if snapshot.loops.iter().any(|l| l.id == source) {
        Some(Page::Loops)
    } else if snapshot.channels.contains_key(source) {
        Some(Page::Channels)
    } else if snapshot.sensors.contains_key(source) {
        Some(Page::Sensors)
    } else {
        None
    }
}

fn alarm_row(input: &ScreenInput<'_>, alarm: &AlarmEntry) -> AlarmRow {
    let locale = input.locale;
    let leakshield = locale.tr("nav.leakshield");
    let source = input.state.source_name(&alarm.source, &leakshield);
    let response = match alarm.response {
        Response::None => locale.tr("overview.response.none"),
        Response::Notify => locale.tr("overview.response.notify"),
        Response::Poweroff => {
            let delay = alarm.poweroff_at_us.saturating_sub(alarm.raised_us) / 1_000_000;
            if alarm.poweroff_at_us != 0 && alarm.raised_us != 0 {
                locale.fmt(
                    "overview.response.poweroff_after",
                    &[("seconds", &delay.to_string())],
                )
            } else {
                locale.tr("overview.response.poweroff")
            }
        }
    };
    AlarmRow {
        title: locale.event_title(&alarm.event),
        detail: locale.fmt(
            "overview.alarm_detail",
            &[
                ("source", &source),
                ("time", &clock(alarm.raised_us)),
                ("response", &response),
            ],
        ),
        tone: match alarm.severity {
            Severity::Alarm => RowTone::Alarm,
            Severity::Warning => RowTone::Warning,
        },
        opens: source_page(input, &alarm.source),
    }
}

/// The pressure card shows while a Leakshield Alarm or Warning is active. The event with the
/// highest severity gives the value, the limit and the time.
fn pressure_card(input: &ScreenInput<'_>) -> Option<PressureCard> {
    let locale = input.locale;
    let snapshot = &input.state.snapshot;
    let alarm = snapshot
        .alarms
        .iter()
        .filter(|alarm| {
            alarm.event.starts_with("leakshield.")
                && snapshot.leakshields.iter().any(|d| d.id == alarm.source)
        })
        .max_by_key(|alarm| alarm.severity)?;
    let sensor = alarm
        .details
        .sensor
        .clone()
        .unwrap_or_else(|| format!("{}_pressure", alarm.source));
    let unit = if alarm.details.unit.is_empty() {
        "mbar".to_owned()
    } else {
        alarm.details.unit.clone()
    };
    Some(PressureCard {
        title: locale.tr("overview.pressure.title"),
        subtitle: locale.tr("overview.pressure.subtitle"),
        limit: alarm.details.limit.map(|limit| {
            let label = locale.fmt(
                "overview.pressure.limit",
                &[("value", &value_text(locale, limit, &unit))],
            );
            (limit, label)
        }),
        raised: (alarm.raised_us != 0).then(|| {
            (
                alarm.raised_us,
                locale.fmt(
                    "overview.pressure.raised",
                    &[("time", &clock(alarm.raised_us))],
                ),
            )
        }),
        sensor,
        unit,
    })
}

/// CPU, GPU, chipset and VRM. A Board that lacks one has no entry.
fn key_temperatures(input: &ScreenInput<'_>) -> Vec<KeyTemperature> {
    let locale = input.locale;
    let state = input.state;
    let live = state.is_online();
    let sample = state.sensors.as_ref();
    let wanted: [(&str, &[&str], &[&str]); 4] = [
        (
            "overview.temp.cpu",
            &["k10temp", "coretemp", "zenpower"],
            &["cpu"],
        ),
        (
            "overview.temp.gpu",
            &["amdgpu", "nouveau", "nvidia", "radeon"],
            &["gpu"],
        ),
        ("overview.temp.chipset", &[], &["chipset", "pch"]),
        ("overview.temp.vrm", &[], &["vrm", "mos"]),
    ];
    wanted
        .iter()
        .filter_map(|(label, drivers, words)| {
            let sensor = state.snapshot.sensors.values().find(|sensor| {
                sensor.kind == "temperature" && sensor.derived.is_none() && {
                    let name = sensor.default_name.to_lowercase();
                    let id = sensor.id.to_lowercase();
                    drivers.contains(&sensor.driver.as_str())
                        || words.iter().any(|w| name.contains(w) || id.contains(w))
                }
            })?;
            let value = sample
                .and_then(|s| s.sensor_value(&sensor.id))
                .filter(|_| live);
            Some(KeyTemperature {
                label: locale.tr(label),
                sensor: sensor.id.clone(),
                value: optional_value(locale, value, &sensor.unit),
            })
        })
        .collect()
}

fn profile_card(input: &ScreenInput<'_>) -> ProfileCard {
    let locale = input.locale;
    let snapshot = &input.state.snapshot;
    if input.state.link == Link::Down || snapshot.active_profile.is_none() {
        return ProfileCard {
            name: locale.tr("overview.profile.none"),
            tag: None,
            detail: None,
        };
    }
    let name = snapshot
        .active_profile
        .as_ref()
        .map_or_else(String::new, |profile| profile.name.clone());
    let (tag, detail) = match snapshot.cause {
        Cause::Rule => (Some(locale.tr("overview.profile.by_group")), None),
        Cause::Manual => (
            Some(locale.tr("overview.profile.manual")),
            Some(locale.tr("overview.profile.manual_detail")),
        ),
        Cause::Default => (None, None),
    };
    ProfileCard { name, tag, detail }
}
