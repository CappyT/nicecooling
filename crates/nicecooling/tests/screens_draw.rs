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
