//! The response kinds that the app runs itself: notification, sound, command, e-mail and MQTT
//! message. The power kinds run in `power.rs` and never come here.
//!
//! The e-mail and the MQTT message have no account until `with_email` and `with_mqtt` give the
//! runner one. A runner without it gives them the outcome `not-configured`.

use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use nicecooling_graph::model::{ParamType, ResponseKind};
use nicecooling_graph::template::{Context, command_environment, expand};
use serde_json::Value;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::responses::{Outcome, Run, Runner, Started};
use crate::mqtt::publish::{
    ResponseDone, ResponseOutcome, ResponseRequest, ResponseSender, default_response_payload,
};
use crate::notify::{Level, NotificationSpec, Plan};
use crate::smtp::send::{Mail, SendError, SmtpAccount, send};

/// How often the wait for a command looks at the program.
const POLL: Duration = Duration::from_millis(20);

/// The longest time limit of a command, in seconds.
const MAX_LIMIT_S: f64 = 3600.0;

/// Where an e-mail response gets its account. Each send calls it, so a change of the SMTP settings
/// holds at the next run. The call can read files: it runs off the thread of the Graph service.
pub type SmtpSource = Arc<dyn Fn() -> SmtpAccount + Send + Sync>;

/// What the e-mail response needs: a runtime for its send tasks and the account.
struct Email {
    runtime: tokio::runtime::Handle,
    source: SmtpSource,
}

/// The link to the MQTT publisher.
struct Mqtt {
    sender: ResponseSender,
    done: UnboundedReceiver<ResponseDone>,
    /// The run id of each request that has no outcome yet, by the id of the request.
    open: BTreeMap<u64, String>,
    /// The id of the last request.
    serial: u64,
}

impl Mqtt {
    /// Add the outcomes that have come in to `ended`.
    fn collect(&mut self, ended: &mut Vec<(String, Outcome)>) {
        loop {
            match self.done.try_recv() {
                Ok(done) => {
                    if let Some(run) = self.open.remove(&done.id) {
                        ended.push((run, outcome_of(done.outcome)));
                    }
                }
                Err(TryRecvError::Empty) => return,
                // The publisher task has ended: no outcome comes for a request that is open.
                Err(TryRecvError::Disconnected) => {
                    let lost = std::mem::take(&mut self.open);
                    ended.extend(lost.into_values().map(|run| (run, Outcome::Failed)));
                    return;
                }
            }
        }
    }
}

fn outcome_of(outcome: ResponseOutcome) -> Outcome {
    match outcome {
        ResponseOutcome::Ok => Outcome::Ok,
        ResponseOutcome::Timeout => Outcome::Timeout,
        ResponseOutcome::Failed => Outcome::Failed,
        ResponseOutcome::NotConfigured => Outcome::NotConfigured,
    }
}

/// What a message of a run can name.
fn context_of(run: &Run) -> Context<'_> {
    Context {
        graph_name: &run.graph_name,
        time: &run.time_text,
        nodes: &run.values,
    }
}

/// Starts the notification, sound, command, e-mail and MQTT message responses.
pub struct KindRunner {
    /// The channel that the notification thread reads.
    notify: UnboundedSender<Plan>,
    /// The commands and the e-mails that run report their outcome here.
    outcomes: Sender<(String, Outcome)>,
    finished: Receiver<(String, Outcome)>,
    email: Option<Email>,
    mqtt: Option<Mqtt>,
}

impl KindRunner {
    pub fn new(notify: UnboundedSender<Plan>) -> Self {
        let (outcomes, finished) = channel();
        Self {
            notify,
            outcomes,
            finished,
            email: None,
            mqtt: None,
        }
    }

    /// Send the e-mail responses on `runtime`, with the account that `source` gives.
    pub fn with_email(mut self, runtime: tokio::runtime::Handle, source: SmtpSource) -> Self {
        self.email = Some(Email { runtime, source });
        self
    }

    /// Send the MQTT message responses to the publisher, and read their outcomes from `done`.
    pub fn with_mqtt(
        mut self,
        sender: ResponseSender,
        done: UnboundedReceiver<ResponseDone>,
    ) -> Self {
        self.mqtt = Some(Mqtt {
            sender,
            done,
            open: BTreeMap::new(),
            serial: 0,
        });
        self
    }

    fn show(&self, spec: NotificationSpec) -> Started {
        match self.notify.send(Plan::Show(spec)) {
            Ok(()) => Started::Done(Outcome::Ok),
            Err(_) => Started::Done(Outcome::Failed),
        }
    }

    fn notification(&self, run: &Run) -> Started {
        self.show(NotificationSpec {
            // A response that repeats replaces its last notification.
            key: format!("graph-response-{}", run.graph_id),
            title: run.node.text("title"),
            body: expand(&run.node.text("text"), &context_of(run)),
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

    /// Send the e-mail in a task of its own. The Graph service never waits for the server: the
    /// outcome comes through `finished`, whatever it is.
    fn mail(&self, run: &Run) -> Started {
        let Some(email) = &self.email else {
            return Started::Done(Outcome::NotConfigured);
        };
        let context = context_of(run);
        let mail = Mail {
            to: run.node.text("to"),
            subject: expand(&run.node.text("subject"), &context),
            text: expand(&run.node.text("text"), &context),
        };
        let source = Arc::clone(&email.source);
        let outcomes = self.outcomes.clone();
        let id = run.id.clone();
        email.runtime.spawn(async move {
            let _ = outcomes.send((id, send_mail(source, mail).await));
        });
        Started::Pending
    }

    /// Give the message to the publisher. Only the payload has placeholders: the topic is as the
    /// node holds it.
    fn publish(&mut self, run: &Run) -> Started {
        let Some(mqtt) = &mut self.mqtt else {
            return Started::Done(Outcome::NotConfigured);
        };
        let topic = run.node.text("topic");
        // The validator refuses a topic with a wildcard, but a draft that is run once is not
        // validated, and a broker closes the connection at a PUBLISH to a wildcard.
        if !ParamType::Topic.accepts(&Value::from(topic.as_str())) {
            return Started::Done(Outcome::Failed);
        }
        let template = run.node.text("payload");
        let payload = if template.is_empty() {
            default_response_payload(&run.graph_id, &run.graph_name, run.time_us)
        } else {
            expand(&template, &context_of(run)).into_bytes()
        };
        mqtt.serial += 1;
        let id = mqtt.serial;
        if mqtt.sender.send(ResponseRequest { id, topic, payload }) {
            mqtt.open.insert(id, run.id.clone());
            Started::Pending
        } else {
            Started::Done(Outcome::Failed)
        }
    }
}

impl Runner for KindRunner {
    fn start(&mut self, run: &Run) -> Started {
        match run.kind {
            ResponseKind::Notification => self.notification(run),
            ResponseKind::Sound => self.sound(run),
            ResponseKind::Command => self.command(run),
            ResponseKind::Email => self.mail(run),
            ResponseKind::Mqtt => self.publish(run),
            ResponseKind::Poweroff | ResponseKind::Reboot | ResponseKind::Suspend => {
                Started::Done(Outcome::NotConfigured)
            }
        }
    }

    fn finished(&mut self) -> Vec<(String, Outcome)> {
        let mut ended: Vec<(String, Outcome)> = self.finished.try_iter().collect();
        if let Some(mqtt) = &mut self.mqtt {
            mqtt.collect(&mut ended);
        }
        ended
    }
}

/// Get the account and send the message. The account comes from files, so it is read off the
/// threads of the runtime. The send ends in the time limit of the account at the latest.
async fn send_mail(source: SmtpSource, mail: Mail) -> Outcome {
    let Ok(account) = tokio::task::spawn_blocking(move || source()).await else {
        return Outcome::Failed;
    };
    match send(&account, &mail).await {
        Ok(()) => Outcome::Ok,
        Err(SendError::NotConfigured) => Outcome::NotConfigured,
        Err(SendError::Timeout) => Outcome::Timeout,
        Err(SendError::Failed(reason)) => {
            eprintln!("nicecooling: e-mail response: {reason}");
            Outcome::Failed
        }
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
