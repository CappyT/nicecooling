//! The values that the app sends for the Leakshield fields fit the registry kind (issue #85).

mod support;

use nicecooling::command::Command;
use nicecooling::i18n::EN;
use nicecooling::model::FieldValue;
use nicecooling::screens::leakshield::alarms::{AlarmsUi, DEVICE_RESPONSES};
use nicecooling::screens::leakshield::display::{TEMPLATES, template_name, template_values};
use nicecooling::screens::leakshield::lighting::{EFFECTS, LightingUi};
use nicecooling::screens::leakshield::settings::SettingsUi;
use nicecooling::screens::{Feedback, ScreenInput};
use support::fixtures::{LEAKSHIELD, NOW_US, leakshield_data, with_leakshield};

const REGISTRY: &str = include_str!("../../nicecooling-protocol/registry/leakshield.toml");

fn registry_names(id: &str) -> Vec<String> {
    let table: toml::Table = REGISTRY.parse().unwrap();
    let field = table["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("{id}"));
    field["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect()
}

fn sent(command: Command, field: &str) -> FieldValue {
    let Command::SetLeakshieldFields { fields, .. } = command else {
        panic!("a field write")
    };
    fields
        .into_iter()
        .find(|f| f.0 == field)
        .unwrap_or_else(|| panic!("{field}"))
        .1
}

#[test]
fn every_name_the_app_sends_is_a_registry_name() {
    let effects = registry_names("led_controller_mode");
    for e in EFFECTS.iter().chain(&["unused"]) {
        assert!(effects.contains(&(*e).to_owned()), "effect {e}");
    }
    let templates = registry_names("page_template");
    for (t, _) in TEMPLATES.iter().chain(&[("none", 0)]) {
        assert!(templates.contains(&(*t).to_owned()), "template {t}");
    }
    let actions = registry_names("leak_rate_warning_response_action");
    for r in DEVICE_RESPONSES {
        assert!(actions.contains(&r.to_owned()), "response {r}");
    }
    for (field, name) in [
        ("default_mode", "shield"),
        ("default_mode", "monitor"),
        ("default_mode", "release"),
        ("reservoir_model", "custom"),
        ("pump_model", "custom"),
        ("display_brightness_active", "maximum"),
        ("display_brightness_active", "mid"),
        ("display_brightness_active", "low"),
        ("display_brightness_active", "off"),
    ] {
        assert!(
            registry_names(field).contains(&name.to_owned()),
            "{field} {name}"
        );
    }
}

/// The label of a template and the number of source rows that the screen shows agree.
#[test]
fn template_value_count_matches_its_label() {
    for (name, values) in [
        ("values-1", 1),
        ("values-2", 2),
        ("values-4", 4),
        ("values-3a", 3),
        ("values-3b", 3),
        ("bars-2", 2),
        ("bars-4a", 4),
        ("bars-4b", 4),
    ] {
        assert_eq!(
            template_values(name),
            values,
            "{name}: label {:?}",
            template_name(EN, name)
        );
    }
}

#[test]
fn a_typed_hue_and_led_range_are_whole() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let input = ScreenInput {
        state: &state,
        now_us: NOW_US,
        locale: EN,
        down_since_us: None,
        feedback: &feedback,
    };
    let mut ui = LightingUi::default();
    ui.toggle(0);
    let command = ui.set_hue(&input, LEAKSHIELD, 0, 120.5).expect("hue");
    let FieldValue::List(hues) = sent(command, "led_controller_color_hue") else {
        panic!("a list")
    };
    assert!(hues.iter().all(|h| h.fract() == 0.0), "{hues:?}");
    let command = ui.set_range(&input, LEAKSHIELD, 1.4, 3.6).expect("range");
    for field in ["led_controller_first_led", "led_controller_led_count"] {
        let FieldValue::List(list) = sent(command.clone(), field) else {
            panic!("a list")
        };
        assert!(list.iter().all(|n| n.fract() == 0.0), "{field} {list:?}");
    }
}

#[test]
fn a_typed_leak_rate_threshold_is_whole_and_a_fill_level_keeps_its_fraction() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let input = ScreenInput {
        state: &state,
        now_us: NOW_US,
        locale: EN,
        down_since_us: None,
        feedback: &feedback,
    };
    let command = AlarmsUi
        .set_threshold(&input, LEAKSHIELD, "leak_rate_warning_threshold", 120.6)
        .expect("leak rate");
    assert_eq!(
        sent(command, "leak_rate_warning_threshold"),
        FieldValue::Number(121.0)
    );
    let command = AlarmsUi
        .set_threshold(&input, LEAKSHIELD, "fill_level_warning_threshold", 55.25)
        .expect("fill level");
    assert_eq!(
        sent(command, "fill_level_warning_threshold"),
        FieldValue::Number(55.25)
    );
}

#[test]
fn a_typed_pump_speed_is_whole_and_a_pressure_keeps_its_fraction() {
    let data = leakshield_data();
    let mut ui = SettingsUi::default();
    ui.edit(&data, "pump_max_speed", "4800,5");
    ui.edit(&data, "pump_max_pressure", "365.5");
    assert_eq!(
        ui.pending.get("pump_max_speed"),
        Some(&FieldValue::Number(4801.0))
    );
    assert_eq!(
        ui.pending.get("pump_max_pressure"),
        Some(&FieldValue::Number(365.5))
    );
}
