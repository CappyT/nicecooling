//! The session of the app in logind. Only the app of the active session runs Graph responses and
//! sends Graph sensor values.
//!
//! The app finds its session with `XDG_SESSION_ID` and `GetSession` on
//! `org.freedesktop.login1.Manager`, reads the property `Active` of
//! `org.freedesktop.login1.Session`, and follows its `PropertiesChanged` signal.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use futures_util::StreamExt;
use tokio::runtime::Handle;

use super::values::ActiveSession;

const LOGIND: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";
/// The alias of logind for the session of the caller.
const AUTO_PATH: &str = "/org/freedesktop/login1/session/auto";

/// logind has not answered yet. No response runs and no value goes out.
const RESOLVING: u8 = 0;
const ACTIVE: u8 = 1;
const INACTIVE: u8 = 2;

fn code(active: bool) -> u8 {
    if active { ACTIVE } else { INACTIVE }
}

/// The `Active` property of the session of the app. It is a handle on a shared flag: a clone sees
/// the same state.
#[derive(Clone, Debug)]
pub struct LogindSession(Arc<AtomicU8>);

impl LogindSession {
    /// Follow the session on `runtime`. The id is `XDG_SESSION_ID`.
    pub fn spawn(conn: zbus::Connection, runtime: &Handle) -> Self {
        let id = std::env::var("XDG_SESSION_ID")
            .ok()
            .filter(|id| !id.is_empty());
        Self::spawn_for(conn, runtime, id)
    }

    /// As `spawn`, for the session `id`. Without an id, or when logind does not know it, the
    /// session of the caller (`auto`) stands in. When logind gives no `Active` at all (a desktop
    /// without logind has one session), the session counts as active.
    pub fn spawn_for(conn: zbus::Connection, runtime: &Handle, id: Option<String>) -> Self {
        let state = Arc::new(AtomicU8::new(RESOLVING));
        runtime.spawn(follow(conn, id, state.clone()));
        Self(state)
    }

    /// True once logind has answered, or has been found missing.
    pub fn resolved(&self) -> bool {
        self.0.load(Ordering::Relaxed) != RESOLVING
    }
}

impl ActiveSession for LogindSession {
    fn active(&self) -> bool {
        self.0.load(Ordering::Relaxed) == ACTIVE
    }
}

/// A session that is always active: the app has no system bus, or a test needs no logind.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlwaysActive;

impl ActiveSession for AlwaysActive {
    fn active(&self) -> bool {
        true
    }
}

/// The object path of the session `id`, from `GetSession`.
async fn find(conn: &zbus::Connection, id: &str) -> zbus::Result<String> {
    let manager = zbus::Proxy::new(conn, LOGIND, MANAGER_PATH, MANAGER_INTERFACE).await?;
    let path: zbus::OwnedObjectPath = manager.call("GetSession", &(id,)).await?;
    Ok(path.as_str().to_owned())
}

async fn follow(conn: zbus::Connection, id: Option<String>, state: Arc<AtomicU8>) {
    let found = match id {
        Some(id) => find(&conn, &id).await.ok(),
        None => None,
    };
    let path = found.unwrap_or_else(|| AUTO_PATH.to_owned());
    let Ok(proxy) = zbus::Proxy::new(&conn, LOGIND, path, SESSION_INTERFACE).await else {
        state.store(ACTIVE, Ordering::Relaxed);
        return;
    };
    // The stream comes before the first read, so no change is lost between the two.
    let mut changes = proxy.receive_property_changed::<bool>("Active").await;
    match proxy.get_property::<bool>("Active").await {
        Ok(now) => state.store(code(now), Ordering::Relaxed),
        Err(_) => {
            state.store(ACTIVE, Ordering::Relaxed);
            return;
        }
    }
    while let Some(change) = changes.next().await {
        if let Ok(now) = change.get().await {
            state.store(code(now), Ordering::Relaxed);
        }
    }
}
