//! The threads that follow the daemon events outside the window: the tray icon, the desktop
//! notifications and the history writer. Each one owns a copy of the app state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc::UnboundedSender;

use nicecooling_core::clock::SystemClock;
use nicecooling_core::rules::proc_scan::ProcFs;

use crate::app::now_us;
use crate::command::Command;
use crate::graphs::service::{GraphService, NoLink};
use crate::graphs::store::{GraphPaths, GraphStore};
use crate::graphs::view::SharedView;
use crate::history::{HistoryDb, HistoryWriter};
use crate::i18n::Locale;
use crate::model::{AppState, DaemonEvent, FLAG_NO_VALUE};
use crate::notify::{Notifier, NotifyPrefs, plan};
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
) {
    thread::spawn(move || {
        let runtime = small_runtime();
        let mut state = AppState::default();
        let view = TrayView::build(&state, now_us(), locale);
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
            let view = TrayView::build(&state, now_us(), locale);
            if view != shown {
                tray.update(view.clone());
                shown = view;
            }
        }
        tray.shutdown();
    });
}

/// The desktop notifications.
pub fn spawn_notifications(
    mut events: broadcast::Receiver<DaemonEvent>,
    commands: UnboundedSender<Command>,
    locale: Locale,
    prefs: Arc<Mutex<NotifyPrefs>>,
) {
    thread::spawn(move || {
        let runtime = small_runtime();
        let mut state = AppState::default();
        let mut notifier = Notifier::new(commands);
        while let Ok(event) = next_event(&runtime, &mut events) {
            let Some(event) = event else { continue };
            state.apply(&event);
            // The first read after a start holds the Alarms that were active before. They
            // arrive as a snapshot, not as an event, so they stay quiet.
            let current = prefs
                .lock()
                .map_or_else(|poisoned| *poisoned.into_inner(), |p| *p);
            for step in plan(&event, &state, now_us(), locale, current) {
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

/// The Graph service: one evaluation of every enabled Graph per second.
pub fn spawn_graphs(
    mut events: broadcast::Receiver<DaemonEvent>,
    paths: GraphPaths,
    view: SharedView,
) {
    thread::spawn(move || {
        let runtime = small_runtime();
        let mut service = GraphService::new(
            GraphStore::open(paths),
            Box::new(SystemClock::new()),
            Box::new(NoLink),
            Box::new(ProcFs::new("/proc")),
            view,
        );
        loop {
            let wait = service.until_next_pass();
            let got = runtime.block_on(async { tokio::time::timeout(wait, events.recv()).await });
            match got {
                Ok(Ok(event)) => service.apply_event(&event),
                Ok(Err(RecvError::Closed)) => break,
                Ok(Err(RecvError::Lagged(_))) | Err(_) => {}
            }
            service.poll();
        }
        service.stop();
    });
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
