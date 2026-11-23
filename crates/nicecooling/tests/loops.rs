mod support;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AppState, FeedState, LastFeed, Link, Response};
use nicecooling::screens::loops::{LoopsModel, LoopsState, LoopsUi};
use nicecooling::screens::{Feedback, ScreenInput, WriteOutcome};
use nicecooling::widgets::calibration_editor::{CalibrationEditorModel, CalibrationUi};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fixtures::{NOW_US, alarm, full};
use support::harness::Harness;
use support::labels;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US + 2_000_000,
        locale: EN,
        down_since_us: None,
        feedback,
    }
}

fn model(state: &AppState, ui: &LoopsUi) -> LoopsModel {
    LoopsModel::build(&input(state, &Feedback::default()), ui)
}

#[test]
fn loops_states() {
    let ui = LoopsUi::default();

    let mut state = full();
    state.link = Link::Connecting;
    assert_eq!(model(&state, &ui).state, LoopsState::Loading);

    let mut state = full();
    state.snapshot.loops.clear();
    let empty = model(&state, &ui);
    assert_eq!(empty.state, LoopsState::Empty);
    assert_eq!(empty.empty_title, "No Loop yet");
    assert_eq!(empty.empty_button, "Set up a Loop");
    assert!(empty.detail.is_none());

    // The list and the detail of the first Loop.
    let state = full();
    let ready = model(&state, &ui);
    assert_eq!(ready.state, LoopsState::Ready);
    assert_eq!(ready.rows[0].caption, "Leakshield, 1 Pump channel");
    let detail = ready.detail.as_ref().unwrap();
    assert_eq!(detail.name, "Main loop");
    assert!(detail.editable);
    assert!(detail.alarm.is_none());
    assert_eq!(detail.pumps.shown, "Waterpump");
    assert!(
        detail
            .pumps
            .choices
            .iter()
            .find(|c| c.id == "nct6798_pwm6")
            .unwrap()
            .selected
    );
    assert_eq!(detail.flow_sensor.shown, "Water flow");
    assert_eq!(
        detail
            .flow_sensor
            .choices
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["", "asusec_fan1", "nct6798_fan6"],
        "None, then the sensors with a pulse rate"
    );
    assert_eq!(detail.water.shown, "Water temperature");
    assert_eq!(detail.leakshield.shown, "Leakshield 20417-31072");
    assert_eq!(detail.floor.value, 40.0);
    assert!(detail.floor.warning.is_none());
    assert!(
        detail
            .diagram
            .bubble(nicecooling::widgets::loop_diagram::Tag::Pt)
            .is_some()
    );
    let feed = detail.feed.as_ref().unwrap();
    assert_eq!(feed.feed_pump, "Pump speed 2840 rpm");
    assert_eq!(feed.flow, "Flow 115 L/h");
    assert_eq!(feed.sent, "Sent 2 s ago");
    assert!(!feed.stale);

    // The Calibration table card, with the live marker.
    let editor = detail.calibration.as_ref().unwrap();
    assert_eq!(editor.rows.len(), 4);
    assert!(editor.preset);
    let marker = editor.plot.marker.as_ref().unwrap();
    assert_eq!(marker.label, "671 rpm, 114 L/h");
    assert!(!marker.out_of_range && editor.out_of_range.is_none());

    // A Loop without Leakshield: no Feed card, and the text of the row.
    let mut bare = full();
    bare.snapshot.loops[0].leakshield = None;
    let bare = model(&bare, &ui);
    let detail = bare.detail.unwrap();
    assert!(detail.feed.is_none());
    assert_eq!(
        detail.leakshield.note.as_deref(),
        Some("None. Pressure and fill level are not measured, and no Feed is sent.")
    );

    // No Pump channel, and a Pump floor of 0 %.
    let mut weak = full();
    weak.snapshot.loops[0].pump_channels.clear();
    weak.snapshot.loops[0].pump_floor = 0.0;
    let detail = model(&weak, &ui).detail.unwrap();
    assert_eq!(
        detail.pumps.warning.as_deref(),
        Some("A Loop needs a Pump channel. Without one, nicecooling cannot see a pump stop.")
    );
    assert_eq!(
        detail.floor.warning.as_deref(),
        Some("With a Pump floor of 0 %, a Curve can stop the pump.")
    );

    // A flow above the table is an estimate.
    let mut late = full();
    late.sensors.as_mut().unwrap().sensors[3].1 = 800.0;
    let detail = model(&late, &ui).detail.unwrap();
    let editor = detail.calibration.unwrap();
    assert!(editor.plot.marker.as_ref().unwrap().out_of_range);
    assert_eq!(
        editor.out_of_range.as_deref(),
        Some("800 rpm is above the last point. The flow is an estimate.")
    );

    // A pump-stop Alarm: a red row at the top of the detail.
    let mut stopped = full();
    stopped.snapshot.alarms = vec![alarm("a1", "pump-stop", "loop_1", Response::Poweroff, 30)];
    stopped.sensors.as_mut().unwrap().sensors[2].1 = 0.0;
    assert_eq!(
        model(&stopped, &ui).detail.unwrap().alarm.as_deref(),
        Some("Pump stopped: Waterpump reads 0 rpm. Every Channel is at 100 %.")
    );

    // Daemon offline: the pickers are disabled with the reason.
    let mut offline = full();
    offline.link = Link::Offline;
    offline.sensors = None;
    let detail = model(&offline, &ui).detail.unwrap();
    assert!(!detail.editable);
    assert_eq!(
        detail.disabled_reason.as_deref(),
        Some("The daemon is offline.")
    );
    assert!(!detail.calibration.unwrap().editable);

    // A Leakshield that another Loop uses is not a choice.
    let mut two = full();
    two.snapshot.loops.push(nicecooling::model::LoopInfo {
        id: "loop_2".into(),
        name: "Second".into(),
        ..Default::default()
    });
    let ids = |detail: &nicecooling::screens::loops::LoopDetail| {
        detail
            .leakshield
            .choices
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    };
    let mut second = LoopsUi::default();
    second.select("loop_2");
    assert_eq!(
        ids(&model(&two, &second).detail.unwrap()),
        [""],
        "loop_1 uses the device"
    );
    let mut first = LoopsUi::default();
    first.select("loop_1");
    assert_eq!(
        ids(&model(&two, &first).detail.unwrap()),
        ["", support::fixtures::LEAKSHIELD]
    );

    // The delete dialog.
    let deleting = LoopsUi {
        delete_dialog: true,
        ..Default::default()
    };
    let dialog = model(&full(), &deleting)
        .detail
        .unwrap()
        .delete_dialog
        .unwrap();
    assert_eq!(dialog.title, "Delete the Loop 'Main loop'?");
    assert_eq!(
        dialog.text,
        "Its Channels keep their Curves. The Pump floor and the pump-stop Alarm stop."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Delete Loop")
    );
}

#[test]
fn a_change_of_a_part_is_one_command() {
    let state = full();
    let ui = LoopsUi::default();
    assert_eq!(
        ui.toggle_pump_channel(&state, "nct6798_pwm2"),
        Some(Command::SetPumpChannels {
            loop_id: "loop_1".into(),
            channels: vec!["nct6798_pwm6".into(), "nct6798_pwm2".into()],
        })
    );
    assert_eq!(
        ui.set_flow_sensor(&state, None),
        Some(Command::SetFlowSensor {
            loop_id: "loop_1".into(),
            sensor: String::new(),
            table: "alphacool_17558".into(),
        })
    );
    assert_eq!(
        ui.set_pump_floor(&state, 120.4),
        Some(Command::SetPumpFloor {
            loop_id: "loop_1".into(),
            duty: 100.0
        })
    );
    let feedback = Feedback::default();
    assert_eq!(
        ui.new_loop(&input(&state, &feedback)),
        Command::CreateLoop {
            name: "Loop 2".into()
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn calibration_table_edit_and_preset_load() {
    let bus = PrivateBus::start("loops-calibration");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the tables are known", |state| {
            !state.snapshot.calibration_tables.is_empty()
                && state
                    .snapshot
                    .loops
                    .iter()
                    .any(|l| l.calibration_table == "alphacool_17558")
        })
        .await;
    let feedback = Feedback::default();
    let mut ui = CalibrationUi::default();

    // The Loop reads the preset. A change goes into a new user table that the Loop uses.
    let command = {
        let input = input(&harness.state, &feedback);
        ui.set_row(&input, "loop_1", 1, 369.0, 55.0)
            .expect("a valid edit")
    };
    match &command {
        Command::SaveCalibration { preset, points, .. } => {
            assert!(*preset);
            assert_eq!(points[1], (369.0, 55.0));
        }
        other => panic!("{other:?}"),
    }
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    let calls = daemon.calls();
    assert!(
        calls[0].starts_with(
            "SetCalibrationTable(,Main loop flow sensor,[(327.0, 40.0), (369.0, 55.0)"
        ),
        "{calls:?}"
    );
    assert!(
        calls[1].starts_with("SetFlowSensor(loop_1,asusec_fan1,table_"),
        "{calls:?}"
    );
    harness
        .until("the Loop uses the user table", |state| {
            state.snapshot.loops[0]
                .calibration_table
                .starts_with("table_")
        })
        .await;
    let user_table = harness.state.snapshot.loops[0].calibration_table.clone();
    let table = harness
        .state
        .snapshot
        .calibration_tables
        .iter()
        .find(|t| t.id == user_table)
        .expect("the user table");
    assert!(!table.preset);
    assert_eq!(table.points[1], (369.0, 55.0));
    ui.sync(&harness.state, "loop_1");
    assert!(ui.draft.is_none());

    // A second edit goes to the same table. Rows sort by raw rate.
    let command = {
        let input = input(&harness.state, &feedback);
        ui.set_row(&input, "loop_1", 3, 380.0, 58.0)
            .expect("a valid edit")
    };
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    let last = daemon.calls().last().cloned().unwrap();
    assert!(
        last.starts_with(&format!("SetCalibrationTable({user_table},")),
        "{last}"
    );
    assert!(
        last.contains("(369.0, 55.0), (380.0, 58.0), (404.0, 60.0)"),
        "{last}"
    );
    assert_eq!(
        ui.selected,
        Some(2),
        "the edited row moves with its raw rate"
    );

    // A table that is not valid is not sent.
    let before = daemon.calls().len();
    {
        let input = input(&harness.state, &feedback);
        assert_eq!(ui.set_row(&input, "loop_1", 1, 327.0, 40.0), None);
        let editor =
            CalibrationEditorModel::build(&input, &ui, &harness.state.snapshot.loops[0]).unwrap();
        assert_eq!(
            editor.error.as_deref(),
            Some("The raw rates must increase.")
        );
    }
    assert_eq!(daemon.calls().len(), before);

    // "Load preset" replaces the table after a confirmation.
    ui.reset();
    ui.request_preset("alphacool_17558");
    {
        let input = input(&harness.state, &feedback);
        let editor =
            CalibrationEditorModel::build(&input, &ui, &harness.state.snapshot.loops[0]).unwrap();
        let confirm = editor.confirm.expect("the confirmation shows");
        assert_eq!(
            confirm.title,
            "Replace the table with Alphacool 17558 flow sensor?"
        );
        assert_eq!(editor.presets.len(), 1);
    }
    ui.cancel_preset();
    assert_eq!(daemon.calls().len(), before, "Cancel sends nothing");
    ui.request_preset("alphacool_17558");
    let command = {
        let input = input(&harness.state, &feedback);
        ui.confirm_preset(&input, "loop_1")
            .expect("the preset loads")
    };
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    let last = daemon.calls().last().cloned().unwrap();
    assert!(
        last.starts_with(&format!("SetCalibrationTable({user_table},")),
        "{last}"
    );
    assert!(
        last.contains("(327.0, 40.0), (369.0, 50.0), (404.0, 60.0), (700.0, 120.0)"),
        "{last}"
    );
    // The daemon sends no signal for a table: the app reads the tables again after the save.
    let refresh = command.refresh(&harness.client).await.expect("a refresh");
    harness.state.apply(&refresh);
    harness
        .until("the table holds the preset points", |state| {
            state
                .snapshot
                .calibration_tables
                .iter()
                .any(|t| t.id == user_table && t.points[1] == (369.0, 50.0))
        })
        .await;

    // Add and remove a row.
    ui.reset();
    let command = {
        let input = input(&harness.state, &feedback);
        ui.add_row(&input, "loop_1").expect("a valid table")
    };
    match command {
        Command::SaveCalibration { points, .. } => assert_eq!(points[4], (750.0, 130.0)),
        other => panic!("{other:?}"),
    }
    ui.reset();
    let command = {
        let input = input(&harness.state, &feedback);
        ui.remove_row(&input, "loop_1", 0).expect("3 points stay")
    };
    match command {
        Command::SaveCalibration { points, .. } => assert_eq!(points.len(), 3),
        other => panic!("{other:?}"),
    }

    // polkit denies the write.
    daemon.deny_writes(true);
    let command = {
        let input = input(&harness.state, &feedback);
        ui.reset();
        ui.set_row(&input, "loop_1", 0, 320.0, 40.0).unwrap()
    };
    let error = command
        .run_on_daemon(&harness.client)
        .await
        .expect_err("denied");
    assert_eq!(
        WriteOutcome::from_error(&error),
        WriteOutcome::NotAuthorized
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn loop_parts_go_to_the_daemon() {
    let bus = PrivateBus::start("loops-parts");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Loop is known", |state| {
            state
                .snapshot
                .loops
                .iter()
                .any(|l| l.id == "loop_1" && !l.pump_channels.is_empty())
        })
        .await;
    let ui = LoopsUi::default();
    assert_eq!(harness.state.snapshot.loops[0].pump_floor, 40.0);
    assert_eq!(
        harness.state.snapshot.loops[0].pump_channels,
        ["nct6798_pwm6"]
    );

    ui.set_pump_floor(&harness.state, 55.0)
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the floor changes", |state| {
            state.snapshot.loops[0].pump_floor == 55.0
        })
        .await;
    ui.set_water_temperature(&harness.state, Some("k10temp_tctl"))
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    ui.set_leakshield(&harness.state, None)
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the parts change", |state| {
            state.snapshot.loops[0].water_temperature.as_deref() == Some("k10temp_tctl")
                && state.snapshot.loops[0].leakshield.is_none()
        })
        .await;
    assert_eq!(
        daemon.calls(),
        [
            "SetPumpFloor(loop_1,55) interactive=true",
            "SetWaterTemperature(loop_1,k10temp_tctl) interactive=true",
            "SetLoopLeakshield(loop_1,) interactive=true"
        ]
    );

    // A new Loop, a rename and a delete.
    let feedback = Feedback::default();
    ui.new_loop(&input(&harness.state, &feedback))
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the new Loop shows", |state| {
            state.snapshot.loops.len() == 2
        })
        .await;
    let mut ui = LoopsUi::default();
    ui.select("loop_1");
    ui.open_rename(&harness.state);
    ui.rename.as_mut().unwrap().1 = "Custom loop".into();
    ui.rename_command()
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the Loop has its name", |state| {
            state.snapshot.loops.iter().any(|l| l.name == "Custom loop")
        })
        .await;
    ui.delete_dialog = true;
    ui.confirm_delete(&harness.state)
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the Loop is gone", |state| {
            state.snapshot.loops.iter().all(|l| l.name != "Custom loop")
        })
        .await;
}

#[test]
fn loops_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/loops.md.
    let table = [
        ("loops.new", "New", "Nuovo"),
        ("loops.part.pumps", "Pump channels", "Canali pompa"),
        ("loops.part.flow", "Flow sensor", "Sensore di portata"),
        ("loops.part.water", "Water temperature", "Temperatura acqua"),
        ("loops.part.leakshield", "Leakshield", "Leakshield"),
        ("loops.none", "None", "Nessuno"),
        ("loops.floor.title", "Pump floor", "Minimo pompa"),
        ("loops.delete.confirm", "Delete Loop", "Elimina il Loop"),
        ("loops.delete.cancel", "Cancel", "Annulla"),
        ("loops.feed.title", "Feed", "Feed"),
        (
            "calibration.title",
            "Calibration table",
            "Tabella di calibrazione",
        ),
        (
            "calibration.x_header",
            "Raw rate (rpm)",
            "Frequenza grezza (rpm)",
        ),
        ("calibration.y_header", "Flow (L/h)", "Portata (L/h)"),
        ("calibration.load_preset", "Load preset", "Carica preset"),
        ("calibration.add_point", "Add point", "Aggiungi punto"),
        ("calibration.out_of_range", "Out of range", "Fuori range"),
        (
            "loops.floor.text",
            "The Pump floor is the minimum Duty of the Pump channel. No Curve and no Profile goes below it. It keeps the water moving when a Curve reads a low temperature.",
            "Il minimo pompa è il Duty minimo del Canale pompa. Nessuna Curva e nessun Profilo scende sotto. Mantiene l'acqua in movimento quando una Curva legge una temperatura bassa.",
        ),
        (
            "loops.feed.caption",
            "The Leakshield uses the Feed to compute the pressure of the Loop.",
            "Il Leakshield usa il Feed per calcolare la pressione del Loop.",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "{key}");
        assert_eq!(it[key], italian, "{key}");
    }
    let state = full();
    let feedback = Feedback::default();
    let italian = LoopsModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &feedback)
        },
        &LoopsUi::default(),
    );
    let detail = italian.detail.unwrap();
    assert_eq!(detail.feed.unwrap().feed_pump, "Velocità pompa 2840 rpm");
    assert_eq!(
        detail.calibration.unwrap().plot.marker.unwrap().label,
        "671 rpm, 114 L/h"
    );
}

fn feed_card_at(state: &AppState, now_us: u64) -> nicecooling::screens::loops::FeedCard {
    LoopsModel::build(
        &ScreenInput {
            now_us,
            ..input(state, &Feedback::default())
        },
        &LoopsUi::default(),
    )
    .detail
    .unwrap()
    .feed
    .expect("the Feed card")
}

#[test]
fn feed_card_from_last_feed() {
    // The values and the time are the ones of `LastFeed`, not the live sensors.
    let mut state = full();
    state.sensors.as_mut().unwrap().sensors[2].1 = 1000.0;
    state.sensors.as_mut().unwrap().sensors[4].1 = 50.0;
    let feed = feed_card_at(&state, NOW_US + 2_000_000);
    assert_eq!(feed.feed_pump, "Pump speed 2840 rpm");
    assert_eq!(feed.flow, "Flow 115 L/h");
    assert_eq!(feed.sent, "Sent 2 s ago");
    assert!(!feed.stale);

    // The age follows the clock.
    assert_eq!(
        feed_card_at(&state, NOW_US + 5_000_000).sent,
        "Sent 5 s ago"
    );

    // Italian.
    let italian = LoopsModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &Feedback::default())
        },
        &LoopsUi::default(),
    )
    .detail
    .unwrap()
    .feed
    .unwrap();
    assert_eq!(italian.sent, "Inviato 2 s fa");
}

#[test]
fn feed_card_stale_after_5_s() {
    let state = full();
    // 5 s old: not stale yet. 6 s old: the Warning color and "Last Feed".
    assert!(!feed_card_at(&state, NOW_US + 5_000_000).stale);
    let feed = feed_card_at(&state, NOW_US + 6_000_000);
    assert!(feed.stale);
    assert_eq!(feed.sent, "Last Feed 6 s ago");
    assert_eq!(
        feed_card_at(&state, NOW_US + 12_000_000).sent,
        "Last Feed 12 s ago"
    );

    // A failing Feed is stale although the last send is recent.
    let mut failing = full();
    failing.snapshot.leakshields[0].feed_state = FeedState::Failing;
    let feed = feed_card_at(&failing, NOW_US + 2_000_000);
    assert!(feed.stale);
    assert_eq!(feed.sent, "Last Feed 2 s ago");

    // An `off` Feed keeps the age of the last send, in the Warning color.
    let mut off = full();
    off.snapshot.leakshields[0].feed_state = FeedState::Off;
    let feed = feed_card_at(&off, NOW_US + 2_000_000);
    assert!(feed.stale);
    assert_eq!(feed.sent, "Last Feed 2 s ago");
}

#[test]
fn feed_card_waiting() {
    let mut state = full();
    state.snapshot.leakshields[0].feed_state = FeedState::Waiting;
    state.snapshot.leakshields[0].last_feed = LastFeed::default();
    let feed = feed_card_at(&state, NOW_US);
    assert_eq!(feed.sent, "Not sent yet");
    assert!(!feed.stale);
    assert_eq!(feed.feed_pump, "Pump speed --");
    assert_eq!(feed.flow, "Flow --");

    // `off` before the first send shows the same text.
    state.snapshot.leakshields[0].feed_state = FeedState::Off;
    assert_eq!(feed_card_at(&state, NOW_US).sent, "Not sent yet");

    // Italian.
    let italian = LoopsModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &Feedback::default())
        },
        &LoopsUi::default(),
    )
    .detail
    .unwrap()
    .feed
    .unwrap();
    assert_eq!(italian.sent, "Non ancora inviato");
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_card_reads_the_properties_of_the_daemon() {
    let bus = PrivateBus::start("loops-feed");
    let daemon = FakeDaemon::start(
        bus.connect().await,
        Options {
            feed_waiting: true,
            ..Options::default()
        },
    )
    .await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Leakshield is read", |state| {
            !state.snapshot.leakshields.is_empty()
        })
        .await;
    let device = &harness.state.snapshot.leakshields[0];
    assert_eq!(device.feed_state, FeedState::Waiting);
    assert_eq!(device.last_feed, LastFeed::default());
    drop(daemon);
}
