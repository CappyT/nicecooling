//! Mode and Action. They share one transport: report 2 command `set_mode` with a code as the
//! parameter (`docs/protocol/leakshield.md`). Code 0 is `cancel_action`.

use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::report3::REPORT3_STRUCT_END;

use super::external::LastKnown;
use super::{DeviceIo, blocking};
use crate::state::SlotStore;

/// aquasuite waits this long before it sends command 12.
pub const COMMAND_DELAY: Duration = Duration::from_millis(25);

/// The device stores a Mode in `default_mode` of report 3 some time after `set_mode` (about 20 s,
/// after a fill-level measure Action). The daemon polls report 3 this often, this many times.
pub const STORE_POLL: Duration = Duration::from_secs(1);
pub const STORE_POLLS: u32 = 60;

/// The persistent state of the Leakshield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Shield,
    Monitor,
    Release,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Shield, Self::Monitor, Self::Release];

    pub fn code(self) -> u32 {
        match self {
            Self::Shield => 1,
            Self::Monitor => 2,
            Self::Release => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shield => "shield",
            Self::Monitor => "monitor",
            Self::Release => "release",
        }
    }

    /// Lowercase only: `Shield` is not a Mode.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == text)
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() as i16 == code)
    }
}

/// `None` when the registry of the firmware has no `set_mode` command.
pub fn set_mode_command(registry: &Registry, code: u32) -> Option<Report2> {
    registry
        .command("set_mode")
        .map(|c| Report2::new(c.code, code))
}

/// `cancel_action`: command 12 with code 0. `None` when the registry lacks it.
pub fn cancel_command(registry: &Registry) -> Option<Report2> {
    registry
        .command("cancel_action")
        .map(|c| Report2::new(c.code, 0))
}

/// What follows the Mode that the daemon set.
pub struct ModeStore {
    pub io: DeviceIo,
    pub registry: Arc<Registry>,
    pub last_known: Arc<LastKnown>,
    pub slots: SlotStore,
    /// The id of the Leakshield: the name of its slot directory.
    pub leakshield: String,
}

impl ModeStore {
    /// Reads report 3. When the device holds `code` in `default_mode`, copies that byte into the
    /// last known report 3 and into the stored report 3 of the slot that the device runs, so a
    /// later activation of the slot writes the same Mode (the Mode belongs to the Profile, design
    /// D14). Returns true when the device holds `code` or there is nothing to follow.
    fn store(&self, code: u8) -> Result<bool, DeviceError> {
        let Some(offset) = self.registry.field("default_mode").map(|f| f.offset) else {
            return Ok(true);
        };
        let mut last = self.last_known.lock();
        let report3 = self.io.read_report3()?;
        if report3.bytes_at(offset, 1) != [code] {
            return Ok(false);
        }
        if let Some(known) = last.get() {
            let mut known = known.clone();
            known.set_bytes(offset, &[code]);
            if let Err(e) = last.set(known) {
                eprintln!("nicecoolingd: last known report 3 not saved: {e}");
            }
        }
        let number = report3.bytes_at(REPORT3_STRUCT_END, 1)[0];
        let Some(slot) = Slot::new(u32::from(number)) else {
            return Ok(true);
        };
        let stored = self.slots.load(&self.leakshield, slot).ok().flatten();
        if let Some(mut stored) = stored
            && stored.meta.firmware == self.registry.firmware
            && stored.meta.structure == self.registry.structure
            && stored.report3.structure_number() == self.registry.structure
        {
            // Only this byte: the rest of the slot stays as the user stored it.
            stored.report3.set_bytes(offset, &[code]);
            if let Err(e) = self.slots.store(
                &self.registry,
                &self.leakshield,
                slot,
                &stored.report3,
                &stored.meta,
            ) {
                eprintln!(
                    "nicecoolingd: the Mode of slot {slot} of {} is not saved: {e}",
                    self.leakshield
                );
            }
        }
        Ok(true)
    }
}

/// Polls report 3 until the device holds the Mode `code` (1 to 3) in `default_mode`, then stores
/// it (`ModeStore::store`). It gives up after `STORE_POLLS` polls and stores nothing.
pub async fn follow_default_mode(store: ModeStore, code: u8) {
    for _ in 0..STORE_POLLS {
        tokio::time::sleep(STORE_POLL).await;
        match blocking(|| store.store(code)) {
            Ok(true) | Err(DeviceError::Disconnected) => {
                store.last_known.end_expected_mode(code);
                return;
            }
            Ok(false) | Err(_) => {}
        }
    }
    store.last_known.end_expected_mode(code);
    eprintln!(
        "nicecoolingd: {} did not store the Mode {code} in report 3 within {} s",
        store.leakshield,
        STORE_POLL.as_secs() * u64::from(STORE_POLLS)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_are_lowercase_only() {
        for mode in Mode::ALL {
            assert_eq!(Mode::parse(mode.as_str()), Some(mode));
        }
        for text in ["Shield", "SHIELD", "", " shield", "test"] {
            assert_eq!(Mode::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn codes_follow_the_protocol() {
        assert_eq!(Mode::Shield.code(), 1);
        assert_eq!(Mode::Release.code(), 3);
        assert_eq!(Mode::from_code(2), Some(Mode::Monitor));
        assert_eq!(Mode::from_code(4), None);
    }
}
