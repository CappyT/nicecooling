mod support;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AppState, DerivedDef, Link, Severity};
use nicecooling::screens::sensors::{
    DerivedEditor, DerivedEditorModel, SensorsModel, SensorsState, SensorsUi,
};
use nicecooling::screens::{Feedback, ScreenInput};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fixtures::{LEAKSHIELD, NOW_US, alarm, full, sensor};
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

fn model(state: &AppState, ui: &SensorsUi) -> SensorsModel {
    SensorsModel::build(&input(state, &Feedback::default()), ui)
}

fn row<'a>(model: &'a SensorsModel, id: &str) -> &'a nicecooling::screens::sensors::SensorRow {
    model
        .groups
        .iter()
        .flat_map(|g| g.rows.iter())
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no row {id}"))
}

#[test]
fn sensors_states() {
    let ui = SensorsUi::default();

    // Loading and empty.
    let mut state = full();
    state.link = Link::Connecting;
    assert_eq!(model(&state, &ui).state, SensorsState::Loading);
    let mut state = full();
    state.snapshot.sensors.clear();
    let empty = model(&state, &ui);
    assert_eq!(empty.state, SensorsState::Empty);
    assert_eq!(
        empty.empty_text,
        "No sensor found. Check that the hwmon drivers of the Board are loaded."
    );

    // Groups by device, in the order of the table, with the driver as caption.
    let state = full();
    let ready = model(&state, &ui);
    assert_eq!(ready.state, SensorsState::Ready);
    let titles: Vec<_> = ready.groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Board chip",
            "Board chip",
            "Board chip",
            "Leakshield",
            "Calibrated flow"
        ]
    );
    let captions: Vec<_> = ready.groups.iter().map(|g| g.caption.as_deref()).collect();
    assert_eq!(
        captions[..3],
        [Some("asusec"), Some("k10temp"), Some("nct6798")]
    );
    let water = row(&ready, "nct6798_temp1");
    assert_eq!(water.value, "31.4 °C");
    assert!(!water.no_value && !water.dim);
    assert_eq!(row(&ready, "nct6798_fan6").value, "2\u{a0}840 rpm");
    assert_eq!(row(&ready, "loop_1_flow").value, "115 L/h");

    // Used by: a Curve, a Loop. A sensor in use cannot be hidden.
    assert_eq!(water.used_by, "Curve of Waterpump, Main loop");
    assert!(!water.hide_enabled);
    assert_eq!(
        water.hide_reason.as_deref(),
        Some("A Curve uses this sensor.")
    );
    let cpu = row(&ready, "k10temp_tctl");
    assert!(cpu.hide_enabled && cpu.used_by.is_empty());
    assert_eq!(
        row(&ready, "asusec_fan1").flow_loop.as_deref(),
        Some("loop_1")
    );

    // Search filters by name and by device.
    let searching = SensorsUi {
        search: "k10".into(),
        ..Default::default()
    };
    let found = model(&state, &searching);
    assert_eq!(found.groups.len(), 1);
    assert_eq!(found.groups[0].rows[0].id, "k10temp_tctl");

    // A hidden sensor is absent. With "Show hidden sensors" it shows dim.
    let mut hiding = SensorsUi::default();
    hiding.toggle_hidden("k10temp_tctl");
    let hidden = model(&state, &hiding);
    assert!(
        hidden
            .groups
            .iter()
            .all(|g| g.rows.iter().all(|r| r.id != "k10temp_tctl"))
    );
    hiding.show_hidden = true;
    let shown = model(&state, &hiding);
    let cpu = row(&shown, "k10temp_tctl");
    assert!(cpu.hidden && cpu.dim);

    // A sensor without value: "--".
    let mut blank = full();
    blank.sensors.as_mut().unwrap().sensors[1].2 = 1;
    let blank = model(&blank, &ui);
    assert!(row(&blank, "k10temp_tctl").no_value);
    assert_eq!(row(&blank, "k10temp_tctl").value, "--");

    // A user name shows the Board file name as caption.
    let mut renamed = full();
    renamed
        .snapshot
        .sensors
        .get_mut("k10temp_tctl")
        .unwrap()
        .name = "CPU".into();
    assert_eq!(
        row(&model(&renamed, &ui), "k10temp_tctl")
            .caption
            .as_deref(),
        Some("CPU temperature")
    );

    // An Alarm or a Warning that a sensor caused: the severity icon.
    let mut alarmed = full();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        nicecooling::model::Response::Notify,
        0,
    );
    leak.details.sensor = Some(format!("{LEAKSHIELD}_pressure"));
    alarmed.snapshot.alarms = vec![leak];
    assert_eq!(
        row(&model(&alarmed, &ui), &format!("{LEAKSHIELD}_pressure")).severity,
        Some(Severity::Alarm)
    );

    // Daemon offline: the values are dim and rename and hide are disabled.
    let mut offline = full();
    offline.link = Link::Offline;
    offline.sensors = None;
    let offline = model(&offline, &ui);
    let water = row(&offline, "nct6798_temp1");
    assert!(water.dim && !water.rename_enabled && !water.hide_enabled);
    assert_eq!(water.value, "31.4 °C", "the last value stays");
    assert!(!offline.writes_enabled);
    assert_eq!(
        offline.writes_tooltip.as_deref(),
        Some("The daemon is offline.")
    );

    // A Derived sensor with a source that has no value.
    let mut derived = full();
    let mut max = sensor("derived_1", "Hottest", "temperature", "degC", "derived", "");
    max.derived = Some(DerivedDef {
        function: "maximum".into(),
        sources: vec!["nct6798_temp1".into(), "k10temp_tctl".into()],
    });
    derived.snapshot.sensors.insert(max.id.clone(), max);
    derived
        .sensors
        .as_mut()
        .unwrap()
        .sensors
        .push(("derived_1".into(), 46.2, 0));
    let ok = model(&derived, &ui);
    assert_eq!(ok.groups.last().unwrap().title, "Derived sensors");
    assert_eq!(row(&ok, "derived_1").value, "46.2 °C");
    assert!(row(&ok, "derived_1").warning.is_none());
    derived.sensors.as_mut().unwrap().sensors[1].2 = 1;
    derived
        .sensors
        .as_mut()
        .unwrap()
        .sensors
        .retain(|s| s.0 != "derived_1");
    derived
        .sensors
        .as_mut()
        .unwrap()
        .sensors
        .push(("derived_1".into(), 0.0, 1));
    let missing = model(&derived, &ui);
    assert_eq!(row(&missing, "derived_1").value, "--");
    assert_eq!(
        row(&missing, "derived_1").warning.as_deref(),
        Some("Source without value: CPU temperature")
    );

    // A Derived sensor that a Curve uses cannot be deleted.
    let mut used = full();
    let mut max = sensor("derived_1", "Hottest", "temperature", "degC", "derived", "");
    max.derived = Some(DerivedDef {
        function: "maximum".into(),
        sources: vec!["nct6798_temp1".into(), "k10temp_tctl".into()],
    });
    used.snapshot.sensors.insert(max.id.clone(), max);
    used.snapshot
        .channels
        .get_mut("nct6798_pwm2")
        .unwrap()
        .control = nicecooling::model::Control::Curve;
    used.snapshot
        .channels
        .get_mut("nct6798_pwm2")
        .unwrap()
        .curve
        .input = Some("derived_1".into());
    assert_eq!(
        row(&model(&used, &ui), "derived_1")
            .delete_reason
            .as_deref(),
        Some("The Curve of Chassis Fan 1 uses this sensor.")
    );
}

#[test]
fn derived_editor_follows_the_rules_of_the_table() {
    let state = full();
    let feedback = Feedback::default();
    let input = input(&state, &feedback);
    let mut editor = DerivedEditor::new_sensor();
    let empty = DerivedEditorModel::build(&input, &editor);
    assert!(!empty.can_save);
    assert_eq!(empty.function_help, "Maximum: the highest source.");
    assert_eq!(empty.live_result, "Now --");

    editor.name = "Hottest".into();
    editor.set_source(0, Some("nct6798_temp1".into()));
    editor.set_source(1, Some("k10temp_tctl".into()));
    let valid = DerivedEditorModel::build(&input, &editor);
    assert!(valid.can_save, "{:?}", valid.save_reason);
    assert_eq!(valid.live_result, "Now 46.2 °C");

    // "Difference" has exactly two sources: "Minuend" and "Subtrahend".
    editor.add_source();
    assert_eq!(editor.sources.len(), 3);
    editor.set_function("difference");
    assert_eq!(editor.sources.len(), 2);
    let difference = DerivedEditorModel::build(&input, &editor);
    assert_eq!(difference.slots[0].label, "Minuend");
    assert_eq!(difference.slots[1].label, "Subtrahend");
    assert!(!difference.can_add_source);
    assert_eq!(difference.live_result, "Now -14.8 °C");
    assert_eq!(
        difference.function_help,
        "Difference: the first source minus the second."
    );

    // Mixed units: Save is disabled with the text of the table.
    editor.set_source(1, Some("nct6798_fan6".into()));
    let mixed = DerivedEditorModel::build(&input, &editor);
    assert!(!mixed.can_save);
    assert_eq!(
        mixed.unit_mismatch.as_deref(),
        Some("All sources must have the same unit.")
    );
    assert!(
        DerivedEditorModel::save_command(&editor).is_some(),
        "the model gates Save"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn derived_sensor_created_through_api() {
    let bus = PrivateBus::start("sensors-derived");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the daemon is online", |state| {
            state.is_online() && state.snapshot.sensors.contains_key("k10temp_tctl")
        })
        .await;

    let mut editor = DerivedEditor::new_sensor();
    editor.name = "Hottest".into();
    editor.set_function("mean");
    editor.set_source(0, Some("nct6798_temp1".into()));
    editor.set_source(1, Some("k10temp_tctl".into()));
    let command = DerivedEditorModel::save_command(&editor).expect("a valid editor");
    assert_eq!(
        command,
        Command::CreateDerivedSensor {
            name: "Hottest".into(),
            function: "mean".into(),
            sources: vec!["nct6798_temp1".into(), "k10temp_tctl".into()],
        }
    );
    assert!(
        command
            .run_on_daemon(&harness.client)
            .await
            .expect("the call works")
    );
    assert_eq!(
        daemon.calls(),
        ["CreateDerivedSensor(Hottest,mean,[nct6798_temp1,k10temp_tctl]) interactive=true"]
    );

    harness
        .until("the Derived sensor shows in the snapshot", |state| {
            state
                .snapshot
                .sensors
                .values()
                .any(|s| s.origin == "derived")
        })
        .await;
    let created = harness
        .state
        .snapshot
        .sensors
        .values()
        .find(|s| s.origin == "derived")
        .expect("the new sensor")
        .clone();
    assert_eq!(created.name, "Hottest");
    assert_eq!(
        created.derived,
        Some(DerivedDef {
            function: "mean".into(),
            sources: vec!["nct6798_temp1".into(), "k10temp_tctl".into()],
        })
    );

    // The editor changes the definition of the saved sensor.
    let mut edit = DerivedEditor::edit(&created).expect("a Derived sensor");
    edit.set_function("maximum");
    let command = DerivedEditorModel::save_command(&edit).expect("valid");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the definition changes", |state| {
            state.snapshot.sensors[&created.id]
                .derived
                .as_ref()
                .is_some_and(|d| d.function == "maximum")
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_through_api() {
    let bus = PrivateBus::start("sensors-rename");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the daemon is online", |state| {
            state.snapshot.sensors.contains_key("k10temp_tctl")
        })
        .await;

    let mut ui = SensorsUi::default();
    ui.open_rename("k10temp_tctl", "CPU temperature");
    ui.rename.as_mut().unwrap().1 = "  CPU  ".into();
    let command = ui.rename_command().expect("a rename");
    assert!(ui.rename.is_none(), "Enter closes the popover");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert_eq!(
        daemon.calls(),
        ["SetSensorName(k10temp_tctl,CPU) interactive=true"]
    );
    harness
        .until("the new name shows", |state| {
            state.snapshot.sensors["k10temp_tctl"].name == "CPU"
        })
        .await;
    let renamed = model(&harness.state, &SensorsUi::default());
    assert_eq!(
        row(&renamed, "k10temp_tctl").caption.as_deref(),
        Some("CPU temperature"),
        "the Board file name is the caption"
    );

    // An empty name restores the Board file name.
    ui.open_rename("k10temp_tctl", "CPU");
    ui.rename.as_mut().unwrap().1 = String::new();
    ui.rename_command()
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the default name returns", |state| {
            state.snapshot.sensors["k10temp_tctl"].name == "CPU temperature"
        })
        .await;

    // polkit denies the write: nothing changes, and the model says so.
    daemon.deny_writes(true);
    ui.open_rename("k10temp_tctl", "x");
    let command = ui.rename_command().unwrap();
    let error = command
        .run_on_daemon(&harness.client)
        .await
        .expect_err("a denied write");
    let mut feedback = Feedback::default();
    feedback.set(
        command.feedback_key().unwrap(),
        nicecooling::screens::WriteOutcome::from_error(&error),
    );
    assert_eq!(
        feedback.get("sensor-name:k10temp_tctl"),
        Some(&nicecooling::screens::WriteOutcome::NotAuthorized)
    );
}

#[test]
fn sensors_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/sensors.md.
    let table = [
        ("sensors.search", "Search sensors", "Cerca sensori"),
        (
            "sensors.show_hidden",
            "Show hidden sensors",
            "Mostra i sensori nascosti",
        ),
        (
            "sensors.new_derived",
            "New Derived sensor",
            "Nuovo sensore derivato",
        ),
        ("sensors.columns.sensor", "Sensor", "Sensore"),
        ("sensors.columns.value", "Value", "Valore"),
        (
            "sensors.columns.last_10_minutes",
            "Last 10 minutes",
            "Ultimi 10 minuti",
        ),
        ("sensors.columns.used_by", "Used by", "Usato da"),
        (
            "sensors.help.maximum",
            "Maximum: the highest source.",
            "Massimo: la sorgente più alta.",
        ),
        (
            "sensors.help.mean",
            "Mean: the average of the sources.",
            "Media: la media delle sorgenti.",
        ),
        (
            "sensors.help.difference",
            "Difference: the first source minus the second.",
            "Differenza: la prima sorgente meno la seconda.",
        ),
        ("sensors.function.maximum", "Maximum", "Massimo"),
        ("sensors.function.mean", "Mean", "Media"),
        ("sensors.function.difference", "Difference", "Differenza"),
        ("sensors.editor.minuend", "Minuend", "Minuendo"),
        ("sensors.editor.subtrahend", "Subtrahend", "Sottraendo"),
        ("sensors.editor.cancel", "Cancel", "Annulla"),
        ("sensors.editor.save", "Save", "Salva"),
        (
            "sensors.editor.mixed_units",
            "All sources must have the same unit.",
            "Tutte le sorgenti devono avere la stessa unità.",
        ),
        (
            "sensors.in_use.curve",
            "A Curve uses this sensor.",
            "Una Curva usa questo sensore.",
        ),
        ("sensors.hide", "Hide", "Nascondi"),
        ("sensors.show", "Show", "Mostra"),
        ("sensors.rename", "Rename", "Rinomina"),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "{key}");
        assert_eq!(it[key], italian, "{key}");
    }
    // The rows show the Italian words too.
    let state = full();
    let feedback = Feedback::default();
    let italian = SensorsModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &feedback)
        },
        &SensorsUi::default(),
    );
    let water = row(&italian, "nct6798_temp1");
    assert_eq!(water.used_by, "Curva di Waterpump, Main loop");
    assert_eq!(water.value, "31,4 °C");
    assert_eq!(
        water.hide_reason.as_deref(),
        Some("Una Curva usa questo sensore.")
    );
}
