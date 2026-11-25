//! The threads that follow the daemon events outside the window: the tray icon, the desktop
//! notifications and the history writer. Each one owns a copy of the app state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use nicecooling_core::clock::SystemClock;
use nicecooling_core::rules::proc_scan::ProcFs;

use crate::app::now_us;
use crate::command::Command;
use crate::daemon_client::DaemonClient;
use crate::graphs::channel::{GraphChannel, Input, Inputs};
use crate::graphs::service::{Env, GraphService, NoLink, NoPower, SensorLink};
use crate::graphs::store::{GraphPaths, GraphStore};
use crate::graphs::values::{DaemonCalls, DaemonLink, LogindSession};
use crate::graphs::view::SharedView;
use crate::history::{HistoryDb, HistoryWriter};
use crate::i18n::Locale;
use crate::model::{AppState, DaemonEvent, FLAG_NO_VALUE};
use crate::notify::{Notifier, NotifyPrefs, Plan, plan};
use crate::tray::{Tray, TrayView};

/// Wait for the next event. `None` after one second without an event. The thread then
/// does its timed work: the countdown of the tray and the flush of the history.
fn next_event(
    runtime: &tokio::runtime::Runtime,
    events: &mut broadcast::Receiver<DaemonEvent>,
) -> Result<Option<DaemonEvent>, ()> {
    // The timer is made inside the runtime: it needs the time driver.
    let wait = runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(1), events.recv()).await });
    match wait {
        Err(_) | Ok(Err(RecvError::Lagged(_))) => Ok(None),
        Ok(Ok(event)) => Ok(Some(event)),
        Ok(Err(RecvError::Closed)) => Err(()),
    }
}

fn small_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a current-thread runtime")
}

/// How long the app waits for a StatusNotifier host before it opens the window. At login the
/// host of the desktop starts some seconds after the app.
pub const TRAY_HOST_GRACE: Duration = Duration::from_secs(30);

/// True when the window must open because no tray host came: once, after the grace time.
pub fn opens_window_without_host(host_online: bool, waited: Duration, opened: bool) -> bool {
    !host_online && !opened && waited >= TRAY_HOST_GRACE
}

/// The tray icon. No StatusNotifier host after the grace time: the window opens, so the user
/// has a way in.
pub fn spawn_tray(
    mut events: broadcast::Receiver<DaemonEvent>,
    commands: UnboundedSender<Command>,
    locale: Locale,
    visible: Arc<AtomicBool>,
    graphs: SharedView,
) {
    thread::spawn(move || {
        let runtime = small_runtime();
        let mut state = AppState::default();
        let build = |state: &AppState| {
            let countdown = graphs.lock().ok().and_then(|view| view.countdown.clone());
            TrayView::build(state, now_us(), locale)
                .with_graph_countdown(countdown.as_ref(), locale)
        };
        let view = build(&state);
        let tray = match Tray::spawn(commands.clone(), view.clone()) {
            Ok(tray) => tray,
            Err(error) => {
                eprintln!("nicecooling: no tray icon: {error}");
                visible.store(false, Ordering::Relaxed);
                let _ = commands.send(Command::OpenWindow(None));
                return;
            }
        };
        let mut shown = view;
        let started = Instant::now();
        let mut opened = false;
        while let Ok(event) = next_event(&runtime, &mut events) {
            if let Some(event) = event {
                state.apply(&event);
            }
            let host_online = tray.host_online();
            visible.store(host_online, Ordering::Relaxed);
            if opens_window_without_host(host_online, started.elapsed(), opened) {
                opened = true;
                let _ = commands.send(Command::OpenWindow(None));
            }
            let view = build(&state);
            if view != shown {
                tray.update(view.clone());
                shown = view;
            }
        }
        tray.shutdown();
    });
}

/// The desktop notifications.
///
/// `requests` carries the notifications of the other services, the Graph service among them:
/// this thread owns the connection to the notification server.
pub fn spawn_notifications(
    mut events: broadcast::Receiver<DaemonEvent>,
    mut requests: UnboundedReceiver<Plan>,
    commands: UnboundedSender<Command>,
    locale: Locale,
    prefs: Arc<Mutex<NotifyPrefs>>,
) {
    enum Wake {
        Event(Result<DaemonEvent, RecvError>),
        Request(Option<Plan>),
    }
    thread::spawn(move || {
        let runtime = small_runtime();
        let mut state = AppState::default();
        let mut notifier = Notifier::new(commands);
        let mut requests_open = true;
        loop {
            let wake = runtime.block_on(async {
                tokio::select! {
                    event = events.recv() => Wake::Event(event),
                    request = requests.recv(), if requests_open => Wake::Request(request),
                }
            });
            let steps = match wake {
                Wake::Request(Some(step)) => vec![step],
                // Every sender is gone. The events go on.
                Wake::Request(None) => {
                    requests_open = false;
                    continue;
                }
                Wake::Event(Ok(event)) => {
                    state.apply(&event);
                    // The first read after a start holds the Alarms that were active before.
                    // They arrive as a snapshot, not as an event, so they stay quiet.
                    let current = prefs
                        .lock()
                        .map_or_else(|poisoned| *poisoned.into_inner(), |p| *p);
                    plan(&event, &state, now_us(), locale, current)
                }
                Wake::Event(Err(RecvError::Lagged(_))) => continue,
                Wake::Event(Err(RecvError::Closed)) => break,
            };
            for step in steps {
                if let Err(error) = notifier.apply(step) {
                    eprintln!("nicecooling: notification: {error:?}");
                }
            }
        }
    });
}

/// The history store: one transaction every 10 s.
pub fn spawn_history(mut events: broadcast::Receiver<DaemonEvent>, path: PathBuf) {
    thread::spawn(move || {
        let db = match HistoryDb::open(&path) {
            Ok(db) => db,
            Err(error) => {
                eprintln!("nicecooling: history database {}: {error}", path.display());
                return;
            }
        };
        let runtime = small_runtime();
        let mut writer = HistoryWriter::new(db);
        let mut state = AppState::default();
        while let Ok(event) = next_event(&runtime, &mut events) {
            if let Some(event) = event {
                state.apply(&event);
                if let DaemonEvent::Sensors(sample) = &event {
                    let t = (sample.time_us / 1_000_000) as i64;
                    for (id, value, flags) in &sample.sensors {
                        let unit = state
                            .snapshot
                            .sensors
                            .get(id)
                            .map_or("", |s| s.unit.as_str());
                        writer.record(t, id, unit, (flags & FLAG_NO_VALUE == 0).then_some(*value));
                    }
                    for (id, duty, _, _, flags) in &sample.channels {
                        writer.record(t, id, "%", (flags & FLAG_NO_VALUE == 0).then_some(*duty));
                    }
                }
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            if let Err(error) = writer.tick(now) {
                eprintln!("nicecooling: history write: {error}");
            }
        }
    });
}

/// SIGTERM, the end of the session, takes the path of "Quit": it sends `Command::Quit`. The
/// handler is in place when the function returns.
pub fn spawn_sigterm(
    runtime: &tokio::runtime::Handle,
    commands: UnboundedSender<Command>,
) -> std::io::Result<()> {
    let _guard = runtime.enter();
    let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    runtime.spawn(async move {
        while signal.recv().await.is_some() {
            let _ = commands.send(Command::Quit);
        }
    });
    Ok(())
}

/// The Graph service on its own thread. It follows the daemon events, takes the commands of the
/// channel, and ends at the stop message of the channel or when the events end.
pub fn spawn_service(
    mut service: GraphService,
    mut events: broadcast::Receiver<DaemonEvent>,
    mut inputs: Inputs,
) {
    enum Wake {
        Event(Result<DaemonEvent, RecvError>),
        Input(Option<Input>),
        Timer,
    }
    thread::spawn(move || {
        // A panic outside the evaluation of a Graph (the store, the link, the channel) ends the
        // service. The view and a notification say so. The thread ends, so the channel closes and
        // a stop of the app learns that nobody wrote the state.
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let runtime = small_runtime();
            let mut stop = None;
            loop {
                let wait = service.until_next_wake();
                let wake = runtime.block_on(async {
                    // The stop message wins when it is due together with an event or the end of a
                    // countdown: after the stop, the service makes no power call.
                    tokio::select! {
                        biased;
                        input = inputs.recv() => Wake::Input(input),
                        event = events.recv() => Wake::Event(event),
                        () = tokio::time::sleep(wait) => Wake::Timer,
                    }
                });
                match wake {
                    Wake::Event(Ok(event)) => service.apply_event(&event),
                    Wake::Event(Err(RecvError::Closed)) | Wake::Input(None) => break,
                    Wake::Event(Err(RecvError::Lagged(_))) | Wake::Timer => {}
                    Wake::Input(Some(Input::Command(command))) => service.handle(command),
                    Wake::Input(Some(Input::Stop(done))) => {
                        stop = Some(done);
                        break;
                    }
                }
                service.poll();
            }
            service.stop();
            if let Some(done) = stop {
                let _ = done.send(());
            }
        }));
        if let Err(payload) = run {
            service.fail(&panic_text(payload.as_ref()));
        }
    });
}

/// The message of a panic.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// The link of the Graph service to the daemon, on the system bus. Without a system bus the link
/// sends nothing.
pub fn graph_link(
    system: Option<(DaemonClient, zbus::Connection)>,
    runtime: &tokio::runtime::Handle,
    view: SharedView,
) -> Box<dyn SensorLink> {
    let Some((client, conn)) = system else {
        return Box::new(NoLink);
    };
    Box::new(DaemonLink::new(
        DaemonCalls::new(client, runtime.clone()),
        Box::new(LogindSession::spawn(conn, runtime)),
        Box::new(SystemClock::new()),
        view,
    ))
}

/// The Graph service with the real clock and the real `/proc`. It returns the channel of its
/// commands.
pub fn spawn_graphs(
    events: broadcast::Receiver<DaemonEvent>,
    paths: GraphPaths,
    notify: UnboundedSender<Plan>,
    locale: Locale,
    view: SharedView,
    link: Box<dyn SensorLink>,
) -> GraphChannel {
    let (channel, inputs) = GraphChannel::new();
    let channel = channel.with_view(view.clone());
    let service = GraphService::new(
        GraphStore::open(paths),
        Env {
            clock: Box::new(SystemClock::new()),
            link,
            scan: Box::new(ProcFs::new("/proc")),
            power: Box::new(NoPower),
            notify,
            locale,
            view,
        },
    );
    spawn_service(service, events, inputs);
    channel
}

/// `$XDG_DATA_HOME/nicecooling/history.db`.
pub fn history_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("nicecooling/history.db")
}
