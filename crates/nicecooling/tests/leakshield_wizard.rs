//! The pressure part of the Leakshield wizard: the control band of SHIELD and the settings that
//! give its values (`docs/ui/screens/leakshield-settings.md`).

mod support;

use nicecooling::i18n::{EN, IT, Locale};
use nicecooling::model::{AppState, FieldEntry, FieldRisk, FieldValue, LeakshieldData};
use nicecooling::screens::leakshield::band::stack;
use nicecooling::screens::leakshield::settings::{
    Band, BandAxis, BandDiagram, Control, Row, SettingsModel, SettingsUi, Step, pump_pressure_at,
    pump_speed_for,
};
use nicecooling::screens::{Feedback, ScreenInput};
use support::fixtures::{LEAKSHIELD, NOW_US, with_leakshield};

fn input<'a>(state: &'a AppState, feedback: &'a Feedback, locale: Locale) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale,
        down_since_us: None,
        feedback,
    }
}

/// The Leakshield of the fixture as the reference device has it: every field of the wizard
/// takes a write, and the fields that the fixture lacks are there.
fn device() -> AppState {
    let mut state = with_leakshield();
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    for field in &mut data.fields {
        field.writable = true;
        field.reason = String::new();
    }
    for (id, value) in [("pump_flags", 0.0), ("loop_height", 30.0)] {
        data.fields.push(FieldEntry {
            id: id.into(),
            risk: FieldRisk::Safety,
            value: FieldValue::Number(value),
            writable: true,
            reason: String::new(),
        });
    }
    state
}

fn data_of(state: &AppState) -> LeakshieldData {
    state.snapshot.leakshield_data[LEAKSHIELD].clone()
}

fn set_field(state: &mut AppState, id: &str, value: FieldValue) {
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.fields.iter_mut().find(|f| f.id == id).unwrap().value = value;
}

fn set_sensor(state: &mut AppState, name: &str, value: f64) {
    for sample in [&mut state.sensors, &mut state.last_sensors]
        .into_iter()
        .flatten()
    {
        let id = format!("{LEAKSHIELD}_{name}");
        sample.sensors.iter_mut().find(|s| s.0 == id).unwrap().1 = value;
    }
}

fn step(state: &AppState, ui: &SettingsUi, step: Step) -> SettingsModel {
    let mut ui = ui.clone();
    ui.go(step);
    SettingsModel::build(&input(state, &Feedback::default(), EN), &ui, LEAKSHIELD)
}

fn row<'a>(model: &'a SettingsModel, key: &str) -> &'a Row {
    model
        .rows
        .iter()
        .find(|row| row.key == key)
        .unwrap_or_else(|| panic!("no row {key}"))
}

fn number_text(row: &Row) -> &str {
    match &row.control {
        Control::Number { text, .. } => text,
        other => panic!("{} is no number: {other:?}", row.key),
    }
}

fn pending(ui: &SettingsUi, field: &str) -> Option<f64> {
    match ui.pending.get(field) {
        Some(FieldValue::Number(value)) => Some(*value),
        _ => None,
    }
}

#[test]
fn the_pressure_step_shows_the_control_band_and_two_choices() {
    let state = device();
    let ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pressure);
    let keys: Vec<&str> = model.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "pressure_values",
            "band",
            "band_max",
            "band_target",
            "band_min",
            "adaptive",
            "vacuum_timer",
            "timer_1",
            "timer_2"
        ]
    );

    // `shield_flags` 127 has bit 4 set: the recommended values.
    let values = row(&model, "pressure_values");
    let Control::Radio(choices) = &values.control else {
        panic!("two choices");
    };
    assert_eq!(
        choices
            .iter()
            .map(|c| (c.0.as_str(), c.2))
            .collect::<Vec<_>>(),
        [("recommended", true), ("own", false)]
    );
    assert!(choices[0].1.starts_with("Recommended values:"));
    assert!(choices[1].1.starts_with("My own values:"));
    assert!(
        values
            .caption
            .as_deref()
            .unwrap()
            .contains("a control band for SHIELD")
    );

    // Each value says what the Leakshield does there. With the recommended values the
    // inputs take no edit, and the badge says why.
    let texts: Vec<(&str, &str, &str)> = ["band_max", "band_target", "band_min"]
        .iter()
        .map(|key| {
            let row = row(&model, key);
            (
                row.label.as_str(),
                number_text(row),
                row.caption.as_deref().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        texts,
        [
            (
                "Maximum pressure",
                "350",
                "Above this value the Leakshield opens its vent valve and releases vacuum. 80 to 500 mbar."
            ),
            (
                "Target pressure",
                "300",
                "The pressure that you want in SHIELD. The pump of the Leakshield builds the vacuum up to this value and stops. 50 to 450 mbar."
            ),
            (
                "Minimum pressure",
                "250",
                "Below this value the Leakshield starts its pump. 30 to 430 mbar."
            ),
        ]
    );
    for key in ["band_max", "band_target", "band_min"] {
        let row = row(&model, key);
        assert!(row.fixed && row.writable, "{key}");
        assert_eq!(row.badge, "Recommended value");
    }
}

#[test]
fn own_values_clear_bit_4_and_open_the_inputs() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pressure_values", "own");
    assert_eq!(pending(&ui, "shield_flags"), Some(111.0), "127 without 16");
    let model = step(&state, &ui, Step::Pressure);
    let values = row(&model, "pressure_values");
    assert!(values.changed);
    let note = values.note.as_ref().expect("the warning of own values");
    assert!(note.warning);
    assert!(note.text.starts_with("With your own values"));
    for key in ["band_max", "band_target", "band_min"] {
        let row = row(&model, key);
        assert!(!row.fixed, "{key}");
        assert_eq!(row.badge, "Verified field");
    }
    // The other switches of `shield_flags` did not change.
    assert!(!row(&model, "adaptive").changed);
    assert!(!row(&model, "vacuum_timer").changed);

    // Back to the recommended values: the bit is set again, which is the device value.
    ui.edit(&data, "pressure_values", "recommended");
    assert_eq!(pending(&ui, "shield_flags"), None);
}

#[test]
fn adaptive_pressure_is_bit_3_and_never_dims_the_values() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pressure);
    let adaptive = row(&model, "adaptive");
    assert_eq!(adaptive.control, Control::Switch(true));
    assert_eq!(
        adaptive.caption.as_deref(),
        Some(
            "The Leakshield lowers the minimum and the target with the pump speed of the Feed. The distance between minimum and target and the maximum stay as set here."
        )
    );
    for key in ["band_max", "band_target", "band_min"] {
        assert!(!row(&model, key).dim, "{key} with adaptive pressure on");
    }

    ui.edit(&data, "adaptive", "0");
    assert_eq!(pending(&ui, "shield_flags"), Some(119.0), "127 without 8");
    let model = step(&state, &ui, Step::Pressure);
    assert_eq!(row(&model, "adaptive").control, Control::Switch(false));
    assert!(row(&model, "adaptive").changed);
    assert!(!row(&model, "pressure_values").changed);
    for key in ["band_max", "band_target", "band_min"] {
        let row = row(&model, key);
        assert!(!row.dim && row.fixed, "{key} with adaptive pressure off");
    }
}

/// The diagram of the Pressure step and the caption under it.
fn diagram(state: &AppState, ui: &SettingsUi, locale: Locale) -> (BandDiagram, String) {
    let mut ui = ui.clone();
    ui.go(Step::Pressure);
    let model = SettingsModel::build(&input(state, &Feedback::default(), locale), &ui, LEAKSHIELD);
    let row = row(&model, "band").clone();
    let Control::Band(diagram) = row.control else {
        panic!("the diagram");
    };
    (*diagram, row.caption.expect("the caption of the diagram"))
}

fn labels(
    lines: &[nicecooling::screens::leakshield::settings::BandLine; 3],
) -> Vec<(&str, Option<&str>)> {
    lines
        .iter()
        .map(|line| (line.label.as_str(), line.note.as_deref()))
        .collect()
}

/// The reference device in adaptive mode: 375 / 425 / 465 stored, and the Leakshield uses
/// 172.8 / 222.8 / 465 at 389 mbar.
fn adaptive() -> AppState {
    let mut state = device();
    set_field(&mut state, "shield_pressure_low", FieldValue::Number(375.0));
    set_field(
        &mut state,
        "shield_pressure_max_range",
        FieldValue::Number(40.0),
    );
    set_sensor(&mut state, "pressure_low", 172.8);
    set_sensor(&mut state, "pressure_setpoint", 222.8);
    set_sensor(&mut state, "pressure_high", 465.0);
    set_sensor(&mut state, "pressure", 389.0);
    state
}

#[test]
fn the_band_diagram_is_true_to_scale() {
    // One linear axis: the same distance in mbar is the same distance in the diagram.
    let axis = BandAxis::covering([172.8, 222.8, 465.0, 375.0, 425.0, 389.0]);
    assert!(
        axis.low < 172.8 && axis.high > 465.0,
        "a margin at both ends"
    );
    assert_eq!(axis.place(axis.high), 0.0);
    assert_eq!(axis.place(axis.low), 1.0);
    let step = axis.place(300.0) - axis.place(350.0);
    assert!((axis.place(400.0) - axis.place(450.0) - step).abs() < 1e-6);
    // The marks are round values inside the axis.
    assert_eq!(axis.ticks(), [200.0, 300.0, 400.0, 500.0]);
    assert_eq!(
        BandAxis::covering([375.0, 425.0, 465.0]).ticks(),
        [400.0, 450.0]
    );
    // A band without a height keeps a span, and no value gives the whole range.
    let flat = BandAxis::covering([100.0, 100.0, 100.0]);
    assert!(flat.high - flat.low >= 20.0);
    assert_eq!(
        BandAxis::covering([]),
        BandAxis {
            low: 0.0,
            high: 500.0
        }
    );

    // The solid lines are the limits that the Leakshield uses now. A stored value that
    // differs shows under the live one.
    let state = adaptive();
    let (band, caption) = diagram(&state, &SettingsUi::default(), EN);
    assert!(band.live);
    assert_eq!(
        labels(&band.lines),
        [
            ("Maximum 465 mbar", None),
            ("Target 222.8 mbar", Some("set: 425 mbar")),
            ("Minimum 172.8 mbar", Some("set: 375 mbar"))
        ]
    );
    assert_eq!(
        caption,
        "Now: minimum 172.8, target 222.8, maximum 465 mbar."
    );
    assert!(band.draft.is_none(), "the wizard holds the device values");
    let [max, target, min] = [0, 1, 2].map(|index| band.lines[index].place);
    assert!(0.0 < max && max < target && target < min && min < 1.0);
    // 389 mbar is between the live target 222.8 and the live maximum 465, at its place on
    // the axis: 76 mbar under the maximum of a distance of 242.2 mbar.
    let (place, text) = band.marker.clone().expect("the live pressure");
    assert_eq!(text, "Now 389 mbar");
    assert!(max < place && place < target, "{max} < {place} < {target}");
    let part = (place - max) / (target - max);
    assert!((part - 76.0 / 242.2).abs() < 1e-4, "{part}");
    // The marks of the axis are at the places of their values.
    let (tick_place, tick) = &band.ticks[0];
    assert_eq!(tick, "200");
    assert!(
        target < *tick_place && *tick_place < min,
        "200 is between 172.8 and 222.8"
    );

    // The same pressure against a draft: dashed lines at 380 / 425 / 490, and the marker is
    // under the dashed 425 and above the dashed 380.
    let mut own = adaptive();
    set_field(&mut own, "shield_flags", FieldValue::Number(111.0));
    let data = data_of(&own);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "band_min", "380");
    ui.edit(&data, "band_max", "490");
    let (band, caption) = diagram(&own, &ui, EN);
    let draft = band.draft.as_ref().expect("the draft band");
    assert_eq!(
        labels(draft),
        [
            ("new: 490 mbar", None),
            ("new: 425 mbar", None),
            ("new: 380 mbar", None)
        ]
    );
    let (place, _) = band.marker.unwrap();
    assert!(
        draft[1].place < place && place < draft[2].place,
        "389 is between the dashed 425 and the dashed 380"
    );
    assert!(draft[0].place < band.lines[0].place, "490 is above 465");
    // The solid lines stay the live band, with the stored values under them.
    assert_eq!(
        labels(&band.lines),
        [
            ("Maximum 465 mbar", None),
            ("Target 222.8 mbar", Some("set: 425 mbar")),
            ("Minimum 172.8 mbar", Some("set: 375 mbar"))
        ]
    );
    assert_eq!(
        caption,
        "Now: minimum 172.8, target 222.8, maximum 465 mbar. Dashed lines: the new values, not written yet."
    );

    // Italian.
    let (band, _) = diagram(&own, &ui, IT);
    assert_eq!(band.lines[1].label, "Obiettivo 222,8 mbar");
    assert_eq!(band.lines[1].note.as_deref(), Some("impostato: 425 mbar"));
    assert_eq!(band.draft.unwrap()[0].label, "nuovo: 490 mbar");
}

#[test]
fn the_dashed_band_shows_only_for_a_changed_draft() {
    // The fixture: 250 / 300 / 350 stored and in use, at 298 mbar.
    let state = own_values();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let (band, _) = diagram(&state, &ui, EN);
    assert!(band.draft.is_none());
    assert_eq!(
        labels(&band.lines),
        [
            ("Maximum 350 mbar", None),
            ("Target 300 mbar", None),
            ("Minimum 250 mbar", None)
        ]
    );
    let (place, _) = band.marker.clone().unwrap();
    assert!(band.lines[1].place < place && place < band.lines[2].place);

    // A change of another setting is no draft band.
    ui.edit(&data, "adaptive", "0");
    ui.edit(&data, "loop_height", "45");
    assert!(diagram(&state, &ui, EN).0.draft.is_none());

    // One edited value: the three dashed lines of the new band.
    ui.edit(&data, "band_max", "400");
    let (band, _) = diagram(&state, &ui, EN);
    assert_eq!(
        labels(band.draft.as_ref().unwrap()),
        [
            ("new: 400 mbar", None),
            ("new: 300 mbar", None),
            ("new: 250 mbar", None)
        ]
    );
    assert_eq!(
        band.lines[0].label, "Maximum 350 mbar",
        "the solid line stays"
    );
    assert_eq!(band.draft.as_ref().unwrap()[1].place, band.lines[1].place);
    // The device value again: no dashed band.
    ui.edit(&data, "band_max", "350");
    assert!(diagram(&state, &ui, EN).0.draft.is_none());

    // The recommended values pending: 375 / 425 / 465.
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "use_recommended", "");
    let (band, _) = diagram(&state, &ui, EN);
    assert_eq!(
        labels(band.draft.as_ref().unwrap()),
        [
            ("new: 465 mbar", None),
            ("new: 425 mbar", None),
            ("new: 375 mbar", None)
        ]
    );
}

#[test]
fn without_a_live_band_the_diagram_has_the_stored_band_alone() {
    let stored = [
        ("Maximum 465 mbar", None),
        ("Target 425 mbar", None),
        ("Minimum 375 mbar", None),
    ];
    let caption =
        "The Leakshield is not regulating in SHIELD now. The lines are the stored values.";
    let check = |state: &AppState, why: &str| {
        let (band, text) = diagram(state, &SettingsUi::default(), EN);
        assert!(!band.live, "{why}");
        assert!(band.marker.is_none(), "{why}: no marker");
        assert_eq!(labels(&band.lines), stored, "{why}");
        assert_eq!(text, caption, "{why}");
        let [max, target, min] = [0, 1, 2].map(|index| band.lines[index].place);
        assert!(
            0.0 < max && max < target && target < min && min < 1.0,
            "{why}"
        );
    };

    // The Leakshield does not answer.
    let mut offline = adaptive();
    offline.snapshot.leakshields[0].connected = false;
    check(&offline, "offline");
    // RELEASE: the limits of report 1 are no SHIELD band.
    let mut release = adaptive();
    release.snapshot.leakshields[0].mode = "release".into();
    check(&release, "RELEASE");
    let mut monitor = adaptive();
    monitor.snapshot.leakshields[0].mode = "monitor".into();
    check(&monitor, "MONITOR");
    // An Action: the three limits read the pressure of the Action.
    let mut action = adaptive();
    action.snapshot.leakshields[0].running_action = "test".into();
    for name in ["pressure_low", "pressure_setpoint", "pressure_high"] {
        set_sensor(&mut action, name, 300.0);
    }
    check(&action, "an Action");
    // Limits that are not in order, as during the fill-level measurement.
    let mut measuring = adaptive();
    for name in ["pressure_low", "pressure_setpoint", "pressure_high"] {
        set_sensor(&mut measuring, name, 100.0);
    }
    check(&measuring, "equal limits");

    // A draft still shows as dashed lines, and the caption names them.
    let mut own = release.clone();
    set_field(&mut own, "shield_flags", FieldValue::Number(111.0));
    let data = data_of(&own);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "band_target", "440");
    let (band, text) = diagram(&own, &ui, EN);
    assert_eq!(band.draft.as_ref().unwrap()[1].label, "new: 440 mbar");
    assert!(band.marker.is_none());
    assert_eq!(
        text,
        format!("{caption} Dashed lines: the new values, not written yet.")
    );
}

#[test]
fn the_values_of_two_close_lines_move_apart() {
    // Nothing in the way: each block keeps its place.
    assert_eq!(stack(&[(10.0, 18.0), (60.0, 34.0)], 220.0), [10.0, 60.0]);
    // The second block starts in the first one: it moves down.
    assert_eq!(stack(&[(10.0, 34.0), (30.0, 18.0)], 220.0), [10.0, 44.0]);
    // A block at the lower edge moves up, and pushes the one above it.
    assert_eq!(
        stack(&[(180.0, 18.0), (210.0, 34.0)], 220.0),
        [168.0, 186.0]
    );
    // A line at the upper edge: its text stays in the column.
    assert_eq!(stack(&[(-9.0, 18.0)], 220.0), [0.0]);
}

/// The device with own values: 250 / 300 / 350 mbar, standby 80.
fn own_values() -> AppState {
    let mut state = device();
    set_field(&mut state, "shield_flags", FieldValue::Number(111.0));
    state
}

fn band(min: f64, target: f64, max: f64, standby: f64) -> Band {
    Band {
        min,
        target,
        max,
        standby,
    }
}

#[test]
fn the_limiter_keeps_each_value_in_its_range() {
    let start = band(250.0, 300.0, 350.0, 80.0);
    let with = |edit: fn(&mut Band)| {
        let mut band = start;
        edit(&mut band);
        (band.min, band.target, band.max)
    };
    // The minimum: 30 to 430 mbar.
    assert_eq!(with(|b| b.set_min(5.0)), (30.0, 300.0, 350.0));
    assert_eq!(with(|b| b.set_min(999.0)), (430.0, 450.0, 480.0));
    // The target: 50 to 450 mbar.
    assert_eq!(with(|b| b.set_target(10.0)), (30.0, 50.0, 350.0));
    assert_eq!(with(|b| b.set_target(999.0)), (250.0, 450.0, 480.0));
    // The maximum: 80 to 500 mbar.
    assert_eq!(with(|b| b.set_max(10.0)), (30.0, 50.0, 80.0));
    assert_eq!(with(|b| b.set_max(999.0)), (250.0, 300.0, 500.0));
}

#[test]
fn a_higher_minimum_raises_the_target_and_then_the_maximum() {
    let mut values = band(250.0, 300.0, 350.0, 80.0);
    // 280 is the last minimum that leaves the target alone: 300 - 20.
    values.set_min(280.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (280.0, 300.0, 350.0)
    );
    values.set_min(290.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (290.0, 310.0, 350.0)
    );
    // The target 330 needs the maximum 360: 330 + 30.
    values.set_min(310.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (310.0, 330.0, 360.0)
    );
}

#[test]
fn a_higher_target_raises_the_maximum() {
    let mut values = band(250.0, 300.0, 350.0, 80.0);
    values.set_target(320.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (250.0, 320.0, 350.0)
    );
    values.set_target(321.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (250.0, 321.0, 351.0)
    );
}

#[test]
fn a_lower_maximum_lowers_the_target_and_then_the_minimum() {
    let mut values = band(250.0, 300.0, 350.0, 80.0);
    values.set_max(330.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (250.0, 300.0, 330.0)
    );
    values.set_max(310.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (250.0, 280.0, 310.0)
    );
    values.set_max(290.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (240.0, 260.0, 290.0)
    );
}

#[test]
fn a_lower_target_lowers_the_minimum() {
    let mut values = band(250.0, 300.0, 350.0, 80.0);
    values.set_target(270.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (250.0, 270.0, 350.0)
    );
    values.set_target(260.0);
    assert_eq!(
        (values.min, values.target, values.max),
        (240.0, 260.0, 350.0)
    );
}

#[test]
fn a_lower_minimum_lowers_the_standby_minimum() {
    let mut values = band(250.0, 300.0, 350.0, 80.0);
    values.set_min(100.0);
    assert_eq!(values.standby, 80.0);
    values.set_min(60.0);
    assert_eq!(values.standby, 60.0);
    // A higher minimum does not raise it again.
    values.set_min(200.0);
    assert_eq!(values.standby, 60.0);
}

#[test]
fn an_edit_of_the_band_stores_the_minimum_and_the_two_distances() {
    let state = own_values();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();

    // The target only: the distance to the minimum grows, the distance to the maximum
    // shrinks, the minimum stays the device value.
    ui.edit(&data, "band_target", "320");
    assert_eq!(pending(&ui, "shield_pressure_low"), None);
    assert_eq!(pending(&ui, "shield_pressure_working_range"), Some(70.0));
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(30.0));
    assert_eq!(pending(&ui, "shield_pressure_low_standby"), None);
    let model = step(&state, &ui, Step::Pressure);
    assert_eq!(number_text(row(&model, "band_target")), "320");
    assert_eq!(number_text(row(&model, "band_max")), "350");

    // The minimum pushes the two others, and the wizard corrects a value out of range.
    ui.edit(&data, "band_min", "999");
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(430.0));
    assert_eq!(pending(&ui, "shield_pressure_working_range"), Some(20.0));
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(30.0));
    let model = step(&state, &ui, Step::Pressure);
    let texts: Vec<&str> = ["band_min", "band_target", "band_max"]
        .iter()
        .map(|key| number_text(row(&model, key)))
        .collect();
    assert_eq!(texts, ["430", "450", "480"]);
    assert!(
        model.rows.iter().all(|row| row.error.is_none()),
        "a corrected value is no error"
    );

    // A low minimum takes the standby minimum with it. A typed fraction is a whole mbar.
    ui.edit(&data, "band_min", "50,4");
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(50.0));
    assert_eq!(pending(&ui, "shield_pressure_low_standby"), Some(50.0));
    // The device values again: nothing is pending.
    let mut ui = SettingsUi::default();
    ui.edit(&data, "band_max", "400");
    ui.edit(&data, "band_max", "350");
    assert!(ui.pending.is_empty());
    // A text that is no number changes nothing.
    ui.edit(&data, "band_max", "nan");
    ui.edit(&data, "band_max", "high");
    assert!(ui.pending.is_empty());
}

#[test]
fn a_value_at_a_limit_has_an_information_line() {
    let state = own_values();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let note = |ui: &SettingsUi, key: &str| {
        let model = step(&state, ui, Step::Pressure);
        row(&model, key).note.clone().map(|note| {
            assert!(!note.warning, "an information, not a warning");
            note.text
        })
    };
    // 250 / 300 / 350: no value is at a limit.
    for key in ["band_min", "band_target", "band_max"] {
        assert_eq!(note(&ui, key), None, "{key}");
        let model = step(&state, &ui, Step::Pressure);
        assert!(
            row(&model, key)
                .caption
                .as_deref()
                .unwrap()
                .ends_with(match key {
                    "band_min" => "30 to 430 mbar.",
                    "band_target" => "50 to 450 mbar.",
                    _ => "80 to 500 mbar.",
                })
        );
    }
    ui.edit(&data, "band_max", "330");
    assert_eq!(
        note(&ui, "band_max").as_deref(),
        Some("The maximum is as close to the target as the Leakshield accepts: 30 mbar.")
    );
    ui.edit(&data, "band_min", "280");
    assert_eq!(
        note(&ui, "band_target").as_deref(),
        Some("The target is as close to the minimum as the Leakshield accepts: 20 mbar.")
    );
    ui.edit(&data, "band_min", "30");
    assert_eq!(
        note(&ui, "band_min").as_deref(),
        Some("This is the lowest minimum that the Leakshield accepts.")
    );
    ui.edit(&data, "band_min", "430");
    assert_eq!(
        note(&ui, "band_min").as_deref(),
        Some("This is the highest minimum that the Leakshield accepts.")
    );
}

#[test]
fn the_recommended_values_for_the_d5_preset() {
    // 375 mbar: the pump pressure, 50 mbar more, 40 mbar more, and 80 mbar in standby.
    assert_eq!(
        Band::recommended(375.0, false),
        band(375.0, 425.0, 465.0, 80.0)
    );
    // More than one pump: twice the pressure, cut to the highest minimum.
    assert_eq!(
        Band::recommended(375.0, true),
        band(430.0, 450.0, 490.0, 80.0)
    );
    assert_eq!(
        Band::recommended(150.0, true),
        band(300.0, 350.0, 390.0, 80.0)
    );
    // A weak pump: the standby minimum is the pump pressure, and no value leaves its range.
    assert_eq!(
        Band::recommended(60.0, false),
        band(60.0, 110.0, 150.0, 60.0)
    );
    assert_eq!(Band::recommended(0.0, false), band(30.0, 50.0, 90.0, 30.0));
    assert_eq!(
        Band::recommended(530.0, false),
        band(430.0, 450.0, 490.0, 80.0)
    );
}

#[test]
fn the_recommended_choice_writes_the_recommended_values() {
    // The fixture has own values that are not the recommended ones: 250 / 300 / 350.
    let state = own_values();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pressure);
    let values = row(&model, "pressure_values");
    assert_eq!(
        values
            .note
            .as_ref()
            .map(|note| (note.text.as_str(), note.warning)),
        Some((
            "With your own values the Leakshield can fail to protect the Loop. Recommended for your pump: minimum 375, target 425, maximum 465 mbar.",
            true
        ))
    );
    assert_eq!(
        values.action,
        Some((
            "Use the recommended values".into(),
            "use_recommended".into()
        ))
    );

    // The button keeps the choice "own values" and writes the four values.
    ui.edit(&data, "use_recommended", "");
    assert_eq!(pending(&ui, "shield_flags"), None);
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(375.0));
    // The distance to the target is 50 mbar on the device already.
    assert_eq!(pending(&ui, "shield_pressure_working_range"), None);
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(40.0));
    // 80 mbar is the device value.
    assert_eq!(pending(&ui, "shield_pressure_low_standby"), None);
    let model = step(&state, &ui, Step::Pressure);
    let texts: Vec<&str> = ["band_min", "band_target", "band_max"]
        .iter()
        .map(|key| number_text(row(&model, key)))
        .collect();
    assert_eq!(texts, ["375", "425", "465"]);

    // The choice "recommended values" sets bit 4 and writes the same values.
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pressure_values", "recommended");
    assert_eq!(pending(&ui, "shield_flags"), Some(127.0));
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(375.0));
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(40.0));
    let model = step(&state, &ui, Step::Pressure);
    let values = row(&model, "pressure_values");
    assert!(values.note.is_none() && values.action.is_none());
    for key in ["band_min", "band_target", "band_max"] {
        assert!(row(&model, key).fixed, "{key}");
    }
}

#[test]
fn a_leakshield_with_other_values_than_the_recommended_ones_says_so() {
    // Bit 4 says "recommended", and the device holds 250 / 300 / 350.
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pressure);
    let values = row(&model, "pressure_values");
    assert_eq!(
        values
            .note
            .as_ref()
            .map(|note| (note.text.as_str(), note.warning)),
        Some((
            "The Leakshield holds other values. Recommended for your pump: minimum 375, target 425, maximum 465 mbar.",
            false
        ))
    );
    // The inputs show what the device holds, read-only.
    assert_eq!(number_text(row(&model, "band_min")), "250");
    assert!(row(&model, "band_min").fixed);
    assert_eq!(
        values.action.as_ref().map(|action| action.0.as_str()),
        Some("Use the recommended values")
    );
    ui.edit(&data, "use_recommended", "");
    let model = step(&state, &ui, Step::Pressure);
    assert_eq!(number_text(row(&model, "band_min")), "375");
    let values = row(&model, "pressure_values");
    assert!(values.note.is_none() && values.action.is_none());
}

#[test]
fn a_pump_change_writes_the_recommended_values() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pump_max_pressure", "400");
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(400.0));
    assert_eq!(
        pending(&ui, "shield_pressure_working_range"),
        None,
        "50 mbar"
    );
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(40.0));
    // The device value of the pump again: the recommended values for it stay pending,
    // because the device holds other ones.
    ui.edit(&data, "pump_max_pressure", "375");
    assert_eq!(pending(&ui, "pump_max_pressure"), None);
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(375.0));

    // With own values a pump change leaves the band alone.
    let state = own_values();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pump_max_pressure", "400");
    assert_eq!(
        ui.pending.keys().map(String::as_str).collect::<Vec<_>>(),
        ["pump_max_pressure"]
    );
}

fn text(ui: &SettingsUi, field: &str) -> Option<String> {
    match ui.pending.get(field) {
        Some(FieldValue::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn a_pump_preset_writes_its_speeds_and_its_pressure() {
    // The device: D5 with 4800 rpm, start speed 2500 rpm, 375 mbar.
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pump);
    let Control::Select { shown, options } = &row(&model, "pump_model").control else {
        panic!("a select");
    };
    assert_eq!(shown, "D5");
    assert_eq!(
        options.iter().map(|o| o.1.as_str()).collect::<Vec<_>>(),
        [
            "D5",
            "aquastream",
            "DDC-1T",
            "DDC-1T Plus",
            "DDC 3.25",
            "Eheim 1046",
            "Eheim 1048",
            "Eheim 1250",
            "Custom"
        ]
    );
    // With a preset the numbers show and take no edit.
    for key in ["pump_max_pressure", "pump_max_speed"] {
        let row = row(&model, key);
        assert!(row.fixed && row.writable, "{key}");
        assert_eq!(row.badge, "Preset value");
    }
    assert_eq!(number_text(row(&model, "pump_max_pressure")), "375");

    ui.edit(&data, "pump_model", "ddc-1t");
    assert_eq!(text(&ui, "pump_model").as_deref(), Some("ddc-1t"));
    assert_eq!(pending(&ui, "pump_max_speed"), Some(3900.0));
    assert_eq!(pending(&ui, "pump_start_speed"), Some(3900.0));
    assert_eq!(pending(&ui, "pump_max_pressure"), Some(350.0));
    // The recommended values follow: 350 / 400 / 440.
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(350.0));
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(40.0));

    // The table of this preset has a start speed above its maximum speed: the start speed is cut
    // to the maximum speed.
    ui.edit(&data, "pump_model", "ddc-325");
    assert_eq!(pending(&ui, "pump_max_speed"), Some(4850.0));
    assert_eq!(pending(&ui, "pump_start_speed"), Some(4850.0));
    assert_eq!(pending(&ui, "pump_max_pressure"), Some(530.0));
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(430.0));

    // "Custom" keeps the numbers and opens them.
    ui.edit(&data, "pump_model", "custom");
    assert_eq!(pending(&ui, "pump_max_pressure"), Some(530.0));
    let model = step(&state, &ui, Step::Pump);
    for key in ["pump_max_pressure", "pump_max_speed"] {
        let row = row(&model, key);
        assert!(!row.fixed, "{key}");
        assert_eq!(row.badge, "Verified field");
    }
    ui.edit(&data, "pump_max_pressure", "410");
    assert_eq!(pending(&ui, "pump_max_pressure"), Some(410.0));
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(410.0));
}

#[test]
fn a_reservoir_preset_writes_its_three_volumes() {
    // The device: "Custom" with 270 / 65 / 260 ml.
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Reservoir);
    let Control::Select { shown, options } = &row(&model, "reservoir_model").control else {
        panic!("a select");
    };
    assert_eq!(shown, "Custom");
    assert_eq!(options.len(), 12);
    assert_eq!(options[0].1, "ULTITUBE 100");
    for key in [
        "reservoir_total_volume",
        "reservoir_air_volume_full",
        "reservoir_air_volume_empty",
    ] {
        assert!(!row(&model, key).fixed, "{key} with Custom");
    }

    ui.edit(&data, "reservoir_model", "aqualis-450");
    assert_eq!(pending(&ui, "reservoir_total_volume"), Some(450.0));
    assert_eq!(pending(&ui, "reservoir_air_volume_full"), Some(70.0));
    assert_eq!(pending(&ui, "reservoir_air_volume_empty"), Some(350.0));
    let model = step(&state, &ui, Step::Reservoir);
    for (key, value) in [
        ("reservoir_total_volume", "450"),
        ("reservoir_air_volume_full", "70"),
        ("reservoir_air_volume_empty", "350"),
    ] {
        let row = row(&model, key);
        assert!(row.fixed, "{key} with a preset");
        assert_eq!(row.badge, "Preset value");
        assert_eq!(number_text(row), value);
    }
    // A pump preset or a reservoir preset does not touch the fields of the other one.
    assert!(!ui.pending.keys().any(|field| field.starts_with("pump_")));
}

#[test]
fn more_than_one_pump_is_bit_1_of_the_pump_flags() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pump);
    let two = row(&model, "two_pumps");
    assert_eq!(two.label, "More than one pump");
    assert_eq!(two.control, Control::Switch(false));
    assert!(two.note.is_none());
    assert!(model.notices.is_empty(), "375 mbar is under the limit");

    ui.edit(&data, "two_pumps", "1");
    assert_eq!(pending(&ui, "pump_flags"), Some(2.0));
    // The recommended values count twice the pump pressure: 750 mbar, cut to 430.
    assert_eq!(pending(&ui, "shield_pressure_low"), Some(430.0));
    assert_eq!(pending(&ui, "shield_pressure_working_range"), Some(20.0));
    assert_eq!(pending(&ui, "shield_pressure_max_range"), Some(40.0));
    let model = step(&state, &ui, Step::Pump);
    let two = row(&model, "two_pumps");
    assert_eq!(two.control, Control::Switch(true));
    assert!(two.changed);
    let note = two.note.as_ref().expect("the warning of two pumps");
    assert!(note.warning && note.text.starts_with("With more than one pump"));

    // A device that has bit 0 of the flags keeps it.
    let mut state = device();
    set_field(&mut state, "pump_flags", FieldValue::Number(1.0));
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "two_pumps", "1");
    assert_eq!(pending(&ui, "pump_flags"), Some(3.0));
    ui.edit(&data, "two_pumps", "0");
    assert_eq!(pending(&ui, "pump_flags"), None);
}

#[test]
fn a_pump_above_430_mbar_has_a_warning_on_the_pressure_and_mode_steps() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    for step_name in [Step::Pump, Step::Pressure, Step::Mode] {
        assert!(step(&state, &ui, step_name).notices.is_empty());
    }
    // 430 mbar is the limit itself: no warning.
    ui.edit(&data, "pump_model", "custom");
    ui.edit(&data, "pump_max_pressure", "430");
    assert!(step(&state, &ui, Step::Pressure).notices.is_empty());
    ui.edit(&data, "pump_max_pressure", "431");
    let warning = "The pump can make 431 mbar. The Leakshield holds 430 mbar at most, so it cannot fully protect the Loop at full pump speed.";
    for step_name in [Step::Pump, Step::Pressure, Step::Mode] {
        assert_eq!(
            step(&state, &ui, step_name).notices,
            [warning],
            "{step_name:?}"
        );
    }
    for step_name in [Step::Reservoir, Step::Flow, Step::Standby] {
        assert!(
            step(&state, &ui, step_name).notices.is_empty(),
            "{step_name:?}"
        );
    }
    // The Summary has it with the pending pump change, and Apply stays on.
    let summary = step(&state, &ui, Step::Summary).summary.unwrap();
    assert_eq!(summary.warnings, [warning]);
    assert!(summary.apply_enabled, "a warning never blocks Apply");

    // Two pumps of 375 mbar count as 750 mbar.
    let mut ui = SettingsUi::default();
    ui.edit(&data, "two_pumps", "1");
    assert_eq!(
        step(&state, &ui, Step::Mode).notices,
        [
            "The pump can make 750 mbar. The Leakshield holds 430 mbar at most, so it cannot fully protect the Loop at full pump speed."
        ]
    );
}

#[test]
fn the_pump_step_has_a_speed_and_pressure_helper() {
    // The pressure follows the square of the speed.
    assert_eq!(pump_pressure_at(4800.0, 4800.0, 375.0), 375.0);
    assert_eq!(pump_pressure_at(2400.0, 4800.0, 375.0), 94.0);
    assert_eq!(pump_pressure_at(2840.0, 4800.0, 375.0), 131.0);
    assert_eq!(pump_pressure_at(2840.0, 0.0, 375.0), 0.0);
    assert_eq!(pump_speed_for(375.0, 4800.0, 375.0), 4800.0);
    assert_eq!(pump_speed_for(450.0, 4850.0, 530.0), 4469.0);
    assert_eq!(pump_speed_for(450.0, 4800.0, 0.0), 0.0);

    // The Pump channel of the Loop runs at 2 840 rpm.
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Pump);
    let keys: Vec<&str> = model.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "pump_model",
            "pump_max_pressure",
            "pump_max_speed",
            "two_pumps",
            "pump_pressure_now",
            "pump_speed_limit",
            "pump_source"
        ]
    );
    let now = row(&model, "pump_pressure_now");
    assert_eq!(
        now.control,
        Control::Info("About 131 mbar at 2\u{a0}840 rpm.".into())
    );
    assert!(now.link.is_none() && !now.writable);
    assert_eq!(
        row(&model, "pump_speed_limit").control,
        Control::Info(
            "At every speed the pump stays under 450 mbar, the highest target pressure of the Leakshield."
                .into()
        )
    );
    // Two pumps: twice the pressure, and the limit is inside the speed range.
    ui.edit(&data, "two_pumps", "1");
    let model = step(&state, &ui, Step::Pump);
    assert_eq!(
        row(&model, "pump_pressure_now").control,
        Control::Info("About 263 mbar at 2\u{a0}840 rpm.".into())
    );
    assert_eq!(
        row(&model, "pump_speed_limit").control,
        Control::Info(
            "Above 3\u{a0}718 rpm the pump makes more than 450 mbar, the highest target pressure of the Leakshield."
                .into()
        )
    );
    // Without a Pump channel the helper has no speed.
    let mut without = device();
    without.snapshot.loops[0].pump_channels.clear();
    let model = step(&without, &SettingsUi::default(), Step::Pump);
    assert_eq!(
        row(&model, "pump_pressure_now").control,
        Control::Info(
            "No pump speed from the Loop. Without a speed the Leakshield counts with the maximum speed."
                .into()
        )
    );
}

#[test]
fn the_loop_step_has_the_height_difference() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    assert_eq!(Step::ALL.len(), 8);
    assert_eq!(Step::ALL[4], Step::Loop, "after the Pressure step");
    let model = step(&state, &ui, Step::Loop);
    assert_eq!(
        (model.title.as_str(), model.counter.as_str()),
        ("Loop", "Step 5 of 8")
    );
    assert_eq!(
        (
            model.back.as_ref().map(|b| b.1.as_str()),
            model.next.as_ref().map(|n| n.1.as_str())
        ),
        (Some("Pressure"), Some("Standby"))
    );
    assert_eq!(model.rows.len(), 1);
    let height = row(&model, "loop_height");
    assert_eq!(height.label, "Height difference");
    assert_eq!(
        height.control,
        Control::Number {
            text: "30".into(),
            unit: "cm".into()
        }
    );
    let caption = height.caption.as_deref().unwrap();
    assert!(caption.contains("The default is 30 cm.") && caption.ends_with("0 to 200 cm."));
    assert!(height.note.is_none() && height.writable);

    // 100 cm has no warning. Above it the warning shows.
    ui.edit(&data, "loop_height", "100");
    assert!(
        row(&step(&state, &ui, Step::Loop), "loop_height")
            .note
            .is_none()
    );
    ui.edit(&data, "loop_height", "101");
    assert_eq!(pending(&ui, "loop_height"), Some(101.0));
    let model = step(&state, &ui, Step::Loop);
    assert_eq!(
        row(&model, "loop_height")
            .note
            .as_ref()
            .map(|n| (n.text.as_str(), n.warning)),
        Some((
            "Above 100 cm the recommended pressure values can be too low. Set your own values on the Pressure step.",
            true
        ))
    );
    assert_eq!(
        model
            .pending
            .iter()
            .map(|c| (c.name.as_str(), c.old.as_str(), c.new.as_str()))
            .collect::<Vec<_>>(),
        [("Loop height", "30 cm", "101 cm")]
    );
    // The wizard corrects a height outside 0 to 200 cm. The warning never blocks Apply.
    ui.edit(&data, "loop_height", "250");
    assert_eq!(pending(&ui, "loop_height"), Some(200.0));
    ui.edit(&data, "loop_height", "-5");
    assert_eq!(pending(&ui, "loop_height"), Some(0.0));
    ui.edit(&data, "loop_height", "150");
    let summary = step(&state, &ui, Step::Summary).summary.unwrap();
    assert!(summary.apply_enabled && summary.errors.is_empty());
}

#[test]
fn the_standby_step_corrects_its_numbers_to_their_ranges() {
    // The device: start speed 2500 rpm, start flow 130 L/h, standby minimum 80 mbar.
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Standby);
    assert!(
        row(&model, "pump_start_speed")
            .caption
            .as_deref()
            .unwrap()
            .ends_with("1\u{a0}000 to 10\u{a0}000 rpm.")
    );
    assert_eq!(
        row(&model, "pump_start_flow").caption.as_deref(),
        Some("30 to 500 L/h.")
    );

    // The assumed pump speed: 1000 to 10000 rpm.
    ui.edit(&data, "pump_start_speed", "500");
    assert_eq!(pending(&ui, "pump_start_speed"), Some(1000.0));
    // Above the range and above the maximum speed of the pump (4800 rpm): the maximum speed.
    ui.edit(&data, "pump_start_speed", "32000");
    assert_eq!(pending(&ui, "pump_start_speed"), Some(4800.0));
    ui.edit(&data, "pump_start_speed", "3200,4");
    assert_eq!(pending(&ui, "pump_start_speed"), Some(3200.0));
    // The assumed flow: 30 to 500 L/h.
    ui.edit(&data, "pump_start_flow", "0");
    assert_eq!(pending(&ui, "pump_start_flow"), Some(30.0));
    ui.edit(&data, "pump_start_flow", "3000");
    assert_eq!(pending(&ui, "pump_start_flow"), Some(500.0));
    let model = step(&state, &ui, Step::Standby);
    assert!(model.rows.iter().all(|row| row.error.is_none()));
    assert!(
        step(&state, &ui, Step::Summary)
            .summary
            .unwrap()
            .errors
            .is_empty()
    );

    // "Use the current value" takes the same correction: the Pump channel runs at 2 840 rpm.
    let mut slow = device();
    for sample in [&mut slow.sensors, &mut slow.last_sensors]
        .into_iter()
        .flatten()
    {
        sample
            .sensors
            .iter_mut()
            .find(|s| s.0 == "nct6798_fan6")
            .unwrap()
            .1 = 600.0;
    }
    let mut ui = SettingsUi::default();
    ui.use_current(
        &input(&slow, &Feedback::default(), EN),
        LEAKSHIELD,
        "pump_start_speed",
    );
    assert_eq!(pending(&ui, "pump_start_speed"), Some(1000.0));
}

#[test]
fn the_standby_minimum_stays_at_or_below_the_minimum_and_needs_its_switch() {
    // The device: minimum 250 mbar, standby minimum 80 mbar, bit 5 of `shield_flags` set.
    let state = own_values();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    let model = step(&state, &ui, Step::Standby);
    assert_eq!(
        row(&model, "standby_pressure").control,
        Control::Switch(true)
    );
    let value = row(&model, "shield_pressure_low_standby");
    assert!(!value.off && !value.dim);
    assert_eq!(number_text(value), "80");
    assert_eq!(
        value.caption.as_deref(),
        Some("The minimum pressure in standby: 30 mbar to the minimum pressure, 250 mbar.")
    );

    // 30 mbar to the minimum.
    ui.edit(&data, "shield_pressure_low_standby", "10");
    assert_eq!(pending(&ui, "shield_pressure_low_standby"), Some(30.0));
    ui.edit(&data, "shield_pressure_low_standby", "400");
    assert_eq!(pending(&ui, "shield_pressure_low_standby"), Some(250.0));
    // A lower minimum takes the standby minimum with it.
    ui.edit(&data, "band_min", "200");
    assert_eq!(pending(&ui, "shield_pressure_low_standby"), Some(200.0));
    let model = step(&state, &ui, Step::Standby);
    let value = row(&model, "shield_pressure_low_standby");
    assert_eq!(number_text(value), "200");
    assert!(value.caption.as_deref().unwrap().ends_with("200 mbar."));
    assert!(value.error.is_none());

    // Without "Reduced vacuum in standby" the value takes no edit.
    ui.edit(&data, "standby_pressure", "0");
    assert_eq!(pending(&ui, "shield_flags"), Some(79.0), "111 without 32");
    let model = step(&state, &ui, Step::Standby);
    assert_eq!(
        row(&model, "standby_pressure").control,
        Control::Switch(false)
    );
    let value = row(&model, "shield_pressure_low_standby");
    assert!(value.off);
    assert_eq!(
        value.caption.as_deref(),
        Some("Turn on \"Reduced vacuum in standby\" to set it.")
    );
}

mod layout {
    use gpui::{AppContext as _, Bounds, Modifiers, Pixels, TestAppContext, VisualTestContext, px};
    use nicecooling::app::{self, Backend, StartMode};
    use nicecooling::i18n::Language;
    use nicecooling::model::DaemonEvent;
    use nicecooling::screen_view::ScreenView;
    use nicecooling::screens::leakshield::Tab;
    use nicecooling::screens::leakshield::settings::{Control, SettingsModel, SettingsUi, Step};
    use nicecooling::screens::{Feedback, ScreenInput};
    use tokio::sync::{broadcast, mpsc};

    use super::{LEAKSHIELD, NOW_US, device, pending};

    /// The sidebar of the shell: the page is this much narrower than the window.
    const SIDEBAR: f32 = 220.0;

    /// The Settings tab of the Leakshield of the fixture, in a window of `width` with the
    /// sidebar. The second part keeps the backend of the app alive.
    fn open(
        cx: &mut TestAppContext,
        language: Language,
        width: f32,
    ) -> (gpui::WindowHandle<ScreenView>, impl Sized + use<>) {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (events, events_rx) = broadcast::channel(64);
        let (commands, inbox) = mpsc::unbounded_channel();
        let backend = Backend {
            events: events_rx,
            commands: commands.clone(),
            inbox,
            client: None,
            graphs: None,
            runtime: runtime.handle().clone(),
            language,
            settings: Default::default(),
        };
        let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
        let state = device();
        events
            .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
            .unwrap();
        events
            .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
            .unwrap();
        cx.run_until_parked();
        let (view_commands, inbox) = mpsc::unbounded_channel();
        let window = cx.update(|cx| {
            cx.open_window(gpui::WindowOptions::default(), |_, cx| {
                cx.new(|cx| ScreenView::new(model.clone(), view_commands.clone(), cx))
            })
            .expect("a window")
        });
        cx.simulate_window_resize(window.into(), gpui::size(px(width - SIDEBAR), px(700.)));
        window
            .update(cx, |view, _, cx| {
                view.show_leakshield_tab(LEAKSHIELD, Tab::Settings, cx);
            })
            .unwrap();
        (window, (runtime, events, commands, inbox))
    }

    /// Every row of every step ends inside its card, and the parts of the band diagram keep
    /// their place. `edit` makes the state of the wizard that the step shows.
    fn check(cx: &mut TestAppContext, language: Language, width: f32, edit: fn(&mut SettingsUi)) {
        let (window, _backend) = open(cx, language, width);
        let state = device();
        window
            .update(cx, |view, _, _| edit(&mut view.leakshield_ui().settings))
            .unwrap();
        let locale = language.resolve();
        let tag = format!("{language:?} {width}");
        for step in Step::ALL {
            window
                .update(cx, |view, _, cx| {
                    view.leakshield_ui().settings.go(step);
                    cx.notify();
                })
                .unwrap();
            cx.run_until_parked();
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let ui = window
                .update(cx, |view, _, _| view.leakshield_ui().settings.clone())
                .unwrap();
            let feedback = Feedback::default();
            let rows = SettingsModel::build(
                &ScreenInput {
                    state: &state,
                    now_us: NOW_US,
                    locale,
                    down_since_us: None,
                    feedback: &feedback,
                },
                &ui,
                LEAKSHIELD,
            )
            .rows;
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            let page = visual.debug_bounds("screen-page").expect("the page");
            let inner = page.right() - px(24. + 18.);
            let mut found = |name: String| -> Option<Bounds<Pixels>> {
                visual.debug_bounds(Box::leak(name.into_boxed_str()))
            };
            for row in &rows {
                let key = &row.key;
                let line = found(format!("ls-row-{key}"))
                    .unwrap_or_else(|| panic!("{tag} {step:?}: no row {key}"));
                assert!(
                    line.left() >= page.left() + px(24.) && line.right() <= page.right() - px(24.),
                    "{tag} {step:?}: row {key} is {line:?} in {page:?}"
                );
                for part in ["control", "use", "link", "action"] {
                    if let Some(bounds) = found(format!("ls-{part}-{key}")) {
                        assert!(
                            bounds.right() <= inner + px(0.5),
                            "{tag} {step:?}: the {part} of {key} ends at {:?}, the row at {inner:?}",
                            bounds.right()
                        );
                        // The name and the caption keep a column to wrap in.
                        assert!(
                            bounds.left() >= line.left() + px(18. + 240.),
                            "{tag} {step:?}: the {part} of {key} starts at {:?}",
                            bounds.left()
                        );
                    }
                }
                if matches!(row.control, Control::Band(_)) {
                    let plot = found("ls-band-plot".to_owned()).expect("the plot");
                    let values = found("ls-band-values".to_owned()).expect("the values");
                    let marker = found("ls-band-marker".to_owned()).expect("the marker");
                    assert!(plot.size.width >= px(480.), "{tag}: the plot is {plot:?}");
                    assert!(plot.left() >= line.left() + px(18.), "{tag}: {plot:?}");
                    assert!(values.left() >= plot.right(), "{tag}: {values:?}");
                    assert!(values.right() <= inner + px(0.5), "{tag}: {values:?}");
                    assert!(
                        marker.left() >= plot.left() && marker.right() <= plot.right(),
                        "{tag}: the marker text is {marker:?} in {plot:?}"
                    );
                    assert!(
                        marker.top() >= plot.top() && marker.bottom() <= plot.bottom(),
                        "{tag}: the marker text is {marker:?} in {plot:?}"
                    );
                    // A zone that is too low for its text has none.
                    let zones: Vec<_> = ["above", "inside", "below"]
                        .into_iter()
                        .filter_map(|zone| found(format!("ls-band-zone-{zone}")))
                        .collect();
                    assert!(zones.len() >= 2, "{tag}: {} zone texts", zones.len());
                    for zone in zones {
                        assert!(
                            zone.left() >= plot.left() && zone.right() <= marker.left(),
                            "{tag}: the zone text is {zone:?}, the marker text {marker:?}"
                        );
                        assert!(
                            zone.top() >= plot.top() && zone.bottom() <= plot.bottom(),
                            "{tag}: the zone text is {zone:?} in {plot:?}"
                        );
                    }
                    // The values of the lines: one under the other, in the height of the plot.
                    let labels: Vec<_> = (0..6)
                        .filter_map(|index| found(format!("ls-band-label-{index}")))
                        .collect();
                    assert!(labels.len() >= 3, "{tag}: {} line values", labels.len());
                    for pair in labels.windows(2) {
                        assert!(
                            pair[1].top() >= pair[0].bottom() - px(0.5),
                            "{tag}: two line values overlap: {pair:?}"
                        );
                    }
                    assert!(
                        labels[0].top() >= plot.top() - px(0.5)
                            && labels[labels.len() - 1].bottom() <= plot.bottom() + px(0.5),
                        "{tag}: the line values leave the plot: {labels:?}"
                    );
                }
            }
        }
        cx.update(|cx| cx.quit());
    }

    /// A number that the user types goes into the draft on each way out of its input, and
    /// Escape puts the old number back. gpui sends `Blur` in an active window only.
    fn leave_an_input(cx: &mut TestAppContext, active: bool) {
        const FIELD: &str = "reservoir_total_volume";
        let (window, _backend) = open(cx, Language::English, 1280.);
        let show = |cx: &mut VisualTestContext, step: Step| {
            window
                .update(cx, |view, _, cx| {
                    view.leakshield_ui().settings.go(step);
                    cx.notify();
                })
                .unwrap();
            cx.run_until_parked();
        };
        let draft = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            window
                .update(cx, |view, _, _| {
                    pending(&view.leakshield_ui().settings, FIELD)
                })
                .unwrap()
        };
        let type_number = |cx: &mut VisualTestContext, number: &str| {
            let input = cx.debug_bounds("ls-control-reservoir_total_volume");
            cx.simulate_click(input.expect("the input").center(), Modifiers::none());
            cx.simulate_keystrokes("ctrl-a");
            cx.simulate_input(number);
        };
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        if active {
            visual.update(|window, _| window.activate_window());
        }
        show(&mut visual, Step::Reservoir);
        assert_eq!(visual.update(|window, _| window.is_window_active()), active);

        type_number(&mut visual, "271");
        assert_eq!(draft(&mut visual), None, "typing commits nothing");
        // Tab: the shell calls `focus_next` for it.
        visual.update(|window, cx| window.focus_next(cx));
        assert_eq!(draft(&mut visual), Some(271.0), "the focus moved on");

        // After Escape the input has the old number: Enter commits that one.
        type_number(&mut visual, "280");
        visual.simulate_keystrokes("escape enter");
        assert_eq!(draft(&mut visual), Some(271.0), "Escape drops the text");
        visual.update(|window, cx| window.focus_next(cx));
        assert_eq!(draft(&mut visual), Some(271.0), "nothing is left to commit");

        // A press on a button moves no focus. The next step takes the input off the screen.
        type_number(&mut visual, "290");
        show(&mut visual, Step::Pump);
        assert_eq!(draft(&mut visual), Some(290.0), "the input left the screen");
        cx.update(|cx| cx.quit());
    }

    #[gpui::test]
    fn a_typed_number_commits_when_its_input_loses_the_focus(cx: &mut TestAppContext) {
        leave_an_input(cx, true);
    }

    #[gpui::test]
    fn a_typed_number_commits_in_a_window_that_is_not_active(cx: &mut TestAppContext) {
        leave_an_input(cx, false);
    }

    fn untouched(_: &mut SettingsUi) {}

    /// The state with the most text: own values, with their warning and their button.
    fn edited(ui: &mut SettingsUi) {
        use nicecooling::model::FieldValue::Number;
        ui.pending.insert("shield_flags".into(), Number(111.0));
        // A draft band next to the stored one: three dashed lines with their values.
        ui.pending
            .insert("shield_pressure_low".into(), Number(260.0));
        ui.pending
            .insert("shield_pressure_max_range".into(), Number(60.0));
        // Two pumps with their warning, and the warning of a pump above the limit.
        ui.pending.insert("pump_flags".into(), Number(2.0));
        ui.pending.insert("loop_height".into(), Number(150.0));
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1100_in_english(cx: &mut TestAppContext) {
        check(cx, Language::English, 1100., untouched);
        check(cx, Language::English, 1100., edited);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1100_in_italian(cx: &mut TestAppContext) {
        check(cx, Language::Italian, 1100., untouched);
        check(cx, Language::Italian, 1100., edited);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1280_in_english(cx: &mut TestAppContext) {
        check(cx, Language::English, 1280., untouched);
        check(cx, Language::English, 1280., edited);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1280_in_italian(cx: &mut TestAppContext) {
        check(cx, Language::Italian, 1280., untouched);
        check(cx, Language::Italian, 1280., edited);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1100_in_german(cx: &mut TestAppContext) {
        check(cx, Language::German, 1100., untouched);
        check(cx, Language::German, 1100., edited);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1280_in_german(cx: &mut TestAppContext) {
        check(cx, Language::German, 1280., untouched);
        check(cx, Language::German, 1280., edited);
    }
}

// aquasuite sets the maximum speed first, and its setter of the start speed cuts the start
// speed to the maximum speed. The table row of the DDC 3.25 has the start speed 5500 and the
// maximum speed 4850, so aquasuite writes 4850 as the start speed.
#[test]
fn the_ddc_325_preset_keeps_the_start_speed_at_the_maximum_speed() {
    let state = device();
    let data = data_of(&state);
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pump_model", "ddc-325");
    let value = |field: &str| pending(&ui, field).or_else(|| data.number(field)).unwrap();
    assert_eq!(value("pump_max_speed"), 4850.0);
    assert!(
        value("pump_start_speed") <= value("pump_max_speed"),
        "start speed {} above the maximum speed {}",
        value("pump_start_speed"),
        value("pump_max_speed")
    );
}

// the wizard keeps a whole `shield_flags` mask in its pending changes. A bit that another
// tab writes after the edit (the buzzer switches of the Alarms tab) goes back with "Apply".
#[test]
fn apply_keeps_a_shield_flags_bit_that_the_alarms_tab_wrote() {
    let mut state = device();
    let mut ui = SettingsUi::default();
    // Adaptive pressure off: bit 3 of 127.
    ui.edit(&data_of(&state), "adaptive", "0");
    // The Alarms tab turns the buzzer off (bit 7), and the next refresh shows 255.
    set_field(&mut state, "shield_flags", FieldValue::Number(255.0));
    let feedback = Feedback::default();
    let command = ui
        .apply(&input(&state, &feedback, EN), LEAKSHIELD)
        .expect("a write");
    let nicecooling::command::Command::SetLeakshieldFields { fields, .. } = command else {
        panic!("a field write");
    };
    let flags = fields
        .iter()
        .find(|(field, _)| field == "shield_flags")
        .map(|(_, value)| value.clone());
    assert_eq!(
        flags,
        Some(FieldValue::Number(247.0)),
        "bit 7 must stay set"
    );
}

// every sequence of edits keeps the band in the device ranges and the distances.
#[test]
fn the_limiter_holds_its_rules_for_every_edit() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 700) as f64 - 100.0
    };
    let mut band = Band {
        min: 375.0,
        target: 425.0,
        max: 465.0,
        standby: 80.0,
    };
    for _ in 0..20_000 {
        let value = next();
        match (value as i64).rem_euclid(4) {
            0 => band.set_min(value),
            1 => band.set_target(value),
            2 => band.set_max(value),
            _ => band.set_standby(value),
        }
        assert!((30.0..=430.0).contains(&band.min), "{band:?}");
        assert!((50.0..=450.0).contains(&band.target), "{band:?}");
        assert!((80.0..=500.0).contains(&band.max), "{band:?}");
        assert!(band.target >= band.min + 20.0, "{band:?}");
        assert!(band.max >= band.target + 30.0, "{band:?}");
        assert!((30.0..=band.min).contains(&band.standby), "{band:?}");
        assert!(band.target - band.min <= 420.0 && band.max - band.target <= 450.0);
    }
    for pressure in [0.0, 10.0, 120.0, 375.0, 430.0, 450.0, 530.0, 1000.0] {
        for two in [false, true] {
            let b = Band::recommended(pressure, two);
            assert!((30.0..=430.0).contains(&b.min) && b.max <= 500.0, "{b:?}");
            assert!(
                b.target >= b.min + 20.0 && b.max >= b.target + 30.0,
                "{b:?}"
            );
            assert!(b.standby <= 80.0 && b.standby <= b.min, "{b:?}");
        }
    }
}
