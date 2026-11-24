use std::fmt;

use crate::bitmap::{Bitmap, BitmapError};
use crate::crc::crc16_usb;
use crate::error::{ReportError, check_id, check_size};

pub const REPORT8_ID: u8 = 8;
pub const REPORT8_SIZE: usize = 3091;
pub const NAME_COUNT: usize = 40;
pub const NAME_SIZE: usize = 24;
pub const ICON_SLOTS: usize = 16;
pub const ICON_SIZE: usize = 69;

/// Longest text of a name in bytes: the 24th byte stays 0.
pub const NAME_MAX_BYTES: usize = NAME_SIZE - 1;
/// Pixels of an icon and of the start image (the logo).
pub const ICON_PIXELS: (u16, u16) = (23, 23);
pub const LOGO_PIXELS: (u16, u16) = (128, 64);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// No such name or icon.
    Index {
        index: usize,
        count: usize,
    },
    /// The text has a character that Windows-1252 lacks, a control character, or too many bytes.
    Text(String),
    Bitmap(BitmapError),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Index { index, count } => write!(f, "index {index} is not below {count}"),
            Self::Text(why) => write!(f, "{why}"),
            Self::Bitmap(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AssetError {}

impl From<BitmapError> for AssetError {
    fn from(e: BitmapError) -> Self {
        Self::Bitmap(e)
    }
}

/// The characters of Windows-1252 at the bytes 0x80 to 0x9F. `None`: the byte has no character.
const CP1252_HIGH: [Option<char>; 32] = [
    Some('\u{20ac}'),
    None,
    Some('\u{201a}'),
    Some('\u{0192}'),
    Some('\u{201e}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02c6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017d}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201c}'),
    Some('\u{201d}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02dc}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203a}'),
    Some('\u{0153}'),
    None,
    Some('\u{017e}'),
    Some('\u{0178}'),
];

fn cp1252_char(byte: u8) -> char {
    match byte {
        0x80..=0x9f => CP1252_HIGH[usize::from(byte - 0x80)].unwrap_or('\u{fffd}'),
        other => char::from(other),
    }
}

fn cp1252_byte(c: char) -> Option<u8> {
    match c {
        '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => Some(c as u8),
        _ => CP1252_HIGH
            .iter()
            .position(|&high| high == Some(c))
            .map(|i| 0x80 + i as u8),
    }
}

impl Report8 {
    /// A report with empty names, a dark logo and dark icons.
    pub fn blank() -> Self {
        Self {
            names: [[0; NAME_SIZE]; NAME_COUNT],
            start_image: [0; 1024],
            icons: [[0; ICON_SIZE]; ICON_SLOTS],
        }
    }

    /// The text of name `index`: the bytes up to the first 0, as Windows-1252, without trailing
    /// spaces. An index outside the 40 names gives `None`.
    pub fn name(&self, index: usize) -> Option<String> {
        let name = self.names.get(index)?;
        let end = name.iter().position(|&b| b == 0).unwrap_or(NAME_SIZE);
        let text: String = name[..end].iter().map(|&b| cp1252_char(b)).collect();
        Some(text.trim_end_matches(' ').to_owned())
    }

    /// Sets name `index`. The text is Windows-1252 with printable characters, at most 23 bytes.
    /// The rest of the 24 bytes is 0.
    pub fn set_name(&mut self, index: usize, text: &str) -> Result<(), AssetError> {
        if index >= NAME_COUNT {
            return Err(AssetError::Index {
                index,
                count: NAME_COUNT,
            });
        }
        let mut bytes = Vec::with_capacity(text.len());
        for c in text.chars() {
            bytes.push(cp1252_byte(c).ok_or_else(|| {
                AssetError::Text(format!("{c:?} is not a printable Windows-1252 character"))
            })?);
        }
        if bytes.len() > NAME_MAX_BYTES {
            return Err(AssetError::Text(format!(
                "the text has {} bytes, {NAME_MAX_BYTES} at most",
                bytes.len()
            )));
        }
        self.names[index] = [0; NAME_SIZE];
        self.names[index][..bytes.len()].copy_from_slice(&bytes);
        Ok(())
    }

    /// Icon `index`, 23 x 23 pixels.
    pub fn icon(&self, index: usize) -> Option<Bitmap> {
        let icon = self.icons.get(index)?;
        Some(
            Bitmap::from_column_layout(ICON_PIXELS.0, ICON_PIXELS.1, icon)
                .expect("an icon has 69 bytes"),
        )
    }

    /// Sets icon `index`. The image must have 23 x 23 pixels.
    pub fn set_icon(&mut self, index: usize, image: &Bitmap) -> Result<(), AssetError> {
        if index >= ICON_SLOTS {
            return Err(AssetError::Index {
                index,
                count: ICON_SLOTS,
            });
        }
        image.expect_size(ICON_PIXELS)?;
        self.icons[index].copy_from_slice(&image.to_column_layout());
        Ok(())
    }

    /// The start image, 128 x 64 pixels.
    pub fn logo(&self) -> Bitmap {
        Bitmap::from_column_layout(LOGO_PIXELS.0, LOGO_PIXELS.1, &self.start_image)
            .expect("the start image has 1024 bytes")
    }

    /// Sets the start image. The image must have 128 x 64 pixels.
    pub fn set_logo(&mut self, image: &Bitmap) -> Result<(), AssetError> {
        image.expect_size(LOGO_PIXELS)?;
        self.start_image.copy_from_slice(&image.to_column_layout());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FW1022: &[u8] = include_bytes!("../../../tests/fixtures/leakshield/fw1022/report8.bin");

    #[test]
    fn the_fixture_decodes_into_names_icons_and_logo() {
        let report = Report8::decode(FW1022).unwrap();
        assert_eq!(report.name(0).as_deref(), Some("Pressure"));
        assert_eq!(report.name(11).as_deref(), Some("Core (Tctl/Tdie)"));
        assert_eq!(report.name(20).as_deref(), Some("Name 1"));
        assert_eq!(report.name(39).as_deref(), Some("Strip"));
        assert_eq!(report.name(40), None);
        assert_eq!(report.icon(0).unwrap().size(), (23, 23));
        assert!(report.icon(16).is_none());
        let logo = report.logo();
        assert_eq!(logo.size(), (128, 64));
        // 2262 of the 8192 pixels are lit, and the top line is dark.
        let lit = (0..64)
            .flat_map(|y| (0..128).map(move |x| (x, y)))
            .filter(|&(x, y)| logo.get(x, y))
            .count();
        assert_eq!(lit, 2262);
        assert!((0..128).all(|x| !logo.get(x, 0)));
    }

    #[test]
    fn the_fixture_survives_a_decode_encode_round_trip() {
        assert_eq!(Report8::decode(FW1022).unwrap().encode(), FW1022);
    }

    #[test]
    fn a_name_is_windows_1252_with_zero_padding() {
        let mut report = Report8::decode(FW1022).unwrap();
        report.set_name(21, "Pump \u{20ac}\u{e9}").unwrap();
        assert_eq!(report.names[21][..8], *b"Pump \x80\xe9\0");
        assert!(report.names[21][7..].iter().all(|&b| b == 0));
        assert_eq!(report.name(21).as_deref(), Some("Pump \u{20ac}\u{e9}"));
        // Trailing spaces are not part of the text.
        report.set_name(21, "Pump  ").unwrap();
        assert_eq!(report.name(21).as_deref(), Some("Pump"));
        // The other names did not change.
        let fixture = Report8::decode(FW1022).unwrap();
        assert_eq!(report.names[20], fixture.names[20]);
        assert_eq!(report.names[22], fixture.names[22]);
    }

    #[test]
    fn a_text_that_does_not_fit_is_refused() {
        let mut report = Report8::blank();
        assert!(report.set_name(0, &"a".repeat(23)).is_ok());
        assert!(matches!(
            report.set_name(0, &"a".repeat(24)),
            Err(AssetError::Text(_))
        ));
        assert!(report.set_name(0, "tab\there").is_err());
        assert!(report.set_name(0, "\u{65e5}").is_err());
        assert!(report.set_name(0, "nul\0").is_err());
        assert!(matches!(
            report.set_name(40, "x"),
            Err(AssetError::Index { .. })
        ));
    }

    #[test]
    fn icon_and_logo_use_the_column_layout() {
        let mut report = Report8::blank();
        let mut icon = Bitmap::new(23, 23);
        icon.set(22, 22, true);
        icon.set(0, 0, true);
        report.set_icon(3, &icon).unwrap();
        // 3 bytes per column: pixel (22, 22) is bit 6 of byte 2 of column 22.
        assert_eq!(report.icons[3][0], 1);
        assert_eq!(report.icons[3][22 * 3 + 2], 1 << 6);
        assert_eq!(report.icon(3).unwrap(), icon);

        let mut logo = Bitmap::new(128, 64);
        logo.set(127, 63, true);
        report.set_logo(&logo).unwrap();
        assert_eq!(report.start_image[127 * 8 + 7], 0x80);
        assert_eq!(report.logo(), logo);

        assert!(report.set_icon(0, &Bitmap::new(24, 23)).is_err());
        assert!(report.set_icon(16, &icon).is_err());
        assert!(report.set_logo(&Bitmap::new(23, 23)).is_err());
    }
}
