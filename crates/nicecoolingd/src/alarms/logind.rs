//! The poweroff call to logind. Everything that powers off goes through the trait, so no test
//! reaches a real `org.freedesktop.login1`.

use std::future::Future;
use std::pin::Pin;

use zbus::Connection;

/// `SD_LOGIND_SKIP_INHIBITORS`: logind ignores the block inhibitors. Since systemd 257 logind
/// obeys them also for root (design D13).
pub const SD_LOGIND_SKIP_INHIBITORS: u64 = 0x10;

pub type CallFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

pub trait Login1: Send + Sync {
    /// Calls `PowerOffWithFlags`. An error is the text of the failure.
    fn power_off_with_flags(&self, flags: u64) -> CallFuture;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Manager {
    fn power_off_with_flags(&self, flags: u64) -> zbus::Result<()>;
}

/// logind on the bus of `connection`. The daemon serves the API on the system bus, so this is the
/// system logind there. A test serves it on a private bus.
pub struct ZbusLogin1 {
    connection: Connection,
}

impl ZbusLogin1 {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }
}

impl Login1 for ZbusLogin1 {
    fn power_off_with_flags(&self, flags: u64) -> CallFuture {
        let connection = self.connection.clone();
        Box::pin(async move {
            let proxy = Login1ManagerProxy::builder(&connection)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| e.to_string())?;
            proxy
                .power_off_with_flags(flags)
                .await
                .map_err(|e| e.to_string())
        })
    }
}
