//! A fake `org.freedesktop.login1`. The manager serves `GetSession`, `PowerOff`, `Reboot` and
//! `Suspend`. It records each power call with its `interactive` argument, and it can refuse them.
//! A session object has an `Active` property that the test sets.

use std::sync::{Arc, Mutex};

use zbus::OwnedObjectPath;
use zbus::{Connection, interface};

pub const MANAGER_PATH: &str = "/org/freedesktop/login1";

/// The object path of the session `id`. The id must be letters and digits.
pub fn session_path(id: &str) -> String {
    format!("/org/freedesktop/login1/session/_{id}")
}

/// One power call that the fake received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerCall {
    /// `PowerOff`, `Reboot` or `Suspend`.
    pub method: String,
    pub interactive: bool,
}

#[derive(Default)]
struct Log {
    calls: Vec<PowerCall>,
    /// The ids that `GetSession` received.
    sessions_asked: Vec<String>,
    refuse: bool,
}

struct Manager {
    /// The id of the one session that `GetSession` knows.
    session_id: String,
    log: Arc<Mutex<Log>>,
}

impl Manager {
    fn power(&self, method: &str, interactive: bool) -> zbus::fdo::Result<()> {
        let mut log = self.log.lock().expect("log");
        log.calls.push(PowerCall {
            method: method.to_owned(),
            interactive,
        });
        if log.refuse {
            // logind refuses a call without the interactive flag when polkit asks for a password.
            Err(zbus::fdo::Error::AccessDenied(format!(
                "not allowed to call {method}"
            )))
        } else {
            Ok(())
        }
    }
}

#[interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    fn get_session(&self, id: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        self.log
            .lock()
            .expect("log")
            .sessions_asked
            .push(id.to_owned());
        if id == self.session_id {
            Ok(OwnedObjectPath::try_from(session_path(id)).expect("path"))
        } else {
            Err(zbus::fdo::Error::Failed(format!("no session {id}")))
        }
    }

    fn power_off(&self, interactive: bool) -> zbus::fdo::Result<()> {
        self.power("PowerOff", interactive)
    }

    fn reboot(&self, interactive: bool) -> zbus::fdo::Result<()> {
        self.power("Reboot", interactive)
    }

    fn suspend(&self, interactive: bool) -> zbus::fdo::Result<()> {
        self.power("Suspend", interactive)
    }
}

struct Session {
    active: bool,
}

#[interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    fn active(&self) -> bool {
        self.active
    }
}

pub struct FakeLogind {
    conn: Connection,
    session_id: String,
    log: Arc<Mutex<Log>>,
}

impl FakeLogind {
    /// A logind with one session `session_id` (letters and digits) whose `Active` is `active`.
    pub async fn start(conn: Connection, session_id: &str, active: bool) -> Self {
        let log = Arc::new(Mutex::new(Log::default()));
        let server = conn.object_server();
        server
            .at(
                MANAGER_PATH,
                Manager {
                    session_id: session_id.to_owned(),
                    log: log.clone(),
                },
            )
            .await
            .expect("manager");
        server
            .at(session_path(session_id), Session { active })
            .await
            .expect("session");
        conn.request_name("org.freedesktop.login1")
            .await
            .expect("logind name");
        Self {
            conn,
            session_id: session_id.to_owned(),
            log,
        }
    }

    /// The power calls so far, in order.
    pub fn calls(&self) -> Vec<PowerCall> {
        self.log.lock().expect("log").calls.clone()
    }

    /// The ids that `GetSession` received.
    pub fn sessions_asked(&self) -> Vec<String> {
        self.log.lock().expect("log").sessions_asked.clone()
    }

    /// From now on the three power methods answer with an error.
    pub fn refuse_power(&self, refuse: bool) {
        self.log.lock().expect("log").refuse = refuse;
    }

    /// Set `Active` and send `PropertiesChanged`.
    pub async fn set_active(&self, active: bool) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Session>(session_path(&self.session_id))
            .await
            .expect("session interface");
        iface.get_mut().await.active = active;
        iface
            .get()
            .await
            .active_changed(iface.signal_emitter())
            .await
            .expect("PropertiesChanged");
    }
}
