mod support;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AppState, BoardInfo, Link};
use nicecooling::screens::first_run::{FirstRunModel, FirstRunState, FirstRunUi, Step};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling::shell::Page;
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, LEAKSHIELD, Options};
use support::fixtures::{BIOS_SNAPSHOT_US, NOW_US, full};
use support::harness::Harness;
use support::labels;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale: EN,
        down_since_us: None,
        feedback,
    }
}

fn model(state: &AppState, ui: &FirstRunUi) -> FirstRunModel {
    FirstRunModel::build(&input(state, &Feedback::default()), ui)
}

/// A Board with a Board file, before the first Loop: the installation of the first start.
fn fresh() -> AppState {
    let mut state = full();
    state.snapshot.loops.clear();
    state.snapshot.board = Some(BoardInfo {
        vendor: "ASUS".into(),
        model: "ROG CROSSHAIR VIII DARK HERO".into(),
        board_file: "asus_rog_crosshair_viii_dark_hero".into(),
    });
    state
        .snapshot
        .channels
        .get_mut("nct6798_pwm6")
        .unwrap()
        .is_pump = true;
    state
}

#[test]
fn first_run_states() {
    let ui = FirstRunUi::default();

    // The daemon is offline: the flow cannot start.
    let mut offline = fresh();
    offline.link = Link::Offline;
    let shown = model(&offline, &ui);
    assert_eq!(shown.state, FirstRunState::Offline);
    let card = shown.offline.expect("the offline card");
    assert_eq!(
        card.title,
        "nicecooling needs its daemon. Start it: systemctl start nicecoolingd"
    );
    assert_eq!(card.try_again, "Try again");
    assert!(!shown.footer.next_enabled, "Next waits for the daemon");
    assert_eq!(shown.footer.skip, "Skip setup", "Skip setup stays");

    // Loading: "Reading the Board".
    let mut loading = fresh();
    loading.link = Link::Connecting;
    let shown = model(&loading, &ui);
    assert_eq!(shown.state, FirstRunState::Loading);
    assert_eq!(shown.board.loading.as_deref(), Some("Reading the Board"));

    // A Board with a Board file: the name, the text, the counts and the table.
    let ready = model(&fresh(), &ui);
    assert_eq!(ready.state, FirstRunState::Ready);
    assert_eq!(ready.board.name, "ASUS ROG CROSSHAIR VIII DARK HERO");
    assert_eq!(
        ready.board.text,
        "ASUS ROG CROSSHAIR VIII DARK HERO. Board file found: Channels and sensors have their Board names."
    );
    assert_eq!(ready.board.counts, "2 Channels and 7 sensors found.");
    assert_eq!(ready.board.channels[0].name, "Chassis Fan 1");
    assert_eq!(ready.board.channels[1].name, "Waterpump");
    assert_eq!(ready.board.channels[1].rpm, "2840 rpm");
    assert_eq!(ready.board.channels[1].duty, "62 %");
    assert!(ready.footer.next_enabled && ready.footer.back.is_none());
    assert_eq!(ready.steps.len(), 5);
    assert!(ready.steps[0].current);

    // No Channel found: a warning, and the Loop step stays closed. "Skip setup" stays.
    let mut empty = fresh();
    empty.snapshot.channels.clear();
    let shown = model(&empty, &ui);
    assert_eq!(shown.state, FirstRunState::NoChannel);
    assert_eq!(
        shown.board.warning.as_deref(),
        Some(
            "No Channel found. nicecooling needs a hwmon driver for the fan controller of this Board."
        )
    );
    assert!(!shown.footer.next_enabled);
    assert!(!shown.loop_step.enabled);

    // The Loop step proposes the pump of the Board file, the flow sensor and the Leakshield.
    let at_loop = FirstRunUi {
        step: Step::Loop,
        ..Default::default()
    };
    let shown = model(&fresh(), &at_loop);
    let step = &shown.loop_step;
    assert_eq!(step.pump.shown, "Waterpump");
    assert_eq!(step.flow.shown, "Water flow");
    assert_eq!(step.water.shown, "Water temperature");
    assert_eq!(step.leakshield.shown, "Leakshield 20417-31072");
    assert_eq!(step.floor, 40.0, "the Pump floor starts at 40 %");
    assert_eq!(step.floor_text, "40 %");
    assert!(step.diagram.has_flow && step.diagram.has_pump && step.diagram.has_water);
    assert!(step.no_leakshield.is_none());
    assert!(shown.footer.next_enabled && shown.footer.back.is_some());

    // No flow sensor: the picker says "None", no preset row, and no flow part in the diagram.
    let no_flow = FirstRunUi {
        step: Step::Loop,
        flow: Some(String::new()),
        ..Default::default()
    };
    let shown = model(&fresh(), &no_flow);
    assert_eq!(shown.loop_step.flow.shown, "None");
    assert!(shown.loop_step.preset.is_none() && shown.loop_step.live_flow.is_none());
    assert!(!shown.loop_step.diagram.has_flow);

    // No pump picked: the step shows the warning and Next is off.
    let no_pump = FirstRunUi {
        step: Step::Loop,
        pump: Some(String::new()),
        ..Default::default()
    };
    let shown = model(&fresh(), &no_pump);
    assert_eq!(
        shown.loop_step.pump.warning.as_deref(),
        Some("The Loop needs a Pump channel.")
    );
    assert!(!shown.footer.next_enabled);

    // No Leakshield: the picker has only "None", step 4 is skipped, and the text says why.
    let mut bare = fresh();
    bare.snapshot.leakshields.clear();
    let shown = model(&bare, &at_loop);
    assert_eq!(shown.loop_step.leakshield.choices.len(), 1);
    assert_eq!(
        shown.loop_step.no_leakshield.as_deref(),
        Some("Without a Leakshield, the Loop has its Pump floor and its pump-stop Alarm.")
    );
    assert_eq!(shown.steps.len(), 4);
    assert!(shown.leakshield.is_none());

    // A Leakshield with known firmware: the counts and the button.
    let mut known = fresh();
    known.snapshot.leakshields[0].firmware = 1022;
    known.snapshot.leakshields[0].field_counts = Some((112, 16));
    let shown = model(&known, &ui);
    let step = shown.leakshield.expect("step 4");
    assert_eq!(
        step.status,
        "Firmware 1022: known. 96 of 112 fields are Verified fields. 16 Safety fields are read-only until a Verification write passes."
    );
    assert_eq!(step.open_fields.as_deref(), Some("Open Fields"));
    assert!(!step.unknown);
    assert_eq!(step.serial, "20417-31072");

    // Unknown firmware: the warning, and no "Open Fields".
    let mut unknown = fresh();
    unknown.snapshot.leakshields[0].firmware = 1042;
    unknown.snapshot.leakshields[0].read_only_reason = "unknown-firmware".into();
    let step = model(&unknown, &ui).leakshield.expect("step 4");
    assert!(step.unknown && step.open_fields.is_none());
    assert_eq!(
        step.status,
        "Firmware 1042 is unknown. nicecooling shows the Leakshield and cannot change its settings."
    );

    // The text of step 3 and the note about Channels without a Curve.
    let shown = model(&fresh(), &ui);
    assert!(
        shown
            .bios
            .text
            .starts_with("Before nicecooling controls a Channel, it saves the BIOS snapshot")
    );
    assert_eq!(
        shown.bios.no_curve,
        "A Channel without a Curve stays on its Hardware curve. nicecooling does not touch it."
    );
}

#[test]
fn board_without_board_file_message() {
    let mut state = fresh();
    state.snapshot.board.as_mut().unwrap().board_file.clear();
    let shown = model(&state, &FirstRunUi::default());
    assert_eq!(
        shown.board.text,
        "No Board file for this Board. Channels and sensors have the names of the kernel. You can rename them in Channels and Sensors."
    );
    // The Italian text.
    let feedback = Feedback::default();
    let italian = FirstRunModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &feedback)
        },
        &FirstRunUi::default(),
    );
    assert!(italian.board.text.starts_with("Nessun file Scheda"));
}

#[test]
fn flow_preset_shows_live_flow() {
    let at_loop = FirstRunUi {
        step: Step::Loop,
        ..Default::default()
    };
    // The preset of the package is the default. The flow sensor reads 671 rpm.
    let shown = model(&fresh(), &at_loop);
    let preset = shown.loop_step.preset.as_ref().expect("the preset row");
    assert_eq!(preset.shown, "Alphacool 17558 flow sensor");
    assert_eq!(
        preset
            .choices
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["Alphacool 17558 flow sensor", "Empty table"]
    );
    assert_eq!(
        shown.loop_step.live_flow.as_deref(),
        Some("671 rpm = 114 L/h")
    );
    // The live preview of the diagram holds the same flow.
    let bubble = shown
        .loop_step
        .diagram
        .bubble(nicecooling::widgets::loop_diagram::Tag::Ft)
        .expect("the flow bubble");
    assert_eq!(bubble.value, "114");

    // The live flow follows the rate of the sensor.
    let mut faster = fresh();
    faster.sensors.as_mut().unwrap().sensors[3].1 = 404.0;
    assert_eq!(
        model(&faster, &at_loop).loop_step.live_flow.as_deref(),
        Some("404 rpm = 60 L/h")
    );

    // "Empty table": no table, so no flow to show.
    let empty = FirstRunUi {
        step: Step::Loop,
        preset: Some(String::new()),
        ..Default::default()
    };
    let shown = model(&fresh(), &empty);
    assert_eq!(
        shown.loop_step.preset.as_ref().unwrap().shown,
        "Empty table"
    );
    assert!(shown.loop_step.live_flow.is_none());

    // The daemon has no value for the sensor: no live flow.
    let mut stale = fresh();
    stale.sensors = None;
    stale.last_sensors = None;
    assert!(model(&stale, &at_loop).loop_step.live_flow.is_none());
}

#[test]
fn the_steps_follow_each_other() {
    let state = fresh();
    let feedback = Feedback::default();
    let mut ui = FirstRunUi::default();
    assert!(ui.next(&input(&state, &feedback)).is_none());
    assert_eq!(ui.step, Step::Loop);
    // "Next" in the Loop step creates the Loop, with the picks.
    ui.floor = Some(55.0);
    let command = ui.next(&input(&state, &feedback)).expect("the command");
    assert_eq!(
        command,
        Command::SetupLoop {
            name: "Main loop".into(),
            pump_channels: vec!["nct6798_pwm6".into()],
            flow_sensor: "asusec_fan1".into(),
            table: "alphacool_17558".into(),
            water: "nct6798_temp1".into(),
            leakshield: LEAKSHIELD.into(),
            floor: 55.0,
        }
    );
    assert_eq!(ui.step, Step::Loop, "the flow waits for the daemon");
    assert!(!model(&state, &ui).footer.next_enabled);
    // The daemon holds the Loop: the flow moves on.
    let mut made = state.clone();
    made.snapshot.loops = full().snapshot.loops;
    ui.sync(&made);
    assert_eq!(ui.step, Step::Bios);
    assert!(ui.next(&input(&made, &feedback)).is_none());
    assert_eq!(ui.step, Step::Leakshield);
    assert!(ui.next(&input(&made, &feedback)).is_none());
    assert_eq!(ui.step, Step::Done);
    let done = model(&made, &ui);
    assert_eq!(
        done.done.text,
        "The Loop is set up. Its Channels still run their Hardware curves: add a Curve in Channels."
    );
    assert_eq!(done.done.overview, "Open the Overview");
    assert_eq!(
        done.done.wizard.as_deref(),
        Some("Open the Leakshield wizard")
    );
    assert_eq!(
        done.done.wizard_page,
        Some(Page::Leakshield(LEAKSHIELD.into()))
    );
    assert!(done.footer.next.is_none());
    assert_eq!(done.done.summary[0], "Pump channel: Waterpump");
    assert_eq!(done.done.summary[3], "Pump floor: 40 %");
    // Back goes through the steps in reverse, and step 4 only exists with a Leakshield.
    ui.back(&made);
    assert_eq!(ui.step, Step::Leakshield);
    ui.back(&made);
    ui.back(&made);
    assert_eq!(ui.step, Step::Loop);
    let mut without = made.clone();
    without.snapshot.leakshields.clear();
    let mut ui = FirstRunUi {
        step: Step::Bios,
        ..Default::default()
    };
    assert!(ui.next(&input(&without, &feedback)).is_none());
    assert_eq!(ui.step, Step::Done);
    ui.back(&without);
    assert_eq!(ui.step, Step::Bios);
}

#[tokio::test(flavor = "multi_thread")]
async fn first_run_creates_the_loop() {
    let bus = PrivateBus::start("first-run");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Board is known", |state| {
            state.snapshot.board.is_some() && state.snapshot.loops.len() == 1
        })
        .await;
    let command = Command::SetupLoop {
        name: "Second loop".into(),
        pump_channels: vec!["nct6798_pwm2".into()],
        flow_sensor: "asusec_fan1".into(),
        table: "alphacool_17558".into(),
        water: "k10temp_tctl".into(),
        leakshield: String::new(),
        floor: 55.0,
    };
    assert_eq!(command.feedback_key().as_deref(), Some("loop:new"));
    command.run_on_daemon(&harness.client).await.expect("setup");
    harness
        .until("the new Loop is complete", |state| {
            state
                .snapshot
                .loops
                .iter()
                .any(|l| l.name == "Second loop" && l.pump_floor == 55.0)
        })
        .await;
    let calls = daemon.calls();
    assert!(calls[0].starts_with("CreateLoop(Second loop)"), "{calls:?}");
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("SetPumpChannels(") && c.contains("[nct6798_pwm2]"))
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("SetFlowSensor(") && c.contains("asusec_fan1,alphacool_17558"))
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("SetWaterTemperature(") && c.contains("k10temp_tctl"))
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("SetLoopLeakshield(")),
        "no Leakshield is linked: {calls:?}"
    );
    assert!(calls.last().unwrap().contains("SetPumpFloor("));
    // The flow never writes the Leakshield.
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("SetFields") || c.contains("SetMode"))
    );
}

#[test]
fn first_run_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy tables of docs/ui/screens/first-run.md.
    let table = [
        ("first_run.skip", "Skip setup", "Salta la configurazione"),
        ("first_run.loop.pump", "Pump channel", "Canale pompa"),
        ("first_run.loop.flow", "Flow sensor", "Sensore di portata"),
        (
            "first_run.loop.preset",
            "Calibration table preset",
            "Preset della tabella di calibrazione",
        ),
        (
            "first_run.loop.water",
            "Water temperature",
            "Temperatura acqua",
        ),
        ("first_run.loop.leakshield", "Leakshield", "Leakshield"),
        ("first_run.loop.empty_table", "Empty table", "Tabella vuota"),
        (
            "first_run.done.overview",
            "Open the Overview",
            "Apri la Panoramica",
        ),
        (
            "first_run.done.wizard",
            "Open the Leakshield wizard",
            "Apri la procedura guidata del Leakshield",
        ),
        (
            "first_run.leakshield.open_fields",
            "Open Fields",
            "Apri i Campi",
        ),
        (
            "first_run.reading",
            "Reading the Board",
            "Lettura della Scheda",
        ),
        (
            "first_run.offline",
            "nicecooling needs its daemon. Start it: systemctl start nicecoolingd",
            "nicecooling ha bisogno del suo demone. Avvialo: systemctl start nicecoolingd",
        ),
        (
            "first_run.board.no_channel",
            "No Channel found. nicecooling needs a hwmon driver for the fan controller of this Board.",
            "Nessun Canale trovato. nicecooling ha bisogno di un driver hwmon per il controller delle ventole di questa Scheda.",
        ),
        (
            "first_run.bios.no_curve",
            "A Channel without a Curve stays on its Hardware curve. nicecooling does not touch it.",
            "Un Canale senza Curva resta sulla sua Curva hardware. nicecooling non lo tocca.",
        ),
        (
            "first_run.loop.no_leakshield",
            "Without a Leakshield, the Loop has its Pump floor and its pump-stop Alarm.",
            "Senza un Leakshield, il Loop ha il suo minimo pompa e il suo Allarme di pompa ferma.",
        ),
        (
            "first_run.leakshield.text",
            "nicecooling writes only Verified fields. A low-risk field is verified by a plausible read. A Safety field needs a Verification write. That is a supervised procedure outside the app: see the project documentation.",
            "nicecooling scrive solo campi Verificati. Un campo a basso rischio è verificato da una lettura plausibile. Un campo di sicurezza richiede una scrittura di verifica. È una procedura supervisionata fuori dall'app: vedi la documentazione del progetto.",
        ),
        (
            "first_run.done.text",
            "The Loop is set up. Its Channels still run their Hardware curves: add a Curve in Channels.",
            "Il Loop è configurato. I suoi Canali usano ancora le Curve hardware: aggiungi una Curva in Canali.",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "English text of {key}");
        assert_eq!(it[key], italian, "Italian text of {key}");
    }
    for key in en.keys().filter(|key| key.starts_with("first_run.")) {
        assert!(it.contains_key(key), "Italian text of {key}");
    }
    assert_eq!(en["nav.first_run"], "First run");
}

#[test]
fn snapshot_step_lists_hardware_curves() {
    let ui = FirstRunUi {
        step: Step::Bios,
        ..Default::default()
    };
    let shown = model(&fresh(), &ui);
    assert_eq!(shown.bios.columns, ["Channel", "Hardware curve"]);
    assert_eq!(
        shown.bios.saved,
        Some(format!(
            "Saved {}",
            nicecooling::screens::format::date_time(BIOS_SNAPSHOT_US)
        ))
    );
    let rows: Vec<(&str, &str)> = shown
        .bios
        .rows
        .iter()
        .map(|row| (row.name.as_str(), row.curve.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            (
                "Chassis Fan 1",
                "30.0 \u{b0}C: 20 %, 40.0 \u{b0}C: 40 %, 50.0 \u{b0}C: 60 %, 60.0 \u{b0}C: 80 %, 70.0 \u{b0}C: 100 %"
            ),
            ("Waterpump", "--"),
        ]
    );

    // No snapshot: no date, and the rows show "--".
    let mut none = fresh();
    for channel in none.snapshot.channels.values_mut() {
        channel.bios_snapshot = Default::default();
    }
    let shown = model(&none, &ui);
    assert!(shown.bios.saved.is_none());
    assert!(shown.bios.rows.iter().all(|row| row.curve == "--"));
}

#[test]
fn empty_table_skips_set_flow_sensor_and_shows_note() {
    let note = "The flow sensor has no value until its Calibration table has 2 points. Build the table on the Loops screen.";
    let at_loop = FirstRunUi {
        step: Step::Loop,
        ..Default::default()
    };
    // A preset: no note.
    assert!(
        model(&fresh(), &at_loop)
            .loop_step
            .empty_table_note
            .is_none()
    );

    // "Empty table" with a flow sensor: the note, in both languages.
    let empty = FirstRunUi {
        step: Step::Loop,
        preset: Some(String::new()),
        ..Default::default()
    };
    assert_eq!(
        model(&fresh(), &empty)
            .loop_step
            .empty_table_note
            .as_deref(),
        Some(note)
    );
    let feedback = Feedback::default();
    let italian = FirstRunModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&fresh(), &feedback)
        },
        &empty,
    );
    assert_eq!(
        italian.loop_step.empty_table_note.as_deref(),
        Some(
            "Il sensore di portata non ha valore finch\u{e9} la sua tabella di calibrazione non ha 2 punti. Crea la tabella nella schermata Loop."
        )
    );

    // Without a flow sensor there is no table to build: no note.
    let no_flow = FirstRunUi {
        step: Step::Loop,
        preset: Some(String::new()),
        flow: Some(String::new()),
        ..Default::default()
    };
    assert!(
        model(&fresh(), &no_flow)
            .loop_step
            .empty_table_note
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_table_setup_does_not_call_set_flow_sensor() {
    let bus = PrivateBus::start("first-run-empty-table");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Board is known", |state| {
            state.snapshot.board.is_some() && state.snapshot.loops.len() == 1
        })
        .await;
    let command = Command::SetupLoop {
        name: "Empty table loop".into(),
        pump_channels: vec!["nct6798_pwm2".into()],
        flow_sensor: "asusec_fan1".into(),
        table: String::new(),
        water: "k10temp_tctl".into(),
        leakshield: String::new(),
        floor: 55.0,
    };
    command.run_on_daemon(&harness.client).await.expect("setup");
    harness
        .until("the new Loop is complete", |state| {
            state
                .snapshot
                .loops
                .iter()
                .any(|l| l.name == "Empty table loop" && l.pump_floor == 55.0)
        })
        .await;
    let calls = daemon.calls();
    assert!(
        !calls.iter().any(|c| c.contains("SetFlowSensor(")),
        "{calls:?}"
    );
    assert!(calls.iter().any(|c| c.starts_with("SetPumpChannels(")));
    assert!(calls.last().unwrap().contains("SetPumpFloor("));
}
