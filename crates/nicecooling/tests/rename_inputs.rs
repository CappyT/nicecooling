//! The rename input of a Channel, a sensor, a Loop and a Profile commits its text when the
//! user leaves the input, with a click on any button or with the focus. Enter commits it as
//! before, Escape drops it, and a commit sends one call.

mod support;

use gpui::{AppContext as _, Modifiers, TestAppContext, VisualTestContext, px};
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::i18n::Language;
use nicecooling::model::DaemonEvent;
use nicecooling::screen_view::ScreenView;
use nicecooling::shell::Page;
use support::fixtures::full;
use tokio::sync::{broadcast, mpsc};

/// A rename input: the page, the buttons that open it, a button that is not the input, and
/// the name that the daemon has now.
struct Kind {
    page: Page,
    open: &'static [&'static str],
    other: &'static str,
    name: &'static str,
    command: fn(&str) -> Command,
}

const CHANNEL: Kind = Kind {
    page: Page::Channels,
    open: &["channel-rename"],
    other: "channel-rename",
    name: "Chassis Fan 1",
    command: |name| Command::SetChannelName {
        channel: "nct6798_pwm2".into(),
        name: name.into(),
    },
};

const SENSOR: Kind = Kind {
    page: Page::Sensors,
    open: &["rename-k10temp_tctl"],
    other: "rename-nct6798_temp1",
    name: "CPU temperature",
    command: |name| Command::SetSensorName {
        sensor: "k10temp_tctl".into(),
        name: name.into(),
    },
};

const LOOP: Kind = Kind {
    page: Page::Loops,
    open: &["loop-rename"],
    other: "loop-menu",
    name: "Main loop",
    command: |name| Command::SetLoopName {
        loop_id: "loop_1".into(),
        name: name.into(),
    },
};

const PROFILE: Kind = Kind {
    page: Page::Profiles,
    open: &["profile-menu", "profile-rename"],
    other: "profile-menu",
    name: "Quiet",
    command: |name| Command::SetProfileName {
        profile: "profile_2".into(),
        name: name.into(),
    },
};

const KINDS: [&Kind; 4] = [&CHANNEL, &SENSOR, &LOOP, &PROFILE];

/// How the user leaves the input.
#[derive(Clone, Copy)]
enum Leave {
    /// A click on a button that is not the input.
    Button,
    /// The focus moves on, as Tab does.
    Focus,
    Enter,
    Escape,
}

struct Rig {
    cx: VisualTestContext,
    commands: mpsc::UnboundedReceiver<Command>,
    _keep: (
        tokio::runtime::Runtime,
        broadcast::Sender<DaemonEvent>,
        mpsc::UnboundedSender<Command>,
    ),
}

impl Rig {
    fn draw(&mut self) {
        self.cx.run_until_parked();
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
        self.cx.run_until_parked();
    }

    fn click(&mut self, selector: &'static str) {
        self.draw();
        let bounds = self
            .cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("no element {selector}"));
        self.cx.simulate_click(bounds.center(), Modifiers::none());
        self.draw();
    }

    /// The calls that the screens sent since the last read.
    fn sent(&mut self) -> Vec<Command> {
        self.draw();
        self.draw();
        std::iter::from_fn(|| self.commands.try_recv().ok()).collect()
    }

    /// Open the rename input of `kind` and type `text` in it.
    fn type_name(&mut self, kind: &Kind, text: &str) {
        for button in kind.open {
            self.click(button);
        }
        self.click("rename-input");
        self.cx.simulate_keystrokes("ctrl-a");
        self.cx.simulate_input(text);
        self.draw();
    }

    fn leave(&mut self, kind: &Kind, how: Leave) {
        match how {
            Leave::Button => self.click(kind.other),
            Leave::Focus => {
                self.cx.update(|window, cx| window.focus_next(cx));
                self.draw();
            }
            Leave::Enter => self.cx.simulate_keystrokes("enter"),
            Leave::Escape => self.cx.simulate_keystrokes("escape"),
        }
    }
}

fn start(cx: &mut TestAppContext, kind: &Kind) -> Rig {
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
        graphs: None,
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    let state = full();
    events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    events
        .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = kind.page.clone();
            cx.notify();
        })
    });
    let (view_commands, received) = mpsc::unbounded_channel();
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model.clone(), view_commands.clone(), cx))
        })
        .expect("a window")
    });
    cx.simulate_window_resize(window.into(), gpui::size(px(1000.), px(700.)));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, _| window.activate_window());
    Rig {
        cx: visual,
        commands: received,
        _keep: (runtime, events, commands),
    }
}

/// Type a new name, leave the input, and read the calls.
fn typed_then(cx: &mut TestAppContext, kind: &Kind, how: Leave) -> Vec<Command> {
    let mut rig = start(cx, kind);
    rig.type_name(kind, "Pump");
    assert!(rig.commands.try_recv().is_err(), "typing sends no call");
    rig.leave(kind, how);
    let sent = rig.sent();
    cx.update(|cx| cx.quit());
    sent
}

fn commits_on_button(cx: &mut TestAppContext, kind: &Kind) {
    assert_eq!(
        typed_then(cx, kind, Leave::Button),
        vec![(kind.command)("Pump")]
    );
}

#[gpui::test]
fn a_channel_name_commits_on_a_click_on_a_button(cx: &mut TestAppContext) {
    commits_on_button(cx, &CHANNEL);
}

#[gpui::test]
fn a_sensor_name_commits_on_a_click_on_a_button(cx: &mut TestAppContext) {
    commits_on_button(cx, &SENSOR);
}

#[gpui::test]
fn a_loop_name_commits_on_a_click_on_a_button(cx: &mut TestAppContext) {
    commits_on_button(cx, &LOOP);
}

#[gpui::test]
fn a_profile_name_commits_on_a_click_on_a_button(cx: &mut TestAppContext) {
    commits_on_button(cx, &PROFILE);
}

#[gpui::test]
fn every_rename_input_commits_when_it_loses_the_focus(cx: &mut TestAppContext) {
    for kind in KINDS {
        assert_eq!(
            typed_then(cx, kind, Leave::Focus),
            vec![(kind.command)("Pump")]
        );
    }
}

#[gpui::test]
fn enter_commits_a_rename_input(cx: &mut TestAppContext) {
    for kind in KINDS {
        assert_eq!(
            typed_then(cx, kind, Leave::Enter),
            vec![(kind.command)("Pump")]
        );
    }
}

#[gpui::test]
fn escape_keeps_the_old_name(cx: &mut TestAppContext) {
    for kind in KINDS {
        let mut rig = start(cx, kind);
        rig.type_name(kind, "Pump");
        rig.leave(kind, Leave::Escape);
        // Neither Enter nor a button brings the text back.
        rig.leave(kind, Leave::Enter);
        rig.leave(kind, Leave::Button);
        assert_eq!(rig.sent(), Vec::<Command>::new());
        cx.update(|cx| cx.quit());
    }
}

#[gpui::test]
fn a_rename_commits_once(cx: &mut TestAppContext) {
    for kind in KINDS {
        let mut rig = start(cx, kind);
        rig.type_name(kind, "Pump");
        rig.leave(kind, Leave::Button);
        rig.leave(kind, Leave::Focus);
        rig.leave(kind, Leave::Enter);
        assert_eq!(rig.sent(), vec![(kind.command)("Pump")]);
        cx.update(|cx| cx.quit());
    }
}

#[gpui::test]
fn an_unchanged_or_empty_name_sends_no_call(cx: &mut TestAppContext) {
    for kind in [&CHANNEL, &LOOP, &PROFILE] {
        for text in [kind.name, "  "] {
            let mut rig = start(cx, kind);
            rig.type_name(kind, text);
            rig.leave(kind, Leave::Button);
            assert_eq!(rig.sent(), Vec::<Command>::new(), "{text:?}");
            cx.update(|cx| cx.quit());
        }
    }
}
