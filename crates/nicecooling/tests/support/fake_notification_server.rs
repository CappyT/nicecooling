//! A fake `org.freedesktop.Notifications` server. It records every `Notify` call.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::OwnedVariant as OwnedValue;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

#[derive(Clone, Debug)]
pub struct Received {
    pub id: u32,
    pub replaces_id: u32,
    pub summary: String,
    pub body: String,
    /// Pairs of `[identifier, label]`, flattened as on the wire.
    pub actions: Vec<String>,
    /// The `urgency` hint: 0 low, 1 normal, 2 critical.
    pub urgency: Option<u8>,
    pub expire_timeout: i32,
}

#[derive(Default)]
struct Log {
    received: Vec<Received>,
    closed: Vec<u32>,
    next_id: u32,
}

struct Server {
    capabilities: Vec<String>,
    log: Arc<Mutex<Log>>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        self.capabilities.clone()
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let mut log = self.log.lock().expect("log");
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            log.next_id += 1;
            log.next_id
        };
        let urgency = hints
            .get("urgency")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| u8::try_from(value).ok());
        log.received.push(Received {
            id,
            replaces_id,
            summary: summary.into(),
            body: body.into(),
            actions,
            urgency,
            expire_timeout,
        });
        id
    }

    fn close_notification(&self, id: u32) {
        self.log.lock().expect("log").closed.push(id);
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "fake".into(),
            "nicecooling".into(),
            "0".into(),
            "1.2".into(),
        )
    }

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

pub struct FakeNotificationServer {
    conn: Connection,
    log: Arc<Mutex<Log>>,
}

impl FakeNotificationServer {
    /// `capabilities` is the answer of `GetCapabilities`: with `actions` or without it.
    pub async fn start(conn: Connection, capabilities: &[&str]) -> Self {
        let log = Arc::new(Mutex::new(Log::default()));
        conn.object_server()
            .at(
                "/org/freedesktop/Notifications",
                Server {
                    capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
                    log: log.clone(),
                },
            )
            .await
            .expect("notification server");
        conn.request_name("org.freedesktop.Notifications")
            .await
            .expect("notification name");
        Self { conn, log }
    }

    pub fn received(&self) -> Vec<Received> {
        self.log.lock().expect("log").received.clone()
    }

    pub fn closed(&self) -> Vec<u32> {
        self.log.lock().expect("log").closed.clone()
    }

    /// The user clicks a button.
    pub async fn invoke(&self, id: u32, action: &str) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Server>("/org/freedesktop/Notifications")
            .await
            .expect("interface");
        Server::action_invoked(iface.signal_emitter(), id, action)
            .await
            .expect("signal");
    }
}
