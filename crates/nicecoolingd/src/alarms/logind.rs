//! The poweroff call to logind. Everything that powers off goes through the trait, so no test
//! reaches a real `org.freedesktop.login1`.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use zbus::Connection;

/// `SD_LOGIND_SKIP_INHIBITORS`: logind ignores the block inhibitors. Since systemd 257 logind
/// obeys them also for root (design D13).
pub const SD_LOGIND_SKIP_INHIBITORS: u64 = 0x10;

/// A logind that does not answer in this time has failed the call: the alarm manager retries.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);

pub type CallFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

pub trait Login1: Send + Sync {
    /// Calls `PowerOffWithFlags`. An error is the text of the failure.
    fn power_off_with_flags(&self, flags: u64) -> CallFuture;
}

/// Calls `PowerOffWithFlags` and counts a call that outlasts `timeout` as failed.
pub async fn call_poweroff(
    login1: &dyn Login1,
    flags: u64,
    timeout: Duration,
) -> Result<(), String> {
    tokio::time::timeout(timeout, login1.power_off_with_flags(flags))
        .await
        .unwrap_or_else(|_| Err(format!("logind did not answer in {} s", timeout.as_secs())))
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Never;

    impl Login1 for Never {
        fn power_off_with_flags(&self, _flags: u64) -> CallFuture {
            Box::pin(std::future::pending())
        }
    }

    struct Accepts;

    impl Login1 for Accepts {
        fn power_off_with_flags(&self, _flags: u64) -> CallFuture {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_call_that_never_answers_fails_after_the_timeout() {
        let error = call_poweroff(&Never, SD_LOGIND_SKIP_INHIBITORS, CALL_TIMEOUT)
            .await
            .unwrap_err();
        assert_eq!(error, "logind did not answer in 10 s");
    }

    #[tokio::test(start_paused = true)]
    async fn an_answer_in_time_passes() {
        assert!(call_poweroff(&Accepts, 0, CALL_TIMEOUT).await.is_ok());
    }
}
