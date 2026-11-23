mod support;

use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::i18n::Language;
use nicecooling::model::{DaemonEvent, Link, Response};
use nicecooling::shell::Page;
use support::fixtures::{LEAKSHIELD, alarm, online};
use tokio::sync::{broadcast, mpsc};

struct Rig {
    events: broadcast::Sender<DaemonEvent>,
    commands: mpsc::UnboundedSender<Command>,
    _runtime: tokio::runtime::Runtime,
}

fn backend() -> (Backend, Rig) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands: commands.clone(),
        inbox,
        client: None,
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
    };
    (
        backend,
        Rig {
            events,
            commands,
            _runtime: runtime,
        },
    )
}

fn start(cx: &mut TestAppContext, mode: StartMode) -> (gpui::Entity<AppModel>, Rig) {
    let (backend, rig) = backend();
    let model = cx.update(|cx| app::init(cx, mode, backend));
    cx.run_until_parked();
    (model, rig)
}

#[gpui::test]
fn background_start_opens_no_window(cx: &mut TestAppContext) {
    let (_model, _rig) = start(cx, StartMode::Background);
    assert!(
        cx.windows().is_empty(),
        "--background shows the tray icon only"
    );
}

#[gpui::test]
fn normal_start_opens_one_window_and_draws_it(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Normal);
    assert_eq!(cx.windows().len(), 1);

    // The daemon answers: the shell draws the online state, then the poweroff banner.
    let mut state = online();
    state.snapshot.alarms = vec![alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    )];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.read_entity(&model, |app, _| app.state.is_online()));
    assert_eq!(
        cx.read_entity(&model, |app, _| app.shell().banners.len()),
        1
    );
}

#[gpui::test]
fn an_alarm_opens_the_window_by_itself(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Background);
    // A Warning does not open it.
    let warning = alarm(
        "w1",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    );
    rig.events.send(DaemonEvent::AlarmRaised(warning)).unwrap();
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    // An Alarm does, on the Overview.
    let alarm = alarm("a1", "pump-stop", "loop_1", Response::Poweroff, 30);
    rig.events.send(DaemonEvent::AlarmRaised(alarm)).unwrap();
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(
        cx.read_entity(&model, |app, _| app.page.clone()),
        Page::Overview
    );
}

#[gpui::test]
fn a_down_daemon_opens_the_window_by_itself(cx: &mut TestAppContext) {
    let (_model, rig) = start(cx, StartMode::Background);
    rig.events.send(DaemonEvent::Link(Link::Offline)).unwrap();
    cx.run_until_parked();
    assert!(cx.windows().is_empty(), "offline alone does not open it");
    rig.events.send(DaemonEvent::Link(Link::Down)).unwrap();
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
}

#[gpui::test]
fn the_tray_opens_a_closed_window_on_a_page(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Normal);
    let handle = cx.windows()[0];
    // The close button hides the window. The app stays.
    cx.update_window(handle, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(cx.windows().is_empty());

    rig.commands
        .send(Command::OpenWindow(Some(Page::Channels)))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(
        cx.read_entity(&model, |app, _| app.page.clone()),
        Page::Channels
    );
    // A second command raises the same window.
    rig.commands.send(Command::OpenWindow(None)).unwrap();
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
}

#[test]
fn start_mode_reads_the_background_flag() {
    let args = |list: &[&str]| list.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        StartMode::from_args(args(&["nicecooling", "--background"])),
        StartMode::Background
    );
    assert_eq!(
        StartMode::from_args(args(&["nicecooling"])),
        StartMode::Normal
    );
}
