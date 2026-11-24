//! The pressure part of the Leakshield wizard: the control band of SHIELD and the settings that
//! give its values (`docs/ui/screens/leakshield-settings.md`).

mod support;

use nicecooling::i18n::{EN, IT, Locale};
use nicecooling::model::{AppState, FieldEntry, FieldRisk, FieldValue, LeakshieldData};
use nicecooling::screens::leakshield::settings::{
    Control, Row, SettingsModel, SettingsUi, Step, marker_place,
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
    assert!(values.note.is_none());

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
                "Above this value the Leakshield opens its vent valve and releases vacuum."
            ),
            (
                "Target pressure",
                "300",
                "The pressure that you want in SHIELD. The pump of the Leakshield builds the vacuum up to this value and stops."
            ),
            (
                "Minimum pressure",
                "250",
                "Below this value the Leakshield starts its pump."
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

#[test]
fn the_band_diagram_shows_the_set_values_and_the_live_values() {
    let mut state = device();
    let ui = SettingsUi::default();
    let diagram = |state: &AppState| {
        let model = step(state, &ui, Step::Pressure);
        let row = row(&model, "band").clone();
        let Control::Band(diagram) = row.control else {
            panic!("the diagram");
        };
        (diagram, row.label, row.caption)
    };

    // The live limits equal the stored values: no second value on a line.
    let (band, title, now) = diagram(&state);
    assert_eq!(title, "What the Leakshield does in SHIELD");
    assert_eq!(
        band.zones,
        [
            "Vent valve open: releases vacuum",
            "Pump off, vent valve closed",
            "Pump on: builds vacuum up to the target"
        ]
    );
    assert_eq!(
        band.lines
            .iter()
            .map(|l| (l.label.as_str(), l.now.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("Maximum 350 mbar", None),
            ("Target 300 mbar", None),
            ("Minimum 250 mbar", None)
        ]
    );
    assert_eq!(
        now.as_deref(),
        Some("Now: minimum 250, target 300, maximum 350 mbar.")
    );
    // 298 mbar is just under the target of 300: between the target line and the minimum line.
    let (place, text) = band.marker.expect("the live pressure");
    assert_eq!(text, "Now 298 mbar");
    assert!((0.5..0.52).contains(&place), "{place}");

    // Adaptive pressure: the device lowered the minimum and the target, not the maximum.
    set_sensor(&mut state, "pressure_low", 215.4);
    set_sensor(&mut state, "pressure_setpoint", 265.4);
    let (band, _, now) = diagram(&state);
    assert_eq!(
        band.lines
            .iter()
            .map(|l| (l.label.as_str(), l.now.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("Maximum 350 mbar", None),
            ("Target 300 mbar", Some("now 265.4 mbar")),
            ("Minimum 250 mbar", Some("now 215.4 mbar"))
        ]
    );
    assert_eq!(
        now.as_deref(),
        Some("Now: minimum 215.4, target 265.4, maximum 350 mbar.")
    );
    // The marker follows the live limits: 298 is now above the target.
    let (place, _) = band.marker.unwrap();
    assert!((0.25..0.5).contains(&place), "{place}");

    // A pending value moves the line of the wizard, not the live value.
    set_field(&mut state, "shield_flags", FieldValue::Number(111.0));
    let data = data_of(&state);
    let mut edited = SettingsUi::default();
    edited.edit(&data, "band_max", "400");
    let model = step(&state, &edited, Step::Pressure);
    let Control::Band(band) = &row(&model, "band").control else {
        panic!("the diagram");
    };
    assert_eq!(band.lines[0].label, "Maximum 400 mbar");
    assert_eq!(band.lines[0].now.as_deref(), Some("now 350 mbar"));

    // Without an answer of the Leakshield the diagram has the stored values only.
    state.snapshot.leakshields[0].connected = false;
    let (band, _, now) = diagram(&state);
    assert!(band.marker.is_none() && now.is_none());
    assert_eq!(band.lines[2].label, "Minimum 250 mbar");
    assert!(band.lines.iter().all(|line| line.now.is_none()));

    // Italian.
    let model = {
        let mut ui = SettingsUi::default();
        ui.go(Step::Pressure);
        SettingsModel::build(&input(&device(), &Feedback::default(), IT), &ui, LEAKSHIELD)
    };
    assert_eq!(row(&model, "band_target").label, "Pressione obiettivo");
    let Control::Band(band) = &row(&model, "band").control else {
        panic!("the diagram");
    };
    assert_eq!(band.lines[1].label, "Obiettivo 300 mbar");
}

#[test]
fn the_marker_of_the_diagram_keeps_its_zone() {
    // The lines are at a quarter, a half and three quarters of the height.
    assert_eq!(marker_place(465.0, 375.0, 425.0, 465.0), 0.25);
    assert_eq!(marker_place(425.0, 375.0, 425.0, 465.0), 0.5);
    assert_eq!(marker_place(375.0, 375.0, 425.0, 465.0), 0.75);
    assert_eq!(marker_place(400.0, 375.0, 425.0, 465.0), 0.625);
    assert_eq!(marker_place(445.0, 375.0, 425.0, 465.0), 0.375);
    // Outside the band the marker is in the hatched zone and stays in the diagram.
    let below = marker_place(350.0, 375.0, 425.0, 465.0);
    assert!(below > 0.75 && below <= 0.94, "{below}");
    assert_eq!(marker_place(0.0, 375.0, 425.0, 465.0), 0.94);
    let above = marker_place(480.0, 375.0, 425.0, 465.0);
    assert!((0.06..0.25).contains(&above), "{above}");
    assert_eq!(marker_place(900.0, 375.0, 425.0, 465.0), 0.06);
    // Limits without a distance give a place and no division by zero.
    assert_eq!(marker_place(100.0, 100.0, 100.0, 100.0), 0.25);
}

mod layout {
    use gpui::{AppContext as _, Bounds, Pixels, TestAppContext, VisualTestContext, px};
    use nicecooling::app::{self, Backend, StartMode};
    use nicecooling::i18n::Language;
    use nicecooling::model::DaemonEvent;
    use nicecooling::screen_view::ScreenView;
    use nicecooling::screens::leakshield::Tab;
    use nicecooling::screens::leakshield::settings::{Control, SettingsModel, SettingsUi, Step};
    use nicecooling::screens::{Feedback, ScreenInput};
    use tokio::sync::{broadcast, mpsc};

    use super::{LEAKSHIELD, NOW_US, device};

    /// The sidebar of the shell: the page is this much narrower than the window.
    const SIDEBAR: f32 = 220.0;

    /// Every row of every step ends inside its card, and the parts of the band diagram keep
    /// their place. `edit` makes the state of the wizard that the step shows.
    fn check(cx: &mut TestAppContext, language: Language, width: f32, edit: fn(&mut SettingsUi)) {
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
        let state = device();
        events
            .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
            .unwrap();
        events
            .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
            .unwrap();
        cx.run_until_parked();
        let (view_commands, _inbox) = mpsc::unbounded_channel();
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
                edit(&mut view.leakshield_ui().settings);
            })
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
                    for zone in ["above", "inside", "below"] {
                        let zone = found(format!("ls-band-zone-{zone}")).expect("a zone");
                        assert!(
                            zone.left() >= plot.left() && zone.right() <= marker.left(),
                            "{tag}: the zone text is {zone:?}, the marker text {marker:?}"
                        );
                    }
                }
            }
        }
        cx.update(|cx| cx.quit());
    }

    fn untouched(_: &mut SettingsUi) {}

    #[gpui::test]
    fn the_wizard_steps_fit_at_1100_in_english(cx: &mut TestAppContext) {
        check(cx, Language::English, 1100., untouched);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1100_in_italian(cx: &mut TestAppContext) {
        check(cx, Language::Italian, 1100., untouched);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1280_in_english(cx: &mut TestAppContext) {
        check(cx, Language::English, 1280., untouched);
    }

    #[gpui::test]
    fn the_wizard_steps_fit_at_1280_in_italian(cx: &mut TestAppContext) {
        check(cx, Language::Italian, 1280., untouched);
    }
}
