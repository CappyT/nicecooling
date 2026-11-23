use crate::crc::crc16_usb;
use crate::error::{ReportError, check_id, check_size};

pub const REPORT2_ID: u8 = 2;
pub const REPORT2_SIZE: usize = 11;

/// Output report 2: the command and its parameter, big-endian, and a CRC-16/USB over those 8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report2 {
    pub command: u32,
    pub parameter: u32,
}

impl Report2 {
    pub fn new(command: u32, parameter: u32) -> Self {
        Self { command, parameter }
    }

    pub fn encode(&self) -> [u8; REPORT2_SIZE] {
        let mut out = [0; REPORT2_SIZE];
        out[0] = REPORT2_ID;
        out[1..5].copy_from_slice(&self.command.to_be_bytes());
        out[5..9].copy_from_slice(&self.parameter.to_be_bytes());
        let crc = crc16_usb(&out[1..9]);
        out[9..].copy_from_slice(&crc.to_be_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReportError> {
        check_size(bytes, REPORT2_SIZE)?;
        check_id(bytes, REPORT2_ID)?;
        let stored = u16::from_be_bytes([bytes[9], bytes[10]]);
        let computed = crc16_usb(&bytes[1..9]);
        if stored != computed {
            return Err(ReportError::BadCrc { stored, computed });
        }
        Ok(Self {
            command: u32::from_be_bytes(bytes[1..5].try_into().expect("4 bytes")),
            parameter: u32::from_be_bytes(bytes[5..9].try_into().expect("4 bytes")),
        })
    }
}
