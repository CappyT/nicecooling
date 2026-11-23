use rusqlite::params;

use super::db::HistoryDb;
use super::{MINUTE_RETENTION_S, SAMPLE_RETENTION_S};

/// Delete the `samples` older than 48 hours and the `minutes` older than 365 days.
/// Returns the number of deleted rows of each table.
pub fn prune(db: &mut HistoryDb, now_s: i64) -> rusqlite::Result<(usize, usize)> {
    let tx = db.connection().unchecked_transaction()?;
    let samples = tx.execute(
        "DELETE FROM samples WHERE t < ?1",
        params![now_s - SAMPLE_RETENTION_S],
    )?;
    let minutes = tx.execute(
        "DELETE FROM minutes WHERE t < ?1",
        params![now_s - MINUTE_RETENTION_S],
    )?;
    tx.commit()?;
    Ok((samples, minutes))
}
