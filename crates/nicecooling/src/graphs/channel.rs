//! The command channel of the Graph service. The window, the tray and the notification buttons
//! send `Command::Graph`. The app routes it here, and not to the daemon client.

use std::fmt;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use nicecooling_graph::model::Graph;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
use tokio::sync::mpsc::{channel, unbounded_channel};

use super::view::SharedView;
use crate::command::Command;

/// How long the app waits for the service at "Quit" and at SIGTERM.
pub const QUIT_WAIT: Duration = Duration::from_secs(2);

/// The commands that the channel holds. A command that comes while the channel is full is refused.
/// The stop message has a path of its own and is never refused.
pub const COMMAND_BOUND: usize = 256;

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

/// Why the channel does not take a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError {
    /// The service has `COMMAND_BOUND` commands to do.
    Full,
    /// The service is gone.
    Closed,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => write!(
                f,
                "the Graph service has {COMMAND_BOUND} commands to do, and the command is refused"
            ),
            Self::Closed => f.write_str("the Graph service is not running"),
        }
    }
}

impl std::error::Error for SendError {}

/// The receiving end, for the service. The stop message comes before every command.
#[derive(Debug)]
pub struct Inputs {
    stop: UnboundedReceiver<Input>,
    commands: Receiver<Input>,
}

impl Inputs {
    /// The next message. `None` when every sender is gone and the commands are done.
    pub async fn recv(&mut self) -> Option<Input> {
        tokio::select! {
            biased;
            Some(stop) = self.stop.recv() => Some(stop),
            input = self.commands.recv() => input,
        }
    }

    pub fn try_recv(&mut self) -> Result<Input, TryRecvError> {
        match self.stop.try_recv() {
            Ok(stop) => Ok(stop),
            Err(_) => self.commands.try_recv(),
        }
    }
}

/// The sending end. A clone goes to each thread that sends a command.
#[derive(Clone, Debug)]
pub struct GraphChannel {
    commands: Sender<Input>,
    stop: UnboundedSender<Input>,
    /// Where a refused command is shown.
    view: Option<SharedView>,
}

impl GraphChannel {
    pub fn new() -> (Self, Inputs) {
        let (commands, command_inputs) = channel(COMMAND_BOUND);
        let (stop, stop_inputs) = unbounded_channel();
        (
            Self {
                commands,
                stop,
                view: None,
            },
            Inputs {
                stop: stop_inputs,
                commands: command_inputs,
            },
        )
    }

    /// Show a command that the channel refuses as the refusal of the Graph view.
    pub fn with_view(mut self, view: SharedView) -> Self {
        self.view = Some(view);
        self
    }

    /// The view that the service writes, for the window.
    pub fn view(&self) -> Option<SharedView> {
        self.view.clone()
    }

    pub fn try_send(&self, command: GraphCommand) -> Result<(), SendError> {
        self.commands
            .try_send(Input::Command(command))
            .map_err(|error| match error {
                TrySendError::Full(_) => SendError::Full,
                TrySendError::Closed(_) => SendError::Closed,
            })
    }

    /// Returns false when the channel is full or the service is gone.
    pub fn send(&self, command: GraphCommand) -> bool {
        self.try_send(command).is_ok()
    }

    /// True once the service has ended.
    pub fn is_closed(&self) -> bool {
        self.commands.is_closed()
    }

    /// Send a Graph command to the service. Any other command comes back to the caller. A Graph
    /// command that the channel refuses goes to the log and to the refusal of the Graph view.
    pub fn route(&self, command: Command) -> Option<Command> {
        match command {
            Command::Graph(command) => {
                if let Err(error) = self.try_send(command) {
                    eprintln!("nicecooling: Graph command: {error}");
                    if let Some(view) = &self.view
                        && let Ok(mut view) = view.lock()
                    {
                        view.refusal = Some(error.to_string());
                    }
                }
                None
            }
            other => Some(other),
        }
    }

    /// Tell the service to stop and wait for it, at most `limit`. Returns whether the service
    /// answered, so whether it wrote its state. A service that is gone gives false.
    pub fn stop(&self, limit: Duration) -> bool {
        let (done, wait) = std_mpsc::channel();
        if self.stop.send(Input::Stop(done)).is_err() {
            return false;
        }
        wait.recv_timeout(limit).is_ok()
    }
}
