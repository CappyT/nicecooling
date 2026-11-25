//! The StatusNotifierItem tray icon and its DBusMenu menu (`ksni`).
//! `TrayView` is what the tray shows. It comes from the app state and has no D-Bus in it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{MenuItem, Status};
use tokio::sync::mpsc::UnboundedSender;

use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::graphs::view::Countdown;
use crate::i18n::Locale;
use crate::model::{AppState, Cause, Link, LoopInfo, SensorSample, format_countdown};
use crate::shell::alarm_summary;

pub const ITEM_ID: &str = "io.github.cappyt.NiceCooling";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayIcon {
    Normal,
    Warning,
    Alarm,
    Offline,
}

impl TrayIcon {
    /// The name of the symbolic icon in the icon theme.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "nicecooling-symbolic",
            Self::Warning => "nicecooling-warning-symbolic",
            Self::Alarm => "nicecooling-alarm-symbolic",
            Self::Offline => "nicecooling-offline-symbolic",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileItem {
    pub id: String,
    pub label: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayEntry {
    CancelPoweroff {
        label: String,
        instance: String,
    },
    /// The countdown of a Graph power response.
    CancelGraphCountdown {
        label: String,
    },
    /// A line that does not react to a click.
    Info {
        label: String,
    },
    Profiles {
        label: String,
        enabled: bool,
        items: Vec<ProfileItem>,
        revoke: Option<String>,
    },
    Separator,
    Open {
        label: String,
    },
    Quit {
        label: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayView {
    pub icon: TrayIcon,
    /// The status "NeedsAttention" shows the attention icon.
    pub needs_attention: bool,
    pub tooltip: String,
    pub entries: Vec<TrayEntry>,
}

impl TrayView {
    pub fn build(state: &AppState, now_us: u64, locale: Locale) -> Self {
        let icon = match state.link {
            Link::Down => TrayIcon::Alarm,
            Link::Offline | Link::Connecting => TrayIcon::Offline,
            Link::Online if state.alarm_count() > 0 => TrayIcon::Alarm,
            Link::Online if state.warning_count() > 0 => TrayIcon::Warning,
            Link::Online => TrayIcon::Normal,
        };
        let tooltip = match state.link {
            Link::Down => locale.tr("tray.tooltip_down"),
            Link::Offline | Link::Connecting => locale.tr("tray.tooltip_offline"),
            Link::Online => locale.fmt(
                "tray.tooltip",
                &[("summary", &alarm_summary(state, now_us, locale))],
            ),
        };

        let mut entries = Vec::new();
        match state.link {
            Link::Down => entries.push(TrayEntry::Info {
                label: locale.tr("tray.daemon_down"),
            }),
            Link::Offline | Link::Connecting => {
                entries.push(TrayEntry::Info {
                    label: locale.tr("tray.daemon_offline"),
                });
            }
            Link::Online => {
                if let Some(alarm) = state.next_poweroff() {
                    let time = format_countdown(alarm.poweroff_in_s(now_us).unwrap_or(0));
                    entries.push(TrayEntry::CancelPoweroff {
                        label: locale.fmt("tray.cancel_poweroff", &[("time", &time)]),
                        instance: alarm.instance.clone(),
                    });
                }
                for found in &state.snapshot.loops {
                    entries.push(TrayEntry::Info {
                        label: loop_line(found, state.sensors.as_ref(), locale),
                    });
                }
            }
        }
        entries.push(profile_entry(state, locale));
        entries.push(TrayEntry::Separator);
        entries.push(TrayEntry::Open {
            label: locale.tr("tray.open"),
        });
        entries.push(TrayEntry::Quit {
            label: locale.tr("tray.quit"),
        });

        Self {
            needs_attention: icon == TrayIcon::Alarm,
            icon,
            tooltip,
            entries,
        }
    }
}

impl TrayView {
    /// Add the entry that cancels the countdown of a Graph power response. It comes after the
    /// entry of an Alarm poweroff, which has the priority.
    pub fn with_graph_countdown(mut self, countdown: Option<&Countdown>, locale: Locale) -> Self {
        if let Some(countdown) = countdown {
            let at = usize::from(matches!(
                self.entries.first(),
                Some(TrayEntry::CancelPoweroff { .. })
            ));
            let time = format_countdown(countdown.seconds_left);
            self.entries.insert(
                at,
                TrayEntry::CancelGraphCountdown {
                    label: locale.fmt(
                        "tray.cancel_graph_countdown",
                        &[("name", &countdown.graph_name), ("time", &time)],
                    ),
                },
            );
        }
        self
    }
}

fn profile_entry(state: &AppState, locale: Locale) -> TrayEntry {
    let enabled = state.is_online();
    let snapshot = &state.snapshot;
    let active = snapshot.active_profile.as_ref();
    let label = match active {
        Some(profile) if enabled => locale.fmt("tray.profile", &[("name", &profile.name)]),
        _ => locale.tr("tray.profile_none"),
    };
    let items = if enabled {
        snapshot
            .profiles
            .iter()
            .map(|profile| ProfileItem {
                id: profile.id.clone(),
                label: profile.name.clone(),
                active: active.is_some_and(|a| a.id == profile.id),
            })
            .collect()
    } else {
        Vec::new()
    };
    let revoke =
        (enabled && snapshot.cause == Cause::Manual).then(|| locale.tr("tray.revoke_manual"));
    TrayEntry::Profiles {
        label,
        enabled,
        items,
        revoke,
    }
}

/// "Main loop: 31.4 °C, 115 L/h, 298 mbar". A missing value is "--". A Loop without a
/// Leakshield has no pressure.
fn loop_line(found: &LoopInfo, sample: Option<&SensorSample>, locale: Locale) -> String {
    let value = |sensor: Option<&str>, unit: &str, decimals: usize| {
        sensor
            .and_then(|id| sample.and_then(|s| s.sensor_value(id)))
            .map_or_else(|| "--".to_owned(), |v| format!("{v:.decimals$} {unit}"))
    };
    let mut values = vec![
        value(found.water_temperature.as_deref(), "°C", 1),
        value(found.flow.as_deref(), "L/h", 0),
    ];
    if let Some(leakshield) = &found.leakshield {
        values.push(value(Some(&format!("{leakshield}_pressure")), "mbar", 0));
    }
    locale.fmt(
        "tray.loop_line",
        &[("name", &found.name), ("values", &values.join(", "))],
    )
}

/// The `ksni` item. The menu callbacks send a `Command` and return at once.
pub struct NiceTray {
    view: TrayView,
    commands: UnboundedSender<Command>,
    host_online: Arc<AtomicBool>,
}

impl NiceTray {
    fn send(tray: &Self, command: Command) {
        let _ = tray.commands.send(command);
    }
}

impl ksni::Tray for NiceTray {
    fn id(&self) -> String {
        ITEM_ID.into()
    }

    fn title(&self) -> String {
        "nicecooling".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn status(&self) -> Status {
        if self.view.needs_attention {
            Status::NeedsAttention
        } else {
            Status::Active
        }
    }

    fn icon_name(&self) -> String {
        self.view.icon.name().into()
    }

    fn attention_icon_name(&self) -> String {
        TrayIcon::Alarm.name().into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.view.tooltip.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        Self::send(self, Command::OpenWindow(None));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.view
            .entries
            .iter()
            .map(|entry| match entry {
                TrayEntry::CancelPoweroff { label, instance } => {
                    let instance = instance.clone();
                    StandardItem {
                        label: label.clone(),
                        activate: Box::new(move |tray: &mut Self| {
                            Self::send(
                                tray,
                                Command::CancelPoweroff {
                                    instance: instance.clone(),
                                },
                            );
                        }),
                        ..Default::default()
                    }
                    .into()
                }
                TrayEntry::CancelGraphCountdown { label } => StandardItem {
                    label: label.clone(),
                    activate: Box::new(|tray: &mut Self| {
                        Self::send(tray, Command::Graph(GraphCommand::CancelCountdown));
                    }),
                    ..Default::default()
                }
                .into(),
                TrayEntry::Info { label } => StandardItem {
                    label: label.clone(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                TrayEntry::Profiles {
                    label,
                    enabled,
                    items,
                    revoke,
                } => {
                    let mut submenu: Vec<MenuItem<Self>> = items
                        .iter()
                        .map(|item| {
                            let id = item.id.clone();
                            CheckmarkItem {
                                label: item.label.clone(),
                                checked: item.active,
                                activate: Box::new(move |tray: &mut Self| {
                                    Self::send(tray, Command::ActivateProfile { id: id.clone() });
                                }),
                                ..Default::default()
                            }
                            .into()
                        })
                        .collect();
                    if let Some(label) = revoke {
                        submenu.push(MenuItem::Separator);
                        submenu.push(
                            StandardItem {
                                label: label.clone(),
                                activate: Box::new(|tray: &mut Self| {
                                    Self::send(tray, Command::ReturnToAutomatic);
                                }),
                                ..Default::default()
                            }
                            .into(),
                        );
                    }
                    SubMenu {
                        label: label.clone(),
                        enabled: *enabled,
                        submenu,
                        ..Default::default()
                    }
                    .into()
                }
                TrayEntry::Separator => MenuItem::Separator,
                TrayEntry::Open { label } => StandardItem {
                    label: label.clone(),
                    activate: Box::new(|tray: &mut Self| {
                        Self::send(tray, Command::OpenWindow(None))
                    }),
                    ..Default::default()
                }
                .into(),
                TrayEntry::Quit { label } => StandardItem {
                    label: label.clone(),
                    activate: Box::new(|tray: &mut Self| Self::send(tray, Command::Quit)),
                    ..Default::default()
                }
                .into(),
            })
            .collect()
    }

    fn watcher_online(&self) {
        self.host_online.store(true, Ordering::Relaxed);
    }

    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        self.host_online.store(false, Ordering::Relaxed);
        true
    }
}

/// A running tray icon. Call `update` from a plain thread, not from a Tokio worker:
/// the blocking API of `ksni` runs its own executor.
pub struct Tray {
    handle: ksni::blocking::Handle<NiceTray>,
    host_online: Arc<AtomicBool>,
}

impl Tray {
    /// Register the item on the session bus. At login the app can start before the
    /// StatusNotifier host: the item then waits for it, and `host_online` is false until it
    /// comes.
    pub fn spawn(commands: UnboundedSender<Command>, view: TrayView) -> Result<Self, ksni::Error> {
        let host_online = Arc::new(AtomicBool::new(true));
        let handle = NiceTray {
            view,
            commands,
            host_online: host_online.clone(),
        }
        .assume_sni_available(true)
        .spawn()?;
        Ok(Self {
            handle,
            host_online,
        })
    }

    pub fn update(&self, view: TrayView) {
        self.handle.update(|tray| tray.view = view);
    }

    /// False after the host left the bus. Settings shows the AppIndicator hint then.
    pub fn host_online(&self) -> bool {
        self.host_online.load(Ordering::Relaxed)
    }

    pub fn shutdown(&self) {
        self.handle.shutdown().wait();
    }
}
