//! Firmware update of the Leakshield: the image container, the report 32 frames and the
//! bootloader. The protocol is in `docs/re/firmware-update.md`.

pub mod bootloader;
pub mod frame;
pub mod image;
