mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::daemon_client::DaemonClient;
use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::service::{Env, GraphService, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::values::{
    ActiveSession, DaemonCalls, DaemonLink, GraphValues, LogindSession, SensorError, ValuesCall,
};
use nicecooling::graphs::view::{ForeignSensor, SharedView, ValuesState, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::DaemonEvent;
use nicecooling::notify::Plan;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::graphs::{Dir, constant_graph};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const SECOND: Duration = Duration::from_secs(1);

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

#[derive(Clone)]
struct FakeSession(Arc<AtomicBool>);

impl ActiveSession for FakeSession {
    fn active(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The Graph service with the real daemon link, on a private bus with the fake daemon.
struct Rig {
    service: GraphService,
    clock: SimClock,
    fake: FakeDaemon,
    client: DaemonClient,
    view: SharedView,
    session: Arc<AtomicBool>,
    _notifications: UnboundedReceiver<Plan>,
    _bus: PrivateBus,
    _dir: Dir,
}

impl Rig {
    async fn start(name: &str, graphs: Vec<Graph>) -> Self {
        let bus = PrivateBus::start(name);
        let fake = FakeDaemon::start(bus.connect().await, Options::default()).await;
        let client = DaemonClient::new(bus.connect().await);
        let dir = Dir::new(name);
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        let clock = SimClock::default();
        let view = shared_view();
        let session = Arc::new(AtomicBool::new(true));
        let (notify, notifications) = unbounded_channel();
        let link = DaemonLink::new(
            DaemonCalls::new(client.clone(), Handle::current()),
            Box::new(FakeSession(session.clone())),
            Box::new(clock.clone()),
            view.clone(),
        );
        let service = GraphService::new(
            store,
            Env {
                clock: Box::new(clock.clone()),
                link: Box::new(link),
                scan: Box::new(NoScan),
                power: Box::new(NoPower),
                notify,
                locale: EN,
                view: view.clone(),
            },
        );
        Self {
            service,
            clock,
            fake,
            client,
            view,
            session,
            _notifications: notifications,
            _bus: bus,
            _dir: dir,
        }
    }

    /// The service runs blocking calls on the runtime: keep the other tasks going meanwhile.
    fn poll(&mut self) -> bool {
        tokio::task::block_in_place(|| self.service.poll())
    }

    /// One second on the clock, and the pass that is due.
    fn tick(&mut self) {
        self.clock.advance(SECOND);
        assert!(self.poll(), "a pass was due");
    }

    fn values_calls(&self) -> Vec<String> {
        self.fake.calls_of("SetGraphSensorValues")
    }

    fn state(&self) -> ValuesState {
        self.view.lock().unwrap().values.clone()
    }

    fn handle(&mut self, command: GraphCommand) {
        tokio::task::block_in_place(|| self.service.handle(command));
    }

    fn event(&mut self, event: &DaemonEvent) {
        tokio::task::block_in_place(|| self.service.apply_event(event));
    }

    /// The snapshot that the daemon client reads now.
    async fn read_snapshot(&self) -> DaemonEvent {
        DaemonEvent::Snapshot(Box::new(self.client.snapshot().await.unwrap()))
    }

    /// A new snapshot of the daemon reaches the service.
    async fn snapshot(&mut self) {
        let event = self.read_snapshot().await;
        self.event(&event);
    }

    fn stored(&self, id: &str) -> Option<Graph> {
        self.service.store().graph(id).cloned()
    }

    fn refusal(&self) -> Option<String> {
        self.view.lock().unwrap().refusal.clone()
    }
}

fn sensor_graph(id: &str, sensor: &str, value: f64) -> Graph {
    let mut graph = constant_graph(id, value);
    graph.sensor_id = Some(sensor.to_owned());
    graph
}

/// A Graph with a response end node: a notification that its constant input starts.
fn response_graph(id: &str) -> Graph {
    let mut graph = Graph::new(id, "Five");
    graph.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", 1.0),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "notification")
            .with_param("title", "t"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn values_call_has_no_interactive_flag() {
    let mut rig = Rig::start("values-flag", vec![sensor_graph("graph_a", "graph_1", 5.0)]).await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    assert!(rig.poll());
    assert_eq!(
        rig.values_calls(),
        ["SetGraphSensorValues(graph_1=5) interactive=false"]
    );
    assert_eq!(
        rig.fake.shared.graph_values.lock().unwrap().get("graph_1"),
        Some(&Some(5.0))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_call_has_the_interactive_flag() {
    let rig = Rig::start("create-flag", vec![]).await;
    let id = rig.client.create_graph_sensor("Hot", "degC").await.unwrap();
    assert_eq!(id, "graph_1");
    rig.client
        .set_graph_sensor_definition(&id, "Hotter", "degC")
        .await
        .unwrap();
    // The object of the sensor goes and comes back after the reply.
    rig.fake.settle().await;
    rig.client.delete_graph_sensor(&id).await.unwrap();
    rig.fake.settle().await;
    assert_eq!(
        rig.fake.calls(),
        [
            "CreateGraphSensor(Hot,degC) interactive=true",
            "SetGraphSensorDefinition(graph_1,Hotter,degC) interactive=true",
            "DeleteGraphSensor(graph_1) interactive=true",
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn values_sent_once_per_second() {
    let mut rig = Rig::start("values-rate", vec![sensor_graph("graph_a", "graph_1", 5.0)]).await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    assert!(rig.poll());
    // Polls inside the same second run no pass and send nothing.
    assert!(!rig.poll());
    rig.clock.advance(Duration::from_millis(400));
    assert!(!rig.poll());
    assert_eq!(rig.values_calls().len(), 1);
    rig.clock.advance(Duration::from_millis(600));
    assert!(rig.poll());
    rig.tick();
    assert_eq!(rig.values_calls().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_value_entry_for_a_disabled_graph() {
    let mut off = sensor_graph("graph_a", "graph_1", 5.0);
    off.enabled = false;
    let mut rig = Rig::start(
        "values-disabled",
        vec![off, sensor_graph("graph_b", "graph_2", 7.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Off", "degC").await;
    rig.fake.add_graph_sensor("On", "degC").await;
    assert!(rig.poll());
    assert_eq!(
        rig.values_calls(),
        ["SetGraphSensorValues(graph_1=none,graph_2=7) interactive=false"]
    );
    let table = rig.fake.shared.graph_values.lock().unwrap().clone();
    assert_eq!(table.get("graph_1"), Some(&None));
    assert_eq!(table.get("graph_2"), Some(&Some(7.0)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denial_stops_the_calls_for_60s() {
    let mut rig = Rig::start(
        "values-denied",
        vec![sensor_graph("graph_a", "graph_1", 5.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.fake.deny_writes(true);
    assert!(rig.poll());
    assert_eq!(rig.values_calls().len(), 1);
    assert_eq!(rig.state(), ValuesState::NoAccess);
    // Seconds 1 to 59 send nothing, and the state stays.
    for _ in 1..60 {
        rig.tick();
    }
    assert_eq!(rig.values_calls().len(), 1);
    assert_eq!(rig.state(), ValuesState::NoAccess);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn calls_resume_after_60s() {
    let mut rig = Rig::start(
        "values-resume",
        vec![sensor_graph("graph_a", "graph_1", 5.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.fake.deny_writes(true);
    assert!(rig.poll());
    rig.fake.deny_writes(false);
    for _ in 1..60 {
        rig.tick();
    }
    assert_eq!(rig.values_calls().len(), 1);
    rig.tick();
    assert_eq!(rig.values_calls().len(), 2);
    assert_eq!(rig.state(), ValuesState::Ok);
    rig.tick();
    assert_eq!(rig.values_calls().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inactive_session_sends_no_values() {
    let mut rig = Rig::start(
        "values-inactive",
        vec![sensor_graph("graph_a", "graph_1", 5.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.session.store(false, Ordering::Relaxed);
    assert!(rig.poll());
    rig.tick();
    rig.tick();
    assert!(rig.values_calls().is_empty());
    assert_eq!(rig.state(), ValuesState::InactiveSession);
    rig.session.store(true, Ordering::Relaxed);
    rig.tick();
    assert_eq!(rig.values_calls().len(), 1);
    assert_eq!(rig.state(), ValuesState::Ok);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denial_is_told_apart_from_another_failure() {
    let rig = Rig::start("values-errors", vec![]).await;
    let calls = DaemonCalls::new(rig.client.clone(), Handle::current());
    rig.fake.deny_writes(true);
    let denied = tokio::task::block_in_place(|| calls.create("Hot", "degC"));
    assert_eq!(denied, Err(SensorError::Denied));
    rig.fake.deny_writes(false);
    let unit = tokio::task::block_in_place(|| calls.create("Hot", "bar"));
    assert!(matches!(unit, Err(SensorError::Refused(_))), "{unit:?}");
}

type Entries = Vec<(String, bool, f64)>;

/// A values call that records its entries.
struct Recorder(Arc<Mutex<Vec<Entries>>>);

impl ValuesCall for Recorder {
    fn send(&mut self, entries: &[(String, bool, f64)]) -> Result<(), SensorError> {
        self.0.lock().unwrap().push(entries.to_vec());
        Ok(())
    }
}

#[test]
fn a_result_that_is_not_finite_goes_as_a_no_value_entry() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut values = GraphValues::new(
        Box::new(Recorder(sent.clone())),
        Box::new(FakeSession(Arc::new(AtomicBool::new(true)))),
        Box::new(SimClock::default()),
        shared_view(),
    );
    values.send(&[
        ("graph_1".into(), Some(f64::NAN)),
        ("graph_2".into(), Some(f64::INFINITY)),
        ("graph_3".into(), Some(2.5)),
        ("graph_4".into(), None),
    ]);
    assert_eq!(
        sent.lock().unwrap().as_slice(),
        [vec![
            ("graph_1".to_owned(), false, 0.0),
            ("graph_2".to_owned(), false, 0.0),
            ("graph_3".to_owned(), true, 2.5),
            ("graph_4".to_owned(), false, 0.0),
        ]]
    );
}

/// A logind with one session whose `Active` property the test sets.
struct FakeSessionObject {
    active: bool,
}

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl FakeSessionObject {
    #[zbus(property)]
    fn active(&self) -> bool {
        self.active
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logind_session_follows_active() {
    let bus = PrivateBus::start("logind");
    let logind = bus.connect().await;
    let path = "/org/freedesktop/login1/session/auto";
    logind
        .object_server()
        .at(path, FakeSessionObject { active: false })
        .await
        .unwrap();
    logind.request_name("org.freedesktop.login1").await.unwrap();
    let session = LogindSession::spawn(bus.connect().await, &Handle::current());
    wait_for(|| !session.active()).await;
    let iface = logind
        .object_server()
        .interface::<_, FakeSessionObject>(path)
        .await
        .unwrap();
    iface.get_mut().await.active = true;
    iface
        .get()
        .await
        .active_changed(iface.signal_emitter())
        .await
        .unwrap();
    wait_for(|| session.active()).await;
}

async fn wait_for(done: impl Fn() -> bool) {
    let wait = async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("the condition held within 10 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn apply_creates_the_sensor_first() {
    let mut rig = Rig::start("apply-creates", vec![]).await;
    rig.handle(GraphCommand::Apply(Box::new(constant_graph(
        "graph_a", 5.0,
    ))));
    assert_eq!(
        rig.fake.calls(),
        ["CreateGraphSensor(Five,degC) interactive=true"]
    );
    let stored = rig.stored("graph_a").expect("the Graph is stored");
    assert_eq!(stored.sensor_id.as_deref(), Some("graph_1"));
    assert_eq!(rig.refusal(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refused_create_stops_apply() {
    let mut rig = Rig::start("apply-refused", vec![]).await;
    rig.fake.set_graph_limit(0);
    rig.handle(GraphCommand::Apply(Box::new(constant_graph(
        "graph_a", 5.0,
    ))));
    assert_eq!(rig.fake.calls_of("CreateGraphSensor").len(), 1);
    assert!(rig.stored("graph_a").is_none(), "nothing is stored");
    let reason = rig.refusal().expect("the reason is shown");
    assert!(reason.contains("limit"), "{reason}");
    // A denial by polkit stops Apply in the same way.
    rig.fake.set_graph_limit(64);
    rig.fake.deny_writes(true);
    rig.handle(GraphCommand::Apply(Box::new(constant_graph(
        "graph_a", 5.0,
    ))));
    assert!(rig.stored("graph_a").is_none());
    let reason = rig.refusal().expect("the reason is shown");
    assert!(reason.contains("does not allow"), "{reason}");
    assert!(rig.fake.graph_sensor_ids().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn apply_renames_the_sensor_through_set_definition() {
    let mut rig = Rig::start(
        "apply-rename",
        vec![sensor_graph("graph_a", "graph_1", 5.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.snapshot().await;
    // Nothing changed: no call.
    let same = rig.stored("graph_a").unwrap();
    rig.handle(GraphCommand::Apply(Box::new(same.clone())));
    assert!(rig.fake.calls().is_empty(), "{:?}", rig.fake.calls());
    // A new name and a new unit: one call, and the id stays.
    let mut changed = same;
    changed.name = "Hot water".into();
    changed.nodes[1] = Node::new(2, Kind::SensorEnd).with_param("unit", "mbar");
    rig.handle(GraphCommand::Apply(Box::new(changed)));
    assert_eq!(
        rig.fake.calls(),
        ["SetGraphSensorDefinition(graph_1,Hot water,mbar) interactive=true"]
    );
    let stored = rig.stored("graph_a").unwrap();
    assert_eq!(stored.sensor_id.as_deref(), Some("graph_1"));
    assert_eq!(stored.name, "Hot water");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enable_creates_the_missing_sensor() {
    let mut off = constant_graph("graph_a", 5.0);
    off.enabled = false;
    let mut rig = Rig::start("enable-creates", vec![off]).await;
    // Disable and Apply of a Graph that is off make the sensor only when asked: Disable none.
    rig.handle(GraphCommand::Enable {
        graph: "graph_a".into(),
        enabled: false,
    });
    assert!(rig.fake.calls().is_empty());
    rig.handle(GraphCommand::Enable {
        graph: "graph_a".into(),
        enabled: true,
    });
    assert_eq!(
        rig.fake.calls(),
        ["CreateGraphSensor(Five,degC) interactive=true"]
    );
    let stored = rig.stored("graph_a").unwrap();
    assert!(stored.enabled);
    assert_eq!(stored.sensor_id.as_deref(), Some("graph_1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refused_enable_keeps_the_graph_disabled() {
    let mut off = constant_graph("graph_a", 5.0);
    off.enabled = false;
    let mut rig = Rig::start("enable-refused", vec![off]).await;
    rig.fake.set_graph_limit(0);
    rig.handle(GraphCommand::Enable {
        graph: "graph_a".into(),
        enabled: true,
    });
    let stored = rig.stored("graph_a").unwrap();
    assert!(!stored.enabled);
    assert_eq!(stored.sensor_id, None);
    assert!(rig.refusal().unwrap().contains("limit"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_in_use_keeps_the_graph_and_names_the_curve() {
    let mut rig = Rig::start(
        "delete-in-use",
        vec![sensor_graph("graph_a", "graph_1", 5.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.fake
        .set_graph_in_use(Some("graph_1 is read by the Curve of Waterpump"));
    rig.handle(GraphCommand::Delete {
        graph: "graph_a".into(),
    });
    assert!(rig.stored("graph_a").is_some(), "the Graph stays");
    assert!(rig.refusal().unwrap().contains("Waterpump"));
    assert_eq!(rig.fake.graph_sensor_ids(), ["graph_1"]);
    // When nothing uses the sensor, the sensor goes first and the Graph after it.
    rig.fake.set_graph_in_use(None);
    rig.handle(GraphCommand::Delete {
        graph: "graph_a".into(),
    });
    assert!(rig.stored("graph_a").is_none());
    assert!(rig.fake.graph_sensor_ids().is_empty());
    assert_eq!(rig.refusal(), None);
    assert_eq!(
        rig.fake.calls_of("DeleteGraphSensor"),
        [
            "DeleteGraphSensor(graph_1) interactive=true",
            "DeleteGraphSensor(graph_1) interactive=true",
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_node_kind_change_of_a_used_sensor_stores_nothing() {
    let mut rig = Rig::start("kind-change", vec![sensor_graph("graph_a", "graph_1", 5.0)]).await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.fake
        .set_graph_in_use(Some("graph_1 is read by the Curve of Waterpump"));
    rig.handle(GraphCommand::Apply(Box::new(response_graph("graph_a"))));
    let stored = rig.stored("graph_a").unwrap();
    assert!(stored.sensor_end().is_some(), "the Graph is as it was");
    assert_eq!(stored.sensor_id.as_deref(), Some("graph_1"));
    assert!(rig.refusal().unwrap().contains("Waterpump"));
    // Once nothing uses the sensor, the change goes through and the sensor is removed.
    rig.fake.set_graph_in_use(None);
    rig.handle(GraphCommand::Apply(Box::new(response_graph("graph_a"))));
    let stored = rig.stored("graph_a").unwrap();
    assert!(stored.sensor_end().is_none());
    assert_eq!(stored.sensor_id, None);
    assert!(rig.fake.graph_sensor_ids().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_sensor_is_created_at_start() {
    let mut rig = Rig::start(
        "start-missing",
        vec![sensor_graph("graph_a", "graph_3", 5.0)],
    )
    .await;
    rig.snapshot().await;
    assert_eq!(
        rig.fake.calls(),
        ["CreateGraphSensor(Five,degC) interactive=true"]
    );
    assert_eq!(
        rig.stored("graph_a").unwrap().sensor_id.as_deref(),
        Some("graph_1")
    );
    assert_eq!(rig.fake.graph_sensor_ids(), ["graph_1"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sensor_of_another_user_is_left_and_listed() {
    let mut rig = Rig::start(
        "start-foreign",
        vec![sensor_graph("graph_a", "graph_1", 5.0)],
    )
    .await;
    rig.fake.add_graph_sensor("Five", "degC").await;
    rig.fake.add_graph_sensor("Other", "W").await;
    rig.snapshot().await;
    assert!(rig.fake.calls().is_empty(), "{:?}", rig.fake.calls());
    assert_eq!(
        rig.view.lock().unwrap().foreign_sensors,
        [ForeignSensor {
            id: "graph_2".into(),
            name: "Other".into(),
            unit: "W".into(),
        }]
    );
    assert_eq!(rig.fake.graph_sensor_ids(), ["graph_1", "graph_2"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn new_snapshot_runs_the_comparison_again() {
    let mut rig = Rig::start("start-again", vec![sensor_graph("graph_a", "graph_3", 5.0)]).await;
    rig.snapshot().await;
    assert_eq!(rig.fake.calls_of("CreateGraphSensor").len(), 1);
    // The snapshot that follows the new sensor lists it: no second call.
    rig.snapshot().await;
    assert_eq!(rig.fake.calls_of("CreateGraphSensor").len(), 1);
    // The daemon loses the sensor, for example after a reset of its state.
    rig.fake.drop_graph_sensor("graph_1").await;
    rig.snapshot().await;
    assert_eq!(rig.fake.calls_of("CreateGraphSensor").len(), 2);
    assert_eq!(
        rig.stored("graph_a").unwrap().sensor_id.as_deref(),
        Some("graph_2")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_snapshot_does_not_make_a_second_sensor() {
    let mut rig = Rig::start("start-stale", vec![sensor_graph("graph_a", "graph_3", 5.0)]).await;
    let before = rig.read_snapshot().await;
    rig.event(&before);
    // A snapshot that the client read before the call can arrive after it.
    rig.event(&before);
    assert_eq!(rig.fake.calls_of("CreateGraphSensor").len(), 1);
    // After the grace time the service believes the snapshot again.
    rig.clock.advance(Duration::from_secs(31));
    rig.event(&before);
    assert_eq!(rig.fake.calls_of("CreateGraphSensor").len(), 2);
}
