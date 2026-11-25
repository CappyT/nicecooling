//! A downgrade: the daemon of the release before the Graph sensor loads a `state.json` that has
//! `graph_sensors` and `graph_sensor_next`, and drops both keys at its next save.

#[path = "fixtures/old_daemon_state.rs"]
mod old_daemon_state;

use std::fs;
use std::path::PathBuf;

use nicecoolingd::state::{DaemonState, GraphSensorDef, STATE_FILE, Source, StateStore};
use old_daemon_state::OldDaemonState;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nicecooling-downgrade-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A state with two Graph sensors, a user name for one of them, and one of them in a Software
/// sensor slot.
fn new_state() -> DaemonState {
    let mut state = DaemonState {
        graph_sensors: vec![
            GraphSensorDef {
                id: "graph_1".into(),
                name: "Hot water".into(),
                unit: "degC".into(),
            },
            GraphSensorDef {
                id: "graph_3".into(),
                name: "Flow".into(),
                unit: "l/h".into(),
            },
        ],
        graph_sensor_next: 4,
        ..DaemonState::default()
    };
    state.names.insert("graph_1".into(), "Mine".into());
    state
        .names
        .insert("nct6798_pwm1".into(), "Front fan".into());
    state
        .software_sensors
        .entry("leakshield_20417_31072".into())
        .or_default()
        .insert(4, "graph_1".into());
    state
}

#[test]
fn old_loader_ignores_graph_sensor_keys() {
    let dir = temp("ignores");
    let state = new_state();
    StateStore::new(&dir).save(STATE_FILE, &state).unwrap();
    let text = fs::read_to_string(dir.join(STATE_FILE)).unwrap();
    assert!(text.contains("\"graph_sensors\"") && text.contains("\"graph_sensor_next\": 4"));

    let loaded = StateStore::new(&dir).load::<OldDaemonState>(STATE_FILE);
    // The file loads as itself: no fallback to the last-known-good copy, no default.
    assert_eq!(loaded.source, Source::Primary);
    assert!(!loaded.fallback);
    assert_eq!(loaded.value.schema_version, 1);
    assert_eq!(loaded.value.names, state.names);
    assert_eq!(loaded.value.software_sensors, state.software_sensors);
    assert_eq!(loaded.value.profiles, state.profiles);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn old_loader_drops_the_keys_on_save() {
    let dir = temp("drops");
    StateStore::new(&dir)
        .save(STATE_FILE, &new_state())
        .unwrap();

    // The older daemon starts, loads the file and saves it 2 s after its first write.
    let old = StateStore::new(&dir);
    let loaded = old.load::<OldDaemonState>(STATE_FILE);
    assert!(old.save(STATE_FILE, &loaded.value).unwrap());
    let text = fs::read_to_string(dir.join(STATE_FILE)).unwrap();
    assert!(!text.contains("graph_sensor"), "{text}");
    assert!(text.contains("\"Front fan\""));

    // The release with the Graph sensor starts again: it has no Graph sensor, and its counter is 1.
    // The app creates the sensors of its Graphs again with new ids.
    let back = StateStore::new(&dir).load::<DaemonState>(STATE_FILE);
    assert_eq!(back.source, Source::Primary);
    assert!(back.value.graph_sensors.is_empty());
    assert_eq!(back.value.graph_sensor_next, 1);
    assert_eq!(back.value.names["nct6798_pwm1"], "Front fan");
    fs::remove_dir_all(dir).unwrap();
}
