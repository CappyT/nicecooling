//! The link of the Graph service to the daemon: the Graph sensor values once per second, and the
//! calls that create, rename and delete a Graph sensor.
//!
//! The values call has no interactive flag, so a user whom polkit does not allow gets
//! `AccessDenied` and no password dialog. After a denial the call stops for 60 s. The calls block
//! the service thread: a test replaces the call, the session and the clock.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_core::clock::Clock;
use tokio::runtime::Handle;

use super::service::SensorLink;
use super::view::{SharedView, ValuesState};
use crate::daemon_client::DaemonClient;

/// How long the values call stays off after a denial.
pub const DENIAL_PAUSE: Duration = Duration::from_secs(60);

/// How long the service waits for the answer of one call.
pub const CALL_LIMIT: Duration = Duration::from_secs(2);

/// Why a call about a Graph sensor failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensorError {
    /// The link has no daemon. No call was made.
    Unavailable,
    /// polkit refused the call.
    Denied,
    /// Something uses the sensor. The text names the user.
    InUse(String),
    /// The daemon refused the arguments, for example at the limit of 64 sensors.
    Refused(String),
    /// The sensor does not exist on the daemon.
    Missing,
    /// Any other error: the daemon is away, or the call timed out.
    Failed(String),
}

impl SensorError {
    pub fn from_zbus(error: &zbus::Error) -> Self {
        let (name, detail) = match error {
            zbus::Error::MethodError(name, detail, _) => (
                name.to_string(),
                detail.clone().unwrap_or_else(|| error.to_string()),
            ),
            zbus::Error::FDO(inner) => match &**inner {
                zbus::fdo::Error::AccessDenied(text) => (
                    "org.freedesktop.DBus.Error.AccessDenied".to_owned(),
                    text.clone(),
                ),
                zbus::fdo::Error::UnknownObject(text) => (
                    "org.freedesktop.DBus.Error.UnknownObject".to_owned(),
                    text.clone(),
                ),
                other => (String::new(), other.to_string()),
            },
            other => (String::new(), other.to_string()),
        };
        match name.as_str() {
            "org.freedesktop.DBus.Error.AccessDenied" => Self::Denied,
            "org.freedesktop.DBus.Error.UnknownObject" => Self::Missing,
            "io.github.cappyt.NiceCooling1.Error.InUse" => Self::InUse(detail),
            "io.github.cappyt.NiceCooling1.Error.InvalidArgument"
            | "org.freedesktop.DBus.Error.InvalidArgs" => Self::Refused(detail),
            _ => Self::Failed(detail),
        }
    }
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str("no connection to the daemon"),
            Self::Denied => f.write_str("the system does not allow this user to change sensors"),
            Self::InUse(text) | Self::Refused(text) | Self::Failed(text) => f.write_str(text),
            Self::Missing => f.write_str("the sensor does not exist"),
        }
    }
}

/// The values call. The call to the daemon is one implementation of it.
pub trait ValuesCall: Send {
    /// One entry per Graph sensor: the id, whether it has a value, and the value.
    fn send(&mut self, entries: &[(String, bool, f64)]) -> Result<(), SensorError>;
}

/// Whether the session of the app is the active one of its seat.
pub trait ActiveSession: Send {
    fn active(&self) -> bool;
}

/// The values call, with the rules of the app around it: no call from an inactive session, none
/// while the pause after a denial runs, and a no-value entry for a value that is not finite.
pub struct GraphValues {
    call: Box<dyn ValuesCall>,
    session: Box<dyn ActiveSession>,
    clock: Box<dyn Clock>,
    view: SharedView,
    paused_until: Option<Duration>,
    /// The last error that was logged, so one failure logs one line.
    logged: Option<String>,
}

impl GraphValues {
    pub fn new(
        call: Box<dyn ValuesCall>,
        session: Box<dyn ActiveSession>,
        clock: Box<dyn Clock>,
        view: SharedView,
    ) -> Self {
        Self {
            call,
            session,
            clock,
            view,
            paused_until: None,
            logged: None,
        }
    }

    /// Send the values of one pass. `None` is the no-value entry.
    pub fn send(&mut self, values: &[(String, Option<f64>)]) {
        if !self.session.active() {
            self.show(ValuesState::InactiveSession);
            return;
        }
        let now = self.clock.now();
        if let Some(until) = self.paused_until {
            if now < until {
                return;
            }
            self.paused_until = None;
        }
        if values.is_empty() {
            return;
        }
        let entries: Vec<(String, bool, f64)> = values
            .iter()
            .map(|(id, value)| match value {
                Some(value) if value.is_finite() => (id.clone(), true, *value),
                _ => (id.clone(), false, 0.0),
            })
            .collect();
        match self.call.send(&entries) {
            Ok(()) => {
                self.logged = None;
                self.show(ValuesState::Ok);
            }
            Err(SensorError::Denied) => {
                self.paused_until = Some(now + DENIAL_PAUSE);
                self.log("denied");
                self.show(ValuesState::NoAccess);
            }
            Err(error) => {
                self.log(&error.to_string());
                self.show(ValuesState::Failed(error.to_string()));
            }
        }
    }

    fn log(&mut self, text: &str) {
        if self.logged.as_deref() != Some(text) {
            eprintln!("nicecooling: graph sensor values: {text}");
            self.logged = Some(text.to_owned());
        }
    }

    fn show(&self, state: ValuesState) {
        if let Ok(mut view) = self.view.lock() {
            view.values = state;
        }
    }
}

/// The calls to the daemon. They block the thread that calls them, at most `CALL_LIMIT` for the
/// values call. A call that needs polkit waits for the user, as long as the dialog stays open.
#[derive(Clone)]
pub struct DaemonCalls {
    client: DaemonClient,
    runtime: Handle,
}

impl DaemonCalls {
    pub fn new(client: DaemonClient, runtime: Handle) -> Self {
        Self { client, runtime }
    }

    pub fn create(&self, name: &str, unit: &str) -> Result<String, SensorError> {
        self.runtime
            .block_on(self.client.create_graph_sensor(name, unit))
            .map_err(|error| SensorError::from_zbus(&error))
    }

    pub fn rename(&self, id: &str, name: &str, unit: &str) -> Result<(), SensorError> {
        self.runtime
            .block_on(self.client.set_graph_sensor_definition(id, name, unit))
            .map_err(|error| SensorError::from_zbus(&error))
    }

    pub fn delete(&self, id: &str) -> Result<(), SensorError> {
        self.runtime
            .block_on(self.client.delete_graph_sensor(id))
            .map_err(|error| SensorError::from_zbus(&error))
    }
}

impl ValuesCall for DaemonCalls {
    fn send(&mut self, entries: &[(String, bool, f64)]) -> Result<(), SensorError> {
        let call = tokio::time::timeout(CALL_LIMIT, self.client.set_graph_sensor_values(entries));
        match self.runtime.block_on(call) {
            Ok(result) => result.map_err(|error| SensorError::from_zbus(&error)),
            Err(_) => Err(SensorError::Failed("the call timed out".to_owned())),
        }
    }
}

/// The link of the Graph service to the daemon.
pub struct DaemonLink {
    values: GraphValues,
    calls: DaemonCalls,
}

impl DaemonLink {
    pub fn new(
        calls: DaemonCalls,
        session: Box<dyn ActiveSession>,
        clock: Box<dyn Clock>,
        view: SharedView,
    ) -> Self {
        Self {
            values: GraphValues::new(Box::new(calls.clone()), session, clock, view),
            calls,
        }
    }
}

impl SensorLink for DaemonLink {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.values.send(values);
    }

    fn create_sensor(&mut self, name: &str, unit: &str) -> Result<String, SensorError> {
        self.calls.create(name, unit)
    }

    fn rename_sensor(&mut self, id: &str, name: &str, unit: &str) -> Result<(), SensorError> {
        self.calls.rename(id, name, unit)
    }

    fn delete_sensor(&mut self, id: &str) -> Result<(), SensorError> {
        self.calls.delete(id)
    }
}

const LOGIND: &str = "org.freedesktop.login1";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

/// The `Active` property of the session of the app in logind. A state that logind does not give
/// counts as active: a desktop without logind has one session and polkit decides the rest.
pub struct LogindSession(Arc<AtomicBool>);

impl LogindSession {
    /// Follow the property on the runtime. The state is the last value that logind gave.
    pub fn spawn(conn: zbus::Connection, runtime: &Handle) -> Self {
        let active = Arc::new(AtomicBool::new(true));
        let flag = active.clone();
        runtime.spawn(async move {
            let Ok(proxy) = zbus::Proxy::new(&conn, LOGIND, SESSION_PATH, SESSION_INTERFACE).await
            else {
                return;
            };
            // The stream comes before the first read, so no change is lost between the two.
            let mut changes = proxy.receive_property_changed::<bool>("Active").await;
            if let Ok(now) = proxy.get_property::<bool>("Active").await {
                flag.store(now, Ordering::Relaxed);
            }
            while let Some(change) = changes.next().await {
                if let Ok(now) = change.get().await {
                    flag.store(now, Ordering::Relaxed);
                }
            }
        });
        Self(active)
    }
}

impl ActiveSession for LogindSession {
    fn active(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
