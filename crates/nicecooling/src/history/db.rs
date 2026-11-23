use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA_VERSION: &str = "1";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta    (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS series  (id INTEGER PRIMARY KEY, sensor TEXT NOT NULL UNIQUE, unit TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS samples (series INTEGER NOT NULL, t INTEGER NOT NULL, value REAL,
                                    PRIMARY KEY (series, t)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS minutes (series INTEGER NOT NULL, t INTEGER NOT NULL,
                                    mean REAL, min REAL, max REAL, n INTEGER NOT NULL,
                                    PRIMARY KEY (series, t)) WITHOUT ROWID;
";

/// One 1 s sample. `value` is `None` when the sensor had no value.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleRow {
    pub sensor: String,
    pub unit: String,
    pub t: i64,
    pub value: Option<f64>,
}

/// One 1 minute aggregate over the samples that have a value. `n` counts them.
#[derive(Clone, Debug, PartialEq)]
pub struct MinuteRow {
    pub sensor: String,
    pub unit: String,
    pub t: i64,
    pub mean: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub n: i64,
}

pub struct HistoryDb {
    conn: Connection,
    series: HashMap<String, i64>,
}

impl HistoryDb {
    /// Open or create the database in WAL mode.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION],
        )?;
        Ok(Self {
            conn,
            series: HashMap::new(),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> rusqlite::Result<String> {
        self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
    }

    /// Write samples and minute rows in one transaction.
    pub fn write_batch(
        &mut self,
        samples: &[SampleRow],
        minutes: &[MinuteRow],
    ) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        let mut ids = std::mem::take(&mut self.series);
        {
            let mut insert_sample = tx.prepare_cached(
                "INSERT OR REPLACE INTO samples (series, t, value) VALUES (?1, ?2, ?3)",
            )?;
            for sample in samples {
                let id = series_id(&tx, &mut ids, &sample.sensor, &sample.unit)?;
                insert_sample.execute(params![id, sample.t, sample.value])?;
            }
            let mut insert_minute = tx.prepare_cached(
                "INSERT OR REPLACE INTO minutes (series, t, mean, min, max, n)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for minute in minutes {
                let id = series_id(&tx, &mut ids, &minute.sensor, &minute.unit)?;
                insert_minute.execute(params![
                    id,
                    minute.t,
                    minute.mean,
                    minute.min,
                    minute.max,
                    minute.n
                ])?;
            }
        }
        let committed = tx.commit();
        // A failed commit can leave ids for rows that were rolled back: start from the table again.
        self.series = if committed.is_ok() {
            ids
        } else {
            HashMap::new()
        };
        committed
    }

    /// The `(t, value)` rows of one sensor in `from..=to`, oldest first.
    pub fn samples(
        &self,
        sensor: &str,
        from: i64,
        to: i64,
    ) -> rusqlite::Result<Vec<(i64, Option<f64>)>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT t, value FROM samples
             WHERE series = (SELECT id FROM series WHERE sensor = ?1) AND t BETWEEN ?2 AND ?3
             ORDER BY t",
        )?;
        statement
            .query_map(params![sensor, from, to], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect()
    }

    /// The minute rows of one sensor in `from..=to`, oldest first.
    pub fn minutes(&self, sensor: &str, from: i64, to: i64) -> rusqlite::Result<Vec<MinuteRow>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT s.sensor, s.unit, m.t, m.mean, m.min, m.max, m.n
             FROM minutes m JOIN series s ON s.id = m.series
             WHERE s.sensor = ?1 AND m.t BETWEEN ?2 AND ?3 ORDER BY m.t",
        )?;
        statement
            .query_map(params![sensor, from, to], |row| {
                Ok(MinuteRow {
                    sensor: row.get(0)?,
                    unit: row.get(1)?,
                    t: row.get(2)?,
                    mean: row.get(3)?,
                    min: row.get(4)?,
                    max: row.get(5)?,
                    n: row.get(6)?,
                })
            })?
            .collect()
    }

    /// The unit of a sensor series, `None` for a sensor that was never recorded.
    pub fn series_unit(&self, sensor: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT unit FROM series WHERE sensor = ?1",
                [sensor],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn count(&self, table: &str) -> rusqlite::Result<i64> {
        let sql = match table {
            "samples" => "SELECT count(*) FROM samples",
            "minutes" => "SELECT count(*) FROM minutes",
            other => panic!("no history table {other}"),
        };
        self.conn.query_row(sql, [], |row| row.get(0))
    }
}

fn series_id(
    tx: &rusqlite::Transaction<'_>,
    ids: &mut HashMap<String, i64>,
    sensor: &str,
    unit: &str,
) -> rusqlite::Result<i64> {
    if let Some(id) = ids.get(sensor) {
        return Ok(*id);
    }
    let existing: Option<i64> = tx
        .query_row("SELECT id FROM series WHERE sensor = ?1", [sensor], |row| {
            row.get(0)
        })
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            tx.execute(
                "INSERT INTO series (sensor, unit) VALUES (?1, ?2)",
                [sensor, unit],
            )?;
            tx.last_insert_rowid()
        }
    };
    ids.insert(sensor.to_owned(), id);
    Ok(id)
}
