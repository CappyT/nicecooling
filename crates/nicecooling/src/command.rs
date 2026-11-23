//! What a tray entry, a notification button or a banner button asks the app to do.

use crate::daemon_client::DaemonClient;
use crate::shell::Page;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    CancelPoweroff {
        instance: String,
    },
    ActivateProfile {
        id: String,
    },
    ReturnToAutomatic,
    /// Show the window, on a page or on the last one.
    OpenWindow(Option<Page>),
    Quit,
}

impl Command {
    /// Run the commands that call the daemon. A window command returns `Ok(false)`: the
    /// window code handles it.
    pub async fn run_on_daemon(&self, client: &DaemonClient) -> zbus::Result<bool> {
        match self {
            Self::CancelPoweroff { instance } => client.cancel_poweroff(instance).await?,
            Self::ActivateProfile { id } => client.activate_profile(id).await?,
            Self::ReturnToAutomatic => client.return_to_automatic().await?,
            Self::OpenWindow(_) | Self::Quit => return Ok(false),
        }
        Ok(true)
    }
}
