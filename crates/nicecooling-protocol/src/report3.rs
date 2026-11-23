use crate::crc::crc16_usb;
use crate::error::{ReportError, check_id, check_size};

pub const REPORT3_ID: u8 = 3;
pub const REPORT3_SIZE: usize = 750;
/// Last offset of the 747 struct bytes. The CRC follows at 748-749.
pub const REPORT3_STRUCT_END: usize = 747;

/// Settings report. The registry gives meaning to the bytes. `set_bytes` refreshes the CRC, so the
/// report always holds a valid CRC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report3 {
    bytes: Vec<u8>,
}

impl Report3 {
    pub fn decode(bytes: &[u8]) -> Result<Self, ReportError> {
        check_size(bytes, REPORT3_SIZE)?;
        check_id(bytes, REPORT3_ID)?;
        let report = Self {
            bytes: bytes.to_vec(),
        };
        let stored = u16::from_be_bytes([bytes[748], bytes[749]]);
        let computed = report.crc();
        if stored == computed {
            Ok(report)
        } else {
            Err(ReportError::BadCrc { stored, computed })
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// CRC-16/USB over offsets 1 to 747, computed from the bytes.
    pub fn crc(&self) -> u16 {
        crc16_usb(&self.bytes[1..=REPORT3_STRUCT_END])
    }

    pub fn structure_number(&self) -> u16 {
        u16::from_be_bytes([self.bytes[1], self.bytes[2]])
    }

    /// Bytes at `offset`, which counts from the report ID byte.
    pub fn bytes_at(&self, offset: usize, len: usize) -> &[u8] {
        &self.bytes[offset..offset + len]
    }

    /// Struct offsets (1 to 747) where the two reports differ and `volatile_mask` does not list
    /// the offset. The CRC bytes follow from the struct bytes, so they are not compared.
    pub fn differing_offsets(&self, other: &Self, volatile_mask: &[usize]) -> Vec<usize> {
        (1..=REPORT3_STRUCT_END)
            .filter(|&o| self.bytes[o] != other.bytes[o] && !volatile_mask.contains(&o))
            .collect()
    }

    /// Overwrites struct bytes and the CRC. Panics when the range leaves offsets 1 to 747.
    pub fn set_bytes(&mut self, offset: usize, value: &[u8]) {
        assert!(offset >= 1 && offset + value.len() <= REPORT3_STRUCT_END + 1);
        self.bytes[offset..offset + value.len()].copy_from_slice(value);
        let crc = self.crc().to_be_bytes();
        self.bytes[748..750].copy_from_slice(&crc);
    }
}
