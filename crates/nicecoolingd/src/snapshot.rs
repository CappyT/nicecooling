//! The BIOS snapshot and its restore (design D8, `docs/storage.md`).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use nicecooling_hwmon::discovery::Channel;
use nicecooling_hwmon::identity::DeviceIdentity;
use nicecooling_hwmon::sysfs::{Sysfs, channel_attributes, read_attr, write_attr};
use serde::{Deserialize, Serialize};

use crate::daemon::Paths;
use crate::lease::Lease;
use crate::state::{self, Source, StateStore};

pub const SNAPSHOT_FILE: &str = "bios-snapshot.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotBoard {
    pub vendor: String,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSnapshot {
    pub channel: u32,
    pub manual_at_snapshot: bool,
    /// `[name, value]` in restore order: all others, then `pwmN`, then `pwmN_enable`.
    pub attributes: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChipSnapshot {
    pub hwmon_name: String,
    pub device_path: String,
    pub channels: Vec<ChannelSnapshot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BiosSnapshot {
    pub schema_version: u32,
    pub taken_us: u64,
    pub board: SnapshotBoard,
    pub chips: Vec<ChipSnapshot>,
}

impl BiosSnapshot {
    /// The entry of a Channel, or `None` when the snapshot was taken without it.
    pub fn channel(&self, device: &DeviceIdentity, number: u32) -> Option<&ChannelSnapshot> {
        self.chips
            .iter()
            .find(|c| c.hwmon_name == device.hwmon_name && c.device_path == device.device_path)
            .and_then(|c| c.channels.iter().find(|c| c.channel == number))
    }
}

/// A Warning to raise: event id and source id (`docs/api.md`).
pub type WarningToRaise = (String, String);

/// Manual mode is `pwmN_enable` 1. A Channel in manual mode at raw 255 reads enable 0.
fn is_manual(enable: &str, raw: &str) -> bool {
    enable == "1" || (enable == "0" && raw == "255")
}

/// Reads every writable `pwmN*` attribute of every Channel. Channels of GPU drivers are
/// not Channels, so they are not part of it. An attribute that cannot be read fails the
/// snapshot: a partial snapshot would restore a partial state.
pub fn take(channels: &[Channel], board: SnapshotBoard, taken_us: u64) -> io::Result<BiosSnapshot> {
    let mut chips: BTreeMap<&DeviceIdentity, Vec<ChannelSnapshot>> = BTreeMap::new();
    for channel in channels {
        let names = channel_attributes(&channel.dir, channel.number)?;
        // Without these two the restore sets no mode and no Duty, and would still read back equal.
        for required in [
            format!("pwm{}", channel.number),
            format!("pwm{}_enable", channel.number),
        ] {
            if !names.contains(&required) {
                return Err(io::Error::other(format!(
                    "{}: {required} is not writable",
                    channel.id
                )));
            }
        }
        let mut attributes = Vec::new();
        for name in names {
            let value = read_attr(&channel.dir, &name).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("{}: {name} is not readable: {e}", channel.id),
                )
            })?;
            attributes.push((name, value));
        }
        let value = |suffix: &str| {
            let name = format!("pwm{}{suffix}", channel.number);
            attributes
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.as_str())
                .unwrap_or_default()
        };
        let manual_at_snapshot = is_manual(value("_enable"), value(""));
        chips
            .entry(&channel.device)
            .or_default()
            .push(ChannelSnapshot {
                channel: channel.number,
                manual_at_snapshot,
                attributes,
            });
    }
    Ok(BiosSnapshot {
        schema_version: 1,
        taken_us,
        board,
        chips: chips
            .into_iter()
            .map(|(device, mut channels)| {
                channels.sort_by_key(|c| c.channel);
                ChipSnapshot {
                    hwmon_name: device.hwmon_name.clone(),
                    device_path: device.device_path.clone(),
                    channels,
                }
            })
            .collect(),
    })
}

/// The BIOS snapshot of the state directory, when a file or its copy loads.
pub fn read_snapshot(store: &StateStore) -> Option<BiosSnapshot> {
    load_snapshot(store).map(|loaded| loaded.value)
}

fn load_snapshot(store: &StateStore) -> Option<state::Loaded<BiosSnapshot>> {
    let loaded = store.load::<BiosSnapshot>(SNAPSHOT_FILE);
    (loaded.source != Source::Defaults).then_some(loaded)
}

/// The takeover steps before the first write. With no lease it takes a new snapshot,
/// before any Channel changes. With a lease it keeps the existing snapshot.
pub fn prepare(
    store: &StateStore,
    lease: &Lease,
    channels: &[Channel],
    board: SnapshotBoard,
    now_us: u64,
) -> io::Result<Vec<WarningToRaise>> {
    let mut warnings = Vec::new();
    if lease.exists() {
        let loaded = load_snapshot(store);
        if loaded.as_ref().is_none_or(|s| s.fallback) {
            warnings.push(("state-fallback".to_owned(), String::new()));
        }
        // The kept snapshot is older than a Channel that appeared after it. The daemon leaves
        // such a Channel on its Hardware curve: it has no BIOS state to give back.
        for channel in channels.iter().filter(|c| {
            loaded
                .as_ref()
                .is_some_and(|s| s.value.channel(&c.device, c.number).is_none())
        }) {
            warnings.push(("no-bios-snapshot".to_owned(), channel.id.clone()));
        }
        return Ok(warnings);
    }
    let snapshot = take(channels, board, now_us)?;
    store.save(SNAPSHOT_FILE, &snapshot)?;
    // The load also writes the last-known-good copy.
    if load_snapshot(store).is_none_or(|s| s.value != snapshot) {
        return Err(io::Error::other("the BIOS snapshot does not read back"));
    }
    for chip in &snapshot.chips {
        for channel in chip.channels.iter().filter(|c| c.manual_at_snapshot) {
            warnings.push((
                "manual-at-snapshot".to_owned(),
                format!("{}_pwm{}", chip.hwmon_name, channel.channel),
            ));
        }
    }
    Ok(warnings)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RestoreReport {
    /// Every attribute reads back as the snapshot has it.
    pub matched: bool,
    pub problems: Vec<String>,
}

type WriteFn<'a> = dyn FnMut(&Path, &str, &str) -> io::Result<()> + 'a;

pub fn restore(sysfs: &Sysfs, snapshot: &BiosSnapshot) -> RestoreReport {
    restore_with(sysfs, snapshot, &mut |dir, name, value| {
        write_attr(dir, name, value)
    })
}

/// `nct6775` keeps `pwmN_stop_time`, `pwmN_step_up_time` and `pwmN_step_down_time` as 1 to 255
/// steps of 400 ms (`pwmN_mode` 1) or 100 ms (mode 0). A written 0 reads back as one step.
/// Without the mode in the snapshot, both step sizes count.
fn time_matches(expected: &str, actual: &str, mode: Option<&str>) -> bool {
    let Ok(msec) = expected.parse::<u64>() else {
        return false;
    };
    let steps = |pwm_mode: bool| {
        let (step, round) = if pwm_mode { (400, 200) } else { (100, 50) };
        ((msec + round) / step).clamp(1, 255) * step
    };
    let modes: &[bool] = match mode {
        Some("0") => &[false],
        Some(_) => &[true],
        None => &[true, false],
    };
    modes.iter().any(|m| actual == steps(*m).to_string())
}

fn readback_matches(name: &str, expected: &str, actual: &str, mode: Option<&str>) -> bool {
    actual == expected || (name.ends_with("_time") && time_matches(expected, actual, mode))
}

/// Writes the attributes of one Channel in the order of the snapshot, then reads all of them
/// back. Returns the problems. For a Channel in an automatic mode (`pwmN_enable` 2 or more) the
/// chip sets `pwmN` itself, so the read-back skips `pwmN` there.
pub fn restore_channel(
    dir: &Path,
    channel: &ChannelSnapshot,
    write: &mut WriteFn<'_>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, value) in &channel.attributes {
        if let Err(e) = write(dir, name, value) {
            problems.push(format!("{name}: write failed: {e}"));
        }
    }
    let value_of = |name: &str| {
        channel
            .attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let n = channel.channel;
    let automatic = value_of(&format!("pwm{n}_enable"))
        .and_then(|v| v.parse::<u32>().ok())
        .is_some_and(|mode| mode >= 2);
    let mode = value_of(&format!("pwm{n}_mode"));
    for (name, value) in &channel.attributes {
        if automatic && *name == format!("pwm{n}") {
            continue;
        }
        let matches =
            read_attr(dir, name).is_ok_and(|actual| readback_matches(name, value, &actual, mode));
        if !matches {
            problems.push(format!("{name}: read-back differs from the snapshot"));
        }
    }
    problems
}

/// Restores every Channel of the snapshot and reports what does not read back.
pub fn restore_with(
    sysfs: &Sysfs,
    snapshot: &BiosSnapshot,
    write: &mut WriteFn<'_>,
) -> RestoreReport {
    let mut report = RestoreReport {
        matched: true,
        problems: Vec::new(),
    };
    let devices = sysfs.hwmon_devices().unwrap_or_default();
    for chip in &snapshot.chips {
        let identity = DeviceIdentity {
            hwmon_name: chip.hwmon_name.clone(),
            device_path: chip.device_path.clone(),
        };
        let Some(device) = devices.iter().find(|d| d.identity == identity) else {
            report.matched = false;
            report.problems.push(format!(
                "{} {}: device not found",
                chip.hwmon_name, chip.device_path
            ));
            continue;
        };
        for channel in &chip.channels {
            let problems = restore_channel(&device.dir, channel, write);
            report.matched &= problems.is_empty();
            report.problems.extend(problems);
        }
    }
    report
}

#[derive(Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// No lease: the daemon never took control. Nothing changes.
    NoLease,
    Restored(RestoreReport),
    /// No readable snapshot: the controlled Channels went to manual mode at raw 255.
    Fallback {
        channels: usize,
        /// Writes that failed. A Channel can still be in the state the daemon left it.
        failed: usize,
    },
}

/// `nicecoolingd --restore-bios`.
pub fn restore_bios(paths: &Paths) -> io::Result<RestoreOutcome> {
    let store = StateStore::new(&paths.state_dir);
    let lease = Lease::new(&paths.state_dir);
    if !lease.exists() {
        return Ok(RestoreOutcome::NoLease);
    }
    let sysfs = Sysfs::new(&paths.sysfs_root);
    if let Some(snapshot) = load_snapshot(&store) {
        let report = restore(&sysfs, &snapshot.value);
        if report.matched {
            lease.remove()?;
        }
        return Ok(RestoreOutcome::Restored(report));
    }

    let loaded = state::load_state(&store);
    let discovery = nicecooling_hwmon::discovery::discover_with(
        &sysfs,
        None,
        &loaded.state.names,
        &state::known_prefixes(&loaded.state.identities),
    )?;
    let controlled: Vec<_> = if loaded.fallback {
        Vec::new()
    } else {
        loaded
            .profile()
            .channels
            .iter()
            .filter(|(_, c)| c.control != state::ControlKind::Hardware)
            .map(|(id, _)| id.as_str())
            .collect()
    };
    let channels: Vec<_> = discovery
        .channels
        .iter()
        .filter(|c| controlled.is_empty() || controlled.contains(&c.id.as_str()))
        .collect();
    let mut failed = 0;
    for channel in &channels {
        let n = channel.number;
        for (name, value) in [(format!("pwm{n}_enable"), "1"), (format!("pwm{n}"), "255")] {
            if let Err(e) = write_attr(&channel.dir, &name, value) {
                eprintln!("nicecoolingd: {}: {name}: write failed: {e}", channel.id);
                failed += 1;
            }
        }
    }
    Ok(RestoreOutcome::Fallback {
        channels: channels.len(),
        failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nicecooling_hwmon::board::select_board_for;
    use nicecooling_hwmon::discovery::{Discovery, discover};
    use nicecooling_hwmon::fake::FakeSysfs;
    use std::fs;
    use std::path::PathBuf;

    struct Env {
        fake: FakeSysfs,
        state: PathBuf,
        discovery: Discovery,
        board: SnapshotBoard,
    }

    impl Env {
        fn new(name: &str) -> Self {
            let fake = FakeSysfs::reference_board();
            let state = std::env::temp_dir().join(format!(
                "nicecooling-snapshot-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&state);
            fs::create_dir_all(&state).unwrap();
            let boards = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/boards");
            let sysfs = fake.sysfs();
            let file = select_board_for(&sysfs, &boards).unwrap();
            let discovery = discover(&sysfs, file.as_ref(), &Default::default()).unwrap();
            let board = SnapshotBoard {
                vendor: sysfs.dmi("board_vendor"),
                model: sysfs.dmi("board_name"),
            };
            Self {
                fake,
                state,
                discovery,
                board,
            }
        }

        fn store(&self) -> StateStore {
            StateStore::new(&self.state)
        }

        fn lease(&self) -> Lease {
            Lease::new(&self.state)
        }

        fn prepare(&self) -> Vec<WarningToRaise> {
            prepare(
                &self.store(),
                &self.lease(),
                &self.discovery.channels,
                self.board.clone(),
                1,
            )
            .unwrap()
        }

        fn paths(&self) -> Paths {
            Paths {
                sysfs_root: self.fake.root().to_owned(),
                state_dir: self.state.clone(),
                boards_dir: PathBuf::from("/nonexistent"),
                calibration_dir: PathBuf::from("/nonexistent"),
            }
        }

        /// Every `pwm*` file of the Super I/O chip and of the GPU, by path.
        fn pwm_files(&self) -> BTreeMap<PathBuf, String> {
            let mut files = BTreeMap::new();
            for device in self.fake.sysfs().hwmon_devices().unwrap() {
                for a in device
                    .attributes()
                    .unwrap()
                    .into_iter()
                    .filter(|a| a.name.starts_with("pwm"))
                {
                    files.insert(device.dir.join(&a.name), device.read(&a.name).unwrap());
                }
            }
            files
        }

        /// The files that differ between two maps, as text.
        fn diff(a: &BTreeMap<PathBuf, String>, b: &BTreeMap<PathBuf, String>) -> Vec<String> {
            a.iter()
                .filter(|(path, value)| b.get(*path) != Some(*value))
                .map(|(path, value)| format!("{} {value} vs {:?}", path.display(), b.get(path)))
                .collect()
        }

        /// The directory of the Super I/O chip.
        fn chip_dir(&self) -> PathBuf {
            self.fake
                .attr_path("nct6798", "pwm1")
                .parent()
                .unwrap()
                .to_owned()
        }

        fn chip(&self, snapshot: &BiosSnapshot) -> ChipSnapshot {
            snapshot
                .chips
                .iter()
                .find(|c| c.hwmon_name == "nct6798")
                .unwrap()
                .clone()
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.state);
        }
    }

    #[test]
    fn first_start_snapshot_before_any_write() {
        let env = Env::new("first");
        let before = env.pwm_files();
        let warnings = env.prepare();
        assert!(env.state.join(SNAPSHOT_FILE).exists());
        assert!(!env.lease().exists());
        assert_eq!(
            Env::diff(&env.pwm_files(), &before),
            Vec::<String>::new(),
            "preparing the snapshot changes no Channel"
        );
        // The reference Board has the pump Channel in manual mode at 255.
        assert_eq!(
            warnings,
            [("manual-at-snapshot".to_owned(), "nct6798_pwm6".to_owned())]
        );
    }

    #[test]
    fn snapshot_holds_every_writable_attribute_of_each_channel() {
        let env = Env::new("attrs");
        env.prepare();
        let snapshot = env.store().load::<BiosSnapshot>(SNAPSHOT_FILE).value;
        let chip = env.chip(&snapshot);
        assert_eq!(chip.channels.len(), 7);
        for channel in &chip.channels {
            let n = channel.channel;
            // `pwm2` of the CPU Fan has 6 more attributes: `pwm2_weight_*`.
            let expected = if n == 2 { 28 } else { 22 };
            assert_eq!(channel.attributes.len(), expected, "pwm{n}");
            let names: Vec<_> = channel
                .attributes
                .iter()
                .map(|(name, _)| name.as_str())
                .collect();
            assert!(
                names
                    .iter()
                    .all(|name| name.starts_with(&format!("pwm{n}")))
            );
            assert_eq!(
                names[names.len() - 2..],
                [format!("pwm{n}"), format!("pwm{n}_enable")]
            );
        }
        let pump = &chip.channels[5];
        assert!(
            pump.attributes
                .contains(&("pwm6_enable".to_owned(), "0".to_owned()))
        );
        assert!(
            pump.attributes
                .contains(&("pwm6".to_owned(), "255".to_owned()))
        );
        assert_eq!(snapshot.board.model, "ROG CROSSHAIR VIII DARK HERO");
    }

    #[test]
    fn snapshot_excludes_gpu_pwm() {
        let env = Env::new("gpu");
        env.prepare();
        let snapshot = env.store().load::<BiosSnapshot>(SNAPSHOT_FILE).value;
        assert!(snapshot.chips.iter().all(|c| c.hwmon_name == "nct6798"));
        let text = fs::read_to_string(env.state.join(SNAPSHOT_FILE)).unwrap();
        assert!(!text.contains("amdgpu"));
    }

    #[test]
    fn first_start_with_manual_channel() {
        let env = Env::new("manual");
        env.fake.set(
            &env.fake
                .attr_path("nct6798", "pwm1_enable")
                .strip_prefix(env.fake.root())
                .unwrap()
                .display()
                .to_string(),
            "1",
        );
        let warnings = env.prepare();
        let snapshot = env.store().load::<BiosSnapshot>(SNAPSHOT_FILE).value;
        let manual: Vec<_> = env
            .chip(&snapshot)
            .channels
            .iter()
            .filter(|c| c.manual_at_snapshot)
            .map(|c| c.channel)
            .collect();
        assert_eq!(manual, [1, 6]);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.contains(&("manual-at-snapshot".to_owned(), "nct6798_pwm1".to_owned())));
    }

    #[test]
    fn restart_with_lease_keeps_snapshot() {
        let env = Env::new("restart");
        env.prepare();
        env.lease().create("boot", 1).unwrap();
        let first = fs::read(env.state.join(SNAPSHOT_FILE)).unwrap();
        // The daemon controls the Channels now.
        for (path, _) in env.pwm_files().iter().filter(|(p, _)| p.ends_with("pwm3")) {
            fs::write(path, "77\n").unwrap();
        }
        let warnings = env.prepare();
        assert!(warnings.is_empty());
        assert_eq!(fs::read(env.state.join(SNAPSHOT_FILE)).unwrap(), first);
    }

    #[test]
    fn restart_with_lease_warns_about_a_channel_without_an_entry() {
        let env = Env::new("no-entry");
        env.prepare();
        env.lease().create("boot", 1).unwrap();
        let mut snapshot = env.store().load::<BiosSnapshot>(SNAPSHOT_FILE).value;
        for chip in &mut snapshot.chips {
            chip.channels.retain(|c| c.channel != 3);
        }
        env.store().save(SNAPSHOT_FILE, &snapshot).unwrap();
        assert_eq!(
            env.prepare(),
            [("no-bios-snapshot".to_owned(), "nct6798_pwm3".to_owned())]
        );
    }

    fn disturb(env: &Env) {
        let dir = env.chip_dir();
        for (path, _) in env.pwm_files() {
            if path.parent() == Some(dir.as_path()) {
                fs::write(&path, "1\n").unwrap();
            }
        }
    }

    #[test]
    fn restore_matches_removes_lease() {
        let env = Env::new("restore");
        let before = env.pwm_files();
        env.prepare();
        env.lease().create("boot", 1).unwrap();
        disturb(&env);
        assert!(!Env::diff(&env.pwm_files(), &before).is_empty());

        let outcome = restore_bios(&env.paths()).unwrap();
        assert_eq!(
            outcome,
            RestoreOutcome::Restored(RestoreReport {
                matched: true,
                problems: vec![]
            })
        );
        assert_eq!(Env::diff(&env.pwm_files(), &before), Vec::<String>::new());
        assert!(!env.lease().exists());
        assert!(env.state.join(SNAPSHOT_FILE).exists());
    }

    #[test]
    fn restore_mismatch_keeps_lease_and_snapshot() {
        let env = Env::new("mismatch");
        env.prepare();
        env.lease().create("boot", 1).unwrap();
        let snapshot = fs::read(env.state.join(SNAPSHOT_FILE)).unwrap();
        let stuck = env.fake.attr_path("nct6798", "pwm3_start");
        fs::write(&stuck, "9\n").unwrap();
        let mut mode = fs::metadata(&stuck).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o444);
        fs::set_permissions(&stuck, mode).unwrap();

        let RestoreOutcome::Restored(report) = restore_bios(&env.paths()).unwrap() else {
            panic!("not restored")
        };
        assert!(!report.matched);
        assert!(report.problems.iter().any(|p| p.starts_with("pwm3_start")));
        assert!(env.lease().exists());
        assert_eq!(fs::read(env.state.join(SNAPSHOT_FILE)).unwrap(), snapshot);
    }

    #[test]
    fn restore_order_others_then_pwm_then_enable() {
        let env = Env::new("order");
        env.prepare();
        let snapshot = env.store().load::<BiosSnapshot>(SNAPSHOT_FILE).value;
        let mut order = Vec::new();
        restore_with(&env.fake.sysfs(), &snapshot, &mut |_, name, _| {
            order.push(name.to_owned());
            Ok(())
        });
        assert_eq!(order.len(), 6 * 22 + 28);
        for n in 1..=7 {
            let start = order
                .iter()
                .position(|name| name.starts_with(&format!("pwm{n}")))
                .unwrap();
            let group = order[start..]
                .iter()
                .take_while(|name| name.starts_with(&format!("pwm{n}")))
                .collect::<Vec<_>>();
            let len = group.len();
            assert_eq!(
                group[len - 2..],
                [&format!("pwm{n}"), &format!("pwm{n}_enable")]
            );
            assert!(
                group[..len - 2]
                    .iter()
                    .all(|name| **name != format!("pwm{n}") && **name != format!("pwm{n}_enable"))
            );
        }
    }

    /// The write of `nct6775`: a `*_time` value becomes 1 to 255 steps of 400 ms (PWM mode).
    fn driver_write(dir: &Path, name: &str, value: &str) -> io::Result<()> {
        let value = if name.ends_with("_time") {
            let msec: u64 = value.parse().unwrap();
            (((msec + 200) / 400).clamp(1, 255) * 400).to_string()
        } else {
            value.to_owned()
        };
        write_attr(dir, name, &value)
    }

    #[test]
    fn restore_accepts_the_step_time_the_driver_clamps() {
        let env = Env::new("step-time");
        env.prepare();
        let snapshot = env.store().load::<BiosSnapshot>(SNAPSHOT_FILE).value;
        assert!(
            env.chip(&snapshot).channels[0]
                .attributes
                .contains(&("pwm1_step_up_time".to_owned(), "0".to_owned()))
        );
        disturb(&env);
        let report = restore_with(&env.fake.sysfs(), &snapshot, &mut driver_write);
        assert_eq!(report.problems, Vec::<String>::new());
        assert!(report.matched);
        assert_eq!(
            read_attr(&env.chip_dir(), "pwm1_step_up_time").unwrap(),
            "400"
        );
    }

    #[test]
    fn time_readback_follows_the_driver_rounding() {
        // `pwmN_mode` 1 is 400 ms per step, 0 is 100 ms per step, and the step count is 1 to 255.
        assert!(time_matches("0", "400", Some("1")));
        assert!(time_matches("0", "100", Some("0")));
        assert!(!time_matches("0", "100", Some("1")));
        assert!(time_matches("0", "100", None) && time_matches("0", "400", None));
        assert!(time_matches("500", "400", Some("1")));
        assert!(time_matches("500", "500", Some("0")));
        assert!(time_matches("1000000", "102000", Some("1")));
        assert!(!time_matches("24000", "400", Some("1")));
        assert!(!time_matches("x", "400", Some("1")));
    }

    #[test]
    fn snapshot_fails_when_an_attribute_cannot_be_read() {
        for attribute in ["pwm3_enable", "pwm3_start"] {
            let env = Env::new(&format!("unreadable-{attribute}"));
            fs::write(env.fake.attr_path("nct6798", attribute), [0xff, 0xfe]).unwrap();
            let result = prepare(
                &env.store(),
                &env.lease(),
                &env.discovery.channels,
                env.board.clone(),
                1,
            );
            let error = result.unwrap_err().to_string();
            assert!(error.contains(attribute), "{error}");
            assert!(!env.state.join(SNAPSHOT_FILE).exists());
            assert!(!env.lease().exists());
        }
    }

    #[test]
    fn snapshot_fails_without_a_writable_enable() {
        let env = Env::new("read-only-enable");
        let path = env.fake.attr_path("nct6798", "pwm3_enable");
        let mut mode = fs::metadata(&path).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o444);
        fs::set_permissions(&path, mode).unwrap();
        let error = prepare(
            &env.store(),
            &env.lease(),
            &env.discovery.channels,
            env.board.clone(),
            1,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("pwm3_enable"), "{error}");
        assert!(!env.state.join(SNAPSHOT_FILE).exists());
    }

    #[test]
    fn fallback_counts_failed_writes() {
        let env = Env::new("fallback-fail");
        env.lease().create("boot", 1).unwrap();
        let path = env.fake.attr_path("nct6798", "pwm3");
        let mut mode = fs::metadata(&path).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o444);
        fs::set_permissions(&path, mode).unwrap();
        assert_eq!(
            restore_bios(&env.paths()).unwrap(),
            RestoreOutcome::Fallback {
                channels: 7,
                failed: 1
            }
        );
    }

    #[test]
    fn restore_bios_without_lease_changes_nothing() {
        let env = Env::new("nolease");
        let before = env.pwm_files();
        assert_eq!(restore_bios(&env.paths()).unwrap(), RestoreOutcome::NoLease);
        assert_eq!(Env::diff(&env.pwm_files(), &before), Vec::<String>::new());
        assert!(fs::read_dir(&env.state).unwrap().next().is_none());
    }

    #[test]
    fn restore_bios_without_snapshot_sets_raw_255() {
        let env = Env::new("nosnapshot");
        env.lease().create("boot", 1).unwrap();
        let before = env.pwm_files();
        assert_eq!(
            restore_bios(&env.paths()).unwrap(),
            RestoreOutcome::Fallback {
                channels: 7,
                failed: 0
            }
        );
        let after = env.pwm_files();
        let chip = env.chip_dir();
        for (path, value) in &after {
            let name = path.file_name().unwrap().to_string_lossy();
            if path.parent() == Some(chip.as_path()) {
                if name.len() == 4 {
                    assert_eq!(value, "255", "{name}");
                } else if name.ends_with("_enable") {
                    assert_eq!(value, "1", "{name}");
                } else {
                    assert_eq!(value, &before[path], "{name}");
                }
            } else {
                assert_eq!(value, &before[path], "GPU {name}");
            }
        }
        assert!(env.lease().exists());
        // The next start raises `state-fallback`.
        assert_eq!(
            env.prepare(),
            [("state-fallback".to_owned(), String::new())]
        );
    }
}
