//! A rename that the user typed commits when the user clicks outside the screens, in the
//! shell with the fake daemon: on the sidebar, on the header bar, and on a button whose own
//! call must come after the rename.

mod support;

use std::time::{Duration, Instant};

use gpui::{Modifiers, TestAppContext, VisualTestContext, px};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::daemon_client::DaemonClient;
use nicecooling::i18n::Language;
use nicecooling::shell::Page;
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use tokio::sync::{broadcast, mpsc};

const LIMIT: Duration = Duration::from_secs(20);

/// A rename input: the page, the buttons that open it, and the start of its call.
struct Kind {
    page: Page,
    open: &'static [&'static str],
    call: &'static str,
}

const CHANNEL: Kind = Kind {
    page: Page::Channels,
    open: &["channel-rename"],
    call: "SetChannelName(",
};

const SENSOR: Kind = Kind {
    page: Page::Sensors,
    open: &["rename-k10temp_tctl"],
    call: "SetSensorName(",
};

const LOOP: Kind = Kind {
    page: Page::Loops,
    open: &["loop-rename"],
    call: "SetLoopName(",
};

const PROFILE: Kind = Kind {
    page: Page::Profiles,
    open: &["profile-menu", "profile-rename"],
    call: "SetProfileName(",
};

struct Rig {
    cx: VisualTestContext,
    model: gpui::Entity<AppModel>,
    daemon: FakeDaemon,
    runtime: tokio::runtime::Runtime,
    _bus: PrivateBus,
}

impl Rig {
    fn draw(&mut self) {
        self.cx.run_until_parked();
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
        self.cx.run_until_parked();
    }

    /// Poll until `done` holds. The daemon answers on the runtime of the test.
    fn wait(&mut self, what: &str, done: impl Fn(&mut Self) -> bool) {
        let start = Instant::now();
        loop {
            self.draw();
            if done(self) {
                return;
            }
            assert!(start.elapsed() < LIMIT, "timed out waiting for: {what}");
            // The runtime runs only here, in the thread of the test: gpui accepts no wake-up
            // from another thread.
            self.runtime
                .block_on(async { tokio::time::sleep(Duration::from_millis(20)).await });
        }
    }

    fn page(&mut self) -> Page {
        self.cx.update(|_, cx| self.model.read(cx).page.clone())
    }

    fn show(&mut self, page: Page) {
        self.cx.update(|_, cx| {
            self.model.update(cx, |app, cx| {
                app.page = page;
                cx.notify();
            })
        });
        self.draw();
    }

    fn click(&mut self, selector: &str) {
        self.wait(selector, |rig| {
            rig.cx
                .debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
                .is_some()
        });
        let bounds = self
            .cx
            .debug_bounds(Box::leak(selector.to_owned().into_boxed_str()));
        self.cx
            .simulate_click(bounds.expect("the element").center(), Modifiers::none());
        self.draw();
    }

    fn type_name(&mut self, kind: &Kind) {
        for button in kind.open {
            self.click(button);
        }
        self.click("rename-input");
        self.cx.simulate_keystrokes("ctrl-a");
        self.cx.simulate_input("Pump");
        self.draw();
    }

    /// The calls of the daemon that start with `prefix`.
    fn calls(&self, prefix: &str) -> Vec<String> {
        self.daemon
            .calls()
            .into_iter()
            .filter(|call| call.starts_with(prefix))
            .collect()
    }
}

fn start(cx: &mut TestAppContext, kind: &Kind) -> Rig {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let bus = PrivateBus::start("rename-shell");
    let (daemon, client) = runtime.block_on(async {
        let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
        (daemon, DaemonClient::new(bus.connect().await))
    });
    let (events, events_rx) = broadcast::channel(256);
    runtime.spawn(client.clone().run(events));
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands,
        inbox,
        client: Some(client),
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
        graphs: None,
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    cx.run_until_parked();
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(px(1280.), px(800.)));
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, _| window.activate_window());
    let mut rig = Rig {
        cx: visual,
        model,
        daemon,
        runtime,
        _bus: bus,
    };
    rig.wait("the daemon state", |rig| {
        rig.cx.update(|_, cx| {
            let snapshot = &rig.model.read(cx).state.snapshot;
            !snapshot.channels.is_empty()
                && !snapshot.sensors.is_empty()
                && !snapshot.loops.is_empty()
                && !snapshot.profiles.is_empty()
        })
    });
    rig.show(kind.page.clone());
    rig
}

fn commits_on_click_outside(cx: &mut TestAppContext, kind: &Kind, target: &str, leaves: bool) {
    let mut rig = start(cx, kind);
    rig.type_name(kind);
    assert!(rig.calls(kind.call).is_empty(), "typing sends no call");
    rig.click(target);
    rig.wait("the rename call", |rig| !rig.calls(kind.call).is_empty());
    // Give a second commit the time to show up, if there is one.
    rig.runtime
        .block_on(async { tokio::time::sleep(Duration::from_millis(200)).await });
    rig.draw();
    let calls = rig.calls(kind.call);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].contains(",Pump)"), "{calls:?}");
    if leaves {
        assert_ne!(rig.page(), kind.page, "the click went on");
    }
    cx.update(|cx| cx.quit());
}

fn sidebar(cx: &mut TestAppContext, kind: &Kind) {
    commits_on_click_outside(cx, kind, "nav-Overview", true);
}

#[gpui::test]
fn a_channel_name_commits_on_a_click_on_the_sidebar(cx: &mut TestAppContext) {
    sidebar(cx, &CHANNEL);
}

#[gpui::test]
fn a_sensor_name_commits_on_a_click_on_the_sidebar(cx: &mut TestAppContext) {
    sidebar(cx, &SENSOR);
}

#[gpui::test]
fn a_loop_name_commits_on_a_click_on_the_sidebar(cx: &mut TestAppContext) {
    sidebar(cx, &LOOP);
}

#[gpui::test]
fn a_profile_name_commits_on_a_click_on_the_sidebar(cx: &mut TestAppContext) {
    sidebar(cx, &PROFILE);
}

#[gpui::test]
fn a_loop_name_commits_on_a_click_on_the_header_bar(cx: &mut TestAppContext) {
    commits_on_click_outside(cx, &LOOP, "header-title", false);
}

/// The rename call reaches the daemon before the call of the button that the user clicked.
#[gpui::test]
fn the_rename_goes_before_the_call_of_the_button_that_is_clicked(cx: &mut TestAppContext) {
    let mut rig = start(cx, &LOOP);
    rig.type_name(&LOOP);
    rig.click("loop-new");
    rig.wait("both calls", |rig| {
        !rig.calls("SetLoopName(").is_empty() && !rig.calls("CreateLoop(").is_empty()
    });
    let calls = rig.daemon.calls();
    let at = |prefix: &str| calls.iter().position(|call| call.starts_with(prefix));
    assert!(
        at("SetLoopName(") < at("CreateLoop("),
        "the rename comes first: {calls:?}"
    );
    cx.update(|cx| cx.quit());
}
