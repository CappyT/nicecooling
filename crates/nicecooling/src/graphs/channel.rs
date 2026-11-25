//! The command channel of the Graph service. The window, the tray and the notification buttons
//! send `Command::Graph`. The app routes it here, and not to the daemon client.

use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use nicecooling_graph::model::Graph;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::command::Command;

/// How long the app waits for the service at "Quit" and at SIGTERM.
pub const QUIT_WAIT: Duration = Duration::from_secs(2);

/// A "Try" of a draft: test values for sensor source nodes, and the time of the try.
#[derive(Clone, Debug, PartialEq)]
pub struct TryRequest {
    pub graph: Graph,
    /// The test value of a sensor source node: the id of the node and the value.
    pub tests: Vec<(u32, f64)>,
    /// 1 to 600.
    pub seconds: u32,
}

/// What the window, the tray and the notifications ask of the Graph service.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphCommand {
    /// Validate a Graph and store it. The state of its nodes starts again.
    Apply(Box<Graph>),
    Enable {
        graph: String,
        enabled: bool,
    },
    Delete {
        graph: String,
    },
    Try(Box<TryRequest>),
    StopTry,
    /// Run the response of a Graph once, whatever its state and timing.
    RunOnce {
        graph: String,
    },
    /// Cancel the running countdown of a power response.
    CancelCountdown,
    /// The button "Turn the Graph responses on" of the Response pause.
    ResponsesOn,
    /// The MQTT or SMTP settings changed.
    NetworkSettingsEdited,
}

/// One message of the channel.
#[derive(Debug)]
pub enum Input {
    Command(GraphCommand),
    /// The stop of the app. The service answers on the sender when it has written its state.
    Stop(std_mpsc::Sender<()>),
}

/// The sending end. A clone goes to each thread that sends a command.
#[derive(Clone, Debug)]
pub struct GraphChannel {
    tx: UnboundedSender<Input>,
}

impl GraphChannel {
    pub fn new() -> (Self, UnboundedReceiver<Input>) {
        let (tx, rx) = unbounded_channel();
        (Self { tx }, rx)
    }

    /// Returns false when the service is gone.
    pub fn send(&self, command: GraphCommand) -> bool {
        self.tx.send(Input::Command(command)).is_ok()
    }

    /// True once the service has ended.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Send a Graph command to the service. Any other command comes back to the caller.
    pub fn route(&self, command: Command) -> Option<Command> {
        match command {
            Command::Graph(command) => {
                self.send(command);
                None
            }
            other => Some(other),
        }
    }

    /// Tell the service to stop and wait for it, at most `limit`. Returns whether the service
    /// answered. A service that is gone has nothing to stop and counts as stopped.
    pub fn stop(&self, limit: Duration) -> bool {
        let (done, wait) = std_mpsc::channel();
        if self.tx.send(Input::Stop(done)).is_err() {
            return true;
        }
        match wait.recv_timeout(limit) {
            Ok(()) => true,
            // The service ended without an answer: it is gone.
            Err(std_mpsc::RecvTimeoutError::Disconnected) => true,
            Err(std_mpsc::RecvTimeoutError::Timeout) => false,
        }
    }
}
