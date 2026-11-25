//! The response kinds that need no network and no power call: notification, sound and command.
//! The MQTT message and e-mail kinds give `not-configured` until their clients exist. The power
//! kinds run in `power.rs` and never come here.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use nicecooling_graph::model::ResponseKind;
use nicecooling_graph::template::{Context, command_environment, expand};
use tokio::sync::mpsc::UnboundedSender;

use super::responses::{Outcome, Run, Runner, Started};
use crate::notify::{Level, NotificationSpec, Plan};

/// How often the wait for a command looks at the program.
const POLL: Duration = Duration::from_millis(20);

/// The longest time limit of a command, in seconds.
const MAX_LIMIT_S: f64 = 3600.0;

/// Starts the notification, sound and command responses.
pub struct KindRunner {
    /// The channel that the notification thread reads.
    notify: UnboundedSender<Plan>,
    /// The commands that run report their outcome here.
    outcomes: Sender<(String, Outcome)>,
    finished: Receiver<(String, Outcome)>,
}

impl KindRunner {
    pub fn new(notify: UnboundedSender<Plan>) -> Self {
        let (outcomes, finished) = channel();
        Self {
            notify,
            outcomes,
            finished,
        }
    }

    fn show(&self, spec: NotificationSpec) -> Started {
        match self.notify.send(Plan::Show(spec)) {
            Ok(()) => Started::Done(Outcome::Ok),
            Err(_) => Started::Done(Outcome::Failed),
        }
    }

    fn notification(&self, run: &Run) -> Started {
        let context = Context {
            graph_name: &run.graph_name,
            time: &run.time_text,
            nodes: &run.values,
        };
        self.show(NotificationSpec {
            // A response that repeats replaces its last notification.
            key: format!("graph-response-{}", run.graph_id),
            title: run.node.text("title"),
            body: expand(&run.node.text("text"), &context),
            level: match run.node.text("urgency").as_str() {
                "low" => Level::Low,
                "critical" => Level::Critical,
                _ => Level::Normal,
            },
            buttons: Vec::new(),
            sound: None,
        })
    }

    /// A notification with the Graph name as its title and the sound as its hint.
    fn sound(&self, run: &Run) -> Started {
        self.show(NotificationSpec {
            key: format!("graph-sound-{}", run.graph_id),
            title: run.graph_name.clone(),
            body: String::new(),
            level: Level::Normal,
            buttons: Vec::new(),
            sound: Some(run.node.text("sound")),
        })
    }

    /// Start the program of a command response. The program and its arguments are literal: no
    /// placeholder is expanded, and no shell reads them. The values of the Graph go in the
    /// environment.
    fn command(&self, run: &Run) -> Started {
        let program = run.node.text("path");
        // The validator refuses a relative path. A draft that is run once is not validated, and a
        // relative name would search `PATH`.
        if !program.starts_with('/') {
            return Started::Done(Outcome::Failed);
        }
        let args: Vec<String> = run
            .node
            .param("args")
            .and_then(|value| {
                value.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect()
                })
            })
            .unwrap_or_default();
        let environment = command_environment(&run.graph_id, &run.graph_name, &run.id, &run.values);
        let mut command = Command::new(&program);
        command
            .args(&args)
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // The group of the program is its own, so the time limit can kill all of it.
            .process_group(0);
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                eprintln!("nicecooling: command {program}: {error}");
                return Started::Done(Outcome::Failed);
            }
        };
        let limit =
            Duration::from_secs_f64(run.node.number("time_limit_s").clamp(0.0, MAX_LIMIT_S));
        let outcomes = self.outcomes.clone();
        let id = run.id.clone();
        thread::spawn(move || {
            let _ = outcomes.send((id, wait_for(child, limit)));
        });
        Started::Pending
    }
}

impl Runner for KindRunner {
    fn start(&mut self, run: &Run) -> Started {
        match run.kind {
            ResponseKind::Notification => self.notification(run),
            ResponseKind::Sound => self.sound(run),
            ResponseKind::Command => self.command(run),
            // Section 6 gives the MQTT message its client, and task 7.4 gives the e-mail its
            // client. Until then both kinds have no account.
            ResponseKind::Mqtt | ResponseKind::Email => Started::Done(Outcome::NotConfigured),
            ResponseKind::Poweroff | ResponseKind::Reboot | ResponseKind::Suspend => {
                Started::Done(Outcome::NotConfigured)
            }
        }
    }

    fn finished(&mut self) -> Vec<(String, Outcome)> {
        self.finished.try_iter().collect()
    }
}

/// Wait for a program. Exit code 0 is `ok`, another exit is `failed`, and a program that still
/// runs at the limit is killed with its process group: the outcome is `timeout`.
fn wait_for(mut child: Child, limit: Duration) -> Outcome {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Outcome::Ok,
            Ok(Some(_)) | Err(_) => return Outcome::Failed,
            Ok(None) if start.elapsed() >= limit => {
                kill_group(child.id());
                let _ = child.wait();
                return Outcome::Timeout;
            }
            Ok(None) => thread::sleep(POLL),
        }
    }
}

/// Send SIGKILL to the process group that the program leads.
fn kill_group(pid: u32) {
    if let Ok(group) = i32::try_from(pid) {
        // SAFETY: `kill` takes two integers and reads no memory. The program has not been reaped
        // yet, so its process id is still its own, and a negative id names its process group.
        unsafe { libc::kill(-group, libc::SIGKILL) };
    }
}
