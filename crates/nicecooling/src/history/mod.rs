//! The history store: `$XDG_DATA_HOME/nicecooling/history.db` (`docs/storage.md`).

pub mod db;
pub mod prune;
pub mod query;
pub mod writer;

pub use db::{HistoryDb, MinuteRow, SampleRow};
pub use writer::HistoryWriter;

/// Seconds between two write transactions.
pub const BATCH_INTERVAL_S: i64 = 10;
/// Seconds between two prunes.
pub const PRUNE_INTERVAL_S: i64 = 3600;
/// 1 s samples stay for 48 hours.
pub const SAMPLE_RETENTION_S: i64 = 48 * 3600;
/// 1 minute aggregates stay for one year.
pub const MINUTE_RETENTION_S: i64 = 365 * 24 * 3600;
