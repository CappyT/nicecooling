//! The app shell: sidebar, banners, status bar and write gate (`docs/ui/screens/app-shell.md`).
//! `ShellModel` holds the decisions. The GPUI view in `crate::shell_view` only draws them.

use crate::i18n::Locale;
use crate::model::{AppState, Link, Severity, format_countdown};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Overview,
    Loops,
    Channels,
    Sensors,
    /// A Leakshield, by its identifier.
    Leakshield(String),
    Profiles,
    Alarms,
    History,
    Settings,
    /// The first-run flow. It has no entry in the sidebar.
    FirstRun,
}

impl Page {
    /// The key of the page title, for the header bar.
    pub fn title_key(&self) -> &'static str {
        match self {
            Self::Overview => "nav.overview",
            Self::Loops => "nav.loops",
            Self::Channels => "nav.channels",
            Self::Sensors => "nav.sensors",
            Self::Leakshield(_) => "nav.leakshield",
            Self::Profiles => "nav.profiles",
            Self::Alarms => "nav.alarms",
            Self::History => "nav.history",
            Self::Settings => "nav.settings",
            Self::FirstRun => "nav.first_run",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavBadge {
    Count(usize),
    Severity(Severity),
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavEntry {
    pub page: Page,
    pub label: String,
    pub badge: Option<NavBadge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavGroup {
    pub title: Option<String>,
    pub entries: Vec<NavEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerStyle {
    Alarm,
    Offline,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BannerAction {
    CancelPoweroff {
        instance: String,
    },
    TryAgain,
    ShowHowToStart,
    OpenFields {
        leakshield: String,
    },
    OpenProfiles,
    ShowNotices,
    /// The button "Turn the Graph responses on" of the Response pause.
    TurnResponsesOn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Poweroff,
    DaemonOffline,
    DaemonDown,
    ResponsePause,
    UnknownFirmware,
    ImmediateSwitching,
    MoreNotices,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banner {
    pub kind: BannerKind,
    pub style: BannerStyle,
    pub title: String,
    pub detail: Option<String>,
    /// The countdown of a poweroff banner: `0:21`.
    pub countdown: Option<String>,
    pub button: Option<(String, BannerAction)>,
}

/// Whether a control that writes works, and what its tooltip says when it does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteGate {
    pub enabled: bool,
    pub tooltip: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusBar {
    pub link: Link,
    pub daemon: String,
    pub profile: Option<String>,
    pub alarms: String,
    pub alarm_severity: Option<Severity>,
}

#[derive(Clone, Debug)]
pub struct ShellModel {
    pub groups: Vec<NavGroup>,
    pub settings: NavEntry,
    pub banners: Vec<Banner>,
    /// The banners that "N more notices" holds.
    pub more_notices: Vec<Banner>,
    pub status: StatusBar,
    pub writes: WriteGate,
    pub connecting: bool,
}

/// A screen shows three banners at most. A fourth one collapses the last into "N more notices".
const MAX_BANNERS: usize = 3;

impl ShellModel {
    pub fn build(state: &AppState, now_us: u64, locale: Locale) -> Self {
        Self::build_with(state, now_us, locale, None)
    }

    /// As `build`, with the Response pause of the Graph service: `response_pause` is the count of
    /// power responses in a row while the pause is on.
    pub fn build_with(
        state: &AppState,
        now_us: u64,
        locale: Locale,
        response_pause: Option<u32>,
    ) -> Self {
        let mut banners = banners(state, now_us, locale, response_pause);
        let mut more_notices = Vec::new();
        if banners.len() > MAX_BANNERS {
            // The Response pause banner shows on every screen: it never goes into the list.
            if let Some(at) = banners
                .iter()
                .position(|banner| banner.kind == BannerKind::ResponsePause)
                && at >= MAX_BANNERS - 1
            {
                let pause = banners.remove(at);
                banners.insert(MAX_BANNERS - 2, pause);
            }
            more_notices = banners.split_off(MAX_BANNERS - 1);
            banners.push(Banner {
                kind: BannerKind::MoreNotices,
                style: BannerStyle::Warning,
                title: locale.fmt(
                    "nav.more_notices",
                    &[("count", &more_notices.len().to_string())],
                ),
                detail: None,
                countdown: None,
                button: None,
            });
        }
        let (groups, settings) = sidebar(state, locale);
        Self {
            groups,
            settings,
            banners,
            more_notices,
            status: status_bar(state, now_us, locale),
            writes: write_gate(state, locale),
            connecting: state.link == Link::Connecting,
        }
    }

    /// The pages in sidebar order. Alt+1 to Alt+9 open the first nine.
    pub fn pages(&self) -> Vec<Page> {
        self.groups
            .iter()
            .flat_map(|group| group.entries.iter().map(|entry| entry.page.clone()))
            .chain(std::iter::once(self.settings.page.clone()))
            .collect()
    }
}

/// Write controls work only while the daemon is online.
pub fn write_gate(state: &AppState, locale: Locale) -> WriteGate {
    let enabled = state.link == Link::Online;
    WriteGate {
        enabled,
        tooltip: (!enabled).then(|| locale.tr("tooltip.daemon_offline")),
    }
}

fn entry(locale: Locale, page: Page, badge: Option<NavBadge>) -> NavEntry {
    NavEntry {
        label: locale.tr(page.title_key()),
        page,
        badge,
    }
}

fn sidebar(state: &AppState, locale: Locale) -> (Vec<NavGroup>, NavEntry) {
    let snapshot = &state.snapshot;
    let mut groups = vec![NavGroup {
        title: None,
        entries: [Page::Overview, Page::Loops, Page::Channels, Page::Sensors]
            .into_iter()
            .map(|page| entry(locale, page, None))
            .collect(),
    }];

    if !snapshot.leakshields.is_empty() {
        let entries = snapshot
            .leakshields
            .iter()
            .map(|device| {
                let text = leakshield_label(locale, snapshot, device);
                let worst = snapshot
                    .alarms
                    .iter()
                    .filter(|alarm| alarm.source == device.id)
                    .map(|alarm| alarm.severity)
                    .max();
                let badge = match worst {
                    Some(severity) => Some(NavBadge::Severity(severity)),
                    None if !device.connected => Some(NavBadge::Offline),
                    None => None,
                };
                NavEntry {
                    page: Page::Leakshield(device.id.clone()),
                    label: text,
                    badge,
                }
            })
            .collect();
        groups.push(NavGroup {
            title: Some(locale.tr("nav.devices")),
            entries,
        });
    }

    // "Daemon down" is an entry of Active Alarms: the count has it.
    let active = snapshot.alarms.len() + usize::from(state.link == Link::Down);
    groups.push(NavGroup {
        title: Some(locale.tr("nav.control")),
        entries: vec![
            entry(locale, Page::Profiles, None),
            entry(
                locale,
                Page::Alarms,
                (active > 0).then_some(NavBadge::Count(active)),
            ),
            entry(locale, Page::History, None),
        ],
    });
    (groups, entry(locale, Page::Settings, None))
}

/// The name of a Leakshield: "Leakshield" when there is one, else "Leakshield 31072" with the
/// last 5 digits of its serial.
pub fn leakshield_label(
    locale: crate::i18n::Locale,
    snapshot: &crate::model::Snapshot,
    device: &crate::model::LeakshieldInfo,
) -> String {
    let label = locale.tr("nav.leakshield");
    if snapshot.leakshields.len() == 1 {
        label
    } else {
        format!("{label} {}", last_digits(&device.serial, 5))
    }
}

fn last_digits(serial: &str, count: usize) -> String {
    let digits: Vec<char> = serial.chars().filter(char::is_ascii_digit).collect();
    digits[digits.len().saturating_sub(count)..]
        .iter()
        .collect()
}

fn banners(
    state: &AppState,
    now_us: u64,
    locale: Locale,
    response_pause: Option<u32>,
) -> Vec<Banner> {
    let mut list = Vec::new();
    let leakshield = locale.tr("nav.leakshield");

    if let Some(alarm) = state.next_poweroff() {
        let source = state.source_name(&alarm.source, &leakshield);
        let loop_name = state
            .source_loop(&alarm.source)
            .map_or_else(|| source.clone(), |found| found.name.clone());
        let remaining = alarm.poweroff_in_s(now_us).unwrap_or(0);
        list.push(Banner {
            kind: BannerKind::Poweroff,
            style: BannerStyle::Alarm,
            title: locale.fmt(
                "banner.poweroff.title",
                &[
                    ("event", &locale.event_title(&alarm.event)),
                    ("source", &source),
                ],
            ),
            detail: Some(locale.fmt("banner.poweroff.detail", &[("loop", &loop_name)])),
            countdown: Some(format_countdown(remaining)),
            button: Some((
                locale.tr("banner.poweroff.button"),
                BannerAction::CancelPoweroff {
                    instance: alarm.instance.clone(),
                },
            )),
        });
    }

    match state.link {
        Link::Offline => list.push(Banner {
            kind: BannerKind::DaemonOffline,
            style: BannerStyle::Offline,
            title: locale.tr("banner.offline.title"),
            detail: None,
            countdown: None,
            button: Some((locale.tr("banner.offline.button"), BannerAction::TryAgain)),
        }),
        Link::Down => list.push(Banner {
            kind: BannerKind::DaemonDown,
            style: BannerStyle::Alarm,
            title: locale.tr("banner.down.title"),
            detail: Some(locale.tr("banner.down.detail")),
            countdown: None,
            button: Some((
                locale.tr("banner.down.button"),
                BannerAction::ShowHowToStart,
            )),
        }),
        Link::Connecting | Link::Online => {}
    }

    if let Some(count) = response_pause {
        list.push(Banner {
            kind: BannerKind::ResponsePause,
            style: BannerStyle::Warning,
            title: locale.fmt(
                "banner.response_pause.title",
                &[("count", &count.to_string())],
            ),
            detail: Some(locale.tr("banner.response_pause.detail")),
            countdown: None,
            button: Some((
                locale.tr("banner.response_pause.button"),
                BannerAction::TurnResponsesOn,
            )),
        });
    }

    // The last read stays on screen while the daemon is away, so these two keep their banner.
    if let Some(device) = state
        .snapshot
        .leakshields
        .iter()
        .find(|device| device.read_only_reason == "unknown-firmware")
    {
        list.push(Banner {
            kind: BannerKind::UnknownFirmware,
            style: BannerStyle::Warning,
            title: locale.fmt(
                "banner.unknown_firmware.title",
                &[("firmware", &device.firmware.to_string())],
            ),
            detail: None,
            countdown: None,
            button: Some((
                locale.tr("banner.unknown_firmware.button"),
                BannerAction::OpenFields {
                    leakshield: device.id.clone(),
                },
            )),
        });
    }

    if state.snapshot.immediate_switching {
        list.push(Banner {
            kind: BannerKind::ImmediateSwitching,
            style: BannerStyle::Warning,
            title: locale.tr("banner.immediate_switching.title"),
            detail: None,
            countdown: None,
            button: Some((
                locale.tr("banner.immediate_switching.button"),
                BannerAction::OpenProfiles,
            )),
        });
    }
    list
}

fn status_bar(state: &AppState, now_us: u64, locale: Locale) -> StatusBar {
    let daemon = match state.link {
        Link::Connecting => locale.tr("status.connecting"),
        Link::Online => locale.tr("status.daemon_connected"),
        Link::Offline => locale.tr("status.daemon_offline"),
        Link::Down => locale.tr("status.daemon_down"),
    };
    let down = state.link == Link::Down;
    let profile = state
        .is_online()
        .then_some(state.snapshot.active_profile.as_ref())
        .flatten()
        .map(|profile| {
            let cause = match state.snapshot.cause {
                crate::model::Cause::Rule => Some("status.cause_rule"),
                crate::model::Cause::Manual => Some("status.cause_manual"),
                crate::model::Cause::Default => None,
            };
            match cause {
                Some(key) => locale.fmt(
                    "status.profile_with_cause",
                    &[("name", &profile.name), ("cause", &locale.tr(key))],
                ),
                None => locale.fmt("status.profile", &[("name", &profile.name)]),
            }
        })
        .or_else(|| down.then(|| locale.tr("status.profile_none")));
    // A daemon that is down runs no alarm response: the bar says so in the Alarm color.
    let (alarms, alarm_severity) = if down {
        (locale.tr("status.protection_off"), Some(Severity::Alarm))
    } else {
        (
            alarm_summary(state, now_us, locale),
            state
                .snapshot
                .alarms
                .iter()
                .map(|alarm| alarm.severity)
                .max(),
        )
    };
    StatusBar {
        link: state.link,
        daemon,
        profile,
        alarms,
        alarm_severity,
    }
}

/// "No active Alarms", "2 Warnings", "1 Alarm", or "1 Alarm, poweroff in 0:21".
pub fn alarm_summary(state: &AppState, now_us: u64, locale: Locale) -> String {
    let (alarms, warnings) = (state.alarm_count(), state.warning_count());
    if alarms + warnings == 0 {
        return locale.tr("status.no_alarms");
    }
    let mut parts = Vec::new();
    if alarms > 0 {
        parts.push(locale.plural("status.alarms", alarms));
    }
    if warnings > 0 {
        parts.push(locale.plural("status.warnings", warnings));
    }
    if let Some(seconds) = state
        .next_poweroff()
        .and_then(|alarm| alarm.poweroff_in_s(now_us))
    {
        parts.push(locale.fmt(
            "status.poweroff_in",
            &[("time", &format_countdown(seconds))],
        ));
    }
    parts.join(", ")
}
