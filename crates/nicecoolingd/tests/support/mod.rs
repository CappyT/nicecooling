//! The test daemon: the daemon library on a small fake sysfs tree (one Board, three sensors, two
//! Channels), a state with one Loop, one Leakshield object and the Default Profile, on a private
//! bus, with the test implementation of `Authorizer`.

#![allow(dead_code)]

pub mod bus;
pub mod logind_mock;
pub mod polkit_mock;
pub mod sim_usb;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use nicecooling_api::ids::{Kind, object_path};
use nicecooling_core::clock::{Clock, SimClock};
use nicecooling_hwmon::fake::FakeSysfs;
use nicecooling_protocol::sim::SimDevice;
use nicecoolingd::authz::{Authorizer, PolkitAuthorizer, TestAuthorizer};
use nicecoolingd::daemon::{self, Daemon, Options, Paths};
use nicecoolingd::dbus::{self, Served};
use nicecoolingd::leakshield::DeviceIo;
use nicecoolingd::leakshield::attached;
use nicecoolingd::leakshield::clock;
use nicecoolingd::tick_runner::{SimSuspendedTime, TokioClock};
use nicecoolingd::watchdog::Notifier;
use tokio::task::JoinHandle;
use zbus::{Connection, OwnedObjectPath};

pub use bus::PrivateBus;

pub const LEAKSHIELD: &str = "leakshield_20417_31072";

/// `set_clock`, report 2 command 37000. The daemon sends it at the attach and every 30 minutes.
pub const SET_CLOCK: u32 = 37000;

/// The commands of a simulated device without the clock sets of the daemon.
pub fn without_clock(
    commands: Vec<nicecooling_protocol::report2::Report2>,
) -> Vec<nicecooling_protocol::report2::Report2> {
    commands
        .into_iter()
        .filter(|c| c.command != SET_CLOCK)
        .collect()
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct NoPing;

impl Notifier for NoPing {
    fn ping(&mut self) {}
}

pub const STATE: &str = r#"{
  "schema_version": 1,
  "loops": [{"id": "loop_1", "name": "Main loop", "pump_channels": ["nct6798_pwm2"],
             "flow_sensor": "nct6798_fan1", "calibration_table": "alphacool_17558",
             "water_temperature": "k10temp_temp1", "leakshield": null, "pump_floor": 40}]
}"#;

pub const PROFILE: &str = r#"{
  "schema_version": 1, "id": "profile_default", "name": "Default",
  "channels": {
    "nct6798_pwm1": {"control": "curve", "input": "k10temp_temp1", "points": [[40, 20], [80, 100]],
                     "smoothing_s": 0, "step_limit_pct_per_s": 100, "fixed_duty": null}
  },
  "pump_floors": {}, "slot": 1
}"#;

/// Three sensors (`k10temp_temp1`, `nct6798_temp1`, `nct6798_fan1`) and two Channels
/// (`nct6798_pwm1`, `nct6798_pwm2`).
pub fn sim_tree() -> FakeSysfs {
    let fake = FakeSysfs::new();
    fake.add_hwmon(
        3,
        "k10temp",
        "devices/pci0000:00/0000:00:18.3",
        Some("k10temp"),
        &[("temp1_input", "61875")],
    );
    fake.add_hwmon(
        9,
        "nct6798",
        "devices/platform/nct6775.656",
        Some("nct6775"),
        &[
            ("temp1_input", "30000"),
            ("fan1_input", "1200"),
            ("pwm1", "96"),
            ("pwm1_enable", "5"),
            ("pwm2", "128"),
            ("pwm2_enable", "5"),
        ],
    );
    fake
}

pub struct Rig {
    pub bus: PrivateBus,
    pub client: Connection,
    pub server: Connection,
    pub authorizer: Arc<TestAuthorizer>,
    pub fake: FakeSysfs,
    /// The simulated Leakshield behind the object `LEAKSHIELD`.
    pub sim: SimDevice,
    pub state: PathBuf,
    /// The options of the running daemon, for a second start on the same state directory.
    pub options: Options,
    pub daemon: Option<Daemon>,
    /// The mock polkit, when the rig asks it instead of the test authorizer.
    pub polkit: Option<polkit_mock::PolkitHandles>,
    /// The mock logind that the daemon calls to power off.
    pub logind: logind_mock::Login1Handles,
    /// The clock of the tick, when the test drives it (`start_with_clock`).
    pub clock: Option<SimClock>,
    /// The time that the simulated PC was suspended, when the tick runs on a simulated clock.
    pub suspended: Option<SimSuspendedTime>,
    _served: Served,
    ticking: Option<JoinHandle<()>>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(task) = self.ticking.take() {
            task.abort();
        }
        let _ = fs::remove_dir_all(&self.state);
    }
}

impl Rig {
    pub async fn start() -> Self {
        Self::build(false, |_, _| {}).await
    }

    /// Runs `prepare` on the fake tree and the state directory before the daemon starts.
    pub async fn start_with(prepare: impl FnOnce(&FakeSysfs, &std::path::Path)) -> Self {
        Self::build(false, prepare).await
    }

    /// The daemon asks `PolkitAuthorizer`, and a mock `org.freedesktop.PolicyKit1` answers.
    pub async fn start_with_polkit() -> Self {
        Self::build(true, |_, _| {}).await
    }

    /// The daemon attaches the Leakshields of `usb` instead of a device in the options.
    pub async fn start_with_usb(usb: sim_usb::SimUsb) -> Self {
        Self::build_full(false, |_, _| {}, Some(usb), None, None).await
    }

    /// The tick runs on a simulated clock: `step` advances it and runs one tick.
    pub async fn start_with_clock() -> Self {
        Self::build_full(false, |_, _| {}, None, Some(SimClock::default()), None).await
    }

    /// Like `start_with_clock`, with `prepare` run on the fake tree and the state directory.
    pub async fn start_with_clock_and(prepare: impl FnOnce(&FakeSysfs, &std::path::Path)) -> Self {
        Self::build_full(false, prepare, None, Some(SimClock::default()), None).await
    }

    async fn build(polkit: bool, prepare: impl FnOnce(&FakeSysfs, &std::path::Path)) -> Self {
        Self::build_full(polkit, prepare, None, None, None).await
    }

    /// Stops the daemon after it wrote its state, and starts a new one on the same state
    /// directory and clock. The new daemon is not on the bus: tests read its hub.
    pub async fn restart_daemon(&mut self) {
        let old = self
            .daemon
            .take()
            .expect("the daemon is not ticking in the background");
        old.hub.flush_now();
        drop(old);
        let clock: Arc<dyn Clock> = match &self.clock {
            Some(clock) => Arc::new(clock.clone()),
            None => Arc::new(TokioClock::new()),
        };
        self.daemon = Some(daemon::start(&self.options, clock, NoPing).await.unwrap());
    }

    /// Advances the simulated clock by one second `seconds` times, with one tick after each. The
    /// pauses give the source tasks time to publish with the new time.
    pub async fn step(&mut self, seconds: u32) {
        let clock = self.clock.clone().expect("the rig has a simulated clock");
        for _ in 0..seconds {
            clock.advance(Duration::from_secs(1));
            tokio::time::sleep(Duration::from_millis(120)).await;
            self.daemon
                .as_mut()
                .expect("the daemon is not ticking in the background")
                .runner
                .tick();
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    /// The device runs `slot` from the start: report 1 `active_profile` and report 3
    /// `device_profile` agree, and the daemon reads both at its start.
    pub async fn start_on_slot(
        slot: u8,
        prepare: impl FnOnce(&FakeSysfs, &std::path::Path),
    ) -> Self {
        Self::build_full(false, prepare, None, None, Some(slot)).await
    }

    async fn build_full(
        polkit: bool,
        prepare: impl FnOnce(&FakeSysfs, &std::path::Path),
        usb: Option<sim_usb::SimUsb>,
        clock: Option<SimClock>,
        device_slot: Option<u8>,
    ) -> Self {
        let with_loop = true;
        let fake = sim_tree();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let state = std::env::temp_dir().join(format!(
            "nicecooling-api-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&state);
        fs::create_dir_all(state.join("profiles/profile_default")).unwrap();
        fs::write(state.join("profiles/profile_default/profile.json"), PROFILE).unwrap();
        if with_loop {
            fs::write(state.join("state.json"), STATE).unwrap();
        }
        prepare(&fake, &state);
        let mut options = Options::new(Paths {
            sysfs_root: fake.root().to_owned(),
            state_dir: state.clone(),
            boards_dir: root.join("data/boards"),
            calibration_dir: root.join("data/calibration"),
        });
        options.tick_period = Duration::from_millis(50);
        options.source_period = Duration::from_millis(50);
        options.rescan_period = Duration::from_millis(50);
        options.boot_id = "test-boot".into();
        let sim = SimDevice::fw1022();
        // The fixture clock is from the day of the dump. A clock far from now makes the daemon set it.
        let now = clock::device_seconds(attached::now_unix()).unwrap();
        sim.edit_report1(|r| r.device_clock = now);
        if let Some(slot) = device_slot {
            sim.set_device_slot(slot);
        }
        match usb {
            Some(usb) => {
                options.usb = Some(Arc::new(usb));
                options.usb_retry_delay = Duration::from_millis(10);
            }
            None => {
                options.leakshields =
                    vec![DeviceIo::new(Arc::new(sim.clone()), Arc::new(sim.clone()))];
            }
        }
        options.save_settings_delay = Duration::from_millis(200);
        let suspended = clock.as_ref().map(|_| SimSuspendedTime::default());
        if let Some(suspended) = &suspended {
            options.suspended = Some(Arc::new(suspended.clone()));
        }
        let tick_clock: Arc<dyn Clock> = match &clock {
            Some(clock) => Arc::new(clock.clone()),
            None => Arc::new(TokioClock::new()),
        };
        let daemon = daemon::start(&options, tick_clock, NoPing).await.unwrap();

        let bus = PrivateBus::start();
        let server = bus.connect().await;
        let client = bus.connect().await;
        let authorizer = TestAuthorizer::allow_all();
        let (polkit, asked): (Option<polkit_mock::PolkitHandles>, Arc<dyn Authorizer>) = if polkit {
            let mock = polkit_mock::PolkitHandles::start(bus.connect().await).await;
            let asked = Arc::new(PolkitAuthorizer::new(&server).await.unwrap());
            (Some(mock), asked)
        } else {
            (None, authorizer.clone())
        };
        let served = dbus::serve(&daemon.hub, &server, asked).await.unwrap();
        let logind = logind_mock::Login1Handles::start(bus.connect().await).await;
        Self {
            bus,
            client,
            server,
            authorizer,
            fake,
            sim,
            state,
            options,
            daemon: Some(daemon),
            polkit,
            logind,
            clock,
            suspended,
            _served: served,
            ticking: None,
        }
    }

    /// Ticks until the rig drops.
    pub fn tick_in_background(&mut self) {
        let mut daemon = self.daemon.take().expect("the daemon is not running yet");
        self.ticking = Some(tokio::spawn(async move {
            daemon.run(None).await;
        }));
    }

    /// Runs exactly `n` ticks.
    pub async fn ticks(&mut self, n: u64) {
        self.daemon
            .as_mut()
            .expect("the daemon ticks in the background")
            .run(Some(n))
            .await;
    }
}

pub fn path(kind: Kind, id: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(object_path(kind, id)).unwrap()
}

pub fn none_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").unwrap()
}

pub fn root() -> OwnedObjectPath {
    OwnedObjectPath::try_from(nicecooling_api::ROOT_PATH).unwrap()
}

/// The D-Bus error name of a failed call.
pub fn error_name(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        zbus::Error::FDO(e) => zbus::DBusError::name(&**e).to_string(),
        other => format!("{other:?}"),
    }
}
