use std::path::PathBuf;

use nicecooling::history::{
    HistoryDb, HistoryWriter, MINUTE_RETENTION_S, MinuteRow, SAMPLE_RETENTION_S, SampleRow,
};

/// A Unix time on a minute boundary: 1_800_000_000 is a multiple of 60.
const T0: i64 = 1_800_000_000;

fn writer() -> HistoryWriter {
    HistoryWriter::new(HistoryDb::open_in_memory().expect("in-memory database"))
}

/// Run `seconds` of simulated time. Each second records `value(sensor, t)` for every sensor.
fn simulate(
    writer: &mut HistoryWriter,
    start: i64,
    seconds: i64,
    sensors: &[&str],
    value: impl Fn(&str, i64) -> Option<f64>,
) {
    for t in start..start + seconds {
        for sensor in sensors {
            writer.record(t, sensor, "degC", value(sensor, t));
        }
        writer.tick(t).expect("tick");
    }
}

#[test]
fn one_transaction_every_10s() {
    let mut writer = writer();
    simulate(&mut writer, T0, 60, &["a", "b", "c"], |_, t| Some(t as f64));
    // The first tick writes the first sample, then one batch per 10 s: 0, 10, 20, 30, 40, 50.
    assert_eq!(writer.transactions(), 6);
    // 60 s x 3 sensors, minus the 9 s of samples still in the buffer.
    let stored = writer.db().count("samples").unwrap();
    assert_eq!(stored, 51 * 3);
    // A batch holds 10 s of samples: no tick between two flushes writes.
    let mut writer = self::writer();
    simulate(&mut writer, T0, 9, &["a"], |_, _| Some(1.0));
    assert_eq!(writer.transactions(), 1, "9 s write only the first batch");
    assert_eq!(writer.db().count("samples").unwrap(), 1);
}

#[test]
fn minute_aggregates_mean_min_max_n() {
    let mut writer = writer();
    // Values 0..59 in the minute T0, a no-value sample at second 7, then the minute T0 + 60.
    simulate(&mut writer, T0, 125, &["temp"], |_, t| {
        let second = t - T0;
        if second == 7 {
            None
        } else {
            Some((second % 60) as f64)
        }
    });
    let minutes = writer.db().minutes("temp", T0, T0 + 120).unwrap();
    assert_eq!(minutes.len(), 2, "the two complete minutes are written");
    let first = &minutes[0];
    assert_eq!(first.t, T0);
    assert_eq!(first.n, 59, "the no-value sample is not counted");
    assert_eq!(first.min, Some(0.0));
    assert_eq!(first.max, Some(59.0));
    let expected_mean = (0..60).filter(|v| *v != 7).sum::<i64>() as f64 / 59.0;
    assert!((first.mean.unwrap() - expected_mean).abs() < 1e-9);
    assert_eq!(minutes[1].t, T0 + 60);
    assert_eq!(minutes[1].n, 60);
    // The no-value sample is a NULL row, not a missing row.
    let row = writer.db().samples("temp", T0 + 7, T0 + 7).unwrap();
    assert_eq!(row, vec![(T0 + 7, None)]);
}

#[test]
fn minute_without_any_value_has_null_aggregates() {
    let mut writer = writer();
    simulate(&mut writer, T0, 70, &["dead"], |_, _| None);
    let minutes = writer.db().minutes("dead", T0, T0).unwrap();
    assert_eq!(
        minutes,
        vec![MinuteRow {
            sensor: "dead".into(),
            unit: "degC".into(),
            t: T0,
            mean: None,
            min: None,
            max: None,
            n: 0,
        }]
    );
}

#[test]
fn hourly_prune_48h_samples_365d_minutes() {
    let mut db = HistoryDb::open_in_memory().unwrap();
    let now = T0 + 400 * 24 * 3600;
    let sample = |t| SampleRow {
        sensor: "s".into(),
        unit: "degC".into(),
        t,
        value: Some(1.0),
    };
    let minute = |t| MinuteRow {
        sensor: "s".into(),
        unit: "degC".into(),
        t,
        mean: Some(1.0),
        min: Some(1.0),
        max: Some(1.0),
        n: 60,
    };
    db.write_batch(
        &[
            sample(now - SAMPLE_RETENTION_S - 1),
            sample(now - SAMPLE_RETENTION_S),
            sample(now - 60),
        ],
        &[
            minute(now - MINUTE_RETENTION_S - 60),
            minute(now - MINUTE_RETENTION_S),
            minute(now - 3600),
        ],
    )
    .unwrap();
    let mut writer = HistoryWriter::new(db);
    // The prune runs on the first tick, then once per hour.
    writer.tick(now).unwrap();
    let samples = writer.db().samples("s", 0, i64::MAX).unwrap();
    assert_eq!(
        samples.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![now - SAMPLE_RETENTION_S, now - 60]
    );
    let minutes = writer.db().minutes("s", 0, i64::MAX).unwrap();
    assert_eq!(
        minutes.iter().map(|row| row.t).collect::<Vec<_>>(),
        vec![now - MINUTE_RETENTION_S, now - 3600]
    );
    // 59 minutes later nothing prunes; at one hour the sample at the cutoff goes.
    writer.tick(now + 59 * 60).unwrap();
    assert_eq!(writer.db().count("samples").unwrap(), 2);
    writer.tick(now + 3600).unwrap();
    assert_eq!(writer.db().count("samples").unwrap(), 1);
}

#[test]
fn gap_is_a_missing_row() {
    let mut writer = writer();
    simulate(&mut writer, T0, 20, &["s"], |_, _| Some(1.0));
    // The daemon is offline from T0 + 20 to T0 + 49: the app records nothing.
    simulate(&mut writer, T0 + 50, 20, &["s"], |_, _| Some(2.0));
    writer.tick(T0 + 200).unwrap();
    let rows = writer.db().samples("s", T0, T0 + 70).unwrap();
    let gap: Vec<_> = rows
        .iter()
        .filter(|(t, _)| (T0 + 20..T0 + 50).contains(t))
        .collect();
    assert!(gap.is_empty(), "no row in the gap, found {gap:?}");
    assert!(
        rows.iter().all(|(_, value)| value.is_some()),
        "no NULL fill"
    );
    assert_eq!(rows.len(), 40);
    // The minute row counts the samples that exist: seconds 0-19 and 50-59.
    let minutes = writer.db().minutes("s", T0, T0).unwrap();
    assert_eq!(minutes[0].n, 30);
}

#[test]
fn open_creates_a_wal_database_with_the_schema() {
    let dir = scratch_dir("schema");
    let path = dir.join("nicecooling/history.db");
    let db = HistoryDb::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), "1");
    let mode: String = db
        .connection()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    drop(db);
    // A second open keeps the data and the version.
    let db = HistoryDb::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), "1");
    std::fs::remove_dir_all(dir).unwrap();
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("nicecooling-history-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 48 h of 60 sensors at 1 Hz. It writes about 10 million rows, so it runs on request:
/// `cargo test --release -p nicecooling --test history -- --ignored --nocapture`.
#[test]
#[ignore = "writes a 250 MB database"]
fn history_size_48h_60_sensors() {
    let dir = scratch_dir("size");
    let path = dir.join("history.db");
    let mut writer = HistoryWriter::new(HistoryDb::open(&path).unwrap());
    let sensors: Vec<String> = (0..60)
        .map(|n| format!("chip{}_temp{}", n / 6, n % 6))
        .collect();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut noise = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let seconds = 48 * 3600;
    for t in T0..T0 + seconds {
        for sensor in &sensors {
            writer.record(t, sensor, "degC", Some(20.0 + 40.0 * noise()));
        }
        writer.tick(t).unwrap();
    }
    writer.tick(T0 + seconds).unwrap();
    assert_eq!(writer.db().count("samples").unwrap(), seconds * 60);
    drop(writer);
    let size = |name: &str| std::fs::metadata(dir.join(name)).map_or(0, |meta| meta.len());
    let total = size("history.db") + size("history.db-wal") + size("history.db-shm");
    println!(
        "history size after 48 h of 60 sensors: {} MB",
        total / 1_000_000
    );
    assert!(total <= 300_000_000, "database is {total} bytes");
    std::fs::remove_dir_all(dir).unwrap();
}
