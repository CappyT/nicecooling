mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::service::{Env, GraphService, NoPower, SensorLink};
use nicecooling::graphs::session::LogindSession;
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::values::ActiveSession;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::notify::Plan;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::EndValue;
use support::bus::PrivateBus;
use support::fake_logind::FakeLogind;
use support::graphs::{Dir, constant_graph};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

type Calls = Arc<Mutex<Vec<Vec<(String, Option<f64>)>>>>;

struct Recorder(Calls);

impl SensorLink for Recorder {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.0.lock().unwrap().push(values.to_vec());
    }
}

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

/// The Graph service with the session of the app in the fake logind on a private bus.
struct Rig {
    service: GraphService,
    calls: Calls,
    view: SharedView,
    session: LogindSession,
    fake: FakeLogind,
    _clock: SimClock,
    _notifications: UnboundedReceiver<Plan>,
    _bus: PrivateBus,
    _dir: Dir,
}

async fn rig(name: &str, active: bool) -> Rig {
    let bus = PrivateBus::start(name);
    let fake = FakeLogind::start(bus.connect().await, "3", active).await;
    let session =
        LogindSession::spawn_for(bus.connect().await, &Handle::current(), Some("3".into()));
    wait_for(|| session.resolved()).await;

    let dir = Dir::new(name);
    let mut store = GraphStore::open(dir.graph_paths());
    let mut graph = constant_graph("graph_1", 5.0);
    graph.sensor_id = Some("graph_1".into());
    store.put(graph).unwrap();
    let clock = SimClock::default();
    let calls = Calls::default();
    let view = shared_view();
    let (notify, notifications) = unbounded_channel();
    let service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(Recorder(calls.clone())),
            scan: Box::new(NoScan),
            power: Box::new(NoPower),
            notify,
            locale: EN,
            view: view.clone(),
        },
    )
    .with_session(Box::new(session.clone()));
    Rig {
        service,
        calls,
        view,
        session,
        fake,
        _clock: clock,
        _notifications: notifications,
        _bus: bus,
        _dir: dir,
    }
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
async fn active_session_runs_responses() {
    let rig = rig("session-active", true).await;
    // The app asks logind for the session of its own id.
    assert_eq!(rig.fake.sessions_asked(), ["3"]);
    assert!(rig.session.active());
    assert!(rig.service.runs_responses());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inactive_session_runs_no_response() {
    let rig = rig("session-inactive", false).await;
    assert_eq!(rig.fake.sessions_asked(), ["3"]);
    assert!(rig.session.resolved());
    assert!(!rig.session.active());
    assert!(!rig.service.runs_responses());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_switch_and_back() {
    let rig = rig("session-switch", true).await;
    assert!(rig.service.runs_responses());

    // The user switches to another session: logind sends PropertiesChanged.
    rig.fake.set_active(false).await;
    wait_for(|| !rig.service.runs_responses()).await;

    rig.fake.set_active(true).await;
    wait_for(|| rig.service.runs_responses()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evaluation_continues_while_inactive() {
    let mut rig = rig("session-evaluation", false).await;
    assert!(!rig.service.runs_responses());
    assert!(rig.service.poll(), "a pass runs in an inactive session");
    assert_eq!(
        rig.calls.lock().unwrap().last().unwrap(),
        &[("graph_1".to_owned(), Some(5.0))]
    );
    let view = rig.view.lock().unwrap();
    assert_eq!(
        view.row("graph_1").unwrap().end,
        EndValue::Sensor(Some(5.0))
    );
}
