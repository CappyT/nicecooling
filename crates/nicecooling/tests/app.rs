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
    backend_in(Language::English)
}

fn backend_in(language: Language) -> (Backend, Rig) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands: commands.clone(),
        inbox,
        client: None,
        runtime: runtime.handle().clone(),
        language,
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

/// Review: every page, at the minimum window size and at 1280x800, starts under the header
/// bar and ends at the status bar. A page that is longer than that area scrolls in it.
#[gpui::test]
fn review_every_page_starts_under_the_header(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Normal);
    let state = support::fixtures::with_leakshield();
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let handle = cx.windows()[0];
    let pages = [
        Page::Overview,
        Page::Loops,
        Page::Channels,
        Page::Sensors,
        Page::Profiles,
        Page::Alarms,
        Page::History,
        Page::Settings,
        Page::FirstRun,
        Page::Leakshield(LEAKSHIELD.to_owned()),
    ];
    let show = |cx: &mut TestAppContext, page: &Page| {
        cx.update(|cx| {
            model.update(cx, |app, cx| {
                app.page = page.clone();
                cx.notify();
            });
        });
        cx.run_until_parked();
    };
    for size in [(1100., 700.), (1280., 800.)] {
        cx.simulate_window_resize(handle, gpui::size(gpui::px(size.0), gpui::px(size.1)));
        for page in &pages {
            show(cx, page);
            let mut visual = VisualTestContext::from_window(handle, cx);
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let mut bounds = |name: &'static str| {
                visual
                    .debug_bounds(name)
                    .unwrap_or_else(|| panic!("{page:?}: no element {name}"))
            };
            let (header, content) = (bounds("shell-header"), bounds("shell-content"));
            let (status, scroll) = (bounds("shell-status"), bounds("screen-scroll"));
            assert_eq!(content.top(), header.bottom(), "{page:?} {size:?}: top");
            assert_eq!(content.bottom(), status.top(), "{page:?} {size:?}: bottom");
            assert!(scroll.top() >= content.top(), "{page:?} {size:?}");
            assert_eq!(scroll.bottom(), content.bottom(), "{page:?} {size:?}");
            assert_eq!(header.size.height, gpui::px(46.), "{page:?}: header height");
            assert_eq!(status.size.height, gpui::px(28.), "{page:?}: status height");
            // The page starts at the top of the scroll area: nothing centers it.
            assert_eq!(
                bounds("screen-page").top(),
                scroll.top(),
                "{page:?} {size:?}"
            );
            // The tab strip of a Leakshield stays under the header: only the tab scrolls.
            if matches!(page, Page::Leakshield(_)) {
                let tabs = bounds("leakshield-tabs");
                assert_eq!(tabs.top(), content.top(), "top of the tab strip");
                assert_eq!(scroll.top(), tabs.bottom(), "top of the tab");
            }
        }
    }

    // A page that is longer than its area moves with the wheel.
    cx.simulate_window_resize(handle, gpui::size(gpui::px(1100.), gpui::px(700.)));
    let mut scrolled = Vec::new();
    for page in &pages {
        show(cx, page);
        let mut visual = VisualTestContext::from_window(handle, cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let scroll = visual.debug_bounds("screen-scroll").expect("scroll area");
        let before = visual.debug_bounds("screen-page").expect("page");
        if before.size.height <= scroll.size.height {
            continue;
        }
        visual.simulate_event(gpui::ScrollEvent {
            position: scroll.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-40.))),
            ..Default::default()
        });
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let after = visual.debug_bounds("screen-page").expect("page");
        // A page that is less than 40 px longer than its area stops at its end.
        let step = (before.size.height - scroll.size.height).min(gpui::px(40.));
        assert_eq!(
            after.top(),
            before.top() - step,
            "{page:?}: the page scrolls"
        );
        scrolled.push(page.clone());
    }
    assert!(
        scrolled.contains(&Page::Loops) && scrolled.contains(&Page::Settings),
        "long pages: {scrolled:?}"
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

/// Every icon of the app loads from the asset source as an SVG, and the icons that the
/// gpui-component controls draw load from it too.
#[test]
fn the_asset_source_has_every_icon() {
    use gpui::AssetSource as _;
    use gpui_component::IconNamed as _;
    use nicecooling::icons::{Assets, NcIcon};

    for icon in NcIcon::ALL {
        let path = icon.path();
        let svg = Assets
            .load(&path)
            .expect("load")
            .unwrap_or_else(|| panic!("no asset {path}"));
        let text = std::str::from_utf8(&svg).expect("text");
        assert!(text.starts_with("<svg ") && text.contains("viewBox=\"0 0 16 16\""));
    }
    for path in [
        "icons/loader.svg",
        "icons/window-close.svg",
        "icons/check.svg",
    ] {
        assert!(
            Assets.load(path).expect("load").is_some(),
            "no asset {path}"
        );
    }
    assert!(Assets.list("nc/").expect("list").len() == NcIcon::ALL.len());
}

/// At the minimum window size a long event name of the alarm response table truncates in its
/// cell: it does not reach the Source cell.
#[gpui::test]
fn a_long_event_name_stays_in_its_cell(cx: &mut TestAppContext) {
    use nicecooling::screens::alarms::{AlarmsModel, AlarmsUi};
    use nicecooling::screens::{Feedback, ScreenInput};

    let (model, rig) = start(cx, StartMode::Normal);
    let mut state = support::fixtures::with_leakshield();
    state.snapshot.responses = ["leakshield.leak-warning", "leakshield-unknown-firmware"]
        .into_iter()
        .map(|event| nicecooling::model::ResponseRule {
            event: event.to_owned(),
            source: String::new(),
            response: Response::Notify,
            delay_s: 30,
            overridden: false,
        })
        .collect();
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = Page::Alarms;
            cx.notify();
        });
    });
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));

    let feedback = Feedback::default();
    let rows = AlarmsModel::build(
        &ScreenInput {
            state: &state,
            now_us: support::fixtures::NOW_US,
            locale: nicecooling::i18n::EN,
            down_since_us: None,
            feedback: &feedback,
        },
        &AlarmsUi::default(),
    );
    let mut checked = 0;
    for row in rows.responses.groups.iter().flat_map(|group| &group.rows) {
        let name = |part: &str| -> &'static str {
            Box::leak(format!("response-{part}-{}", row.key).into_boxed_str())
        };
        let cells = (
            visual.debug_bounds(name("title")),
            visual.debug_bounds(name("source")),
        );
        if let (Some(title), Some(source)) = cells {
            assert!(
                title.right() <= source.left(),
                "{}: the name ends at {:?}, the Source cell starts at {:?}",
                row.title,
                title.right(),
                source.left()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 2, "the rows of the alarm response table");
    // The Event cell keeps room for a name of `docs/ui/screens/alarms.md`.
    let event = visual
        .debug_bounds("response-event-cell")
        .expect("event cell");
    assert!(
        event.size.width >= gpui::px(160.),
        "Event cell is {:?} wide at 1100x700",
        event.size.width
    );
    let titles: Vec<_> = rows
        .responses
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .map(|row| row.title.as_str())
        .collect();
    assert!(
        titles.contains(&"Leakshield firmware not covered"),
        "{titles:?}"
    );
}

/// Review: at the minimum window size the Event cell of "Active Alarms" keeps room for the
/// name of the Alarm, also in the row of a pending poweroff with "Cancel poweroff".
#[gpui::test]
fn review_active_event_cell_has_room_at_minimum_size(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Normal);
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
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = Page::Alarms;
            cx.notify();
        });
    });
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let cell = visual
        .debug_bounds("active-event-cell")
        .expect("event cell");
    let content = visual.debug_bounds("shell-content").expect("content");
    assert!(
        cell.size.width >= gpui::px(120.),
        "Event cell is {:?} wide at 1100x700",
        cell.size.width
    );
    // The row ends in the card: 24 px page padding at the right of the content.
    let cancel = visual
        .debug_bounds("active-cancel")
        .expect("Cancel poweroff");
    assert!(
        cancel.right() <= content.right() - gpui::px(24.),
        "Cancel poweroff ends at {:?}, the card at {:?}",
        cancel.right(),
        content.right() - gpui::px(24.)
    );
}

/// At the minimum window size the Event cell of the Alarm history keeps room for the name.
#[gpui::test]
fn history_event_cell_has_room_at_minimum_size(cx: &mut TestAppContext) {
    use nicecooling::model::{AlarmDetails, AlarmRecord, Outcome, Severity};

    let (model, rig) = start(cx, StartMode::Normal);
    let mut state = online();
    state.snapshot.alarm_history = vec![AlarmRecord {
        instance: "a1".into(),
        event: "pump-stop".into(),
        severity: Severity::Alarm,
        source: LEAKSHIELD.into(),
        response: Response::Poweroff,
        raised_us: support::fixtures::NOW_US,
        cleared_us: Some(support::fixtures::NOW_US + 30_000_000),
        outcome: Outcome::PoweroffCancelled,
        closed_by_restart: true,
        details: AlarmDetails::default(),
    }];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = Page::Alarms;
            cx.notify();
        });
    });
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let cell = visual
        .debug_bounds("history-event-cell")
        .expect("event cell");
    assert!(
        cell.size.width >= gpui::px(160.),
        "Event cell is {:?} wide at 1100x700",
        cell.size.width
    );
}

/// The write feedback of an alarm response row stays in its cell and wraps there. The Italian
/// failure line is the longest one.
#[gpui::test]
fn a_failed_response_write_wraps_in_its_cell(cx: &mut TestAppContext) {
    use nicecooling::command::response_feedback_key;
    use nicecooling::model::ResponseRule;
    use nicecooling::screens::WriteOutcome;

    let (backend, rig) = backend_in(Language::Italian);
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    cx.run_until_parked();
    let mut state = online();
    // An overridden rule shows "Reset" next to the feedback.
    state.snapshot.responses = vec![ResponseRule {
        event: "leakshield.leak-alarm".into(),
        source: String::new(),
        response: Response::Poweroff,
        delay_s: 30,
        overridden: true,
    }];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = Page::Alarms;
            app.feedback.set(
                response_feedback_key("leakshield.leak-alarm", ""),
                WriteOutcome::Refused {
                    error: "InvalidArgument".into(),
                },
            );
            cx.notify();
        });
    });
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let text = visual.debug_bounds("write-line-text").expect("feedback");
    let notes = visual.debug_bounds("response-notes").expect("notes cell");
    let content = visual.debug_bounds("shell-content").expect("content");
    assert!(
        text.left() >= notes.left() && text.right() <= notes.right(),
        "the feedback is at {:?} to {:?}, its cell at {:?} to {:?}",
        text.left(),
        text.right(),
        notes.left(),
        notes.right()
    );
    assert!(notes.right() <= content.right() - gpui::px(24.));
    assert!(
        text.size.height > gpui::px(16.),
        "the feedback wraps: {:?}",
        text.size.height
    );
}

/// The header bar has the page title in its center, with the serial under it on a device
/// page. The test window has server-side decorations: the bar then has no close button,
/// because the window manager draws one.
#[gpui::test]
fn the_header_bar_centers_the_title(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Normal);
    let state = support::fixtures::with_leakshield();
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let handle = cx.windows()[0];
    let mut heights = Vec::new();
    for page in [Page::Overview, Page::Leakshield(LEAKSHIELD.to_owned())] {
        cx.update(|cx| {
            model.update(cx, |app, cx| {
                app.page = page.clone();
                cx.notify();
            });
        });
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(handle, cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let header = visual.debug_bounds("shell-header").expect("header bar");
        let title = visual.debug_bounds("header-title").expect("title");
        let offset = title.center().x - header.center().x;
        assert!(
            offset.abs() <= gpui::px(1.),
            "{page:?}: the title is {offset:?} off the center"
        );
        assert!(title.top() >= header.top() && title.bottom() <= header.bottom());
        assert!(visual.debug_bounds("header-close").is_none());
        heights.push(title.size.height);
    }
    // A device page has two lines: the label and the serial.
    assert!(heights[1] > heights[0], "{heights:?}");
}

/// A popover closes on its control, on a press outside it and on Escape.
#[gpui::test]
fn a_popover_closes_on_a_press_outside_and_on_escape(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, StartMode::Normal);
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(online().snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = Page::Settings;
            cx.notify();
        });
    });
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    let bounds = |visual: &mut VisualTestContext, name: &'static str| {
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        visual.debug_bounds(name)
    };
    let (select, list) = ("select-settings-language", "select-list-settings-language");
    let control = bounds(&mut visual, select).expect("the select").center();
    let press = |visual: &mut VisualTestContext, position| {
        visual.simulate_click(position, gpui::Modifiers::none());
    };
    assert!(bounds(&mut visual, list).is_none(), "closed at the start");

    // The control opens its popover and closes it.
    press(&mut visual, control);
    let open = bounds(&mut visual, list).expect("the list of choices");
    press(&mut visual, control);
    assert!(bounds(&mut visual, list).is_none(), "the control closes it");

    // A press outside the popover closes it.
    press(&mut visual, control);
    assert!(bounds(&mut visual, list).is_some());
    let outside = gpui::point(open.left() - gpui::px(40.), open.bottom() + gpui::px(40.));
    press(&mut visual, outside);
    assert!(bounds(&mut visual, list).is_none(), "a press outside");

    // Escape closes it.
    press(&mut visual, control);
    assert!(bounds(&mut visual, list).is_some());
    visual.simulate_keystrokes("escape");
    assert!(bounds(&mut visual, list).is_none(), "Escape");
}

/// A dialog has the height of its content: it does not reach the bottom of the window.
#[gpui::test]
fn a_dialog_keeps_the_height_of_its_content(cx: &mut TestAppContext) {
    use gpui::prelude::*;
    use nicecooling::screens::ui::modal;
    use nicecooling::tokens::Palette;

    struct Screen;
    impl gpui::Render for Screen {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            gpui::div().relative().size_full().child(modal(
                "Reset the counter?",
                "This cannot be undone.",
                Vec::new(),
                Vec::new(),
                &Palette::new(false),
            ))
        }
    }
    let window = cx.add_window(|_, _| Screen);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let dialog = visual.debug_bounds("modal-dialog").expect("the dialog");
    assert_eq!(dialog.size.width, gpui::px(440.));
    assert!(
        dialog.size.height < gpui::px(160.),
        "the dialog is {:?} high",
        dialog.size.height
    );
}

/// A press on the scrim of a dialog does not reach the control under it.
#[gpui::test]
fn a_dialog_scrim_blocks_the_controls_under_it(cx: &mut TestAppContext) {
    use gpui::prelude::*;
    use nicecooling::screens::ui::modal;
    use nicecooling::tokens::Palette;

    struct Screen {
        pressed: usize,
    }
    impl gpui::Render for Screen {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            gpui::div()
                .relative()
                .size_full()
                .child(
                    gpui::div()
                        .id("under")
                        .debug_selector(|| "under".to_owned())
                        .w(gpui::px(200.))
                        .h(gpui::px(60.))
                        .on_click(cx.listener(|this, _, _, _| this.pressed += 1)),
                )
                .child(modal(
                    "Use the Hardware curve?",
                    "The Board runs its Hardware curve.",
                    Vec::new(),
                    Vec::new(),
                    &Palette::new(false),
                ))
        }
    }
    let window = cx.add_window(|_, _| Screen { pressed: 0 });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let under = visual.debug_bounds("under").expect("the control");
    visual.simulate_click(under.center(), gpui::Modifiers::none());
    visual.run_until_parked();
    let pressed = window.update(cx, |screen, _, _| screen.pressed).unwrap();
    assert_eq!(pressed, 0, "the press went through the scrim");
}

/// The scrim of a dialog covers the whole window: a control outside the page, as the sidebar
/// and the header bar are, takes no press while the dialog is open.
#[gpui::test]
fn a_dialog_scrim_covers_the_whole_window(cx: &mut TestAppContext) {
    use gpui::prelude::*;
    use nicecooling::screens::ui::modal;
    use nicecooling::tokens::Palette;

    struct Screen {
        pressed: usize,
    }
    impl gpui::Render for Screen {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            // The sidebar, then a page that clips and scrolls its content.
            gpui::div()
                .flex()
                .size_full()
                .child(
                    gpui::div()
                        .id("sidebar")
                        .debug_selector(|| "sidebar".to_owned())
                        .w(gpui::px(220.))
                        .h_full()
                        .flex_none()
                        .on_click(cx.listener(|this, _, _, _| this.pressed += 1)),
                )
                .child(
                    gpui::div()
                        .id("page")
                        .flex_1()
                        .mt(gpui::px(46.))
                        .overflow_y_scroll()
                        .child(
                            gpui::div()
                                .relative()
                                .w_full()
                                .h(gpui::px(300.))
                                .child(modal(
                                    "Reset the counter?",
                                    "This cannot be undone.",
                                    Vec::new(),
                                    Vec::new(),
                                    &Palette::new(false),
                                )),
                        ),
                )
        }
    }
    let window = cx.add_window(|_, _| Screen { pressed: 0 });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let scrim = visual.debug_bounds("modal-scrim").expect("the scrim");
    assert_eq!(scrim.origin, gpui::point(gpui::px(0.), gpui::px(0.)));
    assert_eq!(scrim.size, gpui::size(gpui::px(1100.), gpui::px(700.)));
    // The dialog is in the middle of the window, not of the page.
    let dialog = visual.debug_bounds("modal-dialog").expect("the dialog");
    assert_eq!(dialog.center().x, gpui::px(550.));
    let sidebar = visual.debug_bounds("sidebar").expect("the sidebar");
    visual.simulate_click(sidebar.center(), gpui::Modifiers::none());
    visual.run_until_parked();
    let pressed = window.update(cx, |screen, _, _| screen.pressed).unwrap();
    assert_eq!(pressed, 0, "the sidebar took a press under the dialog");
}

/// Loops and First run draw the shared widgets: the buttons of the design, and the sensor
/// picker as a select with its list in a popover.
#[gpui::test]
fn loops_and_first_run_use_the_shared_widgets(cx: &mut TestAppContext) {
    for file in ["screens/loops_view.rs", "screens/first_run_view.rs"] {
        let sources = support::labels::sources();
        let (_, text) = sources.iter().find(|(name, _)| name == file).expect(file);
        for raw in ["Button::new(", "Input::new("] {
            assert!(!text.contains(raw), "{file} draws a raw `{raw}`");
        }
    }

    let (model, rig) = start(cx, StartMode::Normal);
    let state = support::fixtures::full();
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(gpui::px(1100.), gpui::px(700.)));
    let show = |cx: &mut TestAppContext, page: Page| {
        cx.update(|cx| {
            model.update(cx, |app, cx| {
                app.page = page;
                cx.notify();
            });
        });
    };
    let mut bounds = |cx: &mut TestAppContext, name: &'static str| {
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(handle, cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        visual.debug_bounds(name)
    };

    show(cx, Page::Loops);
    let new_loop = bounds(cx, "loop-new").expect("the new Loop button");
    assert_eq!(new_loop.size.height, gpui::px(28.), "a small button");
    let select = bounds(cx, "select-part-pumps").expect("the Pump channel select");
    assert_eq!(
        select.size,
        gpui::size(gpui::px(280.), gpui::px(34.)),
        "the select of the design"
    );
    let next = bounds(cx, "select-part-flow").expect("the flow sensor select");
    assert!(bounds(cx, "picker-part-pumps").is_none(), "closed");

    // The list opens in a popover: it ends at the right edge of the select, stays in the
    // window and does not move the row under it.
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.simulate_click(select.center(), gpui::Modifiers::none());
    let list = bounds(cx, "picker-part-pumps").expect("the list of the picker");
    assert_eq!(list.size.width, gpui::px(360.));
    assert_eq!(list.right(), select.right());
    assert!(list.top() >= select.bottom() && list.right() <= gpui::px(1100.));
    assert_eq!(bounds(cx, "select-part-flow"), Some(next));
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.simulate_click(select.center(), gpui::Modifiers::none());
    assert!(
        bounds(cx, "picker-part-pumps").is_none(),
        "the select closes it"
    );

    show(cx, Page::FirstRun);
    for name in ["first-run-skip", "first-run-next"] {
        let button = bounds(cx, name).unwrap_or_else(|| panic!("no button {name}"));
        assert_eq!(button.size.height, gpui::px(34.), "{name}");
    }
}

/// A tooltip draws outside the shell root, so it sets the tabular figures itself. No screen
/// builds a tooltip another way: a gpui-component tooltip has no font features.
#[test]
fn every_tooltip_has_tabular_figures() {
    use gpui::Styled as _;

    let mut tooltip = nicecooling::screens::ui::tooltip("2 840 rpm");
    assert_eq!(
        tooltip.style().text.font_features,
        Some(nicecooling::tokens::font_features())
    );
    for (file, text) in support::labels::sources() {
        if file == "screens/ui.rs" {
            continue;
        }
        for call in [".tooltip(", "Tooltip::new("] {
            assert!(
                !text.contains(call),
                "{file} builds a tooltip with `{call}`: use `Tip::tip`"
            );
        }
    }
}
