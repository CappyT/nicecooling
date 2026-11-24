//! The Leakshield page: one frame with nine tabs. Each tab has a model that decides what shows
//! (`docs/ui/screens/leakshield-*.md`) and a view that draws it.

pub mod alarms;
pub mod alarms_view;
pub mod charts;
pub mod display;
pub mod display_view;
pub mod fields;
pub mod fields_view;
pub mod frame_view;
pub mod lighting;
pub mod lighting_view;
pub mod overview;
pub mod overview_view;
pub mod parts;
pub mod schematic;
pub mod sensors;
pub mod sensors_view;
pub mod settings;
pub mod settings_view;
pub mod status;
pub mod status_view;
pub mod system;
pub mod system_view;

use std::collections::VecDeque;

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{
    AlarmEntry, FieldEntry, FieldValue, LeakshieldData, LeakshieldInfo, Link, SensorSample,
    Severity,
};
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tab {
    #[default]
    Overview,
    Settings,
    Sensors,
    Display,
    Lighting,
    Alarms,
    Status,
    System,
    Fields,
}

impl Tab {
    pub const ALL: [Tab; 9] = [
        Tab::Overview,
        Tab::Settings,
        Tab::Sensors,
        Tab::Display,
        Tab::Lighting,
        Tab::Alarms,
        Tab::Status,
        Tab::System,
        Tab::Fields,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Overview => "leakshield.tab.overview",
            Self::Settings => "leakshield.tab.settings",
            Self::Sensors => "leakshield.tab.sensors",
            Self::Display => "leakshield.tab.display",
            Self::Lighting => "leakshield.tab.lighting",
            Self::Alarms => "leakshield.tab.alarms",
            Self::Status => "leakshield.tab.status",
            Self::System => "leakshield.tab.system",
            Self::Fields => "leakshield.tab.fields",
        }
    }
}

/// The values that the app keeps for the charts of the Overview and Status tabs.
const TRACE_WINDOW_US: u64 = 600_000_000;

#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub points: VecDeque<(u64, f64)>,
}

impl Trace {
    pub fn push(&mut self, time_us: u64, value: f64) {
        if self.points.back().is_some_and(|(t, _)| *t >= time_us) {
            return;
        }
        self.points.push_back((time_us, value));
        while self
            .points
            .front()
            .is_some_and(|(t, _)| time_us.saturating_sub(*t) > TRACE_WINDOW_US)
        {
            self.points.pop_front();
        }
    }
}

/// The last 10 minutes of the live values of one Leakshield.
#[derive(Clone, Debug, Default)]
pub struct Traces {
    pub pressure: Trace,
    pub volume_change: Trace,
    pub pressure_change: Trace,
    /// The status text with its start time, for the bands of the Status tile.
    pub status: VecDeque<(u64, String)>,
}

impl Traces {
    pub fn record(&mut self, id: &str, sample: &SensorSample, status: &str) {
        let value = |name: &str| sample.sensor_value(&format!("{id}_{name}"));
        if let Some(v) = value("pressure") {
            self.pressure.push(sample.time_us, v);
        }
        if let Some(v) = value("volume_change") {
            self.volume_change.push(sample.time_us, v);
        }
        if let Some(v) = value("pressure_change") {
            self.pressure_change.push(sample.time_us, v);
        }
        if self.status.back().is_none_or(|(_, last)| last != status) {
            self.status.push_back((sample.time_us, status.to_owned()));
        }
        while self
            .status
            .front()
            .is_some_and(|(t, _)| sample.time_us.saturating_sub(*t) > TRACE_WINDOW_US)
        {
            self.status.pop_front();
        }
    }
}

/// What the user changed on the Leakshield page.
#[derive(Default)]
pub struct LeakshieldUi {
    pub tab: Tab,
    pub traces: std::collections::BTreeMap<String, Traces>,
    pub overview: overview::OverviewUi,
    pub settings: settings::SettingsUi,
    pub sensors: sensors::SensorsUi,
    pub display: display::DisplayUi,
    pub lighting: lighting::LightingUi,
    pub alarms: alarms::AlarmsUi,
    pub status: status::StatusUi,
    pub system: system::SystemUi,
    pub fields: fields::FieldsUi,
}

impl LeakshieldUi {
    /// The link "Open Fields" of the other tabs: the Fields tab with the read-only filter.
    pub fn open_fields(&mut self, field: Option<&str>) {
        self.fields.filter = fields::Filter::ReadOnly;
        self.fields.selected = field.map(str::to_owned);
        self.tab = Tab::Fields;
    }
}

/// The write access to one field, and the reason when it has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    pub writable: bool,
    /// The reason identifier of `docs/api.md`, empty for a writable field.
    pub reason: String,
}

/// What a tab reads of one Leakshield: its properties, its data and the link to the daemon.
pub struct Device<'a> {
    pub id: &'a str,
    pub info: Option<&'a LeakshieldInfo>,
    pub data: Option<&'a LeakshieldData>,
    pub locale: Locale,
    pub link: Link,
}

/// The state that every tab shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabState {
    /// The first read of the daemon is not there yet.
    Loading,
    /// The daemon is away or the Leakshield does not answer: the last values show, dim.
    Offline,
    Ready,
}

impl<'a> Device<'a> {
    pub fn new(input: &'a ScreenInput<'_>, id: &'a str) -> Self {
        let snapshot = &input.state.snapshot;
        Self {
            id,
            info: snapshot.leakshields.iter().find(|d| d.id == id),
            data: snapshot.leakshield_data.get(id),
            locale: input.locale,
            link: input.state.link,
        }
    }

    pub fn state(&self) -> TabState {
        match (self.link, self.info) {
            (Link::Connecting, _) => TabState::Loading,
            (Link::Online, Some(info)) if info.connected => TabState::Ready,
            _ => TabState::Offline,
        }
    }

    pub fn unknown_firmware(&self) -> bool {
        self.info
            .is_some_and(|info| info.read_only_reason == "unknown-firmware")
    }

    /// Controls that write work only while the daemon and the Leakshield answer.
    pub fn can_write(&self) -> bool {
        self.state() == TabState::Ready
    }

    pub fn field(&self, id: &str) -> Option<&'a FieldEntry> {
        self.data.and_then(|data| data.field(id))
    }

    pub fn number(&self, id: &str) -> Option<f64> {
        self.data.and_then(|data| data.number(id))
    }

    pub fn list(&self, id: &str) -> Option<&'a [f64]> {
        self.data.and_then(|data| data.list(id))
    }

    pub fn text_list(&self, id: &str) -> Option<&'a [String]> {
        self.data.and_then(|data| data.text_list(id))
    }

    pub fn text(&self, id: &str) -> Option<&'a str> {
        match &self.field(id)?.value {
            FieldValue::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Whether a field accepts a write, and why not.
    pub fn access(&self, id: &str) -> Access {
        let reason = if let Some(info) = self.info
            && !info.read_only_reason.is_empty()
        {
            info.read_only_reason.clone()
        } else {
            match self.field(id) {
                Some(field) if field.writable => String::new(),
                Some(field) if !field.reason.is_empty() => field.reason.clone(),
                _ => "not-verified".to_owned(),
            }
        };
        Access {
            writable: reason.is_empty() && self.can_write(),
            reason,
        }
    }

    /// The text of a read-only reason.
    pub fn reason_text(&self, reason: &str) -> String {
        reason_text(self.locale, reason)
    }

    /// The active Alarms and Warnings of this Leakshield.
    pub fn alarms<'s>(&self, input: &'s ScreenInput<'_>) -> Vec<&'s AlarmEntry> {
        input
            .state
            .snapshot
            .alarms
            .iter()
            .filter(|alarm| alarm.source == self.id)
            .collect()
    }

    /// The latest value of a sensor of this Leakshield: `pressure`, `fill_level`.
    pub fn sensor(&self, input: &ScreenInput<'_>, name: &str) -> Option<f64> {
        let state = input.state;
        let sample = state.sensors.as_ref().or(state.last_sensors.as_ref())?;
        sample.sensor_value(&format!("{}_{name}", self.id))
    }
}

/// A value that is a change, with its sign: "+0.4 ml/h", "-0.1 mbar/min". "--" without a value.
pub fn change_text(locale: Locale, value: Option<f64>, unit: &str) -> String {
    let text = crate::screens::format::optional_value(locale, value, unit);
    match value {
        // A value that rounds to zero has no sign.
        Some(value) if value > 0.0 && text.contains(|c: char| ('1'..='9').contains(&c)) => {
            format!("+{text}")
        }
        _ => text,
    }
}

/// "Read-only: unknown firmware." and the other reasons of `docs/api.md`.
pub fn reason_text(locale: Locale, reason: &str) -> String {
    let key = match reason {
        "unknown-firmware" | "unknown-structure" | "not-verified" | "disconnected" => {
            format!("leakshield.reason.{}", reason.replace('-', "_"))
        }
        _ => "leakshield.reason.not_verified".to_owned(),
    };
    locale.tr(&key)
}

/// The text of a daemon refusal of a Leakshield write.
pub fn refused_text(locale: Locale, error: &str) -> String {
    let key = match error {
        "ReadOnly" | "NotVerified" => "leakshield.refused.read_only",
        "Busy" => "leakshield.refused.busy",
        "DeviceChanged" | "VerifyFailed" => "leakshield.refused.read_back",
        "Disconnected" => "leakshield.refused.disconnected",
        "NotSupported" => "leakshield.refused.not_supported",
        _ => "feedback.refused",
    };
    locale.tr(key)
}

/// The write feedback of a control. The key is `Command::feedback_key`.
pub fn write_line(input: &ScreenInput<'_>, device: &str, control: &str) -> Option<FeedbackLine> {
    let outcome = input.feedback.get(&format!("ls:{device}:{control}"))?;
    let refused = match outcome {
        WriteOutcome::Refused { error } => refused_text(input.locale, error),
        _ => String::new(),
    };
    Some(FeedbackLine::new(outcome, input.locale, &refused, None))
}

/// A write of fields as a command. `control` names the control that shows the result.
pub fn fields_command(device: &str, control: &str, fields: Vec<(String, FieldValue)>) -> Command {
    Command::SetLeakshieldFields {
        leakshield: device.to_owned(),
        control: control.to_owned(),
        fields,
    }
}

/// The kind of tag for a severity.
pub fn severity_of(alarms: &[&AlarmEntry]) -> Option<Severity> {
    alarms.iter().map(|alarm| alarm.severity).max()
}

/// The frame of the page: the title, the serial and the tab labels.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub title: String,
    pub subtitle: String,
    pub tabs: Vec<(Tab, String)>,
    /// The Leakshield does not answer: "The Leakshield does not answer. Check its USB cable."
    pub offline: Option<String>,
}

impl Frame {
    pub fn build(input: &ScreenInput<'_>, id: &str, pending_settings: bool) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let serial = device.info.map_or("", |info| info.serial.as_str());
        let offline = match device.state() {
            TabState::Offline if device.link == Link::Online => {
                Some(locale.tr("leakshield.offline"))
            }
            _ => None,
        };
        Self {
            title: locale.tr("nav.leakshield"),
            subtitle: locale.fmt("leakshield.serial", &[("serial", serial)]),
            tabs: Tab::ALL
                .iter()
                .map(|tab| {
                    let label = locale.tr(tab.key());
                    // A dot marks the pending changes of the wizard.
                    let label = if *tab == Tab::Settings && pending_settings {
                        format!("{label} \u{2022}")
                    } else {
                        label
                    };
                    (*tab, label)
                })
                .collect(),
            offline,
        }
    }
}
