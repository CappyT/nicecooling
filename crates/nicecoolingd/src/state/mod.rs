//! The daemon state in `/var/lib/nicecooling/`. `docs/storage.md` defines the files.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nicecooling_core::calibration::{CalibrationTable, Preset};
use nicecooling_core::curve::Curve;
use nicecooling_core::derived::DerivedSensor;
use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{ChannelSetup, Control, LoopSetup, TickConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

pub mod atomic;
pub mod store;

pub use store::{Loaded, Source, StateStore};

pub const STATE_FILE: &str = "state.json";
/// The daemon persists this long after the last change.
pub const PERSIST_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub hwmon_name: String,
    pub device_path: String,
    pub attribute: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalibrationDef {
    pub id: String,
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlarmResponse {
    pub response: String,
    pub delay_s: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfilesState {
    pub default: String,
    pub active: String,
    pub cause: String,
    pub immediate_switching: bool,
    pub rule_delay_s: u32,
    pub dwell_s: u32,
}

impl Default for ProfilesState {
    fn default() -> Self {
        Self {
            default: "profile_default".into(),
            active: "profile_default".into(),
            cause: "default".into(),
            immediate_switching: false,
            rule_delay_s: 30,
            dwell_s: 300,
        }
    }
}

/// `state.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonState {
    pub schema_version: u32,
    pub identities: BTreeMap<String, Identity>,
    pub names: BTreeMap<String, String>,
    pub derived_sensors: Vec<DerivedSensor>,
    pub calibration_tables: Vec<CalibrationDef>,
    pub loops: Vec<Loop>,
    pub alarm_responses: BTreeMap<String, AlarmResponse>,
    /// Typed by tasks 7.x.
    pub rule_groups: Vec<serde_json::Value>,
    pub profiles: ProfilesState,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            identities: BTreeMap::new(),
            names: BTreeMap::new(),
            derived_sensors: Vec::new(),
            calibration_tables: Vec::new(),
            loops: Vec::new(),
            alarm_responses: BTreeMap::new(),
            rule_groups: Vec::new(),
            profiles: ProfilesState::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlKind {
    Hardware,
    Curve,
    Fixed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileChannel {
    pub control: ControlKind,
    pub input: Option<String>,
    pub points: Vec<(f64, f64)>,
    pub smoothing_s: Option<f64>,
    pub step_limit_pct_per_s: Option<f64>,
    pub fixed_duty: Option<f64>,
}

/// `profiles/<id>/profile.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileFile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub channels: BTreeMap<String, ProfileChannel>,
    pub pump_floors: BTreeMap<String, f64>,
    /// The Leakshield slot of the Profile, 1 to 4.
    pub slot: u32,
}

impl Default for ProfileFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            id: "profile_default".into(),
            name: "Default".into(),
            channels: BTreeMap::new(),
            pump_floors: BTreeMap::new(),
            slot: 1,
        }
    }
}

/// The state that the tick runs on, and the reasons to raise `state-fallback`.
pub struct LoadedState {
    pub state: DaemonState,
    pub profile: ProfileFile,
    pub fallback: bool,
}

fn is_identifier(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Loads `state.json` and the active Profile. A file that fails to load gives its default,
/// which has no Curves: every Channel stays on its Hardware curve.
pub fn load_state(store: &StateStore) -> LoadedState {
    let state = store.load::<DaemonState>(STATE_FILE);
    let mut fallback = state.fallback;
    let active = state.value.profiles.active.clone();
    let profile = if is_identifier(&active) {
        let loaded = store.load::<ProfileFile>(&format!("profiles/{active}/profile.json"));
        fallback |= loaded.fallback;
        loaded.value
    } else {
        fallback = true;
        ProfileFile::default()
    };
    LoadedState {
        state: state.value,
        profile,
        fallback,
    }
}

/// Builds the tick configuration. A Curve that fails its checks leaves its Channel on the
/// Hardware curve, and the message goes to `problems`.
pub fn tick_config(
    state: &DaemonState,
    profile: &ProfileFile,
    presets: &[Preset],
    problems: &mut Vec<String>,
) -> TickConfig {
    let mut channels = Vec::new();
    for (id, channel) in &profile.channels {
        let control = match channel.control {
            ControlKind::Hardware => Control::Hardware,
            ControlKind::Fixed => match channel.fixed_duty {
                Some(duty) if (0.0..=100.0).contains(&duty) => Control::Fixed(duty),
                _ => {
                    problems.push(format!("{id}: invalid fixed Duty"));
                    Control::Hardware
                }
            },
            ControlKind::Curve => {
                let curve = channel
                    .input
                    .clone()
                    .ok_or_else(|| "no Input".to_owned())
                    .and_then(|input| {
                        Curve::new(input, channel.points.clone()).map_err(|e| e.to_string())
                    });
                match curve {
                    Ok(mut curve) => {
                        curve.smoothing_s = channel.smoothing_s.unwrap_or(curve.smoothing_s);
                        curve.step_limit = channel.step_limit_pct_per_s.unwrap_or(curve.step_limit);
                        Control::Curve(curve)
                    }
                    Err(e) => {
                        problems.push(format!("{id}: {e}"));
                        Control::Hardware
                    }
                }
            }
        };
        channels.push(ChannelSetup {
            id: id.clone(),
            control,
        });
    }

    let loops = state
        .loops
        .iter()
        .map(|l| {
            let mut config = l.clone();
            if let Some(floor) = profile.pump_floors.get(&l.id) {
                config.pump_floor = *floor;
            }
            let table = l.calibration_table.as_ref().and_then(|id| {
                state
                    .calibration_tables
                    .iter()
                    .find(|t| &t.id == id)
                    .and_then(|t| CalibrationTable::new(t.points.clone()).ok())
                    .or_else(|| {
                        presets
                            .iter()
                            .find(|p| &p.id == id)
                            .map(|p| p.table.clone())
                    })
            });
            LoopSetup { config, table }
        })
        .collect();

    TickConfig {
        channels,
        derived: state.derived_sensors.clone(),
        loops,
    }
}

/// Runs a flush closure `delay` after the last `changed` call. Ten changes inside the delay give one flush.
pub struct Persister {
    changed: Arc<Notify>,
    flushes: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl Persister {
    pub fn spawn(delay: Duration, mut flush: impl FnMut() + Send + 'static) -> Self {
        let changed = Arc::new(Notify::new());
        let flushes = Arc::new(AtomicUsize::new(0));
        let task = tokio::spawn({
            let (changed, flushes) = (changed.clone(), flushes.clone());
            async move {
                loop {
                    changed.notified().await;
                    loop {
                        tokio::select! {
                            () = tokio::time::sleep(delay) => break,
                            () = changed.notified() => {}
                        }
                    }
                    flush();
                    flushes.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        Self {
            changed,
            flushes,
            task,
        }
    }

    pub fn changed(&self) {
        self.changed.notify_one();
    }

    pub fn flushes(&self) -> usize {
        self.flushes.load(Ordering::SeqCst)
    }
}

impl Drop for Persister {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-state-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_state() -> DaemonState {
        let mut state = DaemonState::default();
        state
            .names
            .insert("nct6798_pwm6".into(), "Loop pump".into());
        state
    }

    #[test]
    fn higher_schema_version_not_written_loads_lkg_and_raises_state_fallback() {
        let dir = temp("too-new");
        let store = StateStore::new(&dir);
        store.save(STATE_FILE, &sample_state()).unwrap();
        // The next start loads the file and writes the copy.
        assert!(!store.load::<DaemonState>(STATE_FILE).fallback);
        let newer = br#"{"schema_version": 2, "names": {"x": "y"}}"#;
        fs::write(dir.join(STATE_FILE), newer).unwrap();

        let store = StateStore::new(&dir);
        let loaded = store.load::<DaemonState>(STATE_FILE);
        assert_eq!(loaded.source, Source::LastKnownGood);
        assert!(loaded.fallback);
        assert_eq!(loaded.value, sample_state());
        assert!(!store.save(STATE_FILE, &DaemonState::default()).unwrap());
        assert_eq!(fs::read(dir.join(STATE_FILE)).unwrap(), newer);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn both_copies_corrupt_starts_without_curves_and_keeps_files() {
        let dir = temp("corrupt");
        let profile_dir = dir.join("profiles/profile_default");
        fs::create_dir_all(&profile_dir).unwrap();
        for file in [
            "state.json",
            "state.json.lkg",
            "profiles/profile_default/profile.json",
            "profiles/profile_default/profile.json.lkg",
        ] {
            fs::write(dir.join(file), b"{ not json").unwrap();
        }
        let store = StateStore::new(&dir);
        let loaded = load_state(&store);
        assert!(loaded.fallback);
        assert!(loaded.profile.channels.is_empty());
        let mut problems = Vec::new();
        let config = tick_config(&loaded.state, &loaded.profile, &[], &mut problems);
        assert!(config.channels.is_empty());
        assert_eq!(fs::read(dir.join("state.json")).unwrap(), b"{ not json");
        assert_eq!(fs::read(dir.join("state.json.lkg")).unwrap(), b"{ not json");

        // A later write keeps the broken copies aside.
        assert!(store.save(STATE_FILE, &sample_state()).unwrap());
        assert_eq!(
            fs::read(dir.join("state.json.broken")).unwrap(),
            b"{ not json"
        );
        assert_eq!(
            fs::read(dir.join("state.json.lkg.broken")).unwrap(),
            b"{ not json"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn first_start_has_defaults_and_no_fallback() {
        let dir = temp("first");
        let loaded = load_state(&StateStore::new(&dir));
        assert!(!loaded.fallback);
        assert_eq!(loaded.state, DaemonState::default());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loop_without_leakshield_is_valid() {
        let dir = temp("loop");
        let pump_loop = Loop {
            id: "loop_1".into(),
            name: "Loop".into(),
            pump_channels: vec!["nct6798_pwm6".into()],
            flow_sensor: Some("asusec_fan2".into()),
            calibration_table: Some("alphacool_17558".into()),
            water_temperature: Some("asusec_temp4".into()),
            leakshield: None,
            pump_floor: 40.0,
        };
        assert_eq!(pump_loop.validate(), Ok(()));
        let mut state = DaemonState::default();
        state.loops.push(pump_loop.clone());
        let store = StateStore::new(&dir);
        store.save(STATE_FILE, &state).unwrap();
        let text = fs::read_to_string(dir.join(STATE_FILE)).unwrap();
        assert!(text.contains(r#""leakshield": null"#));
        assert_eq!(
            store.load::<DaemonState>(STATE_FILE).value.loops,
            [pump_loop]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn curves_and_fixed_duty_come_from_the_active_profile() {
        let mut profile = ProfileFile::default();
        profile.channels.insert(
            "nct6798_pwm1".into(),
            ProfileChannel {
                control: ControlKind::Curve,
                input: Some("k10temp_temp1".into()),
                points: vec![(40.0, 30.0), (60.0, 70.0)],
                smoothing_s: Some(2.0),
                step_limit_pct_per_s: None,
                fixed_duty: None,
            },
        );
        profile.channels.insert(
            "nct6798_pwm2".into(),
            ProfileChannel {
                control: ControlKind::Curve,
                input: None,
                points: vec![],
                smoothing_s: None,
                step_limit_pct_per_s: None,
                fixed_duty: None,
            },
        );
        let mut problems = Vec::new();
        let config = tick_config(&DaemonState::default(), &profile, &[], &mut problems);
        let Control::Curve(curve) = &config.channels[0].control else {
            panic!("not a curve")
        };
        assert_eq!((curve.smoothing_s, curve.step_limit), (2.0, 5.0));
        assert_eq!(config.channels[1].control, Control::Hardware);
        assert_eq!(problems.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn ten_edits_within_2s_one_write() {
        let persister = Persister::spawn(PERSIST_DELAY, || {});
        for _ in 0..10 {
            persister.changed();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // 1.9 s after the last change nothing is written yet.
        tokio::time::sleep(Duration::from_millis(1800)).await;
        assert_eq!(persister.flushes(), 0);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(persister.flushes(), 1);
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(persister.flushes(), 1);
    }
}
