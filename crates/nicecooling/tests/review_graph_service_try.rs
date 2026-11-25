//! Review of "Try": odd test values, and a stop of the app during a try.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::channel::{GraphChannel, GraphCommand, QUIT_WAIT, TryRequest};
use nicecooling::graphs::service::{Env, GraphService, NoPower, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::shared_view;
use nicecooling::i18n::EN;
use nicecooling::services::spawn_service;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::graphs::{Dir, constant_graph};
use tokio::sync::{broadcast, mpsc};

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

type Calls = Vec<Vec<(String, Option<f64>)>>;

#[derive(Clone, Default)]
struct Sent(Arc<Mutex<Calls>>);

impl SensorLink for Sent {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.0.lock().unwrap().push(values.to_vec());
    }
}

fn draft() -> Graph {
    let mut graph = Graph::new("graph_9", "Draft");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "nct6798_temp1"),
        Node::new(2, Kind::SensorEnd),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// "Quit" during a try: the service stops within the limit, the applied Graph gets its state
/// written, and the try sent no value in any pass.
#[test]
fn quit_during_a_try_sends_no_try_value() {
    let dir = Dir::new("rv-try-quit");
    let mut store = GraphStore::open(dir.graph_paths());
    let mut applied = constant_graph("graph_9", 5.0);
    applied.sensor_id = Some("graph_9".into());
    store.put(applied).unwrap();
    let clock = SimClock::default();
    let sent = Sent::default();
    let (notify, _notifications) = mpsc::unbounded_channel();
    let mut service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(sent.clone()),
            scan: Box::new(NoScan),
            power: Box::new(NoPower),
            notify,
            locale: EN,
            view: shared_view(),
        },
    );
    let mut try_draft = draft();
    try_draft.sensor_id = Some("graph_9".into());
    service.handle(GraphCommand::Try(Box::new(TryRequest {
        graph: try_draft,
        tests: vec![(1, 80.0)],
        seconds: 2,
    })));
    let (channel, inputs) = GraphChannel::new();
    let (events, _keep) = broadcast::channel(8);
    spawn_service(service, events.subscribe(), inputs);
    for _ in 0..3 {
        clock.advance(Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(30));
        let _ = channel.send(GraphCommand::ResponsesOn);
    }
    assert!(channel.stop(QUIT_WAIT));
    for call in sent.0.lock().unwrap().iter() {
        assert_eq!(call, &[("graph_9".to_owned(), Some(5.0))], "{call:?}");
    }
}
