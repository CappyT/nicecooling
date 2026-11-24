//! The sleep of the PC. logind sends `PrepareForSleep(true)` before the PC sleeps. The daemon
//! holds a delay inhibitor, so that the sleep waits while the daemon tells each Leakshield to
//! keep its alarms quiet (`suppress_alarm`). aquasuite does the same and waits 100 ms after the
//! command. The Leakshield alarm outputs stay quiet for a time while the PC sleeps. What the
//! device does with the command is open (`docs/protocol/leakshield.md`).

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_core::Stream;
use tokio::task::JoinHandle;
use zbus::Connection;
use zbus::OwnedFd;

use crate::dbus::Hub;

/// The wait after the commands, so that the device has them before the PC sleeps.
pub const SETTLE: Duration = Duration::from_millis(100);

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Sleep {
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd>;

    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

/// Sends `suppress_alarm` to each Leakshield that answers and takes it.
async fn silence(hub: &Hub) {
    for device in hub.leakshields() {
        if let Err(e) = device.suppress_alarm() {
            eprintln!(
                "nicecoolingd: the alarms of {} are not suppressed for the sleep: {e}",
                device.id()
            );
        }
    }
    tokio::time::sleep(SETTLE).await;
}

async fn lock(proxy: &Login1SleepProxy<'_>) -> Option<OwnedFd> {
    match proxy
        .inhibit(
            "sleep",
            "nicecoolingd",
            "Keep the Leakshield alarms quiet while the PC sleeps",
            "delay",
        )
        .await
    {
        Ok(fd) => Some(fd),
        Err(e) => {
            eprintln!("nicecoolingd: no sleep inhibitor: {e}");
            None
        }
    }
}

async fn run(connection: Connection, hub: Arc<Hub>) -> zbus::Result<()> {
    let proxy = Login1SleepProxy::builder(&connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    // The signal stream first, so that no signal falls between the lock and the stream.
    let mut signals = proxy.receive_prepare_for_sleep().await?;
    let mut held = lock(&proxy).await;
    while let Some(signal) = std::future::poll_fn(|cx| Pin::new(&mut signals).poll_next(cx)).await {
        let Ok(args) = signal.args() else { continue };
        if args.start {
            silence(&hub).await;
            // The sleep goes on when the lock closes.
            held = None;
        } else if held.is_none() {
            held = lock(&proxy).await;
        }
    }
    drop(held);
    Ok(())
}

/// Watches the sleep of the PC on the bus of `connection`. A bus without logind ends the task
/// after a message.
pub fn spawn(connection: Connection, hub: Arc<Hub>) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run(connection, hub).await {
            eprintln!("nicecoolingd: the sleep of the PC is not watched: {e}");
        }
    })
}
