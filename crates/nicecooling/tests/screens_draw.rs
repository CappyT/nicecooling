//! The screens draw in a headless window, in each state of the daemon link. This catches a
//! panic of a view: the models have their own tests.

mod support;

use gpui::{TestAppContext, VisualTestContext};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::history::{HistoryDb, HistoryWriter};
use nicecooling::i18n::Language;
use nicecooling::model::{AlarmDetails, DaemonEvent, Link, Response};
use nicecooling::shell::Page;
use support::fixtures::{LEAKSHIELD, NOW_US, alarm, full};
use tokio::sync::{broadcast, mpsc};

struct Rig {
    events: broadcast::Sender<DaemonEvent>,
    _commands: mpsc::UnboundedSender<nicecooling::command::Command>,
    _runtime: tokio::runtime::Runtime,
    dir: std::path::PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn start(cx: &mut TestAppContext, language: Language) -> (gpui::Entity<AppModel>, Rig) {
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
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    // The history of the test: two sensors, with a gap.
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("nicecooling-draw-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directory");
    let path = dir.join("history.db");
    let now = (app::now_us() / 1_000_000) as i64;
    let mut writer = HistoryWriter::new(HistoryDb::open(&path).expect("database"));
    for t in now - 600..now {
        if !(now - 300..now - 240).contains(&t) {
            writer.record(t, "nct6798_temp1", "degC", Some(30.0 + (t % 7) as f64));
        }
        writer.record(t, "nct6798_pwm6", "%", Some(60.0));
        writer.tick(t).expect("tick");
    }
    writer.tick(now + 20).expect("flush");
    drop(writer);
    cx.update(|cx| {
        model.update(cx, |app, _| app.history_path = Some(path.clone()));
    });
    cx.run_until_parked();
    (
        model,
        Rig {
            events,
            _commands: commands,
            _runtime: runtime,
            dir,
        },
    )
}

fn draw(cx: &mut TestAppContext) {
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
}

fn show(cx: &mut TestAppContext, model: &gpui::Entity<AppModel>, page: Page) {
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = page;
            cx.notify();
        });
    });
    cx.run_until_parked();
    draw(cx);
    cx.run_until_parked();
    draw(cx);
}

const PAGES: [Page; 9] = [
    Page::Overview,
    Page::Sensors,
    Page::Channels,
    Page::Loops,
    Page::History,
    Page::Profiles,
    Page::Alarms,
    Page::Settings,
    Page::FirstRun,
];

#[gpui::test]
fn every_screen_draws_online_in_english_and_italian(cx: &mut TestAppContext) {
    for language in [Language::English, Language::Italian] {
        let (model, rig) = start(cx, language);
        let state = full();
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
            .unwrap();
        rig.events
            .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
            .unwrap();
        cx.run_until_parked();
        for page in PAGES {
            show(cx, &model, page);
        }
        cx.update(|cx| cx.quit());
    }
}

#[gpui::test]
fn every_screen_draws_with_an_alarm_and_after_the_daemon_goes_away(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, Language::English);
    let state = full();
    let mut snapshot = state.snapshot.clone();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.details = AlarmDetails {
        value: Some(212.0),
        limit: Some(250.0),
        unit: "mbar".into(),
        sensor: Some(format!("{LEAKSHIELD}_pressure")),
        ..Default::default()
    };
    leak.raised_us = NOW_US;
    let pump = alarm("a2", "pump-stop", "loop_1", Response::Notify, 0);
    snapshot.alarms = vec![leak, pump];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(snapshot)))
        .unwrap();
    rig.events
        .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
        .unwrap();
    cx.run_until_parked();
    for page in PAGES {
        show(cx, &model, page);
    }
    // The daemon leaves: the screens keep the last values, dim.
    for link in [Link::Offline, Link::Down] {
        rig.events.send(DaemonEvent::Link(link)).unwrap();
        cx.run_until_parked();
        for page in PAGES {
            show(cx, &model, page);
        }
    }
}

#[gpui::test]
fn every_screen_draws_without_data(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, Language::English);
    // The start: "Connecting".
    for page in PAGES {
        show(cx, &model, page);
    }
    // An installation with nothing in it.
    rig.events
        .send(DaemonEvent::Snapshot(Box::default()))
        .unwrap();
    cx.run_until_parked();
    for page in PAGES {
        show(cx, &model, page);
    }
}

#[gpui::test]
fn the_history_chart_draws_series_gaps_and_markers(cx: &mut TestAppContext) {
    use gpui::AppContext as _;
    use nicecooling::screen_view::ScreenView;

    let (model, rig) = start(cx, Language::English);
    let state = full();
    let mut snapshot = state.snapshot.clone();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    );
    leak.raised_us = app::now_us() - 120_000_000;
    snapshot.alarms = vec![leak];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(snapshot)))
        .unwrap();
    cx.run_until_parked();
    // A window that holds the screens alone, on the History page.
    let (commands, _inbox) = mpsc::unbounded_channel();
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model.clone(), commands.clone(), cx))
        })
        .expect("a window")
    });
    cx.update_window(window.into(), |_, _, cx| {
        model.update(cx, |app, _| app.page = Page::History);
    })
    .unwrap();
    for sensors in [vec!["nct6798_temp1"], vec!["nct6798_temp1", "nct6798_pwm6"]] {
        window
            .update(cx, |view, _, cx| {
                view.open_history_for(sensors[0], cx);
                for extra in &sensors[1..] {
                    view.toggle_history_sensor(extra);
                }
            })
            .unwrap();
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
}

#[gpui::test]
fn the_profiles_screen_draws_groups_and_every_editor(cx: &mut TestAppContext) {
    use gpui::AppContext as _;
    use nicecooling::model::{Cause, ProfileInfo, Rule, RuleGroup};
    use nicecooling::screen_view::ScreenView;
    use nicecooling::screens::first_run::Step;

    let (model, rig) = start(cx, Language::English);
    let state = full();
    let mut snapshot = state.snapshot.clone();
    snapshot.profiles.push(ProfileInfo {
        id: "profile_3".into(),
        name: "Gaming".into(),
        slot: 2,
        contains_release: true,
        ..Default::default()
    });
    snapshot.rule_delay_s = 30;
    snapshot.dwell_s = 300;
    snapshot.cause = Cause::Rule;
    let rules = vec![
        Rule::Sensor {
            sensor: "nct6798_temp1".into(),
            comparison: "in_range".into(),
            value: 30.0,
            value_max: 40.0,
            hold_s: 60,
        },
        Rule::Process {
            names: vec!["steam".into()],
            match_kind: "any".into(),
            match_on: "name".into(),
        },
        Rule::Time {
            days: vec![1, 2, 3],
            start: 1320,
            end: 420,
        },
        Rule::Interval {
            interval_s: 600,
            delay_s: 60,
            duration_s: 10,
        },
    ];
    snapshot.rule_groups = vec![
        RuleGroup {
            id: "group_1".into(),
            profile: "profile_3".into(),
            enabled: true,
            rules,
            holds: true,
            since_us: app::now_us() - 5_000_000,
        },
        RuleGroup {
            id: "group_2".into(),
            profile: "profile_2".into(),
            enabled: false,
            rules: Vec::new(),
            holds: false,
            since_us: 0,
        },
    ];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(snapshot)))
        .unwrap();
    cx.run_until_parked();
    let (commands, _inbox) = mpsc::unbounded_channel();
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model.clone(), commands.clone(), cx))
        })
        .expect("a window")
    });
    for index in [None, Some(0), Some(1)] {
        window
            .update(cx, |view, _, cx| view.open_group_editor(index, cx))
            .unwrap();
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
    for step in [
        Step::Board,
        Step::Loop,
        Step::Bios,
        Step::Leakshield,
        Step::Done,
    ] {
        window
            .update(cx, |view, _, cx| view.show_first_run_step(step, cx))
            .unwrap();
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
}

#[gpui::test]
fn every_leakshield_tab_draws_in_each_state(cx: &mut TestAppContext) {
    use gpui::AppContext as _;
    use nicecooling::screen_view::ScreenView;
    use nicecooling::screens::leakshield::Tab;
    use nicecooling::screens::leakshield::overview::Confirm;
    use nicecooling::screens::leakshield::settings::Step;
    use nicecooling::screens::leakshield::system::Ask;
    use support::fixtures::with_leakshield;

    for language in [Language::English, Language::Italian] {
        let (model, rig) = start(cx, language);
        let state = with_leakshield();
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
            .unwrap();
        rig.events
            .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
            .unwrap();
        cx.run_until_parked();
        let (commands, _inbox) = mpsc::unbounded_channel();
        let window = cx.update(|cx| {
            cx.open_window(gpui::WindowOptions::default(), |_, cx| {
                cx.new(|cx| ScreenView::new(model.clone(), commands.clone(), cx))
            })
            .expect("a window")
        });
        let draw_window = |cx: &mut TestAppContext| {
            cx.run_until_parked();
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            visual.update(|window, cx| window.draw(cx).clear(cx));
        };
        let show = |cx: &mut TestAppContext, tab: Tab| {
            window
                .update(cx, |view, _, cx| {
                    view.show_leakshield_tab(LEAKSHIELD, tab, cx)
                })
                .unwrap();
            draw_window(cx);
        };
        // Every tab, then the questions and the steps that change their look.
        for tab in Tab::ALL {
            show(cx, tab);
            show(cx, tab);
        }
        show(cx, Tab::Overview);
        for confirm in [
            Confirm::Mode("monitor".into()),
            Confirm::Measure,
            Confirm::Refill,
            Confirm::RefillSteps,
        ] {
            window
                .update(cx, |view, _, _| {
                    view.leakshield_ui().overview.confirm = Some(confirm)
                })
                .unwrap();
            draw_window(cx);
        }
        window
            .update(cx, |view, _, _| {
                let ui = view.leakshield_ui();
                ui.overview.confirm = None;
                ui.overview.test_setup = true;
            })
            .unwrap();
        draw_window(cx);
        show(cx, Tab::Settings);
        for step in Step::ALL {
            window
                .update(cx, |view, _, _| {
                    let ui = view.leakshield_ui();
                    ui.settings.go(step);
                    ui.settings.pending.insert(
                        "pump_max_speed".into(),
                        nicecooling::model::FieldValue::Number(4900.0),
                    );
                    ui.settings.pending.insert(
                        "reservoir_total_volume".into(),
                        nicecooling::model::FieldValue::Number(300.0),
                    );
                    ui.settings.request_discard();
                })
                .unwrap();
            draw_window(cx);
        }
        show(cx, Tab::System);
        for ask in [
            Ask::Restore("backup_1".into()),
            Ask::FactoryReset,
            Ask::RefillHistory,
            Ask::ServiceCounter,
        ] {
            window
                .update(cx, |view, _, _| view.leakshield_ui().system.request(ask))
                .unwrap();
            draw_window(cx);
        }
        show(cx, Tab::Display);
        window
            .update(cx, |view, _, _| {
                let ui = view.leakshield_ui();
                ui.display.select(2);
                ui.display.gallery = true;
            })
            .unwrap();
        draw_window(cx);
        show(cx, Tab::Lighting);
        window
            .update(cx, |view, _, _| {
                let ui = view.leakshield_ui();
                ui.lighting.toggle(1);
                ui.lighting.effect_picker = Some("rainbow".to_owned());
            })
            .unwrap();
        draw_window(cx);
        show(cx, Tab::Sensors);
        window
            .update(cx, |view, _, _| {
                view.leakshield_ui().sensors.clear = Some(3)
            })
            .unwrap();
        draw_window(cx);
        show(cx, Tab::Fields);
        window
            .update(cx, |view, _, _| {
                view.leakshield_ui().fields.select("reservoir_total_volume")
            })
            .unwrap();
        draw_window(cx);

        // An Action runs, an Alarm is active, then the Leakshield leaves.
        let mut running = state.snapshot.clone();
        running.leakshields[0].running_action = "test".into();
        let mut leak = alarm(
            "a1",
            "leakshield.leak-warning",
            LEAKSHIELD,
            Response::Notify,
            0,
        );
        leak.raised_us = NOW_US;
        running.alarms = vec![leak];
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(running)))
            .unwrap();
        cx.run_until_parked();
        for tab in Tab::ALL {
            show(cx, tab);
        }
        for action in ["measure-fill-level", "deaerate"] {
            let mut snapshot = state.snapshot.clone();
            snapshot.leakshields[0].running_action = action.into();
            rig.events
                .send(DaemonEvent::Snapshot(Box::new(snapshot)))
                .unwrap();
            cx.run_until_parked();
            show(cx, Tab::Overview);
            show(cx, Tab::Status);
        }
        let mut gone = state.snapshot.clone();
        gone.leakshields[0].connected = false;
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(gone)))
            .unwrap();
        cx.run_until_parked();
        for tab in Tab::ALL {
            show(cx, tab);
        }
        for link in [Link::Offline, Link::Down] {
            rig.events.send(DaemonEvent::Link(link)).unwrap();
            cx.run_until_parked();
            for tab in Tab::ALL {
                show(cx, tab);
            }
        }
        // A Leakshield with an unknown firmware and one without data.
        let mut unknown = state.snapshot.clone();
        unknown.leakshields[0].read_only_reason = "unknown-firmware".into();
        unknown.leakshield_data.clear();
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(unknown)))
            .unwrap();
        cx.run_until_parked();
        for tab in Tab::ALL {
            show(cx, tab);
        }
        cx.update(|cx| cx.quit());
    }
}

/// The move, length and priority buttons of a controller are the shared icon button with the
/// icons of the app: 28 px square.
#[gpui::test]
fn the_lighting_buttons_are_the_shared_icon_button(cx: &mut TestAppContext) {
    use gpui::AppContext as _;
    use nicecooling::icons::{Assets, NcIcon};
    use nicecooling::screen_view::ScreenView;
    use nicecooling::screens::leakshield::Tab;
    use support::fixtures::with_leakshield;

    for icon in [NcIcon::Up, NcIcon::Minus] {
        let path = gpui_component::IconNamed::path(icon);
        let svg = gpui::AssetSource::load(&Assets, &path).expect("load");
        let svg = String::from_utf8(svg.expect("an asset").into_owned()).expect("text");
        assert!(svg.contains("viewBox=\"0 0 16 16\""), "{path}: {svg}");
        assert!(svg.contains("stroke-width=\"1.5\""), "{path}: {svg}");
    }

    let (model, rig) = start(cx, Language::English);
    let state = with_leakshield();
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let (commands, _inbox) = mpsc::unbounded_channel();
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model.clone(), commands.clone(), cx))
        })
        .expect("a window")
    });
    window
        .update(cx, |view, _, cx| {
            view.show_leakshield_tab(LEAKSHIELD, Tab::Lighting, cx);
            view.leakshield_ui().lighting.selected.insert(0);
        })
        .unwrap();
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    for name in [
        "ls-led-left",
        "ls-led-right",
        "ls-led-shorter",
        "ls-led-longer",
        "ls-led-up",
        "ls-led-down",
    ] {
        let button = visual
            .debug_bounds(name)
            .unwrap_or_else(|| panic!("no button {name}"));
        assert_eq!(
            button.size,
            gpui::size(gpui::px(28.), gpui::px(28.)),
            "{name}"
        );
    }
    let source = include_str!("../src/screens/leakshield/lighting_view.rs");
    assert!(
        !source.contains("IconName") && !source.contains("Button::new("),
        "the Lighting tab draws a raw gpui-component button"
    );
}

/// The "Alarm raised" label of the pressure card stays in the plot. It is at the left of the
/// Alarm line, and at its right when the line is near the left edge of the 10 minutes.
#[gpui::test]
fn the_alarm_time_stays_in_the_pressure_plot(cx: &mut TestAppContext) {
    let (model, rig) = start(cx, Language::English);
    let state = full();
    for (age_s, at_left) in [(60u64, true), (300, true), (540, false), (590, false)] {
        let mut snapshot = state.snapshot.clone();
        let mut leak = alarm(
            "a1",
            "leakshield.leak-alarm",
            LEAKSHIELD,
            Response::Notify,
            0,
        );
        leak.raised_us = app::now_us() - age_s * 1_000_000;
        let leak_raised_us = leak.raised_us;
        snapshot.alarms = vec![leak];
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(snapshot)))
            .unwrap();
        cx.run_until_parked();
        show(cx, &model, Page::Overview);
        let handle = cx.windows()[0];
        let mut visual = VisualTestContext::from_window(handle, cx);
        // The view reads the clock when it draws. A loaded test host can take seconds from the
        // snapshot to the frame, so the Alarm line is between its places at these two times.
        let before_us = app::now_us();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let after_us = app::now_us();
        let plot = visual.debug_bounds("pressure-plot").expect("the plot");
        let label = visual.debug_bounds("pressure-raised").expect("the label");
        assert!(label.size.width > gpui::px(40.), "{age_s} s: {label:?}");
        assert!(
            label.left() >= plot.left() && label.right() <= plot.right(),
            "{age_s} s: the label {label:?} leaves the plot {plot:?}"
        );
        assert!(label.top() >= plot.top() && label.bottom() <= plot.bottom());
        let line_at = |now_us: u64| {
            let age = (now_us - leak_raised_us) as f32 / 1_000_000.0;
            plot.left() + plot.size.width * (1.0 - age / 600.0)
        };
        // The line moves to the left with the time. The view puts the label 8 px from the line
        // and snaps to half pixels, so each bound has 1 px of slack.
        let (rightmost, leftmost) = (line_at(before_us), line_at(after_us));
        let slack = gpui::px(1.);
        if at_left {
            assert!(
                label.right() <= rightmost + slack,
                "{age_s} s: {label:?}, line {rightmost:?}"
            );
            assert!(
                label.right() >= leftmost - gpui::px(8.) - slack,
                "{age_s} s: {label:?}, line {leftmost:?}"
            );
        } else {
            assert!(
                label.left() >= leftmost - slack,
                "{age_s} s: {label:?}, line {leftmost:?}"
            );
            assert!(
                label.left() <= rightmost + gpui::px(8.) + slack,
                "{age_s} s: {label:?}, line {rightmost:?}"
            );
        }
    }
}
