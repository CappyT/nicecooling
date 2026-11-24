//! Leakshield protocol: report codecs, the field registry, the read-only rules and the Feed
//! message. The conventions are in `docs/protocol/leakshield.md`.

pub mod access;
pub mod bitmap;
pub mod crc;
pub mod device;
pub mod display;
pub mod error;
pub mod feed;
pub mod firmware;
pub mod lighting;
pub mod registry;
pub mod report1;
pub mod report2;
pub mod report3;
pub mod report5;
pub mod report8;
#[cfg(feature = "sim")]
pub mod sim;
mod wire;

pub use error::ReportError;
