//! Leakshield protocol: report codecs, the field registry, the read-only rules and the Feed
//! message. The conventions are in `docs/protocol/leakshield.md`.

pub mod access;
pub mod crc;
pub mod error;
pub mod feed;
pub mod registry;
pub mod report1;
pub mod report3;
pub mod report5;
pub mod report8;
mod wire;

pub use error::ReportError;
