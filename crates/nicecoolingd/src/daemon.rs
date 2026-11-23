//! Startup of the daemon: discovery, state, BIOS snapshot, sources and the tick.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_core::calibration::Preset;
use nicecooling_core::clock::Clock;
use nicecooling_core::tick::Pipeline;
use nicecooling_hwmon::board::select_board_for;
use nicecooling_hwmon::discovery::discover_with;
use nicecooling_hwmon::sysfs::Sysfs;

use crate::dbus::stage::ApiStage;
use crate::dbus::{BoardInfo, ChannelMeta, Hub, HubInit, LeakshieldInfo, SensorMeta, sensor_meta};
use crate::lease::Lease;
use crate::snapshot::{self, SnapshotBoard};
use crate::sources::hwmon::{Source, chip_readers, spawn_source};
use crate::state::{self, StateStore};
use crate::tick_runner::{SysfsChannels, TickRunner};
use crate::value_table::ValueTable;
use crate::watchdog::{Notifier, WatchdogStage};

/// The directories of the daemon. Tests point them at a fake tree.
#[derive(Clone, Debug)]
pub struct Paths {
    pub sysfs_root: PathBuf,
    pub state_dir: PathBuf,
    pub boards_dir: PathBuf,
    pub calibration_dir: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            sysfs_root: "/sys".into(),
            state_dir: std::env::var_os("STATE_DIRECTORY")
                .and_then(|d| {
                    d.to_str()
                        .and_then(|d| d.split(':').next())
                        .map(PathBuf::from)
                })
                .unwrap_or_else(|| "/var/lib/nicecooling".into()),
            boards_dir: "/usr/share/nicecooling/boards".into(),
            calibration_dir: "/usr/share/nicecooling/calibration".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub paths: Paths,
    pub tick_period: Duration,
    pub source_period: Duration,
    /// From `/proc/sys/kernel/random/boot_id`.
    pub boot_id: String,
    /// The Leakshield objects of the API, until the device access fills them in (tasks 5.x).
    pub leakshields: Vec<LeakshieldInfo>,
}

impl Options {
    pub fn new(paths: Paths) -> Self {
        let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        Self {
            paths,
            tick_period: Duration::from_secs(1),
            source_period: Duration::from_secs(1),
            boot_id,
            leakshields: Vec::new(),
        }
    }
}

pub struct Daemon {
    pub runner: TickRunner,
    pub values: ValueTable,
    /// The model behind the D-Bus API. `dbus::serve` puts it on a bus.
    pub hub: Arc<Hub>,
    sources: Vec<Source>,
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

/// Discovers the hardware, loads the state, takes the BIOS snapshot when no lease exists,
/// starts the source tasks and builds the tick. No Channel changes before the first tick.
pub async fn start<N: Notifier + 'static>(
    options: &Options,
    clock: Arc<dyn Clock>,
    notifier: N,
) -> io::Result<Daemon> {
    let paths = &options.paths;
    let sysfs = Sysfs::new(&paths.sysfs_root);
    let store = StateStore::new(&paths.state_dir);
    std::fs::create_dir_all(&paths.state_dir)?;

    let board = select_board_for(&sysfs, &paths.boards_dir).unwrap_or_else(|e| {
        eprintln!("nicecoolingd: {e}");
        None
    });
    let mut loaded = state::load_state(&store);
    let known = state::known_prefixes(&loaded.state.identities);
    let discovery = discover_with(&sysfs, board.as_ref(), &loaded.state.names, &known)?;
    let defaults = discover_with(&sysfs, board.as_ref(), &BTreeMap::new(), &known)?;
    // The ids of a chip stay the same when a chip with the same hwmon name appears later.
    let identities = state::identities_of(&discovery);
    if identities
        .iter()
        .any(|(id, identity)| loaded.state.identities.get(id) != Some(identity))
    {
        loaded.state.identities.extend(identities);
        if !loaded.fallback {
            store.save(state::STATE_FILE, &loaded.state)?;
        }
    }

    let presets: Vec<Preset> = std::fs::read_dir(&paths.calibration_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|text| Preset::from_toml(&text).ok())
        .collect();
    let mut problems = Vec::new();
    let config = state::tick_config(&loaded.state, &loaded.profile, &presets, &mut problems);
    for problem in problems {
        eprintln!("nicecoolingd: {problem}");
    }

    let lease = Lease::new(&paths.state_dir);
    let snapshot_board = SnapshotBoard {
        vendor: sysfs.dmi("board_vendor"),
        model: sysfs.dmi("board_name"),
    };
    let mut warnings = snapshot::prepare(
        &store,
        &lease,
        &discovery.channels,
        snapshot_board,
        now_us(),
    )?;
    if loaded.fallback {
        warnings.push(("state-fallback".to_owned(), String::new()));
    }

    let values = ValueTable::default();
    let mut sources = Vec::new();
    for reader in chip_readers(&discovery) {
        sources.push(spawn_source(
            reader,
            values.clone(),
            clock.clone(),
            options.source_period,
        ));
    }
    // Let each source publish once, so that the first tick does not see every Input as lost.
    for source in &mut sources {
        let _ = tokio::time::timeout(Duration::from_secs(1), &mut source.primed).await;
    }

    let state_health = if !loaded.fallback {
        "ok"
    } else if loaded.state == state::DaemonState::default() && loaded.profile.channels.is_empty() {
        "empty"
    } else {
        "last-known-good"
    };
    let hub = Hub::new(HubInit {
        store: StateStore::new(&paths.state_dir),
        sysfs_root: paths.sysfs_root.clone(),
        state: loaded.state,
        profile: loaded.profile,
        presets,
        board: BoardInfo {
            vendor: sysfs.dmi("board_vendor"),
            model: sysfs.dmi("board_name"),
            board_file: board.as_ref().map(|b| b.id.clone()).unwrap_or_default(),
        },
        sensors: defaults
            .sensors
            .iter()
            .map(sensor_meta)
            .collect::<Vec<SensorMeta>>(),
        channels: defaults
            .channels
            .iter()
            .map(|c| ChannelMeta {
                id: c.id.clone(),
                default_name: c.name.clone(),
                device: c.device.clone(),
                number: c.number,
                dir: c.dir.clone(),
            })
            .collect(),
        leakshields: options.leakshields.clone(),
        state_health: state_health.to_owned(),
    });

    let io = SysfsChannels::new(&discovery.channels, lease, options.boot_id.clone())
        .with_snapshot(snapshot::read_snapshot(&store));
    let mut pipeline = Pipeline::standard(Box::new(values.clone()), Box::new(io));
    pipeline.push(Box::new(ApiStage::new(hub.clone())));
    pipeline.push(Box::new(WatchdogStage::new(notifier)));
    let mut runner = TickRunner::new(pipeline, config, clock, options.tick_period);
    for (event, source) in warnings {
        runner.context().warnings.raise(&event, &source);
    }
    Ok(Daemon {
        runner,
        values,
        hub,
        sources,
    })
}

impl Daemon {
    pub async fn run(&mut self, limit: Option<u64>) {
        self.runner.run(limit).await;
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        for source in &self.sources {
            source.task.abort();
        }
    }
}
