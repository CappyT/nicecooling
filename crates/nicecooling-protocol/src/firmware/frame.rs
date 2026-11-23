//! The chunks of an image and the feature report 32 frame that carries one chunk to the
//! bootloader (`docs/re/firmware-update.md`, Report 32 layout).

pub const REPORT32_ID: u8 = 0x20;
pub const CHUNK_SIZE: usize = 1024;
pub const PAD: u8 = 0xFF;
/// The buffer that goes to the HID driver, with the report ID. Task 1.5 measures the length for the
/// bootloader.
pub const FRAME_LEN: usize = 1042;
