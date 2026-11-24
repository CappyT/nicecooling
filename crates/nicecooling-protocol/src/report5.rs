use crate::bitmap::Bitmap;
use crate::error::{ReportError, check_id, check_size};

pub const REPORT5_ID: u8 = 5;
pub const REPORT5_SIZE: usize = 1028;
/// Size of the display in pixels.
pub const DISPLAY_SIZE: (u16, u16) = (128, 64);

/// Display screenshot. The report has no CRC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report5 {
    /// Id of the page on the display, 255 for none.
    pub shown_page: u8,
    /// 128 x 64 monochrome: 8 rows of 128 column bytes, bit 0 at the top of each byte.
    pub image: [u8; 1024],
    /// Offsets 1026-1027, which the aquasuite code does not use.
    pub trailer: [u8; 2],
}

impl Report5 {
    pub fn decode(bytes: &[u8]) -> Result<Self, ReportError> {
        check_size(bytes, REPORT5_SIZE)?;
        check_id(bytes, REPORT5_ID)?;
        Ok(Self {
            shown_page: bytes[1],
            image: bytes[2..1026].try_into().expect("1024 bytes"),
            trailer: bytes[1026..1028].try_into().expect("2 bytes"),
        })
    }

    /// The image as pixels.
    pub fn bitmap(&self) -> Bitmap {
        Bitmap::from_page_layout(DISPLAY_SIZE.0, DISPLAY_SIZE.1, &self.image)
            .expect("the image has 1024 bytes")
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REPORT5_SIZE);
        out.push(REPORT5_ID);
        out.push(self.shown_page);
        out.extend_from_slice(&self.image);
        out.extend_from_slice(&self.trailer);
        out
    }
}
