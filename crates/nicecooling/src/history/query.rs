//! Read the history for the chart: up to 48 hours the 1 s samples, older the 1 minute
//! aggregates. A gap is a time with no row, or a row with no value.

use super::SAMPLE_RETENTION_S;
use super::db::HistoryDb;

/// Two samples that are further apart than this have a gap between them.
pub const SAMPLE_GAP_S: i64 = 10;
/// Two minute rows that are further apart than this have a gap between them.
pub const MINUTE_GAP_S: i64 = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// The 1 s samples.
    Samples,
    /// The 1 minute aggregates: the line is the mean, the band is min to max.
    Minutes,
}

impl Resolution {
    /// The resolution of a window: samples while the window starts within the 48 hours.
    pub fn of_window(from: i64, now: i64) -> Self {
        if from >= now - SAMPLE_RETENTION_S {
            Self::Samples
        } else {
            Self::Minutes
        }
    }

    pub fn gap_s(self) -> i64 {
        match self {
            Self::Samples => SAMPLE_GAP_S,
            Self::Minutes => MINUTE_GAP_S,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPoint {
    pub t: i64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

/// One sensor in one window. A segment is a run of points without a gap.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesData {
    pub sensor: String,
    pub unit: String,
    pub resolution: Resolution,
    pub segments: Vec<Vec<ChartPoint>>,
}

impl SeriesData {
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn points(&self) -> impl Iterator<Item = &ChartPoint> {
        self.segments.iter().flatten()
    }
}

/// Load one sensor for `from..=to` (Unix seconds).
pub fn load_series(
    db: &HistoryDb,
    sensor: &str,
    from: i64,
    to: i64,
    now: i64,
) -> rusqlite::Result<SeriesData> {
    let resolution = Resolution::of_window(from, now);
    let unit = db.series_unit(sensor)?.unwrap_or_default();
    let rows: Vec<(i64, Option<ChartPoint>)> = match resolution {
        Resolution::Samples => db
            .samples(sensor, from, to)?
            .into_iter()
            .map(|(t, value)| {
                (
                    t,
                    value.map(|value| ChartPoint {
                        t,
                        mean: value,
                        min: value,
                        max: value,
                    }),
                )
            })
            .collect(),
        Resolution::Minutes => db
            .minutes(sensor, from, to)?
            .into_iter()
            .map(|row| {
                let point = row
                    .mean
                    .zip(row.min)
                    .zip(row.max)
                    .map(|((mean, min), max)| ChartPoint {
                        t: row.t + 30,
                        mean,
                        min,
                        max,
                    });
                (row.t, point)
            })
            .collect(),
    };

    let gap = resolution.gap_s();
    let mut segments: Vec<Vec<ChartPoint>> = Vec::new();
    let mut last_t: Option<i64> = None;
    let mut current: Vec<ChartPoint> = Vec::new();
    for (t, point) in rows {
        let broken = last_t.is_some_and(|last| t - last > gap);
        if (broken || point.is_none()) && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
        last_t = Some(t);
        if let Some(point) = point {
            current.push(point);
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    Ok(SeriesData {
        sensor: sensor.to_owned(),
        unit,
        resolution,
        segments,
    })
}

/// Reduce a segment to `max_points` or fewer: each bucket keeps its mean, its minimum and its
/// maximum. A chart is not wider than a few thousand pixels, so this keeps it fast.
pub fn downsample(segment: &[ChartPoint], max_points: usize) -> Vec<ChartPoint> {
    if max_points < 2 || segment.len() <= max_points {
        return segment.to_vec();
    }
    let size = segment.len().div_ceil(max_points);
    segment
        .chunks(size)
        .map(|chunk| {
            let n = chunk.len() as f64;
            ChartPoint {
                t: chunk[chunk.len() / 2].t,
                mean: chunk.iter().map(|p| p.mean).sum::<f64>() / n,
                min: chunk.iter().map(|p| p.min).fold(f64::INFINITY, f64::min),
                max: chunk
                    .iter()
                    .map(|p| p.max)
                    .fold(f64::NEG_INFINITY, f64::max),
            }
        })
        .collect()
}
