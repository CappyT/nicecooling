use crate::crc::crc16_usb;
use crate::error::ReportError;

pub const FEED_SIZE: usize = 51;
pub const FEED_SLOTS: usize = 16;
pub const FEED_LEAD_BYTE: u8 = 0x04;
/// Slot value that means "no value".
pub const NO_VALUE: i16 = 0x7FFF;
pub const UNIT_SPEED_RPM: u8 = 3;
/// Liters per hour with one decimal place: the raw value 1150 is 115.0 L/h.
pub const UNIT_LITERS_PER_HOUR: u8 = 12;

const CRC_START: usize = 49;
const UNITS_START: usize = 33;

/// Feed message that carries the software sensor values to bulk endpoint 0x02: the lead byte
/// 0x04, 16 big-endian i16 values, 16 unit codes and a big-endian CRC-16/USB over the first 49 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedMessage {
    values: [i16; FEED_SLOTS],
    units: [u8; FEED_SLOTS],
}

impl Default for FeedMessage {
    fn default() -> Self {
        Self {
            values: [NO_VALUE; FEED_SLOTS],
            units: [0; FEED_SLOTS],
        }
    }
}

impl FeedMessage {
    /// Sets slot `slot` to the raw `value` in the scale of `unit`. Panics when `slot` is 16 or more.
    pub fn set(&mut self, slot: usize, value: i16, unit: u8) {
        self.values[slot] = value;
        self.units[slot] = unit;
    }

    /// Gives the slot no value and unit 0.
    pub fn clear(&mut self, slot: usize) {
        self.values[slot] = NO_VALUE;
        self.units[slot] = 0;
    }

    pub fn value(&self, slot: usize) -> i16 {
        self.values[slot]
    }

    pub fn unit(&self, slot: usize) -> u8 {
        self.units[slot]
    }

    pub fn encode(&self) -> [u8; FEED_SIZE] {
        let mut out = [0; FEED_SIZE];
        out[0] = FEED_LEAD_BYTE;
        for (i, v) in self.values.iter().enumerate() {
            out[1 + 2 * i..3 + 2 * i].copy_from_slice(&v.to_be_bytes());
        }
        out[UNITS_START..CRC_START].copy_from_slice(&self.units);
        let crc = crc16_usb(&out[..CRC_START]);
        out[CRC_START..].copy_from_slice(&crc.to_be_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReportError> {
        if bytes.len() != FEED_SIZE {
            return Err(ReportError::WrongSize {
                expected: FEED_SIZE,
                actual: bytes.len(),
            });
        }
        if bytes[0] != FEED_LEAD_BYTE {
            return Err(ReportError::WrongId {
                expected: FEED_LEAD_BYTE,
                actual: bytes[0],
            });
        }
        let stored = u16::from_be_bytes([bytes[CRC_START], bytes[CRC_START + 1]]);
        let computed = crc16_usb(&bytes[..CRC_START]);
        if stored != computed {
            return Err(ReportError::BadCrc { stored, computed });
        }
        let mut message = Self::default();
        for i in 0..FEED_SLOTS {
            message.values[i] = i16::from_be_bytes([bytes[1 + 2 * i], bytes[2 + 2 * i]]);
        }
        message
            .units
            .copy_from_slice(&bytes[UNITS_START..CRC_START]);
        Ok(message)
    }
}
