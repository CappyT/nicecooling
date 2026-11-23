//! A daemon client on a private bus, with the app state that its events build.

use std::time::Duration;

use nicecooling::daemon_client::DaemonClient;
use nicecooling::model::{AppState, DaemonEvent};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::bus::PrivateBus;

/// Upper bound for one wait. A failing test stops here instead of hanging.
const LIMIT: Duration = Duration::from_secs(20);

pub struct Harness {
    pub client: DaemonClient,
    pub state: AppState,
    pub events: broadcast::Receiver<DaemonEvent>,
    pub seen: Vec<DaemonEvent>,
    task: JoinHandle<()>,
}

impl Harness {
    pub async fn start(bus: &PrivateBus) -> Self {
        let client = DaemonClient::new(bus.connect().await);
        let (tx, events) = broadcast::channel(256);
        let task = tokio::spawn(client.clone().run(tx));
        Self {
            client,
            state: AppState::default(),
            events,
            seen: Vec::new(),
            task,
        }
    }

    /// Apply events until `done` holds for the state.
    pub async fn until(&mut self, what: &str, done: impl Fn(&AppState) -> bool) {
        let wait = async {
            while !done(&self.state) {
                match self.events.recv().await {
                    Ok(event) => {
                        self.state.apply(&event);
                        self.seen.push(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
                }
            }
        };
        if tokio::time::timeout(LIMIT, wait).await.is_err() {
            panic!("timed out waiting for: {what}\nevents: {:#?}", self.seen);
        }
    }

    /// Apply events until one matches. Returns it.
    pub async fn next_event(
        &mut self,
        what: &str,
        matches: impl Fn(&DaemonEvent) -> bool,
    ) -> DaemonEvent {
        let wait = async {
            loop {
                match self.events.recv().await {
                    Ok(event) => {
                        self.state.apply(&event);
                        self.seen.push(event.clone());
                        if matches(&event) {
                            return event;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
                }
            }
        };
        tokio::time::timeout(LIMIT, wait)
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for: {what}"))
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.task.abort();
    }
}
