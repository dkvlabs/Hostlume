//! Bounded history queries (phase 03 §3A.5): resolution auto-selection,
//! stride decimation to `max_points`, gap detection from sample cadence and
//! generation changes, and an explicit missing ratio. The backend never
//! floods the WebView with millions of points.

use rusqlite::Connection;
use serde::Serialize;

pub const DEFAULT_MAX_POINTS: u32 = 400;
pub const MAX_MAX_POINTS: u32 = 2_000;

/// Ranges above this span use rollups; raw is for recent detail only.
pub const RAW_MAX_SPAN_MS: i64 = 30 * 60_000; // 30 min
/// Ranges above this span use the hourly rollup.
pub const ROLLUP_1M_MAX_SPAN_MS: i64 = 7 * 86_400_000; // 7 days

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Raw,
    Rollup1m,
    Rollup1h,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HistoryPoint {
    pub ts_utc_ms: i64,
    pub value: f64,
    /// Only present on rollup points: raw samples behind the bucket.
    pub count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HistoryGap {
    pub from_utc_ms: i64,
    pub to_utc_ms: i64,
    /// Why the gap exists (generation change or missing samples).
    pub reason: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryQuery {
    pub host_id: String,
    pub metric_id: String,
    pub entity_id: String,
    pub from_utc_ms: i64,
    pub to_utc_ms: i64,
    /// Expected collection cadence for missing-ratio math (ms).
    pub cadence_ms: i64,
    pub max_points: u32,
}

impl HistoryQuery {
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.host_id.is_empty() || self.metric_id.is_empty() || self.entity_id.is_empty() {
            return Err(QueryError::Invalid("identifiers must not be empty".into()));
        }
        if self.to_utc_ms <= self.from_utc_ms {
            return Err(QueryError::Invalid("range end must be after start".into()));
        }
        if self.to_utc_ms - self.from_utc_ms > 31 * 86_400_000 {
            return Err(QueryError::RangeTooLarge);
        }
        if self.max_points == 0 || self.max_points > MAX_MAX_POINTS {
            return Err(QueryError::Invalid(format!(
                "max_points must be within 1..={MAX_MAX_POINTS}"
            )));
        }
        if self.cadence_ms <= 0 {
            return Err(QueryError::Invalid("cadence_ms must be positive".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryResponse {
    pub metric_id: String,
    pub entity_id: String,
    pub resolution: Resolution,
    pub points: Vec<HistoryPoint>,
    pub gaps: Vec<HistoryGap>,
    /// Fraction of expected samples that are missing in [from, to).
    pub missing_ratio: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid query: {0}")]
    Invalid(String),
    #[error("requested range exceeds the 31-day maximum")]
    RangeTooLarge,
}

fn choose_resolution(span_ms: i64) -> Resolution {
    if span_ms <= RAW_MAX_SPAN_MS {
        Resolution::Raw
    } else if span_ms <= ROLLUP_1M_MAX_SPAN_MS {
        Resolution::Rollup1m
    } else {
        Resolution::Rollup1h
    }
}

struct RawRow {
    ts_utc_ms: i64,
    value: f64,
    generation: String,
}

fn fetch_raw(conn: &Connection, query: &HistoryQuery) -> Result<Vec<RawRow>, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT ts_utc_ms, value_real, generation FROM samples_raw \
         WHERE host_id = ?1 AND metric_id = ?2 AND entity_id = ?3 \
         AND ts_utc_ms >= ?4 AND ts_utc_ms < ?5 ORDER BY ts_utc_ms ASC",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            query.host_id,
            query.metric_id,
            query.entity_id,
            query.from_utc_ms,
            query.to_utc_ms
        ],
        |row| {
            Ok(RawRow {
                ts_utc_ms: row.get(0)?,
                value: row.get(1)?,
                generation: row.get(2)?,
            })
        },
    )?;
    rows.collect()
}

#[derive(Debug, Clone)]
struct BucketRow {
    bucket_utc_ms: i64,
    value: f64,
    count: u64,
}

fn fetch_rollup(
    conn: &Connection,
    query: &HistoryQuery,
    resolution: Resolution,
) -> Result<Vec<BucketRow>, rusqlite::Error> {
    let table = match resolution {
        Resolution::Rollup1m => "samples_rollup_1m",
        _ => "samples_rollup_1h",
    };
    // Buckets are read without a generation column (rollups aggregate
    // possibly-multiple generations); generation gaps inside rollups are
    // handled by bucket-missing detection below.
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT bucket_utc_ms, avg, count FROM {table} \
         WHERE host_id = ?1 AND metric_id = ?2 AND entity_id = ?3 \
         AND bucket_utc_ms >= ?4 AND bucket_utc_ms < ?5 ORDER BY bucket_utc_ms ASC"
    ))?;
    let rows = stmt.query_map(
        rusqlite::params![
            query.host_id,
            query.metric_id,
            query.entity_id,
            query.from_utc_ms,
            query.to_utc_ms
        ],
        |row| {
            Ok(BucketRow {
                bucket_utc_ms: row.get(0)?,
                value: row.get(1)?,
                count: row.get::<_, i64>(2)?.max(0) as u64,
            })
        },
    )?;
    rows.collect()
}

/// Run a bounded, validated history query.
pub fn get_history(conn: &Connection, query: &HistoryQuery) -> Result<HistoryResponse, QueryError> {
    query.validate()?;
    let span_ms = query.to_utc_ms - query.from_utc_ms;
    let resolution = choose_resolution(span_ms);

    enum Series {
        Raw(Vec<RawRow>),
        Bucketed(Vec<BucketRow>),
    }
    let series = match resolution {
        Resolution::Raw => Series::Raw(fetch_raw(conn, query)?),
        Resolution::Rollup1m | Resolution::Rollup1h => {
            Series::Bucketed(fetch_rollup(conn, query, resolution)?)
        }
    };

    // Stride decimation to max_points (keep the first sample of each stride
    // window; bounded queries are a contract requirement).
    let total = match &series {
        Series::Raw(rows) => rows.len(),
        Series::Bucketed(rows) => rows.len(),
    };
    let max_points = query.max_points.max(1) as usize;
    let stride = total.div_ceil(max_points);

    // Flattened candidate points: (ts, value, count, generation-or-None).
    let mut candidates: Vec<(i64, f64, Option<u64>, Option<String>)> =
        Vec::with_capacity(total.min(max_points));
    match &series {
        Series::Raw(rows) => {
            for (index, row) in rows.iter().enumerate() {
                if index % stride == 0 {
                    candidates.push((row.ts_utc_ms, row.value, None, Some(row.generation.clone())));
                }
            }
        }
        Series::Bucketed(rows) => {
            for (index, row) in rows.iter().enumerate() {
                if index % stride == 0 {
                    candidates.push((row.bucket_utc_ms, row.value, Some(row.count), None));
                }
            }
        }
    }

    let gap_limit = match resolution {
        Resolution::Raw => query.cadence_ms * 5,
        Resolution::Rollup1m => crate::ROLLUP_BUCKET_1M_MS * 3,
        Resolution::Rollup1h => crate::ROLLUP_BUCKET_1H_MS * 3,
    };

    let mut points = Vec::with_capacity(candidates.len());
    let mut gaps: Vec<HistoryGap> = Vec::new();
    let mut previous: Option<(i64, Option<String>)> = None;
    for (ts, value, count, generation) in candidates {
        if let Some((prev_ts, prev_generation)) = previous {
            let generation_changed = matches!(
                (prev_generation.as_deref(), generation.as_deref()),
                (Some(prev_gen), Some(gen)) if prev_gen != gen
            );
            if generation_changed {
                gaps.push(HistoryGap {
                    from_utc_ms: prev_ts,
                    to_utc_ms: ts,
                    reason: "GENERATION_CHANGE",
                });
            } else if ts - prev_ts > gap_limit {
                gaps.push(HistoryGap {
                    from_utc_ms: prev_ts,
                    to_utc_ms: ts,
                    reason: "MISSING_SAMPLES",
                });
            }
        }
        points.push(HistoryPoint {
            ts_utc_ms: ts,
            value,
            count,
        });
        previous = Some((ts, generation));
    }

    // Missing ratio against the expected cadence (rollups: one per bucket).
    let expected = match resolution {
        Resolution::Raw => span_ms / query.cadence_ms,
        Resolution::Rollup1m => span_ms / crate::ROLLUP_BUCKET_1M_MS,
        Resolution::Rollup1h => span_ms / crate::ROLLUP_BUCKET_1H_MS,
    }
    .max(1);
    let ok: i64 = match &series {
        Series::Raw(rows) => rows.len() as i64,
        Series::Bucketed(rows) => rows.iter().map(|r| r.count.max(1) as i64).sum(),
    };
    let missing_ratio = ((expected - ok.min(expected)) as f64 / expected as f64).clamp(0.0, 1.0);

    Ok(HistoryResponse {
        metric_id: query.metric_id.clone(),
        entity_id: query.entity_id.clone(),
        resolution,
        points,
        gaps,
        missing_ratio,
    })
}
