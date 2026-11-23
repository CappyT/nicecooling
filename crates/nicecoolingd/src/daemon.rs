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
use nicecooling_protocol::registry::Registry;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::dbus::stage::ApiStage;
use crate::dbus::{BoardInfo, ChannelMeta, Hub, HubInit, SensorMeta, sensor_meta};
use crate::fleet::{Attacher, Fleet, OPEN_RETRY_DELAY, UsbAttach};
use crate::leakshield::feed::feed_channel_checked;
use crate::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use crate::leakshield::{AttachParams, Attached, DeviceIo, external};
use crate::lease::Lease;
use crate::rescan::{self, Rescan};
use crate::snapshot::{self, SnapshotBoard};
use crate::sources::hotplug::UsbPath;
use crate::sources::hwmon::{Source, chip_readers, spawn_source};
use crate::sources::leakshield::{self as leakshield_source, sensor_metas};
use crate::sources::usb::UsbBus;
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
    /// The Leakshields to attach. The caller opens them: tests pass simulated devices.
    pub leakshields: Vec<DeviceIo>,
    /// The USB bus to enumerate at the start and to watch for hot-plug. `None` attaches no USB
    /// device, so a test never reaches the real bus. `main` sets `NusbBus`.
    pub usb: Option<Arc<dyn UsbBus>>,
    /// The time between two tries to open a Leakshield whose hidraw node is not there yet.
    pub usb_retry_delay: Duration,
    /// The time between the last report 3 write and the save-settings command.
    pub save_settings_delay: Duration,
    /// The time between two reads of report 3 for the external change detection.
    pub external_period: Duration,
    /// The time between two scans for hwmon chips that load after the start.
    pub rescan_period: Duration,
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
            usb: None,
            usb_retry_delay: OPEN_RETRY_DELAY,
            save_settings_delay: SAVE_SETTINGS_DELAY,
            external_period: external::POLL_PERIOD,
            rescan_period: rescan::RESCAN_PERIOD,
        }
    }
}

pub struct Daemon {
    pub runner: TickRunner,
    pub values: ValueTable,
    /// The model behind the D-Bus API. `dbus::serve` puts it on a bus.
    pub hub: Arc<Hub>,
    /// The attached Leakshields, also those that attach after the start.
    pub fleet: Arc<Fleet>,
    sources: Vec<Source>,
    rescan: JoinHandle<()>,
    /// The Feed sender and the external change detection of each Leakshield.
    leakshield_tasks: Vec<JoinHandle<()>>,
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
    let attach = AttachParams {
        state_dir: paths.state_dir.clone(),
        registry: Arc::new(Registry::leakshield()),
        save_delay: options.save_settings_delay,
    };
    let fleet = Arc::new(Fleet::default());
    let mut present: Vec<(Option<UsbPath>, DeviceIo)> = options
        .leakshields
        .iter()
        .map(|io| (None, io.clone()))
        .collect();
    let usb = options.usb.clone().map(|bus| UsbAttach {
        bus,
        retry_delay: options.usb_retry_delay,
    });
    let (usb_events, usb_rx) = mpsc::unbounded_channel();
    if let Some(usb) = &usb {
        // The watch starts before the listing, so that no plug or unplug falls between them.
        usb.bus.watch(usb_events);
        present.extend(
            usb.open_present()
                .await
                .into_iter()
                .map(|(path, io)| (Some(path), io)),
        );
    }
    let mut leakshields: Vec<Arc<Attached>> = Vec::new();
    for (path, io) in present {
        match Attached::open(io, &attach) {
            Ok(device) if fleet.get(&device.id()).is_some() => {
                eprintln!(
                    "nicecoolingd: the Leakshield {} attaches twice",
                    device.id()
                );
            }
            Ok(device) => {
                fleet.insert(device.clone(), path);
                leakshields.push(device);
            }
            Err(e) => eprintln!("nicecoolingd: a Leakshield did not answer: {e}"),
        }
    }
    for device in &leakshields {
        sources.push(leakshield_source::spawn_source(
            device.clone(),
            values.clone(),
            clock.clone(),
            options.source_period,
        ));
    }
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
    let (rescan_names, rescan_identities) =
        (loaded.state.names.clone(), loaded.state.identities.clone());
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
            .chain(
                leakshields
                    .iter()
                    .flat_map(|device| sensor_metas(&device.id())),
            )
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
        leakshields: leakshields.clone(),
        state_health: state_health.to_owned(),
    });

    let rescan = Rescan {
        sysfs: sysfs.clone(),
        board: board.clone(),
        names: rescan_names,
        identities: rescan_identities,
        sensors: discovery.sensors.iter().map(|s| s.id.clone()).collect(),
        hub: hub.clone(),
        values: values.clone(),
        clock: clock.clone(),
        source_period: options.source_period,
        period: options.rescan_period,
    }
    .spawn();

    let io = SysfsChannels::new(&discovery.channels, lease, options.boot_id.clone())
        .with_snapshot(snapshot::read_snapshot(&store));
    let mut pipeline = Pipeline::standard(Box::new(values.clone()), Box::new(io));
    let (feed_stage, feed_link) =
        feed_channel_checked(fleet.software_lookup(), fleet.covered_lookup());
    pipeline.push(Box::new(feed_stage));
    pipeline.push(Box::new(ApiStage::new(hub.clone())));
    pipeline.push(Box::new(WatchdogStage::new(notifier)));
    let mut runner = TickRunner::new(pipeline, config, clock.clone(), options.tick_period);
    for (event, source) in warnings {
        runner.context().warnings.raise(&event, &source);
    }
    let mut leakshield_tasks = vec![tokio::spawn({
        let (devices, reporting) = (fleet.clone(), fleet.clone());
        let (table, feed_clock) = (values.clone(), clock.clone());
        feed_link.run(
            move || devices.io_map(),
            move |id, result| {
                table.publish_feed(id, result.is_ok(), feed_clock.now());
                if let Some(device) = reporting.get(id) {
                    device.feed_result(&result);
                }
            },
        )
    })];
    leakshield_tasks.extend(
        leakshields
            .iter()
            .map(|d| d.spawn_external_detection(options.external_period)),
    );
    if let Some(usb) = usb {
        leakshield_tasks.push(
            Attacher {
                fleet: fleet.clone(),
                usb,
                params: attach,
                hub: hub.clone(),
                values: values.clone(),
                clock: clock.clone(),
                source_period: options.source_period,
                external_period: options.external_period,
            }
            .spawn(usb_rx),
        );
    }
    Ok(Daemon {
        runner,
        values,
        hub,
        fleet,
        sources,
        rescan,
        leakshield_tasks,
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
        self.rescan.abort();
        for task in &self.leakshield_tasks {
            task.abort();
        }
    }
}
