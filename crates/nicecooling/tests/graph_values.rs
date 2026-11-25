mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::daemon_client::DaemonClient;
use nicecooling::graphs::service::{Env, GraphService, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::values::{
    ActiveSession, DaemonCalls, DaemonLink, GraphValues, LogindSession, SensorError, ValuesCall,
};
use nicecooling::graphs::view::{SharedView, ValuesState, shared_view};
use nicecooling::i18n::EN;
use nicecooling::notify::Plan;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::Graph;
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
}

fn sensor_graph(id: &str, sensor: &str, value: f64) -> Graph {
    let mut graph = constant_graph(id, value);
    graph.sensor_id = Some(sensor.to_owned());
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
    rig.client.delete_graph_sensor(&id).await.unwrap();
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
