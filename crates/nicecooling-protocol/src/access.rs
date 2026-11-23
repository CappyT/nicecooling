use crate::registry::{Field, Registry};
use crate::report1::Report1;

/// Why a Leakshield is read-only. The identifiers are those of `docs/api.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnlyReason {
    UnknownFirmware,
    UnknownStructure,
    NotVerified,
    Disconnected,
}

impl ReadOnlyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownFirmware => "unknown-firmware",
            Self::UnknownStructure => "unknown-structure",
            Self::NotVerified => "not-verified",
            Self::Disconnected => "disconnected",
        }
    }
}

/// Why a write to one field is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteRefusal {
    /// The device is read-only as a whole.
    ReadOnly(ReadOnlyReason),
    /// The field is not verified for this firmware. Holds the field id.
    NotVerified(String),
    UnknownField(String),
}

/// Write permission of one device: the firmware and structure of report 1 against the registry.
#[derive(Debug, Clone, Copy)]
pub struct Access<'a> {
    registry: &'a Registry,
    firmware: u16,
    structure: u16,
}

impl<'a> Access<'a> {
    pub fn new(registry: &'a Registry, report1: &Report1) -> Self {
        Self::for_firmware(registry, report1.firmware_version, report1.structure_number)
    }

    /// The access of a report 3 that is not on the device, from the firmware and the structure
    /// that it comes from.
    pub fn for_firmware(registry: &'a Registry, firmware: u16, structure: u16) -> Self {
        Self {
            registry,
            firmware,
            structure,
        }
    }

    /// `Some` when the device is read-only whatever the field. Firmware 1019 and every other
    /// firmware that the registry does not list give `unknown-firmware`.
    pub fn device_reason(&self) -> Option<ReadOnlyReason> {
        if self.firmware != self.registry.firmware {
            Some(ReadOnlyReason::UnknownFirmware)
        } else if self.structure != self.registry.structure {
            Some(ReadOnlyReason::UnknownStructure)
        } else {
            None
        }
    }

    pub fn check_write(&self, field_id: &str) -> Result<&'a Field, WriteRefusal> {
        if let Some(reason) = self.device_reason() {
            return Err(WriteRefusal::ReadOnly(reason));
        }
        let field = self
            .registry
            .field(field_id)
            .ok_or_else(|| WriteRefusal::UnknownField(field_id.to_owned()))?;
        if field.is_writable() {
            Ok(field)
        } else {
            Err(WriteRefusal::NotVerified(field.id.clone()))
        }
    }
}
