use crate::crc::crc16_usb;
use crate::error::{ReportError, check_id, check_size};

pub const REPORT8_ID: u8 = 8;
pub const REPORT8_SIZE: usize = 3091;
pub const NAME_COUNT: usize = 40;
pub const NAME_SIZE: usize = 24;
pub const ICON_SLOTS: usize = 16;
pub const ICON_SIZE: usize = 69;

const NAMES_START: usize = 1;
const IMAGE_START: usize = NAMES_START + NAME_COUNT * NAME_SIZE;
const ICONS_START: usize = IMAGE_START + 1024;
const CRC_START: usize = ICONS_START + ICON_SLOTS * ICON_SIZE;

/// Display assets. The CRC is computed on demand: CRC-16/USB over offsets 1 to 3088.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report8 {
    /// Zero-terminated texts. 0-19 sensors, 20-34 pages, 35-38 LED controllers, 39 LED strip.
    pub names: [[u8; NAME_SIZE]; NAME_COUNT],
    /// 128 x 64 monochrome, same format as report 5.
    pub start_image: [u8; 1024],
    pub icons: [[u8; ICON_SIZE]; ICON_SLOTS],
}

impl Report8 {
    pub fn decode(bytes: &[u8]) -> Result<Self, ReportError> {
        check_size(bytes, REPORT8_SIZE)?;
        check_id(bytes, REPORT8_ID)?;
        let stored = u16::from_be_bytes([bytes[CRC_START], bytes[CRC_START + 1]]);
        let computed = crc16_usb(&bytes[1..CRC_START]);
        if stored != computed {
            return Err(ReportError::BadCrc { stored, computed });
        }
        let mut names = [[0; NAME_SIZE]; NAME_COUNT];
        for (name, chunk) in names
            .iter_mut()
            .zip(bytes[NAMES_START..IMAGE_START].chunks(NAME_SIZE))
        {
            name.copy_from_slice(chunk);
        }
        let mut icons = [[0; ICON_SIZE]; ICON_SLOTS];
        for (icon, chunk) in icons
            .iter_mut()
            .zip(bytes[ICONS_START..CRC_START].chunks(ICON_SIZE))
        {
            icon.copy_from_slice(chunk);
        }
        Ok(Self {
            names,
            start_image: bytes[IMAGE_START..ICONS_START]
                .try_into()
                .expect("1024 bytes"),
            icons,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REPORT8_SIZE);
        out.push(REPORT8_ID);
        self.names.iter().for_each(|n| out.extend_from_slice(n));
        out.extend_from_slice(&self.start_image);
        self.icons.iter().for_each(|i| out.extend_from_slice(i));
        let crc = crc16_usb(&out[1..]);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }
}
