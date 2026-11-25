//! One instance of the app for each user session. The first instance takes the name
//! `io.github.cappyt.NiceCooling` on the session bus and serves one method, `ShowWindow`. A later
//! start asks the first instance to show its window and ends, so two instances never write the
//! same files.

use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use zbus::connection::Builder;

use crate::app::APP_ID;
use crate::command::Command;

const OBJECT_PATH: &str = "/io/github/cappyt/NiceCooling";
const INTERFACE: &str = "io.github.cappyt.NiceCooling.Instance";

/// How long a start waits for the first instance to take the call.
const CALL_LIMIT: Duration = Duration::from_secs(2);

/// The object of the first instance on the session bus.
struct Instance {
    commands: UnboundedSender<Command>,
}

// The name of the interface is `INTERFACE`: an attribute takes a literal only.
#[zbus::interface(name = "io.github.cappyt.NiceCooling.Instance")]
impl Instance {
    /// Show the window of this instance.
    fn show_window(&self) {
        let _ = self.commands.send(Command::OpenWindow(None));
    }
}

/// The result of the claim of the name.
#[derive(Debug)]
pub enum Claim {
    /// This process is the first instance. It keeps the connection: the name is free again when
    /// the connection ends.
    First(zbus::Connection),
    /// Another instance holds the name.
    Taken,
    /// There is no session bus, or the bus refused the connection. The app starts as it did
    /// without the guard.
    NoBus(String),
}

/// A builder for the session bus of the user, or for the bus at `address` (a test).
fn builder(address: Option<&str>) -> Builder<'static> {
    match address {
        Some(address) => Builder::address(address),
        None => Builder::session(),
    }
}

/// Take the name of the app on the bus. `commands` receives `Command::OpenWindow` when a later
/// start asks this instance to show its window.
pub async fn claim(address: Option<&str>, commands: UnboundedSender<Command>) -> Claim {
    let built = builder(address)
        .serve_at(OBJECT_PATH, Instance { commands })
        .name(APP_ID)
        // A later start must find the name taken, and must not take it over.
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .build()
        .await;
    match built {
        Ok(connection) => Claim::First(connection),
        Err(zbus::Error::NameTaken) => Claim::Taken,
        Err(error) => Claim::NoBus(error.to_string()),
    }
}

/// Ask the first instance to show its window.
pub async fn show_first(address: Option<&str>) -> Result<(), String> {
    let call = async {
        let connection = builder(address).build().await?;
        zbus::Proxy::new(&connection, APP_ID, OBJECT_PATH, INTERFACE)
            .await?
            .call_method("ShowWindow", &())
            .await?;
        zbus::Result::Ok(())
    };
    match tokio::time::timeout(CALL_LIMIT, call).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("the first instance did not answer".to_owned()),
    }
}
