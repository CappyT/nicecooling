//! The Leakshield part of a Profile activation. aquasuite writes the stored report 3 of a profile
//! only when the profile number differs from the number that the device reports. It does not
//! compare bytes and does not read the report back (`docs/protocol/leakshield.md`).

use std::fmt;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::access::Access;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;

use crate::leakshield::Attached;
use crate::leakshield::write::WriteError;
use crate::state::SlotStore;
use crate::state::slots::StoredSlot;

/// What the activation does to one Leakshield.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// The device reports the number already: nothing to write.
    Keep,
    /// Write this report 3 as it is.
    Write(Box<Report3>),
    /// The slot is marked as edited: write this report 3 with the read-back of design D11. A good
    /// read-back clears the mark.
    WriteVerified(Box<Report3>),
}

#[derive(Debug)]
pub enum Failure {
    /// Report 1 did not arrive.
    Unreachable(String),
    UnknownFirmware,
    NoSlot,
    FirmwareMismatch,
    SerialMismatch,
    Write(WriteError),
    /// A newer request replaced this one. The newer request reports the result.
    Superseded,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(e) => write!(f, "the Leakshield did not answer: {e}"),
            Self::UnknownFirmware => write!(f, "the firmware of the Leakshield is unknown"),
            Self::NoSlot => write!(f, "the slot has no stored report 3"),
            Self::FirmwareMismatch => {
                write!(f, "the stored report 3 comes from another firmware")
            }
            Self::SerialMismatch => write!(f, "the stored report 3 comes from another serial"),
            Self::Write(e) => write!(f, "{e}"),
            Self::Superseded => write!(f, "a newer Profile write replaced this one"),
        }
    }
}

/// Decides from what the daemon knows. Nothing here touches the device.
pub fn decide(
    registry: &Registry,
    serial: &str,
    report1: &Report1,
    slot: Slot,
    stored: Option<StoredSlot>,
) -> Result<Decision, Failure> {
    if report1.active_profile == slot.get() {
        return Ok(Decision::Keep);
    }
    if Access::new(registry, report1).device_reason().is_some() {
        return Err(Failure::UnknownFirmware);
    }
    let stored = stored.ok_or(Failure::NoSlot)?;
    if stored.meta.firmware != report1.firmware_version
        || stored.meta.structure != report1.structure_number
        || stored.report3.structure_number() != registry.structure
    {
        return Err(Failure::FirmwareMismatch);
    }
    if stored.meta.serial != serial {
        return Err(Failure::SerialMismatch);
    }
    if stored.meta.edited {
        return Ok(Decision::WriteVerified(Box::new(stored.report3)));
    }
    Ok(Decision::Write(Box::new(stored.report3)))
}

/// Applies `slot` to one Leakshield and waits for the end of it. It writes also while an Action
/// runs and in any Mode.
pub async fn apply_slot(
    device: &Attached,
    slots: &SlotStore,
    slot: Slot,
) -> Result<Decision, Failure> {
    device
        .finish_profile(device.request_profile(slot, slots))
        .await
}
