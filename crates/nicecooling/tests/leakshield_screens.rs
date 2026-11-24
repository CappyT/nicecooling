//! The nine tabs of the Leakshield page against the models, and against the fake daemon where a
//! test writes. Tasks 8.14 to 8.22.

mod support;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT, Locale};
use nicecooling::model::{
    AlarmDetails, AppState, FieldValue, Link, Response, ResponseRule, Severity,
};
use nicecooling::screens::leakshield::alarms::{AlarmsModel, AlarmsUi, RecommendationKind};
use nicecooling::screens::leakshield::display::{DisplayModel, DisplayUi};
use nicecooling::screens::leakshield::fields::{FieldsModel, FieldsState, FieldsUi, Filter};
use nicecooling::screens::leakshield::lighting::{LightingModel, LightingUi};
use nicecooling::screens::leakshield::overview::{
    ActionView, Confirm, GaugeState, OverviewModel, OverviewUi,
};
use nicecooling::screens::leakshield::sensors::{SensorsModel, SensorsUi};
use nicecooling::screens::leakshield::settings::{Control, SettingsModel, SettingsUi, Step};
use nicecooling::screens::leakshield::status::{StatusModel, StatusUi, event_title, export_text};
use nicecooling::screens::leakshield::system::{Ask, RESET_WORD, SystemModel, SystemUi};
use nicecooling::screens::leakshield::{Frame, TabState};
use nicecooling::screens::{Feedback, ScreenInput, WriteOutcome};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fake_leakshield::Fv;
use support::fixtures::{LEAKSHIELD, NOW_US, alarm, with_leakshield};
use support::harness::Harness;
use support::labels;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback, locale: Locale) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale,
        down_since_us: None,
        feedback,
    }
}

fn en<'a>(state: &'a AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    input(state, feedback, EN)
}

/// The Leakshield of the fixture with a change.
fn tweak(f: impl FnOnce(&mut AppState)) -> AppState {
    let mut state = with_leakshield();
    f(&mut state);
    state
}

fn unknown_firmware() -> AppState {
    tweak(|state| {
        state.snapshot.leakshields[0].read_only_reason = "unknown-firmware".into();
        let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
        for field in &mut data.fields {
            field.writable = false;
            field.reason = "unknown-firmware".into();
        }
    })
}

fn offline() -> AppState {
    tweak(|state| state.snapshot.leakshields[0].connected = false)
}

fn connecting() -> AppState {
    tweak(|state| state.link = Link::Connecting)
}

fn overview(state: &AppState, ui: &OverviewUi) -> OverviewModel {
    OverviewModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

fn set_field(state: &mut AppState, id: &str, value: FieldValue) {
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.fields.iter_mut().find(|f| f.id == id).unwrap().value = value;
}

fn set_writable(state: &mut AppState, id: &str, writable: bool) {
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    let field = data.fields.iter_mut().find(|f| f.id == id).unwrap();
    field.writable = writable;
    field.reason = if writable {
        String::new()
    } else {
        "not-verified".into()
    };
}

#[test]
fn the_frame_has_nine_tabs_and_the_serial() {
    let state = with_leakshield();
    let frame = Frame::build(&en(&state, &Feedback::default()), LEAKSHIELD, false);
    assert_eq!(frame.title, "Leakshield");
    assert_eq!(frame.subtitle, "S/N 20417-31072");
    assert_eq!(
        frame
            .tabs
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>(),
        [
            "Overview", "Settings", "Sensors", "Display", "Lighting", "Alarms", "Status", "System",
            "Fields"
        ]
    );
    assert!(frame.offline.is_none());
    // The wizard has pending changes: its label has a dot. A Leakshield that does not answer
    // shows its banner.
    let pending = Frame::build(&en(&offline(), &Feedback::default()), LEAKSHIELD, true);
    assert_eq!(pending.tabs[1].1, "Settings \u{2022}");
    assert_eq!(
        pending.offline.as_deref(),
        Some("The Leakshield does not answer. Check its USB cable.")
    );
}

#[test]
fn leakshield_overview_states() {
    // Ready: the gauge, the status and the Mode.
    let state = with_leakshield();
    let model = overview(&state, &OverviewUi::default());
    assert_eq!(model.state, TabState::Ready);
    assert_eq!(model.pressure.gauge, GaugeState::InRange);
    assert_eq!(model.pressure.tag, "In range");
    assert_eq!(model.pressure.value_text, "298 mbar");
    let limits = model.pressure.limits.expect("limits from the fields");
    assert_eq!(
        (limits.min, limits.target, limits.max),
        (250.0, 300.0, 350.0)
    );
    assert_eq!(model.status.tag, "Protecting");
    let rows: Vec<(&str, &str)> = model
        .status
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.value.as_str()))
        .collect();
    assert_eq!(rows[0], ("Fill level", "68 %"));
    assert!(
        model.status.rows[0]
            .caption
            .as_deref()
            .is_some_and(|text| text.starts_with("measured "))
    );
    assert_eq!(rows[1], ("Volume change", "+2 ml/h"));
    assert_eq!(rows[2], ("Pressure change", "-0.4 mbar/min"));
    assert_eq!(rows[3], ("Feed", "2\u{a0}840 rpm, 115 L/h"));
    assert!(
        model.status.rows[3]
            .caption
            .as_deref()
            .is_some_and(|text| text.ends_with(" s ago"))
    );
    let modes: Vec<_> = model
        .mode
        .buttons
        .iter()
        .map(|b| (b.label.as_str(), b.selected, b.enabled))
        .collect();
    assert_eq!(
        modes,
        [
            ("SHIELD", true, true),
            ("MONITOR", false, true),
            ("RELEASE", false, true)
        ]
    );
    assert_eq!(
        model.mode.caption,
        "SHIELD: full protection by negative pressure, and leak detection."
    );
    assert_eq!(
        model
            .mode
            .actions
            .iter()
            .map(|a| a.label.as_str())
            .collect::<Vec<_>>(),
        ["Test", "Measure fill level", "Deaerate", "Refill"]
    );
    assert!(model.mode.actions.iter().all(|a| a.enabled));
    let refill = &model.mode.refill;
    assert_eq!(refill.text, "Refill can also start on the device:");
    assert_eq!(
        &refill.text[refill.action.clone()],
        "Refill",
        "the bold part"
    );
    let italian = OverviewModel::build(
        &input(&state, &Feedback::default(), IT),
        &OverviewUi::default(),
        LEAKSHIELD,
    );
    let refill = &italian.mode.refill;
    assert_eq!(refill.text, "Il Rabbocco si avvia anche dal Leakshield:");
    assert_eq!(&refill.text[refill.action.clone()], "Rabbocco");
    assert!(model.action_view.is_none() && model.dialog.is_none());

    // Below the minimum: a Warning tag on the gauge.
    let mut low = with_leakshield();
    low.sensors
        .as_mut()
        .unwrap()
        .sensors
        .iter_mut()
        .for_each(|s| {
            if s.0.ends_with("_pressure") {
                s.1 = 212.0;
            }
        });
    let model = overview(&low, &OverviewUi::default());
    assert_eq!(model.pressure.gauge, GaugeState::Below);
    assert_eq!(model.pressure.tag, "Below the minimum");
    assert_eq!(model.status.tag, "Building pressure");

    // Loading.
    assert_eq!(
        overview(&connecting(), &OverviewUi::default()).state,
        TabState::Loading
    );
    assert_eq!(
        overview(&connecting(), &OverviewUi::default()).loading_text,
        "Reading the Leakshield"
    );

    // The Leakshield does not answer: no marker, Mode and Actions off.
    let model = overview(&offline(), &OverviewUi::default());
    assert_eq!(model.state, TabState::Offline);
    assert_eq!(model.pressure.gauge, GaugeState::Offline);
    assert_eq!(model.pressure.value_text, "--");
    assert!(model.mode.buttons.iter().all(|b| !b.enabled));
    assert!(model.mode.actions.iter().all(|a| !a.enabled));

    // Unknown firmware: the controls are off and have the tooltip.
    let model = overview(&unknown_firmware(), &OverviewUi::default());
    assert_eq!(
        model.mode.tooltip.as_deref(),
        Some("Read-only: unknown firmware.")
    );
    assert!(model.mode.buttons.iter().all(|b| !b.enabled));
    assert!(
        model.mode.read_only.is_none(),
        "the banner of the shell gives the reason"
    );

    // Mode not verified: the selector shows the Mode and links to Fields.
    let mut locked = with_leakshield();
    set_writable(&mut locked, "default_mode", false);
    let model = overview(&locked, &OverviewUi::default());
    assert_eq!(
        model.mode.read_only,
        Some((
            "Read-only: Mode is not a Verified field yet.".into(),
            "Open Fields".into()
        ))
    );
    assert!(model.mode.buttons[0].selected && !model.mode.buttons[0].enabled);

    // A Warning: the tag and the triangle on the row that caused it.
    let mut warning = with_leakshield();
    warning.snapshot.alarms = vec![alarm(
        "w1",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    )];
    let model = overview(&warning, &OverviewUi::default());
    assert_eq!(model.status.tag, "Leak Warning");
    assert_eq!(model.status.rows[1].severity, Some(Severity::Warning));
    assert_eq!(model.status.rows[0].severity, None);

    // An Alarm below the minimum: the marker and the value are red.
    let mut alarmed = low.clone();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.details = AlarmDetails::default();
    alarmed.snapshot.alarms = vec![leak];
    let model = overview(&alarmed, &OverviewUi::default());
    assert_eq!(model.status.tag, "Large leak Alarm");
    assert!(model.pressure.alarm);

    // Mode RELEASE: the banner.
    let release = tweak(|s| s.snapshot.leakshields[0].mode = "release".into());
    let model = overview(&release, &OverviewUi::default());
    assert_eq!(
        model.mode.release_banner.as_deref(),
        Some("Mode RELEASE: the Loop has no leak protection.")
    );
    assert_eq!(model.status.tag, "Released");

    // Denied by polkit.
    let mut feedback = Feedback::default();
    feedback.set(format!("ls:{LEAKSHIELD}:mode"), WriteOutcome::NotAuthorized);
    let model = OverviewModel::build(&en(&state, &feedback), &OverviewUi::default(), LEAKSHIELD);
    assert_eq!(
        model.mode.denied.as_deref(),
        Some("Not authorized. The Mode did not change.")
    );
}

#[test]
fn leakshield_overview_actions_and_questions() {
    let state = with_leakshield();
    let mut ui = OverviewUi::default();

    // A Mode that lowers the protection asks first.
    assert!(ui.request_mode(&state, LEAKSHIELD, "monitor").is_none());
    assert_eq!(ui.confirm, Some(Confirm::Mode("monitor".into())));
    let dialog = overview(&state, &ui).dialog.expect("a dialog");
    assert_eq!(dialog.title, "Change the Mode to MONITOR?");
    assert_eq!(
        dialog.text,
        "MONITOR detects leaks and does not prevent them."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_deref()),
        ("Cancel", Some("Set MONITOR"))
    );
    assert_eq!(
        ui.confirm(LEAKSHIELD),
        Some(Command::ChangeLeakshieldMode {
            leakshield: LEAKSHIELD.into(),
            mode: "monitor".into()
        })
    );
    // The same Mode sends nothing. A higher protection sends at once.
    assert!(ui.request_mode(&state, LEAKSHIELD, "shield").is_none() && ui.confirm.is_none());
    let monitor = tweak(|s| s.snapshot.leakshields[0].mode = "monitor".into());
    assert_eq!(
        ui.request_mode(&monitor, LEAKSHIELD, "shield"),
        Some(Command::ChangeLeakshieldMode {
            leakshield: LEAKSHIELD.into(),
            mode: "shield".into()
        })
    );
    // RELEASE asks too.
    assert!(ui.request_mode(&state, LEAKSHIELD, "release").is_none());
    assert_eq!(
        overview(&state, &ui).dialog.unwrap().text,
        "RELEASE turns the leak protection off."
    );
    ui.cancel_dialog();

    // The fill level measurement asks first.
    assert!(
        ui.request_action(&state, LEAKSHIELD, "measure-fill-level")
            .is_none()
    );
    let dialog = overview(&state, &ui).dialog.expect("a dialog");
    assert!(
        dialog
            .text
            .starts_with("During the measurement the Loop has no leak protection.")
    );
    assert_eq!(dialog.confirm.as_deref(), Some("Measure"));
    assert_eq!(
        ui.confirm(LEAKSHIELD),
        Some(Command::StartLeakshieldAction {
            leakshield: LEAKSHIELD.into(),
            action: "measure-fill-level".into(),
            test_pressure: None
        })
    );
    // The Deaerate starts at once.
    assert!(matches!(
        ui.request_action(&state, LEAKSHIELD, "deaerate"),
        Some(Command::StartLeakshieldAction { .. })
    ));

    // The Test opens its setup with the pressure of the device. 50 mbar is out of range.
    assert!(ui.request_action(&state, LEAKSHIELD, "test").is_none());
    let model = overview(&state, &ui);
    let Some(ActionView::TestSetup {
        pressure,
        error,
        text,
        ..
    }) = model.action_view
    else {
        panic!("the Test setup");
    };
    assert_eq!((pressure.as_str(), error), ("300", None));
    assert_eq!(
        text,
        "Stop the pump before a Test: a running pump changes the pressure."
    );
    ui.test_pressure = Some("50".into());
    let Some(ActionView::TestSetup { error, .. }) = overview(&state, &ui).action_view else {
        panic!("the Test setup");
    };
    assert_eq!(error.as_deref(), Some("80 to 450 mbar."));
    assert!(
        ui.start_test(&en(&state, &Feedback::default()), LEAKSHIELD)
            .is_none()
    );
    ui.test_pressure = Some("320,5".into());
    assert_eq!(
        ui.start_test(&en(&state, &Feedback::default()), LEAKSHIELD),
        Some(Command::StartLeakshieldAction {
            leakshield: LEAKSHIELD.into(),
            action: "test".into(),
            test_pressure: Some(320.5)
        })
    );

    // While an Action runs, its view replaces the charts and the bar shows "Cancel" only.
    for (action, check) in [("test", 0), ("measure-fill-level", 1), ("deaerate", 2)] {
        let running = tweak(|s| s.snapshot.leakshields[0].running_action = action.into());
        let model = overview(&running, &OverviewUi::default());
        assert_eq!(model.mode.cancel.as_deref(), Some("Cancel"), "{action}");
        assert!(model.mode.buttons.iter().all(|b| !b.enabled));
        match (check, model.action_view.expect("an Action view")) {
            (
                0,
                ActionView::TestRunning {
                    sources, reference, ..
                },
            ) => {
                assert_eq!(sources.len(), 3);
                assert_eq!(reference, "Reference");
            }
            (1, ActionView::MeasureFillLevel { banner, result, .. }) => {
                assert_eq!(
                    banner,
                    "The fill level measurement runs. The Loop has no leak protection until it ends."
                );
                assert_eq!(result.as_deref(), Some("Fill level 68 %"));
            }
            (
                2,
                ActionView::Deaerate {
                    current, overall, ..
                },
            ) => {
                assert_eq!(
                    (current.as_str(), overall.as_str()),
                    ("Current cycle", "Overall")
                );
            }
            other => panic!("unexpected Action view {other:?}"),
        }
    }

    // The Refill is an instruction: the row opens a dialog with four steps and no start button.
    ui.confirm = Some(Confirm::RefillSteps);
    let dialog = overview(&state, &ui).dialog.expect("the steps");
    assert_eq!(dialog.steps.len(), 4);
    assert_eq!(
        dialog.steps[1],
        "2. Hold the Leakshield button for more than 1 s and select FILL."
    );
    assert!(dialog.confirm.is_none());
}

fn refill_command() -> Command {
    Command::StartLeakshieldAction {
        leakshield: LEAKSHIELD.into(),
        action: "refill".into(),
        test_pressure: None,
    }
}

/// The pump of the Loop of the fixture reads `rpm` on its tach sensor, or has no value.
fn pump_reads(rpm: Option<f64>) -> AppState {
    tweak(|state| {
        for sample in [&mut state.sensors, &mut state.last_sensors]
            .into_iter()
            .flatten()
        {
            sample.sensors.retain(|(id, _, _)| id != "nct6798_fan6");
            if let Some(rpm) = rpm {
                sample.sensors.push(("nct6798_fan6".into(), rpm, 0));
            }
        }
    })
}

#[test]
fn refill_asks_only_while_the_pump_runs() {
    // The pump runs: the click opens the warning and sends nothing.
    let state = pump_reads(Some(2840.0));
    let mut ui = OverviewUi::default();
    assert!(ui.request_action(&state, LEAKSHIELD, "refill").is_none());
    assert_eq!(ui.confirm, Some(Confirm::Refill));
    let dialog = overview(&state, &ui).dialog.expect("the warning");
    assert_eq!(dialog.title, "Start Refill?");
    assert_eq!(
        dialog.text,
        "The pump runs. The Leakshield manual asks for the pump off during Refill."
    );
    assert_eq!(dialog.confirm.as_deref(), Some("Start Refill"));
    // Cancel sends nothing. Confirm starts the Refill.
    ui.cancel_dialog();
    assert!(ui.confirm(LEAKSHIELD).is_none());
    assert!(ui.request_action(&state, LEAKSHIELD, "refill").is_none());
    assert_eq!(ui.confirm(LEAKSHIELD), Some(refill_command()));
    let italian = {
        ui.request_action(&state, LEAKSHIELD, "refill");
        OverviewModel::build(&input(&state, &Feedback::default(), IT), &ui, LEAKSHIELD)
            .dialog
            .expect("the warning")
    };
    assert_eq!(italian.title, "Avviare il rabbocco?");
    assert_eq!(italian.confirm.as_deref(), Some("Avvia il rabbocco"));

    // A pump without a tach value counts as running.
    let state = pump_reads(None);
    let mut ui = OverviewUi::default();
    assert!(ui.request_action(&state, LEAKSHIELD, "refill").is_none());
    assert_eq!(ui.confirm, Some(Confirm::Refill));

    // The pump is off: the Refill starts at once.
    let state = pump_reads(Some(0.0));
    let mut ui = OverviewUi::default();
    assert_eq!(
        ui.request_action(&state, LEAKSHIELD, "refill"),
        Some(refill_command())
    );
    assert!(ui.confirm.is_none());
}

#[test]
fn leakshield_overview_labels() {
    // The labels of the screen file all exist in the English text. The ones with data in them
    // are examples: their fixed part is the text of the app.
    let missing = labels::missing_labels("leakshield-overview.md", &[]);
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let en_text = labels::load("en");
    let it_text = labels::load("it");
    assert_eq!(it_text["leakshield.overview.in_range"], "Nell'intervallo");
    assert_eq!(en_text["leakshield.overview.measure_button"], "Measure");
    // The model uses the language of its input.
    let state = with_leakshield();
    let italian = OverviewModel::build(
        &input(&state, &Feedback::default(), IT),
        &OverviewUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(italian.pressure.tag, "Nell'intervallo");
    assert_eq!(italian.status.tag, "In protezione");
}

#[test]
fn leakshield_sensors_states() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let model = SensorsModel::build(&en(&state, &feedback), &SensorsUi::default(), LEAKSHIELD);
    assert_eq!(model.state, TabState::Ready);
    let names: Vec<&str> = model.device_rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Pressure",
            "Fill level",
            "Volume change",
            "Pressure change",
            "Pump speed",
            "Flow"
        ]
    );
    let kinds: Vec<&str> = model.device_rows.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "Local",
            "Local",
            "Local",
            "Local",
            "From the Feed",
            "From the Feed"
        ]
    );
    assert_eq!(model.device_rows[0].value, "298 mbar");
    assert!(model.device_rows.iter().all(|r| r.enabled && !r.dim));

    // 12 Software sensors. A slot without a source says so.
    assert_eq!(model.software_rows.len(), 12);
    let first = &model.software_rows[0];
    assert_eq!(
        (
            first.name.as_str(),
            first.source_text.as_str(),
            first.value.as_str()
        ),
        ("Software sensor 1", "No source", "--")
    );
    // Slot 3 holds the water temperature. A display page and a controller use it.
    let third = &model.software_rows[2];
    assert_eq!(third.source_text, "Water temperature");
    assert_eq!(third.value, "31.4 °C");
    assert_eq!(
        third.used_by,
        ["Used by display page 3", "Used by lighting controller 2"]
    );
    assert!(third.warning.is_none());
    assert!(
        third
            .choices
            .iter()
            .any(|(id, _, selected)| id == "nct6798_temp1" && *selected)
    );
    assert_eq!(
        model.caption,
        "A Software sensor carries a value from the PC to the Leakshield, for display pages and lighting. The Leakshield has the values only while nicecooling runs."
    );

    // A source without a value has the Warning.
    let mut stale = state.clone();
    stale
        .sensors
        .as_mut()
        .unwrap()
        .sensors
        .iter_mut()
        .for_each(|s| {
            if s.0 == "nct6798_temp1" {
                s.2 = 1;
            }
        });
    let model = SensorsModel::build(&en(&stale, &feedback), &SensorsUi::default(), LEAKSHIELD);
    assert_eq!(model.software_rows[2].value, "--");
    assert_eq!(
        model.software_rows[2].warning.as_deref(),
        Some("Water temperature has no value. The Leakshield keeps the last value.")
    );

    // Clearing a slot in use asks first. A free slot clears at once.
    let mut ui = SensorsUi::default();
    assert!(ui.request_clear(&state, LEAKSHIELD, 3).is_none());
    let dialog = SensorsModel::build(&en(&state, &feedback), &ui, LEAKSHIELD)
        .dialog
        .expect("a dialog");
    assert_eq!(dialog.title, "Clear Software sensor 3?");
    assert_eq!(
        dialog.text,
        "Clear Software sensor 3? display page 3, lighting controller 2 uses it and then shows no value."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Clear")
    );
    assert_eq!(
        ui.confirm_clear(LEAKSHIELD),
        Some(Command::SetSoftwareSensor {
            leakshield: LEAKSHIELD.into(),
            slot: 3,
            sensor: String::new()
        })
    );
    assert!(
        SensorsUi::default()
            .request_clear(&state, LEAKSHIELD, 1)
            .is_some()
    );

    // The inputs are off while the Leakshield does not answer. Loading shows frames.
    let model = SensorsModel::build(
        &en(&offline(), &feedback),
        &SensorsUi::default(),
        LEAKSHIELD,
    );
    assert!(model.device_rows.iter().all(|r| !r.enabled && r.dim));
    assert!(model.software_rows.iter().all(|r| !r.enabled));
    assert_eq!(
        SensorsModel::build(
            &en(&connecting(), &feedback),
            &SensorsUi::default(),
            LEAKSHIELD
        )
        .state,
        TabState::Loading
    );
}

#[test]
fn leakshield_sensors_labels() {
    // The Software sensor table has no Scale, Offset and Unit select in the daemon contract:
    // the unit comes from the source, and the value sent is the value of the source.
    let missing = labels::missing_labels(
        "leakshield-sensors.md",
        &[
            "--",
            "GPU temperature has no value. The Leakshield keeps the last value.",
            "Used by display page 3",
            "Scale",
            "Offset",
            "Value sent",
            "The source is in °C.",
        ],
    );
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let it_text = labels::load("it");
    assert_eq!(it_text["leakshield.sensors.clear_confirm"], "Svuota");
}

fn fields(state: &AppState, ui: &FieldsUi) -> FieldsModel {
    FieldsModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

#[test]
fn leakshield_fields_states() {
    let state = with_leakshield();
    let data = &state.snapshot.leakshield_data[LEAKSHIELD];
    let total = data.fields.len();
    let read_only = data.fields.iter().filter(|f| !f.writable).count();
    let model = fields(&state, &FieldsUi::default());
    assert_eq!(model.state, FieldsState::Ready);
    assert_eq!(
        model.summary,
        format!(
            "{total} fields: {} Verified fields, {read_only} read-only",
            total - read_only
        )
    );
    let groups: Vec<&str> = model.groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(
        groups,
        [
            "Mode",
            "Pressure",
            "Reservoir",
            "Pump",
            "Alarms",
            "Standby",
            "Display",
            "Lighting",
            "Clock"
        ]
    );
    // A read-only Safety field: Code-defined. A writable low-risk field: Plausible read.
    let find = |model: &FieldsModel, id: &str| {
        model
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .find(|r| r.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("row {id}"))
    };
    let volume_sum = find(&model, "reservoir_total_volume");
    assert_eq!(
        (
            volume_sum.risk_text.as_str(),
            volume_sum.verification_text.as_str(),
            volume_sum.access_text.as_str()
        ),
        ("Safety field", "Code-defined", "Read-only")
    );
    assert_eq!(volume_sum.value, "270 ml");
    assert_eq!(
        find(&model, "display_page_time").verification_text,
        "Plausible read"
    );
    assert_eq!(find(&model, "display_page_time").risk_text, "Low risk");
    assert_eq!(
        find(&model, "pump_max_speed").verification_text,
        "Verification write passed"
    );

    // Filters and the search.
    let mut ui = FieldsUi {
        filter: Filter::ReadOnly,
        ..Default::default()
    };
    let rows: Vec<String> = fields(&state, &ui).row_ids();
    assert!(
        rows.iter()
            .all(|id| data.field(id).is_some_and(|f| !f.writable))
    );
    assert_eq!(rows.len(), read_only);
    ui.filter = Filter::All;
    ui.search = "ZZZ".into();
    let none = fields(&state, &ui);
    assert_eq!(
        (none.state, none.empty.as_deref()),
        (FieldsState::Empty, Some("No field matches."))
    );
    ui.search = "reservoir".into();
    assert_eq!(fields(&state, &ui).row_ids().len(), 4);
    assert_eq!(
        fields(
            &state,
            &FieldsUi {
                filter: Filter::Safety,
                ..Default::default()
            }
        )
        .filter_label,
        "Safety fields"
    );

    // A selected row opens its explanation. Up and Down move.
    let mut ui = FieldsUi::default();
    ui.select("reservoir_total_volume");
    assert_eq!(
        find(&fields(&state, &ui), "reservoir_total_volume")
            .explanation
            .as_deref(),
        Some(
            "aquasuite's code defines this field. nicecooling did not confirm it on your device yet."
        )
    );
    ui.select("display_page_time");
    assert_eq!(
        find(&fields(&state, &ui), "display_page_time")
            .explanation
            .as_deref(),
        Some(
            "The value on the device is in the range and equals the default. That is enough for a low-risk field."
        )
    );
    ui.select("pump_max_speed");
    assert_eq!(
        find(&fields(&state, &ui), "pump_max_speed")
            .explanation
            .as_deref(),
        Some("A Verification write passed for firmware 1040.")
    );
    let ids = fields(&state, &FieldsUi::default()).row_ids();
    ui.step(&ids, 1);
    assert_eq!(
        ui.selected.as_deref(),
        ids.iter()
            .skip_while(|id| *id != "pump_max_speed")
            .nth(1)
            .map(String::as_str)
    );

    // "Open Fields" of another tab: the read-only filter with the row selected.
    let mut page = nicecooling::screens::leakshield::LeakshieldUi::default();
    page.open_fields(Some("default_mode"));
    assert_eq!(page.tab, nicecooling::screens::leakshield::Tab::Fields);
    assert_eq!(page.fields.filter, Filter::ReadOnly);
    assert_eq!(page.fields.selected.as_deref(), Some("default_mode"));

    // The copy text has one line for each row. The Leakshield does not answer: the table dims.
    assert_eq!(model.copy_text.lines().count(), total);
    assert!(fields(&offline(), &FieldsUi::default()).dim);
    assert_eq!(
        fields(&connecting(), &FieldsUi::default()).state,
        FieldsState::Loading
    );
}

#[test]
fn unknown_firmware_shows_reason() {
    let state = unknown_firmware();
    let model = fields(&state, &FieldsUi::default());
    assert_eq!(model.state, FieldsState::UnknownFirmware);
    assert_eq!(
        model.unknown_text.as_deref(),
        Some("nicecooling cannot decode these settings. Every field is read-only.")
    );
    // Every tab has the reason of the device for each field.
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let device = nicecooling::screens::leakshield::Device::new(&screen_input, LEAKSHIELD);
    let access = device.access("pump_max_speed");
    assert!(!access.writable);
    assert_eq!(access.reason, "unknown-firmware");
    assert_eq!(
        device.reason_text(&access.reason),
        "Read-only: unknown firmware."
    );
    // The wizard hides the per-field reasons: the banner of the shell gives one reason.
    let wizard = SettingsModel::build(&en(&state, &feedback), &SettingsUi::default(), LEAKSHIELD);
    assert!(wizard.unknown_firmware);
    assert!(
        wizard
            .rows
            .iter()
            .all(|row| !row.writable && row.reason.is_none() && row.link.is_none())
    );
    // The System tab says that the firmware is unknown.
    let system = SystemModel::build(&en(&state, &feedback), &SystemUi::default(), LEAKSHIELD);
    assert_eq!(system.device.firmware_tag, "Unknown");
    assert_eq!(
        system.device.unknown_text.as_deref(),
        Some("nicecooling has no field layout for this firmware. Every field is read-only.")
    );
    assert_eq!(
        system.device.update_hint,
        "To update the firmware, use aquasuite on Windows."
    );
}

#[test]
fn no_verification_write_control() {
    // The tab has no control that writes: its model has no command, and its source has none.
    for source in [
        include_str!("../src/screens/leakshield/fields.rs"),
        include_str!("../src/screens/leakshield/fields_view.rs"),
    ] {
        for word in [
            "Command",
            "SetLeakshield",
            "fields_command",
            "SetFields",
            "send(",
        ] {
            assert!(!source.contains(word), "the Fields tab mentions {word}");
        }
    }
    let state = with_leakshield();
    for selected in [None, Some("reservoir_total_volume".to_owned())] {
        let ui = FieldsUi {
            selected,
            ..Default::default()
        };
        let model = fields(&state, &ui);
        // Every row only reads: it has no field that holds a command or a value to write.
        assert!(
            model
                .groups
                .iter()
                .flat_map(|g| &g.rows)
                .all(|row| !row.id.is_empty())
        );
    }
}

#[test]
fn leakshield_fields_labels() {
    let missing = labels::missing_labels(
        "leakshield-fields.md",
        &[
            "112 fields: 96 Verified fields, 16 read-only",
            "A Verification write passed on 2026-11-20 for firmware 1022.",
            // The daemon sends no raw report, no offset and no contradicting read.
            "Offset",
            "Bytes",
            "The value 913 is outside the range 0 to 450. The field layout can be wrong for this firmware.",
            // A Safety field with a plausible read has no state of its own in `GetFields`.
            "A Safety field needs a Verification write before nicecooling writes it. The app does not run a Verification write: it is a supervised procedure. See the project documentation.",
        ],
    );
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    assert_eq!(
        labels::load("it")["leakshield.fields.empty"],
        "Nessun campo corrisponde."
    );
}

fn alarms(state: &AppState, feedback: &Feedback) -> AlarmsModel {
    AlarmsModel::build(&en(state, feedback), &AlarmsUi, LEAKSHIELD)
}

fn row<'a>(
    model: &'a AlarmsModel,
    event: &str,
) -> &'a nicecooling::screens::leakshield::alarms::EventRow {
    model
        .groups
        .iter()
        .flat_map(|g| &g.rows)
        .find(|r| {
            r.event == event
                || r.response.event == format!("leakshield.{event}")
                || r.response.event == event
        })
        .unwrap_or_else(|| panic!("row {event}"))
}

#[test]
fn leakshield_alarms_states() {
    let mut state = with_leakshield();
    // The resolved rows of the daemon: the Large leak powers off after 30 s, the others notify.
    state.snapshot.responses = [
        "leakshield.leak-warning",
        "leakshield.leak-alarm",
        "leakshield-unreachable",
    ]
    .into_iter()
    .map(|event| ResponseRule {
        event: event.into(),
        source: LEAKSHIELD.into(),
        response: if event == "leakshield.leak-alarm" {
            Response::Poweroff
        } else {
            Response::Notify
        },
        delay_s: 30,
        overridden: false,
    })
    .collect();
    let feedback = Feedback::default();
    let model = alarms(&state, &feedback);
    assert_eq!(model.state, TabState::Ready);
    let groups: Vec<&str> = model.groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(groups, ["Leakage", "Fill level", "System", "nicecooling"]);
    let titles: Vec<&str> = model
        .groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.title.as_str()))
        .collect();
    assert_eq!(
        titles,
        [
            "Leak Warning",
            "Small leak Alarm",
            "Large leak Alarm",
            "Fill level Warning",
            "Fill level Alarm",
            "System Alarm",
            "Leakshield unreachable"
        ]
    );
    assert_eq!(
        model.columns,
        ["Event", "Threshold", "Device response", "Alarm response"]
    );

    // The thresholds: the leak Warning in ml/h, the fill level in percent. The Alarms of a leak
    // have none.
    let leak = row(&model, "leak-warning");
    let threshold = leak.threshold.as_ref().unwrap();
    assert_eq!(
        (
            threshold.value,
            threshold.min,
            threshold.max,
            threshold.unit.as_str()
        ),
        (150.0, 50.0, 200.0, "ml/h")
    );
    assert!(row(&model, "leak-alarm").threshold.is_none());
    assert_eq!(
        row(&model, "fill-level-low-alarm")
            .threshold
            .as_ref()
            .unwrap()
            .unit,
        "%"
    );
    assert!(row(&model, "system-alarm").threshold.is_none());
    assert_eq!(
        leak.caption,
        "The volume change passes the threshold. Temperature changes also move this value."
    );
    assert_eq!(
        row(&model, "leak-alarm").caption,
        "The Leakshield detects the leak size. You cannot set a threshold."
    );
    assert_eq!(
        row(&model, "system-alarm").caption,
        "A fault of the Leakshield or of its power supply."
    );

    // The Device response, and the sound for a Warning only.
    let large = row(&model, "leak-alarm").device.as_ref().unwrap();
    assert_eq!(large.current, "signal-long");
    let mut sent = with_leakshield();
    let data = sent.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    for f in &mut data.fields {
        if f.id == "leak_rate_alarm_2_response_action" {
            f.value = FieldValue::Text("key-sequence".into());
        }
    }
    sent.snapshot.responses = state.snapshot.responses.clone();
    let model = alarms(&sent, &Feedback::default());
    assert_eq!(
        row(&model, "leak-alarm").device.as_ref().unwrap().current,
        "key-sequence"
    );
    assert_eq!(
        large
            .options
            .iter()
            .map(|o| o.1.as_str())
            .collect::<Vec<_>>(),
        [
            "None",
            "Signal: short press",
            "Signal: long press",
            "Key sequence"
        ]
    );
    assert!(row(&model, "leak-warning").sound.is_some());
    assert!(row(&model, "leak-alarm").sound.is_none());
    assert!(row(&model, "leakshield-unreachable").is_empty_event());

    // The defaults of the alarm response: the Large leak powers off after 30 s.
    let response = &row(&model, "leak-alarm").response;
    assert_eq!(
        (response.response, response.delay_s),
        (Response::Poweroff, 30)
    );
    assert!(response.delay_enabled);
    assert_eq!(
        row(&model, "leak-warning").response.response,
        Response::Notify
    );
    assert_eq!(
        row(&model, "leakshield-unreachable").response.response,
        Response::Notify
    );
    assert!(row(&model, "leakshield-unreachable").device.is_none());

    // A delay of 0 warns. The daemon holds the response: the control works while the device
    // field is read-only. The System Alarm response is read-only in the fixture.
    let mut zero = state.clone();
    zero.snapshot.responses = vec![ResponseRule {
        event: "leakshield.leak-alarm".into(),
        source: String::new(),
        response: Response::Poweroff,
        delay_s: 0,
        overridden: false,
    }];
    let zero_model = alarms(&zero, &feedback);
    assert_eq!(
        row(&zero_model, "leak-alarm")
            .response
            .delay_warning
            .as_deref(),
        Some("With 0 s you cannot cancel the poweroff.")
    );
    let system = row(&model, "system-alarm");
    assert!(!system.device.as_ref().unwrap().writable);
    assert_eq!(
        system.device.as_ref().unwrap().reason.as_deref(),
        Some("Not a Verified field: this Safety field has no Verification write yet.")
    );
    assert!(
        row(&model, "system-alarm").response.delay_enabled
            || row(&model, "system-alarm").response.response != Response::Poweroff
    );

    // The fill level Alarm above the Warning: the order is wrong.
    let mut wrong = state.clone();
    set_field(
        &mut wrong,
        "fill_level_alarm_threshold",
        FieldValue::Number(70.0),
    );
    let wrong_model = alarms(&wrong, &feedback);
    assert_eq!(
        row(&wrong_model, "fill-level-low-alarm")
            .threshold
            .as_ref()
            .unwrap()
            .error
            .as_deref(),
        Some("The Alarm threshold must be below the Warning threshold.")
    );
    assert!(
        row(&model, "fill-level-low-alarm")
            .threshold
            .as_ref()
            .unwrap()
            .error
            .is_none()
    );

    // An active event shows its time.
    let mut active = state.clone();
    let mut raised = alarm(
        "a1",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    );
    raised.raised_us = 1_000_000_000;
    active.snapshot.alarms = vec![raised];
    let active_model = alarms(&active, &feedback);
    assert!(
        row(&active_model, "leak-warning")
            .active
            .as_deref()
            .is_some_and(|text| text.starts_with("Active since "))
    );
    assert!(row(&active_model, "leak-alarm").active.is_none());

    // The Leakshield does not answer.
    assert_eq!(alarms(&offline(), &feedback).state, TabState::Offline);
    assert_eq!(alarms(&connecting(), &feedback).state, TabState::Loading);
}

trait EmptyEvent {
    fn is_empty_event(&self) -> bool;
}

impl EmptyEvent for nicecooling::screens::leakshield::alarms::EventRow {
    /// The row of nicecooling has no device event.
    fn is_empty_event(&self) -> bool {
        self.threshold.is_none() && self.device.is_none() && self.sound.is_none()
    }
}

#[test]
fn overlap_warning_shown() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let model = alarms(&state, &feedback);
    let kinds: Vec<RecommendationKind> = model.recommendations.iter().map(|r| r.kind).collect();
    // The key sequence of the Small leak Alarm, the overlap of the Large leak Alarm and the
    // signal of the System Alarm.
    assert_eq!(
        kinds,
        [
            RecommendationKind::Warning,
            RecommendationKind::Warning,
            RecommendationKind::Information
        ]
    );
    let texts: Vec<&str> = model
        .recommendations
        .iter()
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(
        texts[0],
        "The key sequence is on for the Small leak Alarm. The Leakshield types it into the focused window, a terminal included."
    );
    assert_eq!(
        texts[1],
        "Two poweroff responses overlap for the Large leak Alarm: the device signal and nicecooling. The signal cuts the power without a clean poweroff."
    );
    assert_eq!(
        texts[2],
        "Recommended: a signal as a long press for the System Alarm. It is the only protection when Linux hangs."
    );
    // Each button writes one setting through the normal write path.
    let buttons: Vec<&str> = model
        .recommendations
        .iter()
        .map(|r| r.button.as_ref().map_or("", |b| b.0.as_str()))
        .collect();
    assert_eq!(
        buttons,
        ["Turn it off", "Set the Device response to None", ""],
        "the System Alarm field is read-only: no button"
    );
    assert_eq!(
        model.recommendations[1].button.as_ref().unwrap().1,
        Command::SetLeakshieldFields {
            leakshield: LEAKSHIELD.into(),
            control: "recommendation:leak-alarm".into(),
            fields: vec![(
                "leak_rate_alarm_2_response_action".into(),
                FieldValue::Text("none".into())
            )]
        }
    );

    // All as recommended: a success row.
    let mut matching = state.clone();
    matching
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .proposals
        .iter_mut()
        .for_each(|p| {
            p.current = "none".into();
            p.proposed = "none".into();
            p.overlap.clear();
        });
    let model = alarms(&matching, &feedback);
    assert_eq!(model.recommendations.len(), 1);
    assert_eq!(model.recommendations[0].kind, RecommendationKind::Success);
    assert_eq!(
        model.recommendations[0].text,
        "The Device responses match the recommendation."
    );

    // The daemon gives no proposal: the card is empty.
    let mut none = state.clone();
    none.snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .proposals
        .clear();
    assert!(alarms(&none, &feedback).recommendations.is_empty());
}

#[test]
fn each_overlap_id_shows_its_own_text() {
    let mut state = with_leakshield();
    let feedback = Feedback::default();
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.proposals.retain(|p| p.alarm == "leak-alarm");
    for (overlap, text) in [
        (
            "double-poweroff",
            "Two poweroff responses overlap for the Large leak Alarm",
        ),
        (
            "key-sequence-on",
            "The key sequence is on for the Large leak Alarm.",
        ),
    ] {
        let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
        data.proposals[0].overlap = overlap.into();
        let model = alarms(&state, &feedback);
        assert_eq!(model.recommendations.len(), 1);
        assert!(
            model.recommendations[0].text.starts_with(text),
            "{overlap}: {}",
            model.recommendations[0].text
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn accepting_a_proposal_sends_the_name() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let state = with_leakshield();
    let model = alarms(&state, &Feedback::default());
    // The Large leak Alarm proposes none, the Small one turns the key sequence off.
    let button = model.recommendations[1].button.as_ref().unwrap();
    run(&harness, &button.1).await;
    assert_eq!(
        daemon.calls().last().unwrap(),
        "SetFields(leak_rate_alarm_2_response_action=none) interactive=true"
    );
}

#[test]
fn bit_3_of_the_standby_conditions_disables_the_alarm_actions_in_standby() {
    let feedback = Feedback::default();
    let conditions = |value: f64| {
        tweak(|state| set_field(state, "standby_conditions", FieldValue::Number(value)))
    };
    // The System tab names the bit. Bit 3 set is "on".
    let system_rows = |value: f64| {
        let state = conditions(value);
        SystemModel::build(&en(&state, &feedback), &SystemUi::default(), LEAKSHIELD)
            .standby
            .rows
    };
    let rows = system_rows(11.0);
    assert_eq!(rows[2].label, "Disable alarm actions during standby");
    assert!(rows[2].on && rows[2].bit == 8);
    assert!(!system_rows(3.0)[2].on);
    // The Alarms tab has the same switch.
    let option = |value: f64| {
        let state = conditions(value);
        alarms(&state, &feedback)
            .options
            .into_iter()
            .find(|o| o.key == "standby_actions")
            .unwrap()
    };
    let on = option(11.0);
    assert_eq!(on.label, "Disable alarm actions during standby");
    assert!(on.on && on.writable);
    assert!(!option(3.0).on);
    // A write changes bit 3 only.
    let state = conditions(3.0);
    let write = |state: &AppState, on: bool| {
        let Some(Command::SetLeakshieldFields { fields, .. }) =
            AlarmsUi.set_option(&en(state, &feedback), LEAKSHIELD, "standby_actions", on)
        else {
            panic!("a field write");
        };
        fields[0].clone()
    };
    assert_eq!(
        write(&state, true),
        ("standby_conditions".to_owned(), FieldValue::Number(11.0))
    );
    assert_eq!(write(&conditions(11.0), false).1, FieldValue::Number(3.0));
    assert!(
        AlarmsUi
            .set_option(&en(&state, &feedback), LEAKSHIELD, "nothing", true)
            .is_none()
    );
    // A read-only device sends nothing.
    let unknown = unknown_firmware();
    assert!(
        AlarmsUi
            .set_option(
                &en(&unknown, &feedback),
                LEAKSHIELD,
                "standby_actions",
                true
            )
            .is_none()
    );
    // Italian.
    let italian = AlarmsModel::build(&input(&state, &feedback, IT), &AlarmsUi, LEAKSHIELD);
    assert_eq!(
        italian.options[0].label,
        "Disattiva le azioni di allarme durante lo standby"
    );
}

#[test]
fn the_daily_pump_time_warning_limit_is_in_the_leak_group() {
    let feedback = Feedback::default();
    let state =
        tweak(|state| set_field(state, "daily_pump_time_warning", FieldValue::Number(90.0)));
    let model = alarms(&state, &feedback);
    let leakage = &model.groups[0];
    assert_eq!(leakage.title, "Leakage");
    let [limit] = &leakage.limits[..] else {
        panic!("one limit: {:?}", leakage.limits);
    };
    assert_eq!(limit.field, "daily_pump_time_warning");
    assert_eq!(limit.label, "Daily pump time warning limit");
    assert_eq!(
        limit.text,
        "The Leakshield warns when the daily pump time passes this limit. From 5 to 240 s."
    );
    assert_eq!(
        (limit.value, limit.unit.as_str(), limit.writable),
        (90.0, "s", true)
    );
    // The other groups have no limit.
    assert!(model.groups[1..].iter().all(|g| g.limits.is_empty()));

    // The value is a whole number of seconds from 5 to 240.
    let input = en(&state, &feedback);
    let written = |value: f64| {
        let Some(Command::SetLeakshieldFields { fields, .. }) =
            AlarmsUi.set_limit(&input, LEAKSHIELD, "daily_pump_time_warning", value)
        else {
            panic!("a field write for {value}");
        };
        fields[0].clone()
    };
    for (typed, raw) in [
        (90.0, 90.0),
        (240.0, 240.0),
        (300.0, 240.0),
        (2.0, 5.0),
        (12.4, 12.0),
    ] {
        assert_eq!(
            written(typed),
            (
                "daily_pump_time_warning".to_owned(),
                FieldValue::Number(raw)
            ),
            "{typed}"
        );
    }
    assert!(
        AlarmsUi
            .set_limit(&input, LEAKSHIELD, "daily_pump_time_warning", f64::NAN)
            .is_none()
    );
    assert!(
        AlarmsUi
            .set_limit(&input, LEAKSHIELD, "loop_height", 5.0)
            .is_none()
    );
    // A read-only device sends nothing and shows the reason.
    let unknown = unknown_firmware();
    assert!(
        AlarmsUi
            .set_limit(
                &en(&unknown, &feedback),
                LEAKSHIELD,
                "daily_pump_time_warning",
                20.0
            )
            .is_none()
    );
    let model = alarms(&unknown, &feedback);
    assert!(!model.groups[0].limits[0].writable && model.groups[0].limits[0].reason.is_some());
    // The old name is gone from the fields.
    let fields = &state.snapshot.leakshield_data[LEAKSHIELD].fields;
    assert!(fields.iter().all(|f| f.id != "refill_duration_warning"));
    // Italian.
    let italian = AlarmsModel::build(&self::input(&state, &feedback, IT), &AlarmsUi, LEAKSHIELD);
    assert_eq!(
        italian.groups[0].limits[0].text,
        "Il Leakshield avvisa quando il tempo pompa giornaliero supera questo limite. Da 5 a 240 s."
    );
}

#[test]
fn bits_6_and_7_of_the_shield_flags_are_the_alarm_buzzer_switches() {
    let feedback = Feedback::default();
    let flags =
        |value: f64| tweak(|state| set_field(state, "shield_flags", FieldValue::Number(value)));
    let option = |state: &AppState, key: &str| {
        alarms(state, &feedback)
            .options
            .into_iter()
            .find(|o| o.key == key)
            .unwrap_or_else(|| panic!("no option {key}"))
    };
    // The fixture holds 127: bit 6 set, bit 7 clear.
    let state = flags(127.0);
    let until = option(&state, "buzzer_until_confirmation");
    assert_eq!(until.label, "Alarm buzzer until confirmation");
    assert!(until.on && until.writable);
    let off = option(&state, "buzzer_off");
    assert_eq!(off.label, "Disable the alarm buzzer");
    assert!(!off.on);
    assert!(option(&flags(128.0), "buzzer_off").on);
    assert!(!option(&flags(128.0), "buzzer_until_confirmation").on);
    // A write changes its bit only.
    let write = |state: &AppState, key: &str, on: bool| {
        let Some(Command::SetLeakshieldFields { fields, .. }) =
            AlarmsUi.set_option(&en(state, &feedback), LEAKSHIELD, key, on)
        else {
            panic!("a field write");
        };
        fields[0].clone()
    };
    assert_eq!(
        write(&state, "buzzer_off", true),
        ("shield_flags".to_owned(), FieldValue::Number(255.0))
    );
    assert_eq!(
        write(&state, "buzzer_until_confirmation", false).1,
        FieldValue::Number(63.0)
    );
    // Italian.
    let italian = AlarmsModel::build(&input(&state, &feedback, IT), &AlarmsUi, LEAKSHIELD);
    let texts: Vec<_> = italian.options.iter().map(|o| o.label.as_str()).collect();
    assert!(texts.contains(&"Segnale acustico fino alla conferma"));
    assert!(texts.contains(&"Disattiva il segnale acustico degli allarmi"));
}

#[test]
fn leakshield_alarms_commands() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let ui = AlarmsUi;
    // A threshold writes one field. The Alarm threshold above the Warning sends nothing.
    assert_eq!(
        ui.set_threshold(
            &screen_input,
            LEAKSHIELD,
            "fill_level_warning_threshold",
            55.0
        ),
        Some(Command::SetLeakshieldFields {
            leakshield: LEAKSHIELD.into(),
            control: "threshold:fill_level_warning_threshold".into(),
            fields: vec![(
                "fill_level_warning_threshold".into(),
                FieldValue::Number(55.0)
            )]
        })
    );
    assert!(
        ui.set_threshold(
            &screen_input,
            LEAKSHIELD,
            "fill_level_alarm_threshold",
            65.0
        )
        .is_none()
    );
    assert!(
        ui.set_threshold(
            &screen_input,
            LEAKSHIELD,
            "leak_rate_warning_threshold",
            20.0
        )
        .is_none(),
        "below 50 ml/h"
    );
    // The sound is one bit of the buzzer mask.
    assert_eq!(
        ui.set_sound(&screen_input, LEAKSHIELD, "leak-warning", true),
        Some(Command::SetLeakshieldFields {
            leakshield: LEAKSHIELD.into(),
            control: "sound:leak-warning".into(),
            fields: vec![("alarm_buzzer".into(), FieldValue::Number(5.0))]
        })
    );
    assert!(
        ui.set_sound(&screen_input, LEAKSHIELD, "leak-alarm", true)
            .is_none(),
        "an Alarm always sounds"
    );
    // The Device response of a read-only field sends nothing.
    assert!(
        ui.set_device_response(&screen_input, LEAKSHIELD, "system-alarm", "signal-long")
            .is_none()
    );
    assert!(
        ui.set_device_response(&screen_input, LEAKSHIELD, "leak-warning", "key-sequence")
            .is_some()
    );
    assert!(
        ui.set_device_response(&screen_input, LEAKSHIELD, "leak-warning", "nonsense")
            .is_none()
    );
    // The alarm response goes to the daemon, with the delay that it holds.
    assert_eq!(
        ui.set_alarm_response(
            &screen_input,
            LEAKSHIELD,
            "leakshield.leak-alarm",
            Response::Notify,
            None
        ),
        Command::SetAlarmResponse {
            event: "leakshield.leak-alarm".into(),
            source: LEAKSHIELD.into(),
            response: Response::Notify,
            delay_s: 30
        }
    );
}

#[test]
fn leakshield_alarms_labels() {
    let missing = labels::missing_labels("leakshield-alarms.md", &["Active since 21:46:51"]);
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let model = AlarmsModel::build(
        &input(&with_leakshield(), &Feedback::default(), IT),
        &AlarmsUi,
        LEAKSHIELD,
    );
    assert_eq!(model.columns[3], "Risposta all'allarme");
    assert_eq!(model.groups[0].title, "Perdite");
}

fn display(state: &AppState, ui: &DisplayUi) -> DisplayModel {
    DisplayModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

/// The pages of the reference device, bytes 73 + 46 n of its report 3.
fn reference_pages(state: &mut AppState) {
    let templates = [
        "leakshield-state",
        "values-1",
        "chart",
        "values-2",
        "values-2",
        "chart",
        "value-and-bar",
        "values-2",
        "time",
        "logo",
    ];
    set_field(
        state,
        "page_template",
        FieldValue::TextList(templates.map(String::from).to_vec()),
    );
    let mut sources = vec![255.0; 80];
    for (page, bytes) in [
        (1, &[0.0][..]),
        (2, &[0.0]),
        (3, &[0.0, 12.0]),
        (4, &[4.0, 8.0]),
        (5, &[8.0]),
        (6, &[4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        (7, &[16.0, 20.0]),
    ] {
        sources[page * 8..page * 8 + bytes.len()].copy_from_slice(bytes);
    }
    set_field(state, "page_sources", FieldValue::List(sources));
}

#[test]
fn a_page_source_byte_is_the_sensor_index_times_four() {
    use nicecooling::screens::leakshield::display::{NO_SOURCE, source_byte, source_sensor};
    assert_eq!(
        [0.0, 4.0, 16.0, 20.0, 76.0, 79.0].map(source_sensor),
        [0, 1, 4, 5, 19, 19]
    );
    // 80 or more has no sensor. 255 is none.
    assert_eq!([80.0, 100.0, 255.0].map(source_sensor), [NO_SOURCE; 3]);
    assert_eq!(
        [0, 1, 5, 19, 255].map(source_byte),
        [Some(0), Some(4), Some(20), Some(76), Some(255)]
    );
    assert_eq!([20, 21, 254].map(source_byte), [None; 3]);
}

#[test]
fn the_display_reads_the_sources_of_the_reference_device() {
    let mut state = with_leakshield();
    reference_pages(&mut state);
    let picked = |model: &DisplayModel| -> Vec<u8> {
        model
            .page
            .rows
            .iter()
            .map(|row| row.sources.iter().find(|s| s.2).map_or(0, |s| s.0))
            .collect()
    };
    let mut ui = DisplayUi::default();
    // Page: the sensors that the report names (0 pressure, 1 fill level, 2 volume change,
    // 3 pressure change, 4 pump speed, 5 flow).
    for (page, sensors) in [
        (1, vec![0]),
        (2, vec![0]),
        (3, vec![0, 3]),
        (4, vec![1, 2]),
        (5, vec![2]),
        (6, vec![1]),
        (7, vec![4, 5]),
    ] {
        ui.select(page);
        assert_eq!(picked(&display(&state, &ui)), sensors, "page {}", page + 1);
    }
    let model = display(&state, &DisplayUi::default());
    assert_eq!(
        model.pages[4]
            .lines
            .iter()
            .map(|l| l.0.as_str())
            .collect::<Vec<_>>(),
        ["Fill level", "Volume change"]
    );
    assert_eq!(
        model.pages[7]
            .lines
            .iter()
            .map(|l| l.0.as_str())
            .collect::<Vec<_>>(),
        ["Pump speed", "Flow"]
    );
    // No page of the reference device shows a Software sensor.
    assert!(
        model
            .pages
            .iter()
            .all(|p| p.lines.iter().all(|l| !l.0.contains("Software")))
    );
}

#[test]
fn a_source_picked_in_the_app_is_written_as_the_sensor_index_times_four() {
    let mut state = with_leakshield();
    reference_pages(&mut state);
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let mut ui = DisplayUi::default();
    ui.select(7);
    let written = |ui: &DisplayUi, slot: usize, sensor: u8| {
        let Command::SetLeakshieldFields { fields, .. } = ui
            .set_source(&screen_input, LEAKSHIELD, slot, sensor)
            .expect("a command")
        else {
            panic!("a field write");
        };
        let FieldValue::List(sources) = &fields[0].1 else {
            panic!("a list")
        };
        sources.clone()
    };
    // Volume change (sensor 2) in slot 0 of page 8: byte 8.
    assert_eq!(written(&ui, 0, 2)[7 * 8..7 * 8 + 2], [8.0, 20.0]);
    // Software sensor 3 (sensor 10) is byte 40. No source is 255.
    assert_eq!(written(&ui, 1, 10)[7 * 8 + 1], 40.0);
    assert_eq!(written(&ui, 1, 255)[7 * 8 + 1], 255.0);
    // The device has 20 sensors.
    assert!(ui.set_source(&screen_input, LEAKSHIELD, 0, 20).is_none());
    // The written list reads back as the picked sensors.
    let mut again = state.clone();
    set_field(
        &mut again,
        "page_sources",
        FieldValue::List(written(&ui, 1, 10)),
    );
    let model = display(&again, &ui);
    let picked: Vec<u8> = model
        .page
        .rows
        .iter()
        .map(|row| row.sources.iter().find(|s| s.2).unwrap().0)
        .collect();
    assert_eq!(picked, [4, 10]);
}

#[test]
fn the_sensors_tab_finds_the_pages_that_use_a_software_sensor() {
    // Software sensor 3 is sensor 10: the byte 40 of page 3 in the fixture.
    let model = SensorsModel::build(
        &en(&with_leakshield(), &Feedback::default()),
        &SensorsUi::default(),
        LEAKSHIELD,
    );
    assert!(
        model.software_rows[2]
            .used_by
            .contains(&"Used by display page 3".to_owned())
    );
    // A byte of 4 is the sensor 1, not a Software sensor.
    let mut state = with_leakshield();
    reference_pages(&mut state);
    let model = SensorsModel::build(
        &en(&state, &Feedback::default()),
        &SensorsUi::default(),
        LEAKSHIELD,
    );
    assert!(model.software_rows.iter().all(|row| {
        row.used_by
            .iter()
            .all(|text| !text.contains("display page"))
    }));
}

#[test]
fn leakshield_display_states() {
    let state = with_leakshield();
    let model = display(&state, &DisplayUi::default());
    assert_eq!(model.state, TabState::Ready);
    assert_eq!(model.pages.len(), 10);
    assert_eq!(model.pages[0].template_name, "Leakshield state");
    assert!(model.pages[0].selected && !model.pages[1].selected);
    // Page 2 shows the pressure. Page 3 shows the pressure and Software sensor 3.
    assert_eq!(model.pages[1].template_name, "1 value");
    assert_eq!(
        model.pages[1].lines,
        [("Pressure".to_owned(), "298 mbar".to_owned())]
    );
    assert_eq!(model.pages[2].template_name, "2 values");
    assert_eq!(
        model.pages[2].lines[1],
        (
            "Software sensor 3: Water temperature".to_owned(),
            "31.4 °C".to_owned()
        )
    );

    // A page without a template: "Empty" and the button.
    assert!(model.pages[3].empty);
    assert_eq!(model.pages[3].template_name, "Empty");
    let mut ui = DisplayUi::default();
    ui.select(3);
    let empty = display(&state, &ui);
    assert_eq!(empty.page.choose.as_deref(), Some("Choose a template"));
    assert_eq!(empty.page.title, "Page 4");

    // The selected page: one row for each value of the template, with its sources.
    ui.select(2);
    let model = display(&state, &ui);
    assert_eq!(model.page.title, "Page 3");
    assert_eq!(model.page.template_name, "2 values");
    assert_eq!(model.page.rows.len(), 2);
    assert_eq!(model.page.rows[0].sources.len(), 6 + 12 + 1);
    assert!(
        model.page.rows[1]
            .sources
            .iter()
            .any(|(i, _, selected)| *i == 10 && *selected)
    );
    assert!(model.page.normal.on && model.page.standby.on);

    // A value on a Software sensor without a source: the warning and the link.
    let mut dangling = state.clone();
    set_field(&mut dangling, "page_sources", {
        let mut sources = vec![255.0; 80];
        sources[16] = 0.0;
        sources[17] = 48.0;
        FieldValue::List(sources)
    });
    let model = display(&dangling, &ui);
    assert_eq!(
        model.page.rows[1].warning.as_deref(),
        Some("Software sensor 5 has no data source.")
    );
    assert_eq!(model.page.rows[1].value, "--");
    assert_eq!(model.page.link, "Open Sensors");

    // A page that both switches turn off: dim, "Not shown".
    let mut hidden = state.clone();
    set_field(&mut hidden, "page_options", FieldValue::List(vec![2.0; 10]));
    let model = display(&hidden, &DisplayUi::default());
    assert_eq!(model.pages[0].off.as_deref(), Some("Not shown"));
    assert!(!model.page.standby.on && !model.page.normal.on);

    // The settings.
    let settings = &display(&state, &DisplayUi::default()).settings;
    assert_eq!(settings.brightness[0].shown, "Low");
    assert_eq!(settings.brightness[0].options.len(), 4);
    assert!(
        settings.auto_page.on
            && !settings.rotate.on
            && !settings.invert.on
            && settings.auto_invert.on
            && settings.standby.on
    );
    assert_eq!(settings.interval, "10");
    assert!(settings.interval_writable);
    assert_eq!(
        settings.auto_invert_text,
        "The display inverts half of the time. This slows the wear of the OLED pixels."
    );

    // The template gallery.
    ui.gallery = true;
    let gallery = display(&state, &ui).gallery.expect("the gallery");
    assert_eq!(gallery.len(), 14);
    assert_eq!(gallery[0], ("logo".to_owned(), "Logo".to_owned()));
    assert_eq!(gallery[13].0, "leakshield-state");

    // Read-only with an unknown firmware, and the states of the link.
    let unknown = display(&unknown_firmware(), &DisplayUi::default());
    assert_eq!(
        unknown.reason.as_deref(),
        Some("Read-only: unknown firmware.")
    );
    assert!(!unknown.settings.brightness[0].writable && !unknown.page.normal.writable);
    assert_eq!(
        display(&offline(), &DisplayUi::default()).state,
        TabState::Offline
    );
    assert_eq!(
        display(&connecting(), &DisplayUi::default()).state,
        TabState::Loading
    );
}

#[test]
fn leakshield_display_commands() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let mut ui = DisplayUi::default();
    ui.select(3);
    let command = ui
        .set_template(&screen_input, LEAKSHIELD, "bars-4a")
        .expect("a command");
    let Command::SetLeakshieldFields {
        fields, control, ..
    } = command
    else {
        panic!("a field write");
    };
    assert_eq!(control, "display:template");
    assert_eq!(
        fields[0].1,
        FieldValue::TextList(
            [
                "leakshield-state",
                "values-1",
                "values-2",
                "bars-4a",
                "none",
                "none",
                "none",
                "none",
                "none",
                "none"
            ]
            .map(String::from)
            .to_vec()
        )
    );
    assert!(!ui.gallery);
    // The source of a value of page 4 is one entry of the flat list.
    let Command::SetLeakshieldFields { fields, .. } = ui
        .set_source(&screen_input, LEAKSHIELD, 1, 4)
        .expect("a command")
    else {
        panic!("a field write");
    };
    let FieldValue::List(sources) = &fields[0].1 else {
        panic!("a list")
    };
    assert_eq!(sources[3 * 8 + 1], 16.0);
    // "Show in normal operation" is the bit that hides the page.
    let Command::SetLeakshieldFields { fields, .. } = ui
        .set_option(&screen_input, LEAKSHIELD, 2, true)
        .expect("a command")
    else {
        panic!("a field write");
    };
    assert_eq!(
        fields[0].1,
        FieldValue::List(vec![1.0, 1.0, 1.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0])
    );
    let Command::SetLeakshieldFields { fields, .. } = ui
        .set_option(&screen_input, LEAKSHIELD, 1, false)
        .expect("a command")
    else {
        panic!("a field write");
    };
    assert_eq!(
        fields[0].1,
        FieldValue::List(vec![1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0])
    );
    // The flags of the display, the brightness and the interval.
    use nicecooling::screens::leakshield::display::{set_brightness, set_flag, set_interval};
    assert!(matches!(
        set_flag(&screen_input, LEAKSHIELD, 4, true),
        Some(Command::SetLeakshieldFields { fields, .. }) if fields[0].1 == FieldValue::Number(4.0)
    ));
    // The brightness goes out as its registry name.
    let Some(Command::SetLeakshieldFields { fields, .. }) =
        set_brightness(&screen_input, LEAKSHIELD, "display_brightness_idle", "off")
    else {
        panic!("a field write");
    };
    assert_eq!(
        fields,
        [(
            "display_brightness_idle".to_owned(),
            FieldValue::Text("off".into())
        )]
    );
    assert!(set_brightness(&screen_input, LEAKSHIELD, "display_brightness_idle", "dark").is_none());
    assert!(set_interval(&screen_input, LEAKSHIELD, 15.0).is_some());
    assert!(set_interval(&screen_input, LEAKSHIELD, f64::NAN).is_none());
    // A read-only device sends nothing.
    let unknown = unknown_firmware();
    let unknown_input = en(&unknown, &feedback);
    assert!(
        ui.set_template(&unknown_input, LEAKSHIELD, "values-1")
            .is_none()
    );
    // A name that the registry does not list sends nothing.
    assert!(ui.set_template(&screen_input, LEAKSHIELD, "2").is_none());
    // Arrow keys move in the grid of 5 columns.
    let mut grid = DisplayUi::default();
    grid.step(1, 0);
    grid.step(0, 1);
    assert_eq!(grid.page, 6);
    grid.step(-1, -1);
    assert_eq!(grid.page, 0);
    grid.step(-1, 0);
    assert_eq!(grid.page, 0);
}

fn interval_write(command: Option<Command>) -> Option<f64> {
    let Some(Command::SetLeakshieldFields { fields, .. }) = command else {
        return None;
    };
    let [(id, FieldValue::Number(seconds))] = &fields[..] else {
        panic!("one number: {fields:?}");
    };
    assert_eq!(id, "display_page_time");
    Some(*seconds)
}

#[test]
fn the_automatic_page_change_is_the_page_time_below_30_s() {
    use nicecooling::screens::leakshield::display::set_interval;
    let feedback = Feedback::default();
    let with_time = |seconds: f64, flags: f64| {
        tweak(|state| {
            set_field(state, "display_page_time", FieldValue::Number(seconds));
            set_field(state, "display_flags", FieldValue::Number(flags));
        })
    };
    // 29 s is on. 30 s and more is off. The interval is writable only while it is on.
    for (seconds, on) in [
        (1.0, true),
        (29.0, true),
        (29.9, true),
        (30.0, false),
        (50.0, false),
    ] {
        let state = with_time(seconds, 0.0);
        let settings = display(&state, &DisplayUi::default()).settings;
        assert_eq!(settings.auto_page.on, on, "{seconds} s");
        assert_eq!(settings.interval_writable, on, "{seconds} s");
    }
    // The flags do not change it: bit 0 rotates the content.
    let state = with_time(10.0, 15.0);
    let settings = display(&state, &DisplayUi::default()).settings;
    assert!(settings.auto_page.on && settings.rotate.on);
    let state = with_time(30.0, 0.0);
    assert!(!display(&state, &DisplayUi::default()).settings.rotate.on);

    // Off writes 30 s. On writes 29 s, or the interval from before.
    let mut ui = DisplayUi::default();
    let state = with_time(10.0, 0.0);
    assert_eq!(
        interval_write(ui.set_auto_page(&en(&state, &feedback), LEAKSHIELD, false)),
        Some(30.0)
    );
    let off = with_time(30.0, 0.0);
    assert_eq!(
        interval_write(ui.set_auto_page(&en(&off, &feedback), LEAKSHIELD, true)),
        Some(10.0)
    );
    let mut fresh = DisplayUi::default();
    assert_eq!(
        interval_write(fresh.set_auto_page(&en(&off, &feedback), LEAKSHIELD, true)),
        Some(29.0)
    );
    // The switch does not write the rotation bit.
    let Some(Command::SetLeakshieldFields { fields, .. }) =
        fresh.set_auto_page(&en(&off, &feedback), LEAKSHIELD, true)
    else {
        panic!("a field write");
    };
    assert!(fields.iter().all(|(id, _)| id != "display_flags"));

    // The interval is cut to 1 to 29 s.
    let input = en(&state, &feedback);
    for (typed, written) in [
        (15.0, 15.0),
        (0.0, 1.0),
        (-4.0, 1.0),
        (45.0, 29.0),
        (2.5, 2.5),
    ] {
        assert_eq!(
            interval_write(set_interval(&input, LEAKSHIELD, typed)),
            Some(written)
        );
    }
    assert!(set_interval(&input, LEAKSHIELD, f64::NAN).is_none());
    // A read-only field sends nothing.
    let unknown = unknown_firmware();
    let mut ui = DisplayUi::default();
    assert!(
        ui.set_auto_page(&en(&unknown, &feedback), LEAKSHIELD, false)
            .is_none()
    );
}

#[test]
fn the_rotate_switch_is_bit_0_of_the_display_flags() {
    use nicecooling::screens::leakshield::display::set_flag;
    let feedback = Feedback::default();
    let state = tweak(|state| set_field(state, "display_flags", FieldValue::Number(12.0)));
    let input = en(&state, &feedback);
    let flags = |command: Option<Command>| {
        let Some(Command::SetLeakshieldFields { fields, .. }) = command else {
            panic!("a field write");
        };
        fields[0].1.clone()
    };
    assert_eq!(
        flags(set_flag(&input, LEAKSHIELD, 1, true)),
        FieldValue::Number(13.0)
    );
    let rotated = tweak(|state| set_field(state, "display_flags", FieldValue::Number(13.0)));
    assert!(display(&rotated, &DisplayUi::default()).settings.rotate.on);
    assert_eq!(
        flags(set_flag(&en(&rotated, &feedback), LEAKSHIELD, 1, false)),
        FieldValue::Number(12.0)
    );
}

#[test]
fn off_in_standby_is_on_while_bit_1_of_the_display_flags_is_clear() {
    use nicecooling::screens::leakshield::display::{FlagSwitch, OFF_IN_STANDBY, ROTATE};
    let feedback = Feedback::default();
    let flags_state =
        |flags: f64| tweak(|state| set_field(state, "display_flags", FieldValue::Number(flags)));
    // The reference device has the bit clear and its display goes off in standby.
    for (flags, on) in [
        (0.0, true),
        (1.0, true),
        (2.0, false),
        (15.0, false),
        (13.0, true),
    ] {
        assert_eq!(
            display(&flags_state(flags), &DisplayUi::default())
                .settings
                .standby
                .on,
            on,
            "flags {flags}"
        );
    }
    let written = |flag: FlagSwitch, flags: f64, on: bool| {
        let state = flags_state(flags);
        let Some(Command::SetLeakshieldFields { fields, .. }) =
            flag.set(&en(&state, &feedback), LEAKSHIELD, on)
        else {
            panic!("a field write");
        };
        fields[0].1.clone()
    };
    // The switch on clears the bit. The switch off sets it. The other bits stay.
    assert_eq!(written(OFF_IN_STANDBY, 3.0, true), FieldValue::Number(1.0));
    assert_eq!(written(OFF_IN_STANDBY, 1.0, false), FieldValue::Number(3.0));
    assert_eq!(written(ROTATE, 2.0, true), FieldValue::Number(3.0));
}

#[test]
fn invert_waits_for_the_automatic_invert_to_be_off() {
    let flags_state =
        |flags: f64| tweak(|state| set_field(state, "display_flags", FieldValue::Number(flags)));
    // Bit 3 clear: the automatic invert is on and "Invert" is disabled, with its caption.
    let on = display(&flags_state(0.0), &DisplayUi::default()).settings;
    assert!(on.auto_invert.on && on.auto_invert.writable);
    assert!(!on.invert.writable);
    assert_eq!(
        on.invert_text.as_deref(),
        Some("Turn automatic invert off to set it by hand.")
    );
    // Bit 3 set: the automatic invert is off, "Invert" is writable and shows bit 2.
    let off = display(&flags_state(8.0), &DisplayUi::default()).settings;
    assert!(!off.auto_invert.on && off.invert.writable && off.invert_text.is_none());
    assert!(!off.invert.on);
    assert!(
        display(&flags_state(12.0), &DisplayUi::default())
            .settings
            .invert
            .on
    );
    // A device that is not writable disables both.
    let mut locked = flags_state(8.0);
    set_writable(&mut locked, "display_flags", false);
    let settings = display(&locked, &DisplayUi::default()).settings;
    assert!(!settings.invert.writable && !settings.auto_invert.writable);
    // Italian.
    let italian = DisplayModel::build(
        &input(&flags_state(0.0), &Feedback::default(), IT),
        &DisplayUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        italian.settings.invert_text.as_deref(),
        Some("Disattiva l'inversione automatica per impostarla a mano.")
    );
}

#[test]
fn leakshield_display_labels() {
    // The daemon has no logo upload, no separate switch for the display off when inactive (the brightness "Off" does it) and no sensor in a page icon.
    let missing = labels::missing_labels(
        "leakshield-display.md",
        &[
            "Page 3",
            "--",
            "Software sensor 3 has no data source.",
            "Choose an image",
            "Use a PNG, JPEG or BMP image.",
            "Writing the logo, 60 %",
            "Display off when inactive",
        ],
    );
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let italian = DisplayModel::build(
        &input(&with_leakshield(), &Feedback::default(), IT),
        &DisplayUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(italian.pages[0].template_name, "Stato del Leakshield");
    assert_eq!(italian.pages_title, "Pagine");
}

fn lighting(state: &AppState, ui: &LightingUi) -> LightingModel {
    LightingModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

#[test]
fn leakshield_lighting_states() {
    let state = with_leakshield();
    let model = lighting(&state, &LightingUi::default());
    assert_eq!(model.state, TabState::Ready);
    assert_eq!(model.cells.len(), 6);
    assert!(model.cells.iter().all(|c| c.lit));
    assert_eq!(model.bars.len(), 2);
    assert_eq!(model.bars[0].title, "Controller 1");
    assert_eq!((model.bars[0].first, model.bars[0].count), (0, 6));
    // Controller 1 covers the whole strip: controller 2 is dim.
    assert_eq!(
        model.bars[1].covered_by.as_deref(),
        Some("Covered by Controller 1")
    );
    assert!(model.bars[0].covered_by.is_none());
    assert_eq!(
        model.priority_hint,
        "A controller higher in the list wins where two controllers share an LED."
    );
    assert_eq!(
        model.external,
        "External LEDs: 0 connected. Set the count to match the strip on the RGBpx output."
    );
    // Both controllers of the device are in use, and nothing is selected.
    assert!(!model.add.enabled && !model.duplicate.enabled && !model.delete.enabled);
    assert!(model.panel.is_none() && model.empty.is_none());

    // A selected controller: its parameters.
    let mut ui = LightingUi::default();
    ui.toggle(1);
    let panel = lighting(&state, &ui).panel.expect("the panel");
    assert_eq!(panel.title, "Controller 2");
    assert_eq!(panel.effect, "Rainbow");
    assert_eq!((panel.first, panel.count), (2.0, 3.0));
    assert_eq!((panel.speed, panel.brightness), (50.0, 100.0));
    assert!(panel.driven, "a sensor drives it");
    // The device sensors 0 to 5 and the Software sensors 8 to 19. Controller 2 follows sensor 10.
    let ids: Vec<i32> = panel.sources.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        ids,
        [
            0, 1, 2, 3, 4, 5, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19
        ]
    );
    let selected: Vec<_> = panel.sources.iter().filter(|(_, _, on)| *on).collect();
    assert_eq!(selected.len(), 1);
    assert_eq!(
        (selected[0].0, selected[0].1.as_str()),
        (10, "Software sensor 3: Water temperature")
    );
    assert_eq!(panel.sources[6].1, "Software sensor 1");
    assert_eq!(panel.sources[0].1, "Pressure");

    // No controller: the text and the button.
    let mut none = state.clone();
    set_field(
        &mut none,
        "led_controller_mode",
        FieldValue::TextList(vec!["unused".into(), "unused".into()]),
    );
    let empty = lighting(&none, &LightingUi::default());
    assert_eq!(
        empty.empty,
        Some((
            "No controller. The LEDs are off.".to_owned(),
            "Add a controller".to_owned()
        ))
    );
    assert!(empty.cells.iter().all(|c| !c.lit));

    // Both controllers in use: "Add" and "Duplicate" are off, with the reason.
    let mut full = state.clone();
    for id in [
        "led_controller_mode",
        "led_controller_first_led",
        "led_controller_led_count",
        "led_controller_source",
    ] {
        set_field(
            &mut full,
            id,
            if id == "led_controller_mode" {
                FieldValue::TextList(vec!["fixed-color".into(); 2])
            } else {
                FieldValue::List(vec![0.0; 2])
            },
        );
    }
    for (id, size) in [
        ("led_controller_parameters", 12),
        ("led_controller_color_hue", 6),
        ("led_controller_color_saturation", 6),
        ("led_controller_color_value", 6),
    ] {
        set_field(&mut full, id, FieldValue::List(vec![1.0; 2 * size]));
    }
    let mut ui = LightingUi::default();
    ui.toggle(0);
    let model = lighting(&full, &ui);
    assert_eq!(model.bars.len(), 2);
    assert!(!model.add.enabled && !model.duplicate.enabled);
    assert_eq!(
        model.add.tooltip.as_deref(),
        Some("The Leakshield has 2 controllers.")
    );
    assert!(model.delete.enabled);

    // The effect picker: the list and the preview strip of 12 cells.
    let mut ui = LightingUi::default();
    ui.toggle(0);
    ui.effect_picker = Some("rainbow".to_owned());
    let picker = lighting(&state, &ui).picker.expect("the picker");
    assert_eq!(picker.preview.len(), 12);
    assert_eq!(picker.effects.len(), 16);
    assert!(
        picker
            .effects
            .iter()
            .any(|(effect, name, _, selected)| effect == "rainbow"
                && name == "Rainbow"
                && *selected)
    );
    assert_eq!(
        (picker.cancel.as_str(), picker.confirm.as_str()),
        ("Cancel", "Use this effect")
    );

    // Read-only with an unknown firmware. The states of the link.
    let model = lighting(&unknown_firmware(), &LightingUi::default());
    assert_eq!(
        model.reason.as_deref(),
        Some("Read-only: unknown firmware.")
    );
    assert!(!model.add.enabled);
    assert_eq!(
        lighting(&offline(), &LightingUi::default()).state,
        TabState::Offline
    );
    assert_eq!(
        lighting(&connecting(), &LightingUi::default()).state,
        TabState::Loading
    );
}

#[test]
fn leakshield_lighting_commands() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let modes_of = |command: &Command| -> Vec<String> {
        let Command::SetLeakshieldFields { fields, .. } = command else {
            panic!("a field write")
        };
        match &fields
            .iter()
            .find(|f| f.0 == "led_controller_mode")
            .expect("field led_controller_mode")
            .1
        {
            FieldValue::TextList(names) => names.clone(),
            other => panic!("not a list of names: {other:?}"),
        }
    };
    let list_of = |command: &Command, id: &str| -> Vec<f64> {
        let Command::SetLeakshieldFields { fields, .. } = command else {
            panic!("a field write")
        };
        match &fields
            .iter()
            .find(|f| f.0 == id)
            .unwrap_or_else(|| panic!("field {id}"))
            .1
        {
            FieldValue::List(list) => list.clone(),
            other => panic!("not a list: {other:?}"),
        }
    };
    // The device has 2 controllers. "Add" and "Duplicate" take the free slot and never add an
    // entry: with both controllers in use they send nothing.
    let mut ui = LightingUi::default();
    assert!(ui.add(&screen_input, LEAKSHIELD).is_none());
    let mut free = state.clone();
    set_field(
        &mut free,
        "led_controller_mode",
        FieldValue::TextList(vec!["rainbow".into(), "unused".into()]),
    );
    let free_input = en(&free, &feedback);
    let add = ui.add(&free_input, LEAKSHIELD).expect("add");
    assert_eq!(modes_of(&add), ["rainbow", "fixed-color"]);
    assert_eq!(list_of(&add, "led_controller_color_hue").len(), 12);
    // "Duplicate" needs one selected controller.
    assert!(ui.duplicate(&free_input, LEAKSHIELD).is_none());
    ui.toggle(0);
    let copy = ui.duplicate(&free_input, LEAKSHIELD).expect("duplicate");
    assert_eq!(modes_of(&copy), ["rainbow", "rainbow"]);
    assert!(ui.duplicate(&screen_input, LEAKSHIELD).is_none());
    let mut ui = LightingUi::default();
    ui.toggle(1);
    // A bar moves, grows, and changes its priority.
    let moved = ui.shift(&screen_input, LEAKSHIELD, 1, 0).expect("move");
    assert_eq!(list_of(&moved, "led_controller_first_led"), [0.0, 3.0]);
    let longer = ui.shift(&screen_input, LEAKSHIELD, 0, 1).expect("longer");
    assert_eq!(list_of(&longer, "led_controller_led_count"), [6.0, 4.0]);
    assert!(
        ui.shift(&screen_input, LEAKSHIELD, 0, 5).is_some(),
        "the length stops at the strip"
    );
    let up = ui.reorder(&screen_input, LEAKSHIELD, -1).expect("up");
    assert_eq!(modes_of(&up), ["rainbow", "fixed-color"]);
    assert_eq!(list_of(&up, "led_controller_first_led"), [2.0, 0.0]);
    assert_eq!(ui.selected.iter().copied().collect::<Vec<_>>(), [0]);
    assert!(
        ui.reorder(&screen_input, LEAKSHIELD, -1).is_none(),
        "the first bar stays first"
    );
    // The effect, the speed, the color and the sensor.
    let effect = ui
        .set_effect(&screen_input, LEAKSHIELD, "gradient")
        .expect("effect");
    assert_eq!(modes_of(&effect), ["gradient", "rainbow"]);
    assert!(ui.set_effect(&screen_input, LEAKSHIELD, "99").is_none());
    // An effect that the picker does not offer sends nothing.
    assert!(ui.set_effect(&screen_input, LEAKSHIELD, "unused").is_none());
    let speed = ui
        .set_param(&screen_input, LEAKSHIELD, 0, 80.0)
        .expect("speed");
    assert_eq!(list_of(&speed, "led_controller_parameters")[0], 80.0);
    let hue = ui
        .set_hue(&screen_input, LEAKSHIELD, 0, 200.0)
        .expect("hue");
    assert_eq!(list_of(&hue, "led_controller_color_hue")[0], 200.0);
    let source = ui.set_source(&screen_input, LEAKSHIELD, 3).expect("sensor");
    assert_eq!(list_of(&source, "led_controller_source"), [3.0, 10.0]);
    // A Software sensor is the sensor index from 8: slot 1 is 8 and slot 12 is 19.
    let source = ui
        .set_source(&screen_input, LEAKSHIELD, 19)
        .expect("sensor");
    assert_eq!(list_of(&source, "led_controller_source"), [19.0, 10.0]);

    // "Delete" removes the selection at once and "Undo" writes the table back.
    let mut ui = LightingUi::default();
    ui.toggle(1);
    let delete = ui.delete(&screen_input, LEAKSHIELD).expect("delete");
    assert_eq!(modes_of(&delete), ["fixed-color", "unused"]);
    let model = lighting(&state, &ui);
    assert_eq!(
        model.undo,
        Some(("Controller deleted.".to_owned(), "Undo".to_owned()))
    );
    let undo = ui.undo(LEAKSHIELD).expect("undo");
    assert_eq!(modes_of(&undo), ["fixed-color", "rainbow"]);
    assert!(ui.undo(LEAKSHIELD).is_none());
    // After 10 s the offer is gone.
    let mut late = LightingUi::default();
    late.toggle(0);
    late.delete(&screen_input, LEAKSHIELD);
    let later = ScreenInput {
        now_us: NOW_US + 11_000_000,
        ..en(&state, &feedback)
    };
    assert!(
        LightingModel::build(&later, &late, LEAKSHIELD)
            .undo
            .is_none()
    );
    // A read-only device sends nothing.
    let unknown = unknown_firmware();
    assert!(
        LightingUi::default()
            .add(&en(&unknown, &feedback), LEAKSHIELD)
            .is_none()
    );
}

#[test]
fn leakshield_lighting_labels() {
    // The device protocol lists no controller name, no LED in use by an alarm and no external LED
    // count: the tab shows the fixed text only.
    let missing = labels::missing_labels(
        "leakshield-lighting.md",
        &[
            "Covered by Controller 1",
            "LEDs used by the device",
            "The Leakshield uses these LEDs while an alarm is active.",
        ],
    );
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let italian = LightingModel::build(
        &input(&with_leakshield(), &Feedback::default(), IT),
        &LightingUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        italian.bars[1].covered_by.as_deref(),
        Some("Coperto dal controller 1")
    );
}

fn status(state: &AppState, ui: &StatusUi) -> StatusModel {
    StatusModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

#[test]
fn leakshield_status_states() {
    use nicecooling::screens::leakshield::status::Tile;
    let state = with_leakshield();
    let model = status(&state, &StatusUi::default());
    assert_eq!(model.state, TabState::Ready);
    let tiles: Vec<(&str, &str)> = model
        .tiles
        .iter()
        .map(|t| (t.label.as_str(), t.value.as_str()))
        .collect();
    assert_eq!(
        tiles,
        [
            ("Status", "Protecting"),
            ("Pressure, target 300", "298 mbar"),
            ("Volume change", "+2 ml/h"),
            ("Pressure change", "-0.4 mbar/min")
        ]
    );
    assert!(model.tiles[0].selected);
    assert_eq!(model.operating_caption, "select a value to chart it");

    // Idle: no part is active.
    assert!(model.schematic.parts.iter().all(|p| !p.active));
    let labels: Vec<&str> = model
        .schematic
        .parts
        .iter()
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "Atmosphere",
            "Vent valve, closed",
            "Pump valve, closed",
            "Vacuum pump, off",
            "Membrane",
            "Reservoir"
        ]
    );
    assert_eq!(
        model.schematic.caption,
        "active parts are filled and labelled"
    );
    assert_eq!(model.schematic.bubbles[0].value, "298 mbar");
    assert_eq!(
        model.schematic.bubbles[0].caption,
        "target 300, -0.4 mbar/min"
    );

    // The pump runs: the vacuum pump and the pump valve are active.
    let mut building = state.clone();
    building
        .sensors
        .as_mut()
        .unwrap()
        .sensors
        .iter_mut()
        .for_each(|s| {
            if s.0.ends_with("_pressure") {
                s.1 = 200.0;
            }
        });
    let model = status(&building, &StatusUi::default());
    assert_eq!(model.tiles[0].value, "Building pressure");
    let active: Vec<&str> = model
        .schematic
        .parts
        .iter()
        .filter(|p| p.active)
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(active, ["Pump valve, open", "Vacuum pump, running"]);

    // An Action running: its name.
    for (action, text) in [
        ("test", "Test"),
        ("measure-fill-level", "Measuring the fill level"),
        ("deaerate", "Deaerating"),
        ("refill", "Refill"),
    ] {
        let running = tweak(|s| s.snapshot.leakshields[0].running_action = action.into());
        assert_eq!(status(&running, &StatusUi::default()).tiles[0].value, text);
    }
    let released = tweak(|s| s.snapshot.leakshields[0].mode = "released".into());
    assert_eq!(
        status(&released, &StatusUi::default()).status_key,
        "protecting",
        "an unknown Mode counts as SHIELD"
    );

    // The events: newest first, with the details. A Warning has the triangle.
    let events = &model.events;
    assert_eq!(events.columns, ["Time", "Event", "Details"]);
    assert_eq!(
        events
            .rows
            .iter()
            .map(|r| (r.title.as_str(), r.details.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Mode changed", "MONITOR -> SHIELD"),
            ("Vacuum pump started", "Pressure 281 mbar"),
            ("System start", "")
        ]
    );
    assert!(
        events
            .rows
            .iter()
            .all(|r| r.severity.is_none() && !r.opens_alarms)
    );
    let mut warned = state.clone();
    warned
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .events
        .insert(
            0,
            nicecooling::model::DeviceEvent {
                time_us: 1_700_000_200_000_000,
                id: "leakshield.leak-warning".into(),
                details: Default::default(),
            },
        );
    let row = &status(&warned, &StatusUi::default()).events.rows[0];
    assert_eq!(
        (row.title.as_str(), row.severity, row.opens_alarms),
        ("Leak Warning", Some(Severity::Warning), true)
    );

    // No event: the text.
    let mut empty = state.clone();
    empty
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .events
        .clear();
    assert_eq!(
        status(&empty, &StatusUi::default()).events.empty.as_deref(),
        Some("No event since the Leakshield got power.")
    );

    // The tile selection and the height of the chart.
    let mut ui = StatusUi::default();
    ui.step(1);
    assert_eq!(ui.selected, Tile::Pressure);
    ui.step(5);
    assert_eq!(ui.selected, Tile::PressureChange);
    ui.step(-9);
    assert_eq!(ui.selected, Tile::Status);
    ui.set_chart_height(10.0);
    assert_eq!(ui.chart_height, 84.0);
    ui.set_chart_height(900.0);
    assert_eq!(ui.chart_height, 240.0);

    // The Leakshield does not answer: the values show "--" and the schematic is dim.
    let model = status(&offline(), &StatusUi::default());
    assert_eq!(model.state, TabState::Offline);
    assert_eq!(model.tiles[1].value, "--");
    assert!(model.schematic.dim);
    assert_eq!(
        status(&connecting(), &StatusUi::default()).state,
        TabState::Loading
    );
    assert_eq!(
        status(&connecting(), &StatusUi::default()).loading_text,
        "Reading the Leakshield"
    );
}

#[test]
fn export_file_has_serial_firmware_and_one_line_per_event() {
    let state = with_leakshield();
    let data = &state.snapshot.leakshield_data[LEAKSHIELD];
    let info = &state.snapshot.leakshields[0];
    let text = export_text(EN, info, &data.events);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Serial: 20417-31072");
    assert_eq!(lines[1], "Firmware: 1040");
    assert_eq!(
        lines.len(),
        2 + data.events.len(),
        "one line for each event"
    );
    assert!(
        lines[2].ends_with("| Mode changed | MONITOR -> SHIELD"),
        "{}",
        lines[2]
    );
    assert!(lines[4].ends_with("| System start | "), "{}", lines[4]);
    // The file on the disk has the same text.
    let dir = std::env::temp_dir().join(format!("nicecooling-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("events.txt");
    nicecooling::screens::leakshield::status::export_to(&path, EN, info, &data.events).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The names of one list of the event table of the daemon.
fn daemon_event_names(list: &str) -> Vec<&'static str> {
    const SOURCE: &str = include_str!("../../nicecoolingd/src/leakshield/events.rs");
    let start = SOURCE
        .find(&format!("const {list}:"))
        .expect("the list is in events.rs");
    let body = &SOURCE[start..];
    let body = &body[body.find("= [").expect("the list has a body")..];
    let body = &body[..body.find("];").expect("the list ends")];
    body.split('"').skip(1).step_by(2).collect()
}

#[test]
fn every_event_of_the_daemon_has_a_title_in_both_languages() {
    let device = daemon_event_names("DEVICE_EVENTS");
    let alarms = daemon_event_names("ALARM_EVENTS");
    assert_eq!((device.len(), alarms.len()), (28, 11));
    for locale in [EN, IT] {
        for name in device.iter().chain(&alarms) {
            let id = format!("leakshield.{name}");
            let title = event_title(locale, &id);
            assert!(
                !title.contains(&id),
                "{id} has no title in {}: {title}",
                locale.code()
            );
        }
    }
    assert_eq!(
        event_title(EN, "leakshield.release-run"),
        "Vacuum release running"
    );
    assert_eq!(
        event_title(IT, "leakshield.shield-pressure-low"),
        "SHIELD: pressione bassa"
    );
    assert_eq!(
        event_title(EN, "leakshield.event-42"),
        "Event leakshield.event-42",
        "a code that the protocol document does not name"
    );

    // A device event is a plain row. Only a Warning or an Alarm has the icon and opens Alarms.
    let state = tweak(|state| {
        let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
        data.events = ["system-start", "pump-runtime-alarm", "event-42"]
            .iter()
            .map(|name| nicecooling::model::DeviceEvent {
                time_us: 1_700_000_000_000_000,
                id: format!("leakshield.{name}"),
                details: Default::default(),
            })
            .collect();
    });
    let rows = status(&state, &StatusUi::default()).events.rows;
    assert_eq!(
        rows.iter()
            .map(|r| (r.title.as_str(), r.severity, r.opens_alarms))
            .collect::<Vec<_>>(),
        [
            ("System start", None, false),
            ("Pump runtime Alarm", Some(Severity::Alarm), true),
            ("Event leakshield.event-42", None, false),
        ]
    );
}

#[test]
fn leakshield_status_labels() {
    let missing = labels::missing_labels("leakshield-status.md", &["--"]);
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let italian = StatusModel::build(
        &input(&with_leakshield(), &Feedback::default(), IT),
        &StatusUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(italian.tiles[0].value, "In protezione");
    assert_eq!(italian.events.export, "Esporta");
}

fn system(state: &AppState, ui: &SystemUi, feedback: &Feedback) -> SystemModel {
    SystemModel::build(&en(state, feedback), ui, LEAKSHIELD)
}

#[test]
fn leakshield_system_states() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let model = system(&state, &SystemUi::default(), &feedback);
    assert_eq!(model.state, TabState::Ready);
    assert_eq!(
        (model.device.serial.as_str(), model.device.firmware.as_str()),
        ("20417-31072", "1040")
    );
    assert_eq!(model.device.firmware_tag, "Known");
    assert_eq!(model.device.maintenance_text, "--");
    assert_eq!(
        model.device.maintenance_caption,
        "100 % is new. At 0 % the vacuum pump and the valves need maintenance by Aqua Computer."
    );

    // The clock: the device time follows the offset. 42 s are no unset clock.
    assert_eq!(model.clock.zone_text, "UTC+01:00");
    assert_eq!(model.clock.zone_options.len(), 27);
    assert!(!model.clock.hour12);
    assert!(model.clock.warning.is_none());
    let mut unset = state.clone();
    unset
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .clock_offset_s = -86_400;
    assert_eq!(
        system(&unset, &SystemUi::default(), &feedback)
            .clock
            .warning
            .as_deref(),
        Some(
            "The Leakshield has no battery for its clock. After a power loss, set the time again."
        )
    );

    // The standby behavior reads `standby_conditions`: 11 is bits 0, 1 and 3.
    assert!(model.standby.rows.iter().all(|r| r.on));
    assert_eq!(
        model.standby.rows[0].label,
        "Standby when the USB bus is suspended"
    );

    // The backups: newest first, the reason, a button to export and a button to restore.
    assert_eq!(model.backups.rows.len(), 2);
    assert_eq!(model.backups.rows[0].id, "backup_2");
    assert_eq!(model.backups.rows[0].reason, "Manual");
    assert_eq!(model.backups.rows[1].reason, "Before a write");
    assert!(model.backups.empty.is_none());
    assert_eq!(
        model.backups.caption,
        "Backups are in /var/lib/nicecooling/backups, one folder per serial."
    );
    let mut none = state.clone();
    none.snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .backups
        .clear();
    assert_eq!(
        system(&none, &SystemUi::default(), &feedback)
            .backups
            .empty
            .as_deref(),
        Some("No backup yet. nicecooling saves one before each write.")
    );
    // The refill history is empty: the daemon gives no rows.
    assert_eq!(model.refill.empty, "The refill history is empty.");

    // Maintenance at 0 %: the warning.
    let mut worn = state.clone();
    for sample in [&mut worn.sensors, &mut worn.last_sensors]
        .into_iter()
        .flatten()
    {
        sample
            .sensors
            .push((format!("{LEAKSHIELD}_maintenance"), 0.0, 0));
    }
    let model = system(&worn, &SystemUi::default(), &feedback);
    assert_eq!(model.device.maintenance_text, "0 %");
    assert_eq!(
        model.device.maintenance_warning.as_deref(),
        Some("The vacuum pump and the valves need maintenance by Aqua Computer.")
    );

    // The Leakshield does not answer: the backups stay listed and "Export" works. The rest is off.
    let model = system(&offline(), &SystemUi::default(), &feedback);
    assert_eq!(model.state, TabState::Offline);
    assert_eq!(model.backups.rows.len(), 2);
    assert!(
        model
            .backups
            .rows
            .iter()
            .all(|r| r.export.enabled && !r.restore.enabled)
    );
    assert!(
        !model.clock.sync.enabled && !model.backups.back_up.enabled && !model.reset.factory.enabled
    );
    assert_eq!(
        system(&connecting(), &SystemUi::default(), &feedback).state,
        TabState::Loading
    );

    // A restore that runs turns the other writes off. A restore that failed has its buttons.
    let mut restoring = Feedback::default();
    restoring.set(format!("restore:{LEAKSHIELD}"), WriteOutcome::Pending);
    let model = system(&state, &SystemUi::default(), &restoring);
    assert!(model.backups.restoring);
    assert!(
        !model.clock.sync.enabled && !model.refill.reset.enabled && !model.reset.factory.enabled
    );
    let mut failed = Feedback::default();
    failed.set(
        format!("restore:{LEAKSHIELD}"),
        WriteOutcome::Refused {
            error: "VerifyFailed".into(),
        },
    );
    let mut ui = SystemUi::default();
    let model = system(&state, &ui, &failed);
    let failure = model.backups.restore_failed.expect("the failure");
    assert_eq!(
        failure.text,
        "Restore failed: the read-back does not match the backup."
    );
    assert_eq!(
        (failure.try_again.as_str(), failure.close.as_str()),
        ("Try again", "Close")
    );
    ui.close_failure();
    assert!(
        system(&state, &ui, &failed)
            .backups
            .restore_failed
            .is_none()
    );
}

#[test]
fn restore_and_factory_reset_have_no_padlock() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let model = system(&state, &SystemUi::default(), &feedback);
    assert!(
        model
            .backups
            .rows
            .iter()
            .all(|r| r.restore.enabled && r.restore.label == "Restore")
    );
    assert!(model.reset.factory.enabled);
    assert_eq!(model.reset.factory.label, "Reset to factory settings");

    // The restore asks first, with the time of the backup.
    let mut ui = SystemUi::default();
    ui.request(Ask::Restore("backup_1".into()));
    let dialog = system(&state, &ui, &feedback).dialog.expect("a dialog");
    assert!(dialog.text.starts_with("Restore the backup from "));
    assert!(dialog.text.ends_with("Every setting of the Leakshield goes back to that state. nicecooling saves a backup of the current state first."));
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Restore")
    );
    assert_eq!(
        ui.confirm(LEAKSHIELD),
        Some(Command::RestoreBackup {
            leakshield: LEAKSHIELD.into(),
            backup: "backup_1".into()
        })
    );
    assert_eq!(
        ui.retry_restore(LEAKSHIELD),
        Some(Command::RestoreBackup {
            leakshield: LEAKSHIELD.into(),
            backup: "backup_1".into()
        })
    );

    // The factory reset needs the typed word.
    let mut ui = SystemUi::default();
    ui.request(Ask::FactoryReset);
    let dialog = system(&state, &ui, &feedback).dialog.expect("a dialog");
    assert!(dialog.text.starts_with(
        "Reset the Leakshield to factory settings? You must set it up again with the wizard."
    ));
    assert_eq!(
        (
            dialog.word.as_deref(),
            dialog.confirm.as_str(),
            dialog.confirm_enabled
        ),
        (Some("RESET"), "Reset", false)
    );
    assert!(ui.confirm(LEAKSHIELD).is_none());
    ui.reset_word = "reset".into();
    assert!(ui.confirm(LEAKSHIELD).is_none(), "the word is in capitals");
    ui.reset_word = RESET_WORD.into();
    assert!(
        system(&state, &ui, &feedback)
            .dialog
            .unwrap()
            .confirm_enabled
    );
    assert_eq!(
        ui.confirm(LEAKSHIELD),
        Some(Command::FactoryReset {
            leakshield: LEAKSHIELD.into()
        })
    );
    assert!(ui.ask.is_none());

    // After a denial: the text.
    let mut denied = Feedback::default();
    denied.set(
        format!("ls:{LEAKSHIELD}:factory-reset"),
        WriteOutcome::NotAuthorized,
    );
    assert_eq!(
        system(&state, &SystemUi::default(), &denied)
            .reset
            .denied
            .as_deref(),
        Some("Not authorized. Nothing changed.")
    );
}

#[test]
fn reset_buttons_are_plain_buttons() {
    let state = with_leakshield();
    let model = system(&state, &SystemUi::default(), &Feedback::default());
    assert!(model.device.reset_counter.enabled);
    assert!(model.refill.reset.enabled);
    assert_eq!(model.device.reset_counter.label, "Reset service counter");
    assert_eq!(model.refill.reset.label, "Reset refill history");
}

#[test]
fn no_pump_runtime_button() {
    // The device has no command that clears the pump runtime.
    let state = with_leakshield();
    let model = system(&state, &SystemUi::default(), &Feedback::default());
    let buttons = [
        &model.device.reset_counter.label,
        &model.clock.sync.label,
        &model.backups.back_up.label,
        &model.backups.import.label,
        &model.refill.reset.label,
        &model.reset.factory.label,
    ];
    assert!(
        buttons
            .iter()
            .all(|label| !label.to_lowercase().contains("runtime"))
    );
    let en_text = labels::load("en");
    assert!(
        en_text
            .iter()
            .filter(|(key, _)| key.starts_with("leakshield.system"))
            .all(|(key, text)| !key.contains("runtime")
                && !text.to_lowercase().contains("pump runtime"))
    );
    // The Overview chart of the pump runtime is read-only: it has no button.
    let overview = overview(&state, &OverviewUi::default());
    assert!(
        overview
            .mode
            .actions
            .iter()
            .all(|a| !a.action.contains("runtime"))
    );
    assert!(
        !include_str!("../src/command.rs")
            .to_lowercase()
            .contains("pumpruntime")
    );
}

#[test]
fn leakshield_system_labels() {
    // The daemon gives no device description, no date format, no refill rows and no size of a
    // backup.
    let missing = labels::missing_labels(
        "leakshield-system.md",
        &["Device description", "Date format", "Size"],
    );
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let italian = SystemModel::build(
        &input(&with_leakshield(), &Feedback::default(), IT),
        &SystemUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        italian.reset.factory.label,
        "Ripristina impostazioni di fabbrica"
    );
    assert_eq!(italian.backups.rows[0].reason, "Manuale");
}

#[test]
fn backup_file_round_trips() {
    use nicecooling::screens::leakshield::system::{backup_file, parse_backup_file};
    let reports = (1022, vec![3, 0, 100, 255], vec![5, 1], vec![8, 2, 0]);
    let text = backup_file(&reports);
    assert_eq!(
        text,
        "firmware 1022\nreport3 030064ff\nreport5 0501\nreport8 080200\n"
    );
    assert_eq!(parse_backup_file(&text), Some(reports));
    assert_eq!(
        parse_backup_file("firmware 1022\nreport3 0g\nreport5 01\nreport8 02\n"),
        None
    );
    // A file without the firmware is no backup of this version.
    assert_eq!(
        parse_backup_file("report3 03\nreport5 05\nreport8 08\n"),
        None
    );
    assert_eq!(parse_backup_file("nothing"), None);
}

fn settings(state: &AppState, ui: &SettingsUi) -> SettingsModel {
    SettingsModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

fn data_of(state: &AppState) -> nicecooling::model::LeakshieldData {
    state.snapshot.leakshield_data[LEAKSHIELD].clone()
}

#[test]
fn leakshield_settings_states() {
    let state = with_leakshield();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = settings(&state, &ui);
    assert_eq!(model.state, TabState::Ready);
    assert_eq!(
        model
            .stepper
            .iter()
            .map(|s| s.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Reservoir",
            "Pump",
            "Flow",
            "Pressure",
            "Standby",
            "Mode",
            "Summary"
        ]
    );
    assert_eq!(
        (model.title.as_str(), model.counter.as_str()),
        ("Reservoir", "Step 1 of 7")
    );
    assert!(model.stepper[0].current && !model.stepper[0].done);
    assert_eq!(model.no_changes.as_deref(), Some("No change yet."));
    assert!(model.back.is_none());
    assert_eq!(model.next.as_ref().map(|n| n.1.as_str()), Some("Pump"));

    // The Reservoir step: the model and, for "Custom", the three volumes. One is read-only.
    let keys: Vec<&str> = model.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "reservoir_model",
            "reservoir_total_volume",
            "reservoir_air_volume_full",
            "reservoir_air_volume_empty"
        ]
    );
    let total = &model.rows[1];
    assert!(!total.writable);
    assert_eq!(
        total.reason.as_deref(),
        Some("Not a Verified field: this Safety field has no Verification write yet.")
    );
    assert_eq!(
        total.link.as_ref().map(|l| l.0.as_str()),
        Some("Open Fields")
    );
    assert_eq!(total.badge, "Read-only");
    assert_eq!(model.rows[2].badge, "Verified field");
    assert!(model.rows[2].writable);

    // An edit goes into the pending changes, not to the device.
    ui.edit(&data, "reservoir_air_volume_full", "70");
    let model = settings(&state, &ui);
    assert!(model.rows[2].changed);
    assert_eq!(
        model
            .pending
            .iter()
            .map(|c| (c.name.as_str(), c.old.as_str(), c.new.as_str()))
            .collect::<Vec<_>>(),
        [("Reservoir air volume full", "65 ml", "70 ml")]
    );
    assert!(model.no_changes.is_none());
    // The same value as the device is no change.
    ui.edit(&data, "reservoir_air_volume_full", "65");
    assert!(ui.pending.is_empty());

    // The Pump step: the custom values and the speed source.
    ui.go(Step::Pump);
    let model = settings(&state, &ui);
    assert_eq!(model.counter, "Step 2 of 7");
    assert!(model.stepper[0].done && model.stepper[1].current);
    let source = model.rows.last().unwrap();
    assert_eq!(source.key, "pump_source");
    assert_eq!(
        source.control,
        Control::Info("Pump channel of Main loop: Waterpump".into())
    );
    assert_eq!(
        source.link.as_ref().map(|l| l.0.as_str()),
        Some("Open Loop")
    );
    let pump = &model.rows[0];
    assert!(matches!(&pump.control, Control::Select { shown, .. } if shown == "D5"));

    // The Flow step: the flow sensor of the Loop.
    ui.go(Step::Flow);
    let model = settings(&state, &ui);
    assert_eq!(
        model.rows[0].control,
        Control::Info("Flow sensor of Main loop: Flow, 115 L/h".into())
    );
    let mut without = state.clone();
    without.snapshot.loops[0].flow = None;
    assert_eq!(
        settings(&without, &ui).rows[0].control,
        Control::Info("None. The Leakshield uses the pump speed only.".into())
    );

    // The Pressure step: 127 has adaptive pressure and the timer on. The expert values dim.
    ui.go(Step::Pressure);
    let model = settings(&state, &ui);
    assert_eq!(model.rows[0].control, Control::Switch(true));
    assert_eq!(
        model.rows[0].caption.as_deref(),
        Some(
            "The Leakshield computes the pressure from the pump speed and the flow of the Feed. Recommended."
        )
    );
    let expert: Vec<(&str, bool)> = model.rows[1..4]
        .iter()
        .map(|r| {
            (
                match &r.control {
                    Control::Number { text, .. } => text.as_str(),
                    _ => "",
                },
                r.dim,
            )
        })
        .collect();
    assert_eq!(expert, [("250", true), ("300", true), ("350", true)]);
    assert_eq!(
        model.rows[1].caption.as_deref(),
        Some("Not used while adaptive pressure is on.")
    );
    assert_eq!(
        model.rows[5].control,
        Control::Time {
            text: "09:00".into()
        }
    );
    assert_eq!(
        model.rows[6].control,
        Control::Time {
            text: "18:00".into()
        }
    );
    // Turn adaptive pressure off: the expert values apply. A value out of range has an error.
    ui.edit(&data, "adaptive", "0");
    ui.edit(&data, "expert_min", "500");
    let model = settings(&state, &ui);
    assert_eq!(model.rows[0].control, Control::Switch(false));
    assert!(!model.rows[1].dim);
    assert_eq!(model.rows[1].error.as_deref(), Some("30 to 430 mbar."));
    assert!(
        !model.rows[1].writable,
        "the pressure limits are read-only in the fixture"
    );

    // The Standby step.
    ui.go(Step::Standby);
    let model = settings(&state, &ui);
    assert_eq!(
        model.rows[0].use_current.as_deref(),
        Some("Use the current value")
    );
    assert_eq!(
        model.rows[0].caption.as_deref(),
        Some("After standby the PC sends no Feed yet. The Leakshield then uses these values.")
    );
    ui.use_current(
        &en(&state, &Feedback::default()),
        LEAKSHIELD,
        "pump_start_speed",
    );
    assert_eq!(ui.pending["pump_start_speed"], FieldValue::Number(2840.0));
    ui.use_current(
        &en(&state, &Feedback::default()),
        LEAKSHIELD,
        "pump_start_flow",
    );
    assert_eq!(ui.pending["pump_start_flow"], FieldValue::Number(115.0));

    // The Mode step: three radio rows.
    ui.go(Step::Mode);
    let Control::Radio(options) = settings(&state, &ui).rows[0].control.clone() else {
        panic!("radio rows");
    };
    assert_eq!(
        options
            .iter()
            .map(|o| (o.0.as_str(), o.2))
            .collect::<Vec<_>>(),
        [("shield", true), ("monitor", false), ("release", false)]
    );
    assert!(options[1].1.starts_with("MONITOR: low negative pressure"));

    // The link states.
    assert_eq!(settings(&offline(), &ui).state, TabState::Offline);
    assert!(settings(&offline(), &ui).rows.iter().all(|r| !r.writable));
    let model = settings(&connecting(), &SettingsUi::default());
    assert_eq!(
        (model.state, model.loading_text.as_str()),
        (TabState::Loading, "Reading the settings")
    );
}

#[test]
fn leakshield_settings_summary_and_apply() {
    let state = with_leakshield();
    let data = data_of(&state);
    let feedback = Feedback::default();
    let mut ui = SettingsUi::default();
    ui.go(Step::Summary);
    let model = settings(&state, &ui);
    let summary = model.summary.expect("the summary");
    assert_eq!(summary.nothing.as_deref(), Some("Nothing to apply."));
    assert!(!summary.apply_enabled);
    assert_eq!(summary.apply, "Apply 0 changes");

    // Five changes: the monitor mode warning, the pump pressure warning and a read-only field.
    ui.edit(&data, "default_mode", "monitor");
    ui.edit(&data, "pump_max_pressure", "480");
    ui.edit(&data, "pump_max_speed", "4900");
    ui.edit(&data, "reservoir_air_volume_full", "70");
    ui.edit(&data, "reservoir_total_volume", "300");
    let model = settings(&state, &ui);
    let summary = model.summary.unwrap();
    assert_eq!(summary.rows.len(), 5);
    assert_eq!(
        summary.apply, "Apply 4 changes",
        "the read-only row is not counted"
    );
    assert!(summary.apply_enabled);
    assert_eq!(
        summary.warnings,
        [
            "MONITOR does not prevent a leak.",
            "The pump pressure is above 470 mbar: the Leakshield can only work as a leak monitor."
        ]
    );
    let locked = summary
        .rows
        .iter()
        .find(|r| r.field == "reservoir_total_volume")
        .unwrap();
    assert!(locked.padlock && !locked.counted);
    assert_eq!(
        locked.note.as_deref(),
        Some("Not a Verified field: this Safety field has no Verification write yet.")
    );
    let pressure = summary
        .rows
        .iter()
        .find(|r| r.field == "pump_max_pressure")
        .unwrap();
    assert_eq!(
        (pressure.now.as_str(), pressure.new.as_str()),
        ("375 mbar", "480 mbar")
    );

    // The apply writes the changes that the device takes, in one call.
    let command = summary.apply_command.expect("a command");
    let Command::SetLeakshieldFields {
        fields, control, ..
    } = &command
    else {
        panic!("a field write");
    };
    assert_eq!(control, "wizard");
    assert_eq!(
        fields.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        [
            "default_mode",
            "pump_max_pressure",
            "pump_max_speed",
            "reservoir_air_volume_full"
        ]
    );
    let applied = ui.apply(&en(&state, &feedback), LEAKSHIELD);
    assert_eq!(applied, Some(command));
    assert_eq!(ui.applying, Some(4));

    // A value out of range lists the error and turns "Apply" off.
    ui.edit(&data, "pump_max_pressure", "9999");
    let model = settings(&state, &ui).summary.unwrap();
    assert_eq!(model.errors, ["Pump max pressure is out of range."]);
    assert!(ui.apply(&en(&state, &feedback), LEAKSHIELD).is_none());
    assert!(!settings(&state, &ui).summary.unwrap().apply_enabled);

    // The result: "Applying", then the success row, and the pending changes are gone.
    let mut running = Feedback::default();
    running.set(format!("ls:{LEAKSHIELD}:wizard"), WriteOutcome::Pending);
    ui.edit(&data, "pump_max_pressure", "380");
    let model = SettingsModel::build(&en(&state, &running), &ui, LEAKSHIELD)
        .summary
        .unwrap();
    assert_eq!(model.apply, "Applying 4 changes");
    assert!(model.applying && !model.apply_enabled);
    assert!(
        model
            .rows
            .iter()
            .all(|r| r.status.as_deref().is_some() || !r.counted)
    );
    let mut done = Feedback::default();
    done.set(
        format!("ls:{LEAKSHIELD}:wizard"),
        WriteOutcome::Applied { at_us: NOW_US },
    );
    ui.applying = Some(4);
    ui.reconcile(&en(&state, &done), LEAKSHIELD);
    assert!(ui.pending.is_empty());
    let summary = SettingsModel::build(&en(&state, &done), &ui, LEAKSHIELD)
        .summary
        .unwrap();
    assert_eq!(
        summary.success.as_deref(),
        Some("4 settings written and verified.")
    );

    // A failed write: the row, the restore and "Keep the current state".
    let mut failed = state.clone();
    failed.write_failure = Some(nicecooling::model::WriteFailure {
        leakshield: LEAKSHIELD.into(),
        error: "VerifyFailed".into(),
        backup: "backup_2".into(),
    });
    let summary = SettingsModel::build(&en(&failed, &feedback), &ui, LEAKSHIELD)
        .summary
        .unwrap();
    let failure = summary.failed.expect("the failure");
    assert_eq!(failure.text, "Write failed: the read-back does not match.");
    assert_eq!(
        failure.restore_command,
        Some(Command::RestoreBackup {
            leakshield: LEAKSHIELD.into(),
            backup: "backup_2".into()
        })
    );
    assert_eq!(
        (failure.restore.as_str(), failure.keep.as_str()),
        ("Restore the backup", "Keep the current state")
    );
    ui.kept = true;
    assert!(
        SettingsModel::build(&en(&failed, &feedback), &ui, LEAKSHIELD)
            .summary
            .unwrap()
            .failed
            .is_none()
    );

    // "Cancel" asks, and "Discard" clears the changes.
    let mut ui = SettingsUi::default();
    ui.request_discard();
    assert!(!ui.confirm_discard, "nothing to discard");
    ui.edit(&data, "pump_max_speed", "4900");
    ui.edit(&data, "pump_max_pressure", "380");
    ui.request_discard();
    assert_eq!(
        settings(&state, &ui).discard.unwrap().0,
        "Discard 2 changes?"
    );
    ui.discard();
    assert!(ui.pending.is_empty() && !ui.confirm_discard && ui.step == Step::Reservoir);
    // The two vacuum windows: the end follows the start by a minute.
    ui.edit(&data, "timer_1", "21:30");
    assert_eq!(
        ui.pending["vacuum_timer_start_hour"],
        FieldValue::List(vec![21.0, 18.0])
    );
    assert_eq!(
        ui.pending["vacuum_timer_end_minute"],
        FieldValue::List(vec![31.0, 1.0])
    );
    // Alt+Left and Alt+Right move between the steps.
    ui.step_by(1);
    ui.step_by(1);
    assert_eq!(ui.step, Step::Flow);
    ui.step_by(-9);
    assert_eq!(ui.step, Step::Reservoir);
}

#[test]
fn leakshield_settings_labels() {
    // The device has no "automatic fill-level measurement" field in the daemon contract.
    let missing = labels::missing_labels(
        "leakshield-settings.md",
        &[
            "Step 4 of 7",
            "Applying 2 of 5",
            "5 settings written and verified.",
            "80 to 450 mbar.",
            "Automatic fill-level measurement",
            "Automatic fill-level measurement releases the pressure for a short time.",
            "The Leakshield measures the fill level after a change, for example when you open the reservoir. During the measurement the Loop has no leak protection.",
            "Discard 2 changes?",
            "Not a Verified field: this Safety field has no Verification write yet.",
        ],
    );
    assert!(missing.is_empty(), "labels without text: {missing:?}");
    let italian = SettingsModel::build(
        &input(&with_leakshield(), &Feedback::default(), IT),
        &SettingsUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(italian.counter, "Passo 1 di 7");
    assert_eq!(
        italian.no_changes.as_deref(),
        Some("Ancora nessuna modifica.")
    );
}

/// A Harness on a private bus with a fake daemon, once the Leakshield data is read.
async fn started(options: Options) -> (PrivateBus, FakeDaemon, Harness) {
    let bus = PrivateBus::start("leakshield-screens");
    let daemon = FakeDaemon::start(bus.connect().await, options).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Leakshield data is read", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_some_and(|data| !data.fields.is_empty())
        })
        .await;
    (bus, daemon, harness)
}

/// What the app shows after a command: the result of the write as the window stores it.
async fn run(harness: &Harness, command: &Command) -> Feedback {
    let mut feedback = Feedback::default();
    if let Some(key) = command.feedback_key() {
        let outcome = match command.run_on_daemon(&harness.client).await {
            Ok(_) => WriteOutcome::Applied { at_us: NOW_US },
            Err(error) => WriteOutcome::from_error(&error),
        };
        feedback.set(key, outcome);
    }
    feedback
}

#[tokio::test(flavor = "multi_thread")]
async fn the_snapshot_holds_what_the_fake_leakshield_gives() {
    let (_bus, _daemon, harness) = started(Options::default()).await;
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert!(data.writable && data.structure == 100 && data.software_sensor_slots == 12);
    assert_eq!(data.clock_offset_s, -42);
    assert!(data.fields.len() > 40);
    assert_eq!(data.number("fill_level_warning_threshold"), Some(60.0));
    assert_eq!(
        data.text_list("page_template").map(<[String]>::len),
        Some(10)
    );
    assert_eq!(
        data.field("reservoir_total_volume").map(|f| f.writable),
        Some(false)
    );
    assert_eq!(data.events.len(), 3);
    assert_eq!(data.events[0].id, "mode-changed", "newest first");
    assert_eq!(data.events[0].details["from"], "monitor");
    assert_eq!(data.backups.len(), 1);
    assert_eq!(data.software_sensors.len(), 12);
    assert_eq!(data.proposals.len(), 6);
    let limits = data.pressure_limits("shield").expect("limits");
    assert_eq!(
        (limits.min, limits.target, limits.max),
        (250.0, 300.0, 350.0)
    );
    let info = &harness.state.snapshot.leakshields[0];
    assert_eq!(
        info.field_counts.map(|c| c.0 as usize),
        Some(data.fields.len())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn mode_and_actions_call_the_api() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let mut ui = OverviewUi::default();
    // A Mode that lowers the protection calls `SetMode` after the confirmation.
    assert!(
        ui.request_mode(&harness.state, LEAKSHIELD, "release")
            .is_none()
    );
    assert!(daemon.calls().is_empty(), "nothing before the answer");
    let command = ui.confirm(LEAKSHIELD).expect("a command");
    run(&harness, &command).await;
    assert_eq!(daemon.calls(), ["SetMode(release) interactive=true"]);
    // The Test writes its pressure, then starts.
    let no_feedback = Feedback::default();
    let screen_input = en(&harness.state, &no_feedback);
    ui.test_pressure = Some("320".into());
    let command = ui.start_test(&screen_input, LEAKSHIELD).expect("a command");
    run(&harness, &command).await;
    assert_eq!(
        daemon.calls()[1..],
        [
            "SetFields(test_pressure=320) interactive=true",
            "StartAction(test) interactive=true"
        ]
    );
    assert_eq!(
        daemon.stored_field("test_pressure").await,
        Some(Fv::Num(320.0))
    );
    // Cancel.
    let cancel = Command::CancelLeakshieldAction {
        leakshield: LEAKSHIELD.into(),
    };
    assert!(
        !run(&harness, &cancel)
            .await
            .get(&format!("ls:{LEAKSHIELD}:action"))
            .unwrap()
            .is_failure()
    );
    assert_eq!(
        daemon.calls().last().unwrap(),
        "CancelAction() interactive=true"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn software_sensor_and_alarm_writes_reach_the_fake_daemon() {
    let (_bus, daemon, mut harness) = started(Options::default()).await;
    let command = Command::SetSoftwareSensor {
        leakshield: LEAKSHIELD.into(),
        slot: 4,
        sensor: "nct6798_temp1".into(),
    };
    run(&harness, &command).await;
    let data = harness.client.leakshield_data(LEAKSHIELD).await;
    assert_eq!(data.software_sensors[3], (4, Some("nct6798_temp1".into())));
    let clear = SensorsUi::default().request_clear(&harness.state, LEAKSHIELD, 4);
    // The slot is free in the state of the harness: the clear goes at once.
    assert!(clear.is_some());
    run(&harness, &clear.unwrap()).await;
    assert_eq!(
        harness
            .client
            .leakshield_data(LEAKSHIELD)
            .await
            .software_sensors[3]
            .1,
        None
    );

    // A threshold, a Device response and the sound.
    harness.until("the state is read", |s| s.is_online()).await;
    let feedback = Feedback::default();
    let screen_input = en(&harness.state, &feedback);
    let ui = AlarmsUi;
    let threshold = ui
        .set_threshold(
            &screen_input,
            LEAKSHIELD,
            "leak_rate_warning_threshold",
            120.0,
        )
        .expect("a command");
    run(&harness, &threshold).await;
    assert_eq!(
        daemon.stored_field("leak_rate_warning_threshold").await,
        Some(Fv::Int(120))
    );
    let response = ui
        .set_device_response(&screen_input, LEAKSHIELD, "leak-warning", "key-sequence")
        .expect("a command");
    run(&harness, &response).await;
    assert_eq!(
        daemon.calls().last().unwrap(),
        "SetFields(leak_rate_warning_response_action=key-sequence) interactive=true"
    );
    assert_eq!(
        daemon
            .stored_field("leak_rate_warning_response_action")
            .await,
        Some(Fv::Text("key-sequence".into()))
    );
    let refreshed = harness.client.leakshield_data(LEAKSHIELD).await;
    assert_eq!(refreshed.number("leak_rate_warning_threshold"), Some(120.0));
    // The proposals follow the stored responses: the key sequence is a warning now.
    assert!(
        refreshed.proposals.iter().any(|p| p.alarm == "leak-warning"
            && p.current == "key-sequence"
            && p.proposed == "none")
    );
    // The alarm response goes to the daemon of the alarm responses.
    let alarm_response = ui.set_alarm_response(
        &screen_input,
        LEAKSHIELD,
        "leakshield.leak-warning",
        Response::Poweroff,
        Some(20),
    );
    run(&harness, &alarm_response).await;
    assert!(daemon.calls().iter().any(|c| c
        == &format!(
            "SetAlarmResponse(leakshield.leak-warning@{LEAKSHIELD},poweroff,20) interactive=true"
        )));
    // A field that the device does not take: the daemon refuses.
    let refused = Command::SetLeakshieldFields {
        leakshield: LEAKSHIELD.into(),
        control: "wizard".into(),
        fields: vec![("reservoir_total_volume".into(), FieldValue::Number(300.0))],
    };
    let feedback = run(&harness, &refused).await;
    assert_eq!(
        feedback.get(&format!("ls:{LEAKSHIELD}:wizard")),
        Some(&WriteOutcome::Refused {
            error: "NotVerified".into()
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wizard_writes_once_through_the_fake_daemon() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let data = harness.state.snapshot.leakshield_data[LEAKSHIELD].clone();
    let feedback = Feedback::default();
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pump_max_pressure", "380");
    ui.edit(&data, "pump_max_speed", "4900");
    ui.edit(&data, "reservoir_total_volume", "300");
    assert!(
        daemon.calls().is_empty(),
        "the wizard writes nothing before Apply"
    );
    let command = ui
        .apply(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("a command");
    let done = run(&harness, &command).await;
    assert_eq!(
        daemon.calls(),
        ["SetFields(pump_max_pressure=380,pump_max_speed=4900) interactive=true"],
        "one call, and not the read-only field"
    );
    assert_eq!(
        daemon.stored_field("pump_max_speed").await,
        Some(Fv::Int(4900))
    );
    assert_eq!(
        daemon.stored_field("reservoir_total_volume").await,
        Some(Fv::Int(270))
    );
    ui.reconcile(&en(&harness.state, &done), LEAKSHIELD);
    assert_eq!(ui.applied, Some(2));
    // A denial by polkit.
    daemon.deny_writes(true);
    ui.edit(&data, "pump_max_speed", "4700");
    let command = ui
        .apply(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("a command");
    let denied = run(&harness, &command).await;
    assert_eq!(
        denied.get(&format!("ls:{LEAKSHIELD}:wizard")),
        Some(&WriteOutcome::NotAuthorized)
    );
    assert_eq!(
        daemon.stored_field("pump_max_speed").await,
        Some(Fv::Int(4900)),
        "nothing changed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn display_and_lighting_writes_reach_the_fake_daemon() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let feedback = Feedback::default();
    let screen_input = en(&harness.state, &feedback);
    let mut display = DisplayUi::default();
    display.select(4);
    let command = display
        .set_template(&screen_input, LEAKSHIELD, "time")
        .expect("a command");
    run(&harness, &command).await;
    // The template goes out as a list of names, and comes back as one.
    let stored = daemon.stored_field("page_template").await;
    let expected: Vec<String> = [
        "leakshield-state",
        "values-1",
        "chart",
        "values-2",
        "time",
        "chart",
        "time",
        "none",
        "none",
        "none",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(stored, Some(Fv::TextList(expected.clone())));
    assert_eq!(
        harness
            .client
            .leakshield_data(LEAKSHIELD)
            .await
            .text_list("page_template"),
        Some(&expected[..])
    );
    // The brightness goes out as its name and comes back as one.
    let brightness = nicecooling::screens::leakshield::display::set_brightness(
        &screen_input,
        LEAKSHIELD,
        "display_brightness_idle",
        "off",
    )
    .expect("a command");
    run(&harness, &brightness).await;
    assert_eq!(
        daemon.stored_field("display_brightness_idle").await,
        Some(Fv::Text("off".into()))
    );
    let refreshed = harness.client.leakshield_data(LEAKSHIELD).await;
    assert_eq!(
        refreshed.field("display_brightness_idle").map(|f| &f.value),
        Some(&FieldValue::Text("off".into()))
    );
    let modes = |list: [&str; 2]| Some(Fv::TextList(list.map(String::from).to_vec()));
    // Both controllers of the device are in use: "Add" sends nothing.
    let mut lighting = LightingUi::default();
    assert!(lighting.add(&screen_input, LEAKSHIELD).is_none());
    lighting.toggle(1);
    let command = lighting
        .delete(&screen_input, LEAKSHIELD)
        .expect("a command");
    run(&harness, &command).await;
    assert_eq!(
        daemon.stored_field("led_controller_mode").await,
        modes(["rainbow", "unused"])
    );
    let refreshed = |data| {
        let mut state = harness.state.clone();
        state
            .snapshot
            .leakshield_data
            .insert(LEAKSHIELD.into(), data);
        state
    };
    let state = refreshed(harness.client.leakshield_data(LEAKSHIELD).await);
    let command = LightingUi::default()
        .add(&en(&state, &feedback), LEAKSHIELD)
        .expect("a command");
    run(&harness, &command).await;
    assert_eq!(
        daemon.stored_field("led_controller_mode").await,
        modes(["rainbow", "fixed-color"])
    );
    let state = refreshed(harness.client.leakshield_data(LEAKSHIELD).await);
    let model = LightingModel::build(&en(&state, &feedback), &LightingUi::default(), LEAKSHIELD);
    assert_eq!(model.bars.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_device_alarms_button_shows_while_a_latched_bit_is_set() {
    use support::fake_daemon::Details;
    let (_bus, daemon, mut harness) = started(Options::default()).await;
    // A current bit without a latched bit: no button.
    daemon
        .raise_alarm(
            "a1",
            "leakshield.leak-warning",
            "warning",
            LEAKSHIELD,
            "notify",
            0,
        )
        .await;
    harness
        .until("the Warning arrives", |state| {
            state.snapshot.alarms.len() == 1
        })
        .await;
    let feedback = Feedback::default();
    assert!(alarms(&harness.state, &feedback).reset.is_none());

    // The Alarm holds a latched bit.
    daemon
        .raise_alarm_with_details(
            "a2",
            "leakshield.leak-alarm",
            "alarm",
            LEAKSHIELD,
            "poweroff",
            30_000_000,
            Details {
                latched_bits: 1 << 6,
                ..Details::default()
            },
        )
        .await;
    harness
        .until("the latched Alarm arrives", |state| {
            state
                .snapshot
                .alarms
                .iter()
                .any(|a| a.details.latched_bits == 1 << 6)
        })
        .await;
    let model = alarms(&harness.state, &feedback);
    let reset = model.reset.as_ref().expect("the reset row");
    assert_eq!(reset.label, "Reset device alarms");
    assert!(reset.enabled && daemon.calls().is_empty());
    let italian = AlarmsModel::build(&input(&harness.state, &feedback, IT), &AlarmsUi, LEAKSHIELD);
    assert_eq!(
        italian.reset.expect("the reset row").label,
        "Azzera gli allarmi del dispositivo"
    );

    let result = run(&harness, &reset.command).await;
    assert_eq!(daemon.calls(), ["ResetAlarms() interactive=true"]);
    assert!(
        !result
            .get(&format!("ls:{LEAKSHIELD}:alarm-reset"))
            .unwrap()
            .is_failure()
    );
    // The daemon clears the Alarm: the button goes.
    daemon.clear_alarm("a2").await;
    harness
        .until("the Alarm clears", |state| {
            !state
                .snapshot
                .alarms
                .iter()
                .any(|a| a.details.latched_bits != 0)
        })
        .await;
    assert!(alarms(&harness.state, &feedback).reset.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_device_alarms_not_supported_shows_the_refusal() {
    let (_bus, _daemon, harness) = started(Options {
        no_reset_commands: true,
        ..Options::default()
    })
    .await;
    let command = Command::ResetLeakshieldAlarms {
        leakshield: LEAKSHIELD.into(),
    };
    let result = run(&harness, &command).await;
    assert_eq!(
        result.get(&format!("ls:{LEAKSHIELD}:alarm-reset")),
        Some(&WriteOutcome::Refused {
            error: "NotSupported".into()
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_refill_history_button_calls_api_after_confirmation() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let mut ui = SystemUi::default();
    ui.request(Ask::RefillHistory);
    let feedback = Feedback::default();
    let dialog = system(&harness.state, &ui, &feedback)
        .dialog
        .expect("a dialog");
    assert_eq!(
        dialog.text,
        "Delete the refill history on the Leakshield? This cannot be undone."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Reset history")
    );
    assert!(daemon.calls().is_empty(), "nothing before the answer");
    // "Cancel" sends nothing.
    ui.cancel();
    assert!(ui.confirm(LEAKSHIELD).is_none() && daemon.calls().is_empty());
    ui.request(Ask::RefillHistory);
    let command = ui.confirm(LEAKSHIELD).expect("a command");
    assert_eq!(
        command,
        Command::ResetRefillHistory {
            leakshield: LEAKSHIELD.into()
        }
    );
    let result = run(&harness, &command).await;
    assert_eq!(daemon.calls(), ["ResetRefillHistory() interactive=true"]);
    assert!(
        !result
            .get(&format!("ls:{LEAKSHIELD}:refill-history"))
            .unwrap()
            .is_failure()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_service_counter_button_calls_api_after_confirmation() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let mut ui = SystemUi::default();
    ui.request(Ask::ServiceCounter);
    let dialog = system(&harness.state, &ui, &Feedback::default())
        .dialog
        .expect("a dialog");
    assert_eq!(
        dialog.text,
        "Reset the service counter of the Leakshield? Do this only after Aqua Computer did the maintenance. This cannot be undone."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Reset counter")
    );
    assert!(daemon.calls().is_empty());
    let command = ui.confirm(LEAKSHIELD).expect("a command");
    assert_eq!(
        command,
        Command::ResetServiceCounter {
            leakshield: LEAKSHIELD.into()
        }
    );
    run(&harness, &command).await;
    assert_eq!(daemon.calls(), ["ResetServiceCounter() interactive=true"]);
    // The backups, the clock and the restore.
    for command in [
        Command::CreateBackup {
            leakshield: LEAKSHIELD.into(),
        },
        Command::SyncLeakshieldClock {
            leakshield: LEAKSHIELD.into(),
        },
        Command::RestoreBackup {
            leakshield: LEAKSHIELD.into(),
            backup: "backup_1".into(),
        },
        Command::FactoryReset {
            leakshield: LEAKSHIELD.into(),
        },
    ] {
        run(&harness, &command).await;
    }
    assert_eq!(
        daemon.calls()[1..],
        [
            "CreateBackup() interactive=true",
            "SyncClock() interactive=true",
            format!("RestoreBackup({LEAKSHIELD},backup_1) interactive=true").as_str(),
            "FactoryReset() interactive=true"
        ]
    );
    let data = harness.client.leakshield_data(LEAKSHIELD).await;
    assert_eq!(data.backups.len(), 2);
    assert_eq!(data.backups[0].reason, "manual", "newest first");
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_not_supported_shows_message() {
    let (_bus, daemon, harness) = started(Options {
        no_reset_commands: true,
        ..Options::default()
    })
    .await;
    let mut feedback = Feedback::default();
    for command in [
        Command::ResetRefillHistory {
            leakshield: LEAKSHIELD.into(),
        },
        Command::ResetServiceCounter {
            leakshield: LEAKSHIELD.into(),
        },
    ] {
        let key = command.feedback_key().unwrap();
        let outcome = run(&harness, &command).await.get(&key).cloned().unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::Refused {
                error: "NotSupported".into()
            }
        );
        feedback.set(key, outcome);
    }
    assert_eq!(daemon.calls().len(), 2, "the app sent both calls");
    let model = system(&harness.state, &SystemUi::default(), &feedback);
    assert_eq!(
        model.refill.unsupported.as_deref(),
        Some("This firmware has no such command.")
    );
    assert_eq!(
        model.device.counter_unsupported.as_deref(),
        Some("This firmware has no such command.")
    );
    assert!(model.refill.line.is_none() && model.device.counter_line.is_none());
    // The text is the same in Italian.
    let italian = SystemModel::build(
        &input(&harness.state, &feedback, IT),
        &SystemUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        italian.refill.unsupported.as_deref(),
        Some("Questo firmware non ha questo comando.")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_backup_file_goes_through_the_fake_daemon() {
    let (_bus, daemon, harness) = started(Options::default()).await;
    let dir = std::env::temp_dir().join(format!("nicecooling-backup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("backup.txt");
    let export = Command::ExportBackup {
        leakshield: LEAKSHIELD.into(),
        backup: "backup_1".into(),
        path: path.clone(),
    };
    run(&harness, &export).await;
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "firmware 1022\nreport3 030064\nreport5 0501\nreport8 0802\n"
    );
    let import = Command::ImportBackup {
        leakshield: LEAKSHIELD.into(),
        path: path.clone(),
    };
    run(&harness, &import).await;
    assert!(
        daemon
            .calls()
            .iter()
            .any(|c| c == "ImportBackup(1022,3,2,2) interactive=true")
    );
    assert_eq!(
        harness.client.leakshield_data(LEAKSHIELD).await.backups[0].reason,
        "import"
    );
    // A file that is no backup is refused before the call.
    std::fs::write(&path, "nothing").unwrap();
    let refused = run(&harness, &import).await;
    assert!(
        refused
            .get(&format!("ls:{LEAKSHIELD}:backups"))
            .unwrap()
            .is_failure()
    );
    assert_eq!(
        daemon
            .calls()
            .iter()
            .filter(|c| c.starts_with("ImportBackup"))
            .count(),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Review 11 (adversarial): the tab of one Leakshield must show and write what that Leakshield
// resolves, not the default of the event, once an override exists.
#[test]
fn review_tab_shows_the_override_of_its_leakshield() {
    let mut state = with_leakshield();
    state.snapshot.responses = vec![
        ResponseRule {
            event: "leakshield.leak-alarm".into(),
            source: String::new(),
            response: Response::Poweroff,
            delay_s: 30,
            overridden: false,
        },
        ResponseRule {
            event: "leakshield.leak-alarm".into(),
            source: LEAKSHIELD.into(),
            response: Response::None,
            delay_s: 30,
            overridden: true,
        },
    ];
    let model = alarms(&state, &Feedback::default());
    // The daemon resolves `none` for this Leakshield.
    assert_eq!(row(&model, "leak-alarm").response.response, Response::None);
}

#[test]
fn review_tab_writes_the_override_of_its_leakshield() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let command = AlarmsUi.set_alarm_response(
        &screen_input,
        LEAKSHIELD,
        "leakshield.leak-alarm",
        Response::Notify,
        None,
    );
    let Command::SetAlarmResponse { source, .. } = command else {
        panic!("not a response command");
    };
    // The tab belongs to one Leakshield: the write targets it, not every Leakshield.
    assert_eq!(source, LEAKSHIELD);
}

#[test]
fn tab_reset_only_on_a_row_with_override() {
    let mut state = with_leakshield();
    let rule = |source: &str, response: Response, overridden: bool| ResponseRule {
        event: "leakshield.leak-alarm".into(),
        source: source.into(),
        response,
        delay_s: 30,
        overridden,
    };
    state.snapshot.responses = vec![
        rule("", Response::Poweroff, false),
        rule(LEAKSHIELD, Response::Poweroff, false),
    ];
    let feedback = Feedback::default();
    let model = alarms(&state, &feedback);
    assert_eq!(row(&model, "leak-alarm").response.reset, None);

    state.snapshot.responses[1] = rule(LEAKSHIELD, Response::None, true);
    let model = alarms(&state, &feedback);
    assert_eq!(
        row(&model, "leak-alarm").response.reset,
        Some(Command::ResetResponse {
            event: "leakshield.leak-alarm".into(),
            source: LEAKSHIELD.into(),
        })
    );
    // The write feedback of the row is the one of its Leakshield.
    let mut done = Feedback::default();
    done.set(
        format!("alarm-response:leakshield.leak-alarm@{LEAKSHIELD}"),
        nicecooling::screens::WriteOutcome::Applied { at_us: 1 },
    );
    let model = alarms(&state, &done);
    assert!(row(&model, "leak-alarm").response.write_line.is_some());
}

#[test]
fn refill_ignores_pumps_of_other_loops() {
    use nicecooling::model::LoopInfo;
    let state = tweak(|state| {
        state.snapshot.loops.push(LoopInfo {
            id: "loop_2".into(),
            name: "Other".into(),
            leakshield: Some("leakshield_1_1".into()),
            pump_channels: vec!["nct6798_pwm2".into()],
            ..Default::default()
        });
        for sample in [&mut state.sensors, &mut state.last_sensors]
            .into_iter()
            .flatten()
        {
            sample
                .sensors
                .retain(|(id, _, _)| id != "nct6798_fan6" && id != "nct6798_fan2");
            sample.sensors.push(("nct6798_fan6".into(), 0.0, 0));
            sample.sensors.push(("nct6798_fan2".into(), 1800.0, 0));
        }
    });
    let mut ui = OverviewUi::default();
    assert!(ui.request_action(&state, LEAKSHIELD, "refill").is_some());
    assert!(ui.confirm.is_none());
}

#[test]
fn refill_warns_when_one_of_two_pumps_has_no_value() {
    let state = tweak(|state| {
        state.snapshot.loops[0].pump_channels = vec!["nct6798_pwm6".into(), "nct6798_pwm2".into()];
        for sample in [&mut state.sensors, &mut state.last_sensors]
            .into_iter()
            .flatten()
        {
            sample
                .sensors
                .retain(|(id, _, _)| id != "nct6798_fan6" && id != "nct6798_fan2");
            sample.sensors.push(("nct6798_fan6".into(), 0.0, 0));
            sample.sensors.push((
                "nct6798_fan2".into(),
                0.0,
                nicecooling::model::FLAG_NO_VALUE,
            ));
        }
    });
    let mut ui = OverviewUi::default();
    assert!(ui.request_action(&state, LEAKSHIELD, "refill").is_none());
    assert_eq!(ui.confirm, Some(Confirm::Refill));
}

#[test]
fn refill_warns_with_only_a_stale_sample() {
    let state = tweak(|state| {
        let mut last = state.sensors.take().unwrap();
        last.sensors.retain(|(id, _, _)| id != "nct6798_fan6");
        last.sensors.push(("nct6798_fan6".into(), 0.0, 0));
        state.last_sensors = Some(last);
    });
    let mut ui = OverviewUi::default();
    // After a reconnect the old sample can say 0 rpm while the pump runs now: no fresh value warns.
    assert!(ui.request_action(&state, LEAKSHIELD, "refill").is_none());
    assert_eq!(ui.confirm, Some(Confirm::Refill));
}

#[test]
fn chart_axis_and_signed_changes() {
    use nicecooling::screens::leakshield::change_text;
    use nicecooling::screens::leakshield::charts::value_axis;
    // The value axis starts at a round value under the data and has about three steps.
    assert_eq!(value_axis(212.0, 350.0), (200.0, 50.0, 3));
    assert_eq!(value_axis(-8.0, 465.0), (-200.0, 200.0, 4));
    // One value still gives an axis with a height.
    assert_eq!(value_axis(300.0, 300.0), (299.0, 1.0, 2));

    // A change shows its sign. A value that rounds to zero has none.
    let locale = nicecooling::i18n::EN;
    assert_eq!(change_text(locale, Some(0.4), "mbar/min"), "+0.4 mbar/min");
    assert_eq!(change_text(locale, Some(-2.0), "ml/h"), "-2 ml/h");
    assert_eq!(change_text(locale, Some(0.04), "ml/h"), "0 ml/h");
    assert_eq!(change_text(locale, None, "ml/h"), "--");
}
