//! The chunks of an image and the feature report 32 frame that carries one chunk to the
//! bootloader (`docs/re/firmware-update.md`, Report 32 layout).

use std::fmt;

use super::image::Image;
use crate::crc::crc16_usb;
use crate::error::ReportError;

pub const REPORT32_ID: u8 = 0x20;
pub const CHUNK_SIZE: usize = 1024;
pub const PAD: u8 = 0xFF;
/// The buffer that goes to the HID driver, with the report ID. Task 1.5 measures the length for the
/// bootloader.
pub const FRAME_LEN: usize = 1042;

/// Bytes 1-3 of the frame, then the constant 2 (design D5). aquasuite sends both unchanged.
const HEADER_ZERO: [u8; 3] = [0; 3];
const WRITE_FLASH: u32 = 2;
const CRC_AT: usize = FRAME_LEN - 2;

#[derive(Clone, PartialEq, Eq)]
pub struct Chunk {
    pub address: u32,
    pub data: [u8; CHUNK_SIZE],
}

impl fmt::Debug for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Chunk {{ address: {:#x} }}", self.address)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    Report(ReportError),
    BadHeader,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Report(e) => e.fmt(f),
            Self::BadHeader => write!(f, "the frame header is not 00 00 00, 2, address, 1024"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<ReportError> for FrameError {
    fn from(e: ReportError) -> Self {
        Self::Report(e)
    }
}

/// Cuts each segment of the image into chunks, in address order. The last chunk of a segment is
/// padded with 0xFF.
pub fn chunks(image: &Image) -> Vec<Chunk> {
    let mut out = Vec::new();
    for segment in image.segments() {
        for (n, part) in segment.data().chunks(CHUNK_SIZE).enumerate() {
            let mut data = [PAD; CHUNK_SIZE];
            data[..part.len()].copy_from_slice(part);
            let address = u64::from(segment.adr_start()) + (n * CHUNK_SIZE) as u64;
            out.push(Chunk {
                address: u32::try_from(address).expect("parse keeps every chunk below 2^32"),
                data,
            });
        }
    }
    out
}

pub fn chunk_count(image: &Image) -> usize {
    image
        .segments()
        .iter()
        .map(|s| s.data().len().div_ceil(CHUNK_SIZE))
        .sum()
}

impl Chunk {
    pub fn frame(&self) -> [u8; FRAME_LEN] {
        let mut out = [0; FRAME_LEN];
        out[0] = REPORT32_ID;
        out[1..4].copy_from_slice(&HEADER_ZERO);
        out[4..8].copy_from_slice(&WRITE_FLASH.to_be_bytes());
        out[8..12].copy_from_slice(&self.address.to_be_bytes());
        out[12..16].copy_from_slice(&(CHUNK_SIZE as u32).to_be_bytes());
        out[16..16 + CHUNK_SIZE].copy_from_slice(&self.data);
        let crc = crc16_usb(&out[1..CRC_AT]);
        out[CRC_AT..].copy_from_slice(&crc.to_be_bytes());
        out
    }

    /// What the bootloader checks: the size, the report ID, the header and the CRC.
    pub fn parse_frame(frame: &[u8]) -> Result<Self, FrameError> {
        crate::error::check_size(frame, FRAME_LEN)?;
        crate::error::check_id(frame, REPORT32_ID)?;
        let word = |at: usize| u32::from_be_bytes(frame[at..at + 4].try_into().expect("4 bytes"));
        if frame[1..4] != HEADER_ZERO || word(4) != WRITE_FLASH || word(12) != CHUNK_SIZE as u32 {
            return Err(FrameError::BadHeader);
        }
        let stored = u16::from_be_bytes([frame[CRC_AT], frame[CRC_AT + 1]]);
        let computed = crc16_usb(&frame[1..CRC_AT]);
        if stored != computed {
            return Err(ReportError::BadCrc { stored, computed }.into());
        }
        let mut data = [0; CHUNK_SIZE];
        data.copy_from_slice(&frame[16..16 + CHUNK_SIZE]);
        Ok(Self {
            address: word(8),
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::image::tests::image_of;

    fn image(segments: &[(u32, usize)]) -> Image {
        let segments: Vec<(u32, Vec<u8>)> = segments
            .iter()
            .map(|&(adr_start, len)| (adr_start, (0..len).map(|i| (i % 251) as u8).collect()))
            .collect();
        image_of(&segments)
    }

    #[test]
    fn chunk_count_108_for_109952_bytes() {
        let image = image(&[(0x4000, 109952)]);
        let chunks = chunks(&image);
        assert_eq!(chunks.len(), 108);
        assert_eq!(chunk_count(&image), 108);
        assert_eq!(chunks[0].address, 0x4000);
        assert_eq!(chunks[107].address, 0x1EC00);
    }

    #[test]
    fn last_chunk_of_each_segment_padded_with_ff() {
        let image = image(&[(0x4000, 1500), (0x8000, 109952)]);
        let chunks = chunks(&image);
        let last_of_first = &chunks[1];
        assert_eq!(last_of_first.address, 0x4400);
        assert_eq!(
            last_of_first.data[..476],
            image.segments()[0].data()[1024..]
        );
        assert!(last_of_first.data[476..].iter().all(|&b| b == 0xFF));
        let last = chunks.last().unwrap();
        assert!(last.data[384..].iter().all(|&b| b == 0xFF));
        assert_eq!(last.data[..384], image.segments()[1].data()[107 * 1024..]);
    }

    #[test]
    fn chunk_count_is_the_sum_over_segments() {
        let image = image(&[(0x4000, 1024), (0x8000, 1025), (0x10000, 1)]);
        assert_eq!(chunk_count(&image), 1 + 2 + 1);
        assert_eq!(chunks(&image).len(), 4);
    }

    #[test]
    fn addresses_step_1024_from_adr_start() {
        let chunks = chunks(&image(&[(0x4000, 4096), (0x9000, 2048)]));
        let addresses: Vec<u32> = chunks.iter().map(|c| c.address).collect();
        assert_eq!(addresses, [0x4000, 0x4400, 0x4800, 0x4C00, 0x9000, 0x9400]);
    }

    #[test]
    fn frame_layout_bytes() {
        let chunk = &chunks(&image(&[(0x4000, 2000)]))[0];
        let frame = chunk.frame();
        assert_eq!(
            frame[..16],
            [0x20, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0x40, 0, 0, 0, 0x04, 0]
        );
        assert_eq!(frame[16..16 + CHUNK_SIZE], chunk.data);
    }

    #[test]
    fn crc_covers_bytes_after_id() {
        let frame = chunks(&image(&[(0x4000, 2000)]))[0].frame();
        let stored = u16::from_be_bytes([frame[1040], frame[1041]]);
        assert_eq!(stored, crc16_usb(&frame[1..1040]));
        let mut other_id = frame;
        other_id[0] = 0x21;
        assert_eq!(crc16_usb(&other_id[1..1040]), stored);
        let mut other_data = frame;
        other_data[1039] ^= 1;
        assert_ne!(crc16_usb(&other_data[1..1040]), stored);
    }

    #[test]
    fn frame_is_1042_bytes() {
        let chunk = &chunks(&image(&[(0x4000, 1)]))[0];
        assert_eq!(chunk.frame().len(), 1042);
        assert_eq!(Chunk::parse_frame(&chunk.frame()).as_ref(), Ok(chunk));
        assert!(Chunk::parse_frame(&chunk.frame()[..1041]).is_err());
    }
}
