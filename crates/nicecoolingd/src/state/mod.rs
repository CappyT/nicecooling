//! The daemon state in `/var/lib/nicecooling/`. `docs/storage.md` defines the files.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nicecooling_api::ids::DEFAULT_PROFILE;
use nicecooling_core::calibration::{CalibrationTable, Preset};
use nicecooling_core::curve::Curve;
use nicecooling_core::derived::DerivedSensor;
use nicecooling_core::loops::Loop;
use nicecooling_core::profile::Slot;
use nicecooling_core::rules::group::RuleGroupDef;
use nicecooling_core::tick::{ChannelSetup, Control, LoopSetup, TickConfig};
use nicecooling_hwmon::discovery::Discovery;
use nicecooling_hwmon::identity::DeviceIdentity;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

pub mod atomic;
pub mod profiles;
pub mod slots;
pub mod store;

pub use profiles::ProfileSet;
pub use slots::SlotStore;
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
    /// Ordered: the first group has the highest priority.
    pub rule_groups: Vec<RuleGroupDef>,
    /// Leakshield id -> Software sensor slot (1 to 12) -> sensor id.
    pub software_sensors: BTreeMap<String, BTreeMap<u16, String>>,
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
            software_sensors: BTreeMap::new(),
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
    /// Every Profile. `state.profiles.active` is one of them.
    pub profiles: ProfileSet,
    pub fallback: bool,
    /// No `state.json` existed: the daemon created the Default Profile and three more.
    pub first_start: bool,
}

impl LoadedState {
    /// The active Profile.
    pub fn profile(&self) -> &ProfileFile {
        self.profiles
            .get(&self.state.profiles.active)
            .unwrap_or_else(|| self.profiles.default_profile())
    }
}

fn is_identifier(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The relative path of the file of a Profile.
pub fn profile_file(id: &str) -> String {
    format!("profiles/{id}/profile.json")
}

/// Loads `state.json` and every Profile. A file that fails to load gives its default, which has
/// no Curves: every Channel stays on its Hardware curve. When `state.json` itself falls back to
/// its default, no Profile is loaded: their Curves would run without the Loops, so no Pump floor
/// would apply.
pub fn load_state(store: &StateStore) -> LoadedState {
    let loaded = store.load::<DaemonState>(STATE_FILE);
    let mut fallback = loaded.fallback;
    let first_start = loaded.source == Source::Defaults && !loaded.fallback;
    let mut state = loaded.value;
    let mut map = BTreeMap::new();
    if !(loaded.source == Source::Defaults && loaded.fallback) {
        let names = std::fs::read_dir(store.path("profiles"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| is_identifier(name));
        for name in names {
            let profile = store.load::<ProfileFile>(&profile_file(&name));
            fallback |= profile.fallback;
            // A slot outside 1 to 4 makes the file a broken one: no Leakshield slot to write.
            let bad_slot =
                profile.source != Source::Defaults && Slot::new(profile.value.slot).is_none();
            fallback |= bad_slot;
            // A directory without a file has no Profile. A broken one stays on disk.
            if profile.source != Source::Defaults && !bad_slot {
                map.insert(
                    name.clone(),
                    ProfileFile {
                        id: name,
                        ..profile.value
                    },
                );
            }
        }
    }
    // Two groups with one id would give the rules stage one timer for both.
    let mut group_ids = BTreeSet::new();
    let groups = state.rule_groups.len();
    state.rule_groups.retain(|g| group_ids.insert(g.id.clone()));
    fallback |= state.rule_groups.len() != groups;
    let first_profiles = map.is_empty() && first_start;
    let profiles = if first_profiles {
        ProfileSet::first_start()
    } else {
        ProfileSet::new(map)
    };
    if !profiles.contains(&state.profiles.active) {
        if state.profiles.active != DEFAULT_PROFILE {
            fallback = true;
        }
        state.profiles.active = DEFAULT_PROFILE.to_owned();
        state.profiles.cause = "default".to_owned();
    }
    LoadedState {
        state,
        profiles,
        fallback,
        first_start: first_profiles,
    }
}

/// The identifier prefix of each device that `state.json` has seen. Discovery keeps these, so a
/// chip that appears later never renames the ids of a chip with the same hwmon name.
pub fn known_prefixes(identities: &BTreeMap<String, Identity>) -> BTreeMap<DeviceIdentity, String> {
    let mut known = BTreeMap::new();
    for (id, identity) in identities {
        if let Some(prefix) = id.strip_suffix(&format!("_{}", identity.attribute)) {
            known
                .entry(DeviceIdentity {
                    hwmon_name: identity.hwmon_name.clone(),
                    device_path: identity.device_path.clone(),
                })
                .or_insert_with(|| prefix.to_owned());
        }
    }
    known
}

/// The `state.json` identity of each Channel and sensor of a discovery.
pub fn identities_of(discovery: &Discovery) -> BTreeMap<String, Identity> {
    let identity = |device: &DeviceIdentity, attribute: String| Identity {
        hwmon_name: device.hwmon_name.clone(),
        device_path: device.device_path.clone(),
        attribute,
    };
    let channels = discovery.channels.iter().map(|c| {
        (
            c.id.clone(),
            identity(&c.device, format!("pwm{}", c.number)),
        )
    });
    let sensors = discovery
        .sensors
        .iter()
        .map(|s| (s.id.clone(), identity(&s.device, s.attribute.clone())));
    channels.chain(sensors).collect()
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
                        let mut curve =
                            Curve::new(input, channel.points.clone()).map_err(|e| e.to_string())?;
                        curve
                            .set_limits(channel.smoothing_s, channel.step_limit_pct_per_s)
                            .map_err(|e| e.to_string())?;
                        Ok(curve)
                    });
                match curve {
                    Ok(curve) => Control::Curve(curve),
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

    fn group(id: &str, profile: &str) -> RuleGroupDef {
        RuleGroupDef {
            id: id.into(),
            profile: profile.into(),
            enabled: true,
            rules: Vec::new(),
        }
    }

    #[test]
    fn duplicate_group_ids_load_the_first_group_and_raise_state_fallback() {
        let dir = temp("duplicate-groups");
        let store = StateStore::new(&dir);
        let mut state = sample_state();
        state.rule_groups = vec![
            group("group_1", "profile_2"),
            group("group_2", "profile_3"),
            group("group_1", "profile_4"),
        ];
        store.save(STATE_FILE, &state).unwrap();

        let loaded = load_state(&StateStore::new(&dir));

        assert!(loaded.fallback);
        assert_eq!(
            loaded.state.rule_groups,
            [group("group_1", "profile_2"), group("group_2", "profile_3")]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_profile_with_a_slot_outside_1_to_4_is_a_broken_file() {
        let dir = temp("bad-slot");
        let store = StateStore::new(&dir);
        let mut state = sample_state();
        state.profiles.active = "profile_3".into();
        state.profiles.cause = "manual".into();
        store.save(STATE_FILE, &state).unwrap();
        for (id, slot) in [("profile_2", 2), ("profile_3", 9), ("profile_4", 0)] {
            let file = ProfileFile {
                id: id.into(),
                slot,
                ..ProfileFile::default()
            };
            store.save(&profile_file(id), &file).unwrap();
        }

        let loaded = load_state(&StateStore::new(&dir));

        assert!(loaded.fallback);
        assert!(loaded.profiles.contains("profile_2"));
        assert!(!loaded.profiles.contains("profile_3"));
        assert!(!loaded.profiles.contains("profile_4"));
        // The active Profile is gone: the Default Profile takes over. The files stay on disk.
        assert_eq!(loaded.state.profiles.active, DEFAULT_PROFILE);
        assert!(dir.join("profiles/profile_3/profile.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_active_profile_that_has_no_files_falls_back_to_the_default_profile() {
        let dir = temp("active-gone");
        let store = StateStore::new(&dir);
        let mut state = sample_state();
        state.profiles.active = "profile_3".into();
        state.profiles.cause = "manual".into();
        store.save(STATE_FILE, &state).unwrap();

        let loaded = load_state(&StateStore::new(&dir));

        assert!(loaded.fallback);
        assert_eq!(loaded.state.profiles.active, DEFAULT_PROFILE);
        assert_eq!(loaded.state.profiles.cause, "default");
        assert!(loaded.profiles.contains(DEFAULT_PROFILE));
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
        assert!(loaded.profile().channels.is_empty());
        let mut problems = Vec::new();
        let config = tick_config(&loaded.state, loaded.profile(), &[], &mut problems);
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

    fn fixed_channel(duty: f64) -> ProfileChannel {
        ProfileChannel {
            control: ControlKind::Fixed,
            input: None,
            points: vec![],
            smoothing_s: None,
            step_limit_pct_per_s: None,
            fixed_duty: Some(duty),
        }
    }

    #[test]
    fn state_fallback_to_defaults_does_not_run_the_profile_without_loops() {
        let dir = temp("state-lost-profile-kept");
        let store = StateStore::new(&dir);
        let mut profile = ProfileFile::default();
        profile
            .channels
            .insert("nct6798_pwm6".into(), fixed_channel(10.0));
        store
            .save("profiles/profile_default/profile.json", &profile)
            .unwrap();
        for file in ["state.json", "state.json.lkg"] {
            fs::write(dir.join(file), b"{ not json").unwrap();
        }
        let loaded = load_state(&StateStore::new(&dir));
        assert!(loaded.fallback);
        assert!(loaded.profile().channels.is_empty());
        let mut problems = Vec::new();
        let config = tick_config(&loaded.state, loaded.profile(), &[], &mut problems);
        assert!(config.channels.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn profile_fallback_to_defaults_runs_no_curves_and_raises_state_fallback() {
        let dir = temp("profile-lost");
        let store = StateStore::new(&dir);
        store.save(STATE_FILE, &sample_state()).unwrap();
        fs::create_dir_all(dir.join("profiles/profile_default")).unwrap();
        for file in ["profile.json", "profile.json.lkg"] {
            fs::write(dir.join("profiles/profile_default").join(file), b"{ no").unwrap();
        }
        let loaded = load_state(&StateStore::new(&dir));
        assert!(loaded.fallback);
        assert!(loaded.profile().channels.is_empty());
        assert_eq!(loaded.state, sample_state());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn curve_limits_that_panic_or_freeze_leave_the_channel_on_hardware() {
        let bad: [(Option<f64>, Option<f64>); 7] = [
            (Some(-1.0), None),
            (Some(1e300), None),
            (Some(601.0), None),
            (Some(f64::NAN), None),
            (None, Some(-5.0)),
            (None, Some(0.0)),
            (None, Some(f64::NAN)),
        ];
        let mut profile = ProfileFile::default();
        for (i, (smoothing_s, step_limit_pct_per_s)) in bad.into_iter().enumerate() {
            profile.channels.insert(
                format!("bad_{i}"),
                ProfileChannel {
                    control: ControlKind::Curve,
                    input: Some("k10temp_temp1".into()),
                    points: vec![(40.0, 30.0), (60.0, 70.0)],
                    smoothing_s,
                    step_limit_pct_per_s,
                    fixed_duty: None,
                },
            );
        }
        profile.channels.insert(
            "good".into(),
            ProfileChannel {
                control: ControlKind::Curve,
                input: Some("k10temp_temp1".into()),
                points: vec![(40.0, 30.0), (60.0, 70.0)],
                smoothing_s: Some(0.0),
                step_limit_pct_per_s: Some(0.5),
                fixed_duty: None,
            },
        );
        let mut problems = Vec::new();
        let config = tick_config(&DaemonState::default(), &profile, &[], &mut problems);
        for setup in &config.channels {
            let on_curve = matches!(setup.control, Control::Curve(_));
            assert_eq!(on_curve, setup.id == "good", "{}", setup.id);
        }
        assert_eq!(problems.len(), 7);
    }

    #[test]
    fn identities_keep_ids_when_a_chip_with_the_same_name_appears() {
        use nicecooling_hwmon::discovery::discover_with;
        use nicecooling_hwmon::fake::FakeSysfs;

        let fake = FakeSysfs::new();
        let attrs = [("name", "nvme"), ("temp1_input", "40000")];
        let old_chip = "devices/pci0000:00/0000:00:01.2/nvme/nvme1";
        fake.add_hwmon(0, "nvme", old_chip, None, &attrs);
        let mut state = DaemonState::default();
        let first = discover_with(
            &fake.sysfs(),
            None,
            &state.names,
            &known_prefixes(&state.identities),
        )
        .unwrap();
        state.identities.extend(identities_of(&first));
        assert_eq!(first.sensors[0].id, "nvme_temp1");

        // The state survives a restart as JSON.
        let dir = temp("identities");
        let store = StateStore::new(&dir);
        store.save(STATE_FILE, &state).unwrap();
        let state = store.load::<DaemonState>(STATE_FILE).value;

        // The new chip has the lower device path. Without the identities it would take `nvme`.
        fake.add_hwmon(
            1,
            "nvme",
            "devices/pci0000:00/0000:00:01.1/nvme/nvme0",
            None,
            &attrs,
        );
        let second = discover_with(
            &fake.sysfs(),
            None,
            &state.names,
            &known_prefixes(&state.identities),
        )
        .unwrap();
        let id_of = |path: &str| {
            second
                .sensors
                .iter()
                .find(|s| s.device.device_path == format!("/{path}"))
                .map(|s| s.id.as_str())
        };
        assert_eq!(id_of(old_chip), Some("nvme_temp1"));
        assert_eq!(
            id_of("devices/pci0000:00/0000:00:01.1/nvme/nvme0"),
            Some("nvme_2_temp1")
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
