//! Leakshield device access, write path, backups, Feed message, external change detection, the
//! clock, Mode and Action, and the D-Bus API, all on the simulated device. No test touches a real device, `/dev`, `/sys` or the USB bus.

mod access;
mod action;
mod api;
mod backup;
mod clock;
mod events;
mod external;
mod feed;
mod hotplug;
mod mode;
mod reset;
mod software_sensors;
#[path = "../support/mod.rs"]
mod support;
mod write;

use std::path::PathBuf;
use std::sync::Arc;

use nicecooling_protocol::device::{BulkTransport, HidTransport};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::sim::SimDevice;
use nicecoolingd::leakshield::backup::Backups;
use nicecoolingd::leakshield::external::LastKnown;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::{AttachParams, Attached, DeviceIo};

pub const SERIAL: &str = "20417-31072";
pub const LEAKSHIELD_ID: &str = "leakshield_20417_31072";

/// One simulated Leakshield with its state directory.
pub struct Rig {
    pub sim: SimDevice,
    pub io: DeviceIo,
    pub registry: Arc<Registry>,
    pub dir: PathBuf,
    pub last_known: Arc<LastKnown>,
}

impl Rig {
    pub fn new(name: &str) -> Self {
        Self::with_registry(name, Registry::leakshield())
    }

    pub fn with_registry(name: &str, registry: Registry) -> Self {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-ls-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sim = SimDevice::fw1022();
        let hid: Arc<dyn HidTransport> = Arc::new(sim.clone());
        let bulk: Arc<dyn BulkTransport> = Arc::new(sim.clone());
        Self {
            io: DeviceIo::new(hid, bulk),
            sim,
            registry: Arc::new(registry),
            last_known: Arc::new(LastKnown::open(&dir, LEAKSHIELD_ID)),
            dir,
        }
    }

    /// The device as the daemon attaches it. Call it inside a runtime.
    pub fn attached(&self) -> Arc<Attached> {
        Attached::open(
            self.io.clone(),
            &AttachParams {
                state_dir: self.dir.clone(),
                registry: Arc::clone(&self.registry),
                save_delay: SAVE_SETTINGS_DELAY,
            },
        )
        .unwrap()
    }

    pub fn backups(&self) -> Backups {
        Backups::new(&self.dir, SERIAL)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
