//! Desktop notifications through `org.freedesktop.Notifications` (`notify-rust`).
//! `plan` turns a daemon event into notifications and the app owns the text.

use std::collections::HashMap;
use std::thread;

use notify_rust::{Notification, Timeout, Urgency};
use tokio::sync::mpsc::UnboundedSender;

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{AlarmEntry, AppState, Cause, DaemonEvent, Link, Response, Severity};
use crate::shell::Page;

pub const APP_NAME: &str = "nicecooling";
const APP_ICON: &str = "io.github.cappyt.NiceCooling";
const DAEMON_DOWN_KEY: &str = "daemon-down";

/// The switches of the Settings screen (`notifications` in `settings.toml`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotifyPrefs {
    pub warnings: bool,
    pub profile_switched_by_rule: bool,
    pub external_change: bool,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self {
            warnings: true,
            profile_switched_by_rule: false,
            external_change: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Low,
    Normal,
    Critical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    /// The action identifier on the wire.
    pub id: &'static str,
    pub label: String,
    pub command: Command,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationSpec {
    /// A later notification with the same key replaces this one: an Alarm instance.
    pub key: String,
    pub title: String,
    pub body: String,
    pub level: Level,
    pub buttons: Vec<Button>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Show(NotificationSpec),
    Close(String),
}

fn open_button(locale: Locale) -> Button {
    Button {
        id: "open",
        label: locale.tr("notification.action.open"),
        command: Command::OpenWindow(None),
    }
}

/// The notifications for one event. `state` is the state after the event.
pub fn plan(
    event: &DaemonEvent,
    state: &AppState,
    now_us: u64,
    locale: Locale,
    prefs: NotifyPrefs,
) -> Vec<Plan> {
    match event {
        DaemonEvent::AlarmRaised(alarm) => alarm_notification(alarm, state, now_us, locale, prefs)
            .map(Plan::Show)
            .into_iter()
            .collect(),
        DaemonEvent::AlarmCleared { instance } => vec![Plan::Close(instance.clone())],
        DaemonEvent::PoweroffCancelled { instance } => vec![Plan::Show(NotificationSpec {
            key: instance.clone(),
            title: locale.tr("notification.poweroff_cancelled.title"),
            body: locale.tr("notification.poweroff_cancelled.body"),
            level: Level::Normal,
            buttons: vec![open_button(locale)],
        })],
        DaemonEvent::PoweroffFailed { instance, error } => vec![Plan::Show(NotificationSpec {
            key: instance.clone(),
            title: locale.tr("notification.poweroff_failed.title"),
            body: locale.fmt("notification.poweroff_failed.body", &[("error", error)]),
            level: Level::Critical,
            buttons: vec![open_button(locale)],
        })],
        DaemonEvent::Link(Link::Down) => vec![Plan::Show(NotificationSpec {
            key: DAEMON_DOWN_KEY.into(),
            title: locale.tr("notification.daemon_down.title"),
            body: locale.tr("notification.daemon_down.body"),
            level: Level::Critical,
            buttons: vec![open_button(locale)],
        })],
        DaemonEvent::Link(Link::Online) => vec![Plan::Close(DAEMON_DOWN_KEY.into())],
        DaemonEvent::ExternalChange { leakshield, fields } if prefs.external_change => {
            vec![Plan::Show(NotificationSpec {
                key: format!("external-change-{leakshield}"),
                title: locale.tr("notification.external_change.title"),
                body: locale.fmt(
                    "notification.external_change.body",
                    &[("fields", &fields.join(", "))],
                ),
                level: Level::Normal,
                buttons: vec![Button {
                    id: "open_leakshield",
                    label: locale.tr("notification.action.open_leakshield"),
                    command: Command::OpenWindow(Some(Page::Leakshield(leakshield.clone()))),
                }],
            })]
        }
        DaemonEvent::ProfileActivated {
            profile,
            cause: Cause::Rule,
            ..
        } if prefs.profile_switched_by_rule => {
            let name = state
                .snapshot
                .profiles
                .iter()
                .find(|p| &p.id == profile)
                .map_or(profile.as_str(), |p| p.name.as_str());
            vec![Plan::Show(NotificationSpec {
                key: "profile-switched".into(),
                title: locale.fmt("notification.profile_switched.title", &[("name", name)]),
                body: locale.tr("notification.profile_switched.body"),
                level: Level::Low,
                buttons: Vec::new(),
            })]
        }
        _ => Vec::new(),
    }
}

fn alarm_notification(
    alarm: &AlarmEntry,
    state: &AppState,
    now_us: u64,
    locale: Locale,
    prefs: NotifyPrefs,
) -> Option<NotificationSpec> {
    if alarm.response == Response::None {
        return None;
    }
    if alarm.severity == Severity::Warning && !prefs.warnings {
        return None;
    }
    let leakshield = locale.tr("nav.leakshield");
    let source = state.source_with_loop(&alarm.source, &leakshield);
    let seconds = alarm.poweroff_in_s(now_us);
    let seconds_text = seconds.unwrap_or(0).to_string();
    let title = locale.event_title(&alarm.event);
    let mut buttons = Vec::new();
    if alarm.poweroff_at_us != 0 {
        buttons.push(Button {
            id: "cancel_poweroff",
            label: locale.tr("notification.action.cancel_poweroff"),
            command: Command::CancelPoweroff {
                instance: alarm.instance.clone(),
            },
        });
    }
    let body = match (alarm.event.as_str(), alarm.severity) {
        ("pump-stop", _) => locale.fmt(
            "notification.pump_stop.body",
            &[("source", &source), ("seconds", &seconds_text)],
        ),
        ("leakshield-unreachable", _) => locale.fmt(
            "notification.leakshield_unreachable.body",
            &[("source", &source)],
        ),
        ("lost-input", _) => locale.fmt("notification.lost_input.body", &[("source", &source)]),
        (_, Severity::Alarm) if alarm.poweroff_at_us != 0 => locale.fmt(
            "notification.poweroff.body",
            &[("source", &source), ("seconds", &seconds_text)],
        ),
        _ => locale.fmt("notification.warning.body", &[("source", &source)]),
    };
    if alarm.event == "lost-input" {
        buttons.push(Button {
            id: "open_channels",
            label: locale.tr("notification.action.open_channels"),
            command: Command::OpenWindow(Some(Page::Channels)),
        });
    } else {
        buttons.push(open_button(locale));
    }
    Some(NotificationSpec {
        key: alarm.instance.clone(),
        title,
        body,
        level: match alarm.severity {
            Severity::Alarm => Level::Critical,
            Severity::Warning => Level::Normal,
        },
        buttons,
    })
}

#[derive(Debug)]
pub enum NotifyError {
    Show(notify_rust::error::Error),
    Close(zbus::Error),
}

impl From<notify_rust::error::Error> for NotifyError {
    fn from(error: notify_rust::error::Error) -> Self {
        Self::Show(error)
    }
}

impl From<zbus::Error> for NotifyError {
    fn from(error: zbus::Error) -> Self {
        Self::Close(error)
    }
}

/// Shows notifications and turns a click on a button into a `Command`.
pub struct Notifier {
    commands: UnboundedSender<Command>,
    ids: HashMap<String, u32>,
}

impl Notifier {
    pub fn new(commands: UnboundedSender<Command>) -> Self {
        Self {
            commands,
            ids: HashMap::new(),
        }
    }

    pub fn apply(&mut self, plan: Plan) -> Result<(), NotifyError> {
        match plan {
            Plan::Show(spec) => Ok(self.show(&spec)?),
            Plan::Close(key) => {
                if let Some(id) = self.ids.remove(&key) {
                    close(id)?;
                }
                Ok(())
            }
        }
    }

    /// Show a notification. A server without the `actions` capability gets no buttons.
    pub fn show(&mut self, spec: &NotificationSpec) -> notify_rust::error::Result<()> {
        let actions = notify_rust::get_capabilities()
            .is_ok_and(|caps| caps.iter().any(|cap| cap == "actions"));
        let mut note = Notification::new();
        note.appname(APP_NAME)
            .icon(APP_ICON)
            .summary(&spec.title)
            .body(&spec.body)
            .urgency(match spec.level {
                Level::Low => Urgency::Low,
                Level::Normal => Urgency::Normal,
                Level::Critical => Urgency::Critical,
            });
        // A critical notification stays until its Alarm ends.
        note.timeout(if spec.level == Level::Critical {
            Timeout::Never
        } else {
            Timeout::Default
        });
        if actions {
            for button in &spec.buttons {
                note.action(button.id, &button.label);
            }
        }
        if let Some(id) = self.ids.get(&spec.key) {
            note.id(*id);
        }
        let handle = note.show()?;
        self.ids.insert(spec.key.clone(), handle.id());
        if actions && !spec.buttons.is_empty() {
            let commands = self.commands.clone();
            let buttons = spec.buttons.clone();
            thread::spawn(move || {
                handle.wait_for_action(|action| {
                    if let Some(button) = buttons.iter().find(|b| b.id == action) {
                        let _ = commands.send(button.command.clone());
                    }
                });
            });
        }
        Ok(())
    }
}

/// Close a notification through `CloseNotification`. A missing id is harmless. Call it from a
/// plain thread: it runs its own small runtime.
fn close(id: u32) -> zbus::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| zbus::Error::InputOutput(std::sync::Arc::new(error)))?;
    runtime.block_on(async {
        let conn = zbus::Connection::session().await?;
        zbus::Proxy::new(
            &conn,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await?
        .call_method("CloseNotification", &(id,))
        .await?;
        Ok(())
    })
}
