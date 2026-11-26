//! The command channel of the Graph service. The window, the tray and the notification buttons
//! send `Command::Graph`. The app routes it here, and not to the daemon client.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use nicecooling_graph::model::Graph;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
use tokio::sync::mpsc::{channel, unbounded_channel};

use super::values::SensorError;
use super::view::{Applied, ApplyError, SharedView, lock_view};
use crate::command::Command;

/// How long the app waits for the service at "Quit" and at SIGTERM.
pub const QUIT_WAIT: Duration = Duration::from_secs(2);

/// The commands that the channel holds. A command that comes while the channel is full is refused.
/// The stop message and the cancel of a countdown have a path of their own and are never refused.
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
    /// Store a Graph that `transfer::import_file` made. It is disabled and has no Graph sensor:
    /// no call goes to the daemon before the user enables it.
    Import(Box<Graph>),
    Enable {
        graph: String,
        enabled: bool,
    },
    Delete {
        graph: String,
    },
    Try(Box<TryRequest>),
    StopTry,
    /// Run the response of a Graph once, whatever its state, its timing and the Response pause. The
    /// Graph is the draft of the editor: the response runs with its parameters.
    RunOnce {
        graph: Box<Graph>,
    },
    /// Cancel the running countdown of a power response.
    CancelCountdown,
    /// The button "Turn the Graph responses on" of the Response pause.
    ResponsesOn,
    /// The MQTT or SMTP settings changed.
    NetworkSettingsEdited,
    /// "Retry" after a read error of `graphs.json`: read the file again.
    Reload,
}

/// The answer of the daemon to a Graph sensor call that the service started on a task.
#[derive(Debug)]
pub struct SensorAnswer {
    /// The number that the service gave to the call.
    pub ticket: u64,
    /// The id of the new sensor for a create, and nothing for the two other calls.
    pub result: Result<Option<String>, SensorError>,
}

/// One message of the channel.
#[derive(Debug)]
pub enum Input {
    Command(GraphCommand),
    /// The stop of the app. The service answers on the sender when it has written its state.
    Stop(std_mpsc::Sender<()>),
    /// The daemon answered a Graph sensor call.
    Answer(SensorAnswer),
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

/// The stop of the app. `GraphChannel::stop` sets it before it sends the stop message. The service
/// reads it before each poll and before each power call: a stop message that waits behind other
/// work still keeps the power call back.
#[derive(Clone, Debug, Default)]
pub struct StopSignal(Arc<AtomicBool>);

impl StopSignal {
    pub fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Where the task of a Graph sensor call gives the answer of the daemon: the channel of the
/// service. An answer has the path of the stop message, so it is never refused.
#[derive(Clone, Debug)]
pub struct AnswerSender(UnboundedSender<Input>);

impl AnswerSender {
    pub fn send(&self, answer: SensorAnswer) {
        let _ = self.0.send(Input::Answer(answer));
    }
}

/// The receiving end, for the service. The stop message, the cancel of a countdown and the
/// answers of the daemon come before every other command.
#[derive(Debug)]
pub struct Inputs {
    stop: UnboundedReceiver<Input>,
    commands: Receiver<Input>,
    signal: StopSignal,
}

impl Inputs {
    /// The stop of the app, for the service.
    pub fn stop_signal(&self) -> StopSignal {
        self.signal.clone()
    }

    /// Wait for the stop message and read nothing else. An answer of the daemon that came before
    /// it is dropped. `None` when every sender is gone.
    pub async fn stop_message(&mut self) -> Option<std_mpsc::Sender<()>> {
        loop {
            match self.stop.recv().await? {
                Input::Stop(done) => return Some(done),
                Input::Command(_) | Input::Answer(_) => {}
            }
        }
    }

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
    signal: StopSignal,
    /// Where a refused command is shown.
    view: Option<SharedView>,
}

impl GraphChannel {
    pub fn new() -> (Self, Inputs) {
        let (commands, command_inputs) = channel(COMMAND_BOUND);
        let (stop, stop_inputs) = unbounded_channel();
        let signal = StopSignal::default();
        (
            Self {
                commands,
                stop,
                signal: signal.clone(),
                view: None,
            },
            Inputs {
                stop: stop_inputs,
                commands: command_inputs,
                signal,
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

    /// The sender for the answers of the Graph sensor calls.
    pub fn answers(&self) -> AnswerSender {
        AnswerSender(self.stop.clone())
    }

    pub fn try_send(&self, command: GraphCommand) -> Result<(), SendError> {
        // A full channel must not let a power call go ahead: the cancel takes the path of the
        // stop message.
        if command == GraphCommand::CancelCountdown {
            return self
                .stop
                .send(Input::Command(command))
                .map_err(|_| SendError::Closed);
        }
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
                let applied = match &command {
                    GraphCommand::Apply(graph) => Some(graph.id.clone()),
                    _ => None,
                };
                if let Err(error) = self.try_send(command) {
                    eprintln!("nicecooling: Graph command: {error}");
                    if let Some(view) = &self.view {
                        let mut view = lock_view(view);
                        view.refusal = Some(error.to_string());
                        // The editor waits for the answer to its Apply.
                        if let Some(graph) = applied {
                            view.applied = Some(Applied {
                                graph,
                                result: Err(ApplyError::Other(error.to_string())),
                            });
                        }
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
        self.signal.set();
        if self.stop.send(Input::Stop(done)).is_err() {
            return false;
        }
        wait.recv_timeout(limit).is_ok()
    }
}
