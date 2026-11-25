//! The power responses: poweroff, reboot and suspend. A response counts down, and the call of
//! logind comes at the end. The call has `interactive` false, so logind never asks for a password:
//! a refusal is the outcome `failed`.
//!
//! The service starts, cancels and ends the countdown. This file holds the countdown, the call of
//! logind and the text of the notification.

use std::time::Duration;

use nicecooling_graph::pause::PowerKind;
use tokio::runtime::Handle;

use super::responses::Run;
use super::view::Countdown;
use crate::i18n::Locale;
use crate::notify::{Level, NotificationSpec, cancel_countdown_button};

const LOGIND: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";

/// How long the service waits for the answer of logind.
pub const CALL_LIMIT: Duration = Duration::from_secs(15);

/// The notification key of the countdown. Each second replaces the text of the same notification.
pub const NOTIFICATION_KEY: &str = "graph-countdown";

/// The power call at the end of a countdown. The call to logind is one implementation of it. A test
/// records the calls. An error is the reason of a refusal.
pub trait PowerCall: Send {
    fn call(&mut self, kind: PowerKind) -> Result<(), String>;
}

/// A power call for an app that has no system bus: it always fails.
pub struct NoPower;

impl PowerCall for NoPower {
    fn call(&mut self, _kind: PowerKind) -> Result<(), String> {
        Err("no connection to logind".to_owned())
    }
}

/// `PowerOff`, `Reboot` and `Suspend` of `org.freedesktop.login1.Manager` on the system bus. The
/// call blocks the service thread, at most `CALL_LIMIT`.
pub struct LogindPower {
    conn: zbus::Connection,
    runtime: Handle,
}

impl LogindPower {
    pub fn new(conn: zbus::Connection, runtime: Handle) -> Self {
        Self { conn, runtime }
    }
}

/// The method of logind for a power kind.
pub const fn method(kind: PowerKind) -> &'static str {
    match kind {
        PowerKind::Poweroff => "PowerOff",
        PowerKind::Reboot => "Reboot",
        PowerKind::Suspend => "Suspend",
    }
}

impl PowerCall for LogindPower {
    fn call(&mut self, kind: PowerKind) -> Result<(), String> {
        let conn = self.conn.clone();
        let call = async move {
            let manager = zbus::Proxy::new(&conn, LOGIND, MANAGER_PATH, MANAGER_INTERFACE).await?;
            // The argument is `interactive`. It is false: logind asks for no password.
            manager.call_method(method(kind), &(false,)).await?;
            Ok::<(), zbus::Error>(())
        };
        match self
            .runtime
            .block_on(tokio::time::timeout(CALL_LIMIT, call))
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err("logind did not answer".to_owned()),
        }
    }
}

/// The countdown that runs now. One runs at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub run: Run,
    pub kind: PowerKind,
    /// The length of the countdown in seconds.
    pub total_s: u32,
    /// The end of the countdown on the monotonic clock of the service.
    pub ends: Duration,
    /// The seconds that the notification shows, so one second makes one update.
    pub shown_s: Option<u64>,
}

impl Running {
    pub fn new(run: Run, kind: PowerKind, total_s: u32, now: Duration) -> Self {
        Self {
            run,
            kind,
            total_s,
            ends: now + Duration::from_secs(u64::from(total_s)),
            shown_s: None,
        }
    }

    /// The whole seconds left, rounded up.
    pub fn seconds_left(&self, now: Duration) -> u64 {
        self.ends.saturating_sub(now).as_secs_f64().ceil() as u64
    }

    pub fn view(&self, now: Duration) -> Countdown {
        Countdown {
            graph_id: self.run.graph_id.clone(),
            graph_name: self.run.graph_name.clone(),
            kind: self.kind,
            seconds_left: self.seconds_left(now),
            total_s: u64::from(self.total_s),
        }
    }
}

/// The key of the text "Overheat: power off in 30 s".
pub const fn countdown_key(kind: PowerKind) -> &'static str {
    match kind {
        PowerKind::Poweroff => "graph_countdown.poweroff",
        PowerKind::Reboot => "graph_countdown.reboot",
        PowerKind::Suspend => "graph_countdown.suspend",
    }
}

/// "Overheat: power off in 30 s".
pub fn countdown_text(countdown: &Countdown, locale: Locale) -> String {
    locale.fmt(
        countdown_key(countdown.kind),
        &[
            ("name", &countdown.graph_name),
            ("seconds", &countdown.seconds_left.to_string()),
        ],
    )
}

/// The notification of a running countdown: critical, with the button "Cancel".
pub fn countdown_notification(countdown: &Countdown, locale: Locale) -> NotificationSpec {
    NotificationSpec {
        key: NOTIFICATION_KEY.to_owned(),
        title: countdown_text(countdown, locale),
        body: locale.tr("graph_countdown.body"),
        level: Level::Critical,
        buttons: vec![cancel_countdown_button(locale)],
        sound: None,
    }
}

/// The notification of a power call that logind refused.
pub fn failure_notification(graph_name: &str, error: &str, locale: Locale) -> NotificationSpec {
    NotificationSpec {
        key: "graph-power-failed".to_owned(),
        title: locale.fmt(
            "notification.graph_power_failed.title",
            &[("name", graph_name)],
        ),
        body: locale.fmt("notification.graph_power_failed.body", &[("error", error)]),
        level: Level::Critical,
        buttons: Vec::new(),
        sound: None,
    }
}
