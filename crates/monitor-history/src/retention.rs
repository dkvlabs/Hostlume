//! Rollups and retention (phase 03 §3A.4).
//!
//! Model: rollups are recomputed **from raw** over a bounded recompute
//! window and UPSERTed (idempotent — a bucket is always the exact
//! aggregation of the raw rows still existing for it). While raw retention
//! (default 24 h) covers a bucket, the worker keeps it exact; after raw
//! expires the bucket is final. Gaps (app offline) stay missing and show
//! up as gaps in queries — never interpolated.
//!
//! Bucketing is pure UTC epoch math; DST cannot affect stored bins
//! (renderers localize for display only — testing-qa HIST-01).

use rusqlite::Connection;
use serde::Serialize;

pub const ROLLUP_BUCKET_1M_MS: i64 = 60_000;
pub const ROLLUP_BUCKET_1H_MS: i64 = 3_600_000;

/// Default retention horizons (phase doc: raw 24h, 1m 30d, 1h 365d).
pub const DEFAULT_RAW_HOURS: u32 = 24;
pub const DEFAULT_ROLLUP_1M_DAYS: u32 = 30;
pub const DEFAULT_ROLLUP_1H_DAYS: u32 = 365;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RollupRow {
    pub metric_id: String,
    pub entity_id: String,
    pub bucket_utc_ms: i64,
    pub count: u64,
    pub ok_count: u64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub avg: Option<f64>,
    pub last: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollupGranularity {
    OneMinute,
    OneHour,
}

impl RollupGranularity {
    pub fn bucket_ms(self) -> i64 {
        match self {
            RollupGranularity::OneMinute => ROLLUP_BUCKET_1M_MS,
            RollupGranularity::OneHour => ROLLUP_BUCKET_1H_MS,
        }
    }

    pub fn table(self) -> &'static str {
        match self {
            RollupGranularity::OneMinute => "samples_rollup_1m",
            RollupGranularity::OneHour => "samples_rollup_1h",
        }
    }
}

/// Fetch raw rows in `[window_start_ms, window_end_ms)` and group them into
/// rollup rows in Rust — small windows keep this cheap and fully
/// deterministic/testable (no SQL aggregation dialect risk).
pub fn rollups_from_raw(
    conn: &Connection,
    host_id: &str,
    granularity: RollupGranularity,
    window_start_ms: i64,
    window_end_ms: i64,
) -> Result<Vec<RollupRow>, rusqlite::Error> {
    let bucket_ms = granularity.bucket_ms();
    let mut stmt = conn.prepare_cached(
        "SELECT metric_id, entity_id, ts_utc_ms, value_real FROM samples_raw \
         WHERE host_id = ?1 AND ts_utc_ms >= ?2 AND ts_utc_ms < ?3 ORDER BY ts_utc_ms ASC",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![host_id, window_start_ms, window_end_ms],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        },
    )?;
    let mut series: std::collections::BTreeMap<(String, String), Vec<(i64, f64)>> =
        std::collections::BTreeMap::new();
    for row in rows {
        let (metric_id, entity_id, ts, value) = row?;
        series
            .entry((metric_id, entity_id))
            .or_default()
            .push((ts, value));
    }
    let mut out = Vec::new();
    for ((metric_id, entity_id), points) in series {
        let mut buckets: std::collections::BTreeMap<i64, Vec<(i64, f64)>> =
            std::collections::BTreeMap::new();
        for (ts, value) in points {
            let bucket = ts.div_euclid(bucket_ms) * bucket_ms;
            buckets.entry(bucket).or_default().push((ts, value));
        }
        for (bucket, bucket_points) in buckets {
            let values: Vec<f64> = bucket_points.iter().map(|(_, v)| *v).collect();
            let count = values.len() as u64;
            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let sum: f64 = values.iter().sum();
            let avg = sum / count as f64;
            let last_ts = bucket_points.iter().map(|(ts, _)| *ts).max().unwrap_or(0);
            let last = bucket_points
                .iter()
                .filter(|(ts, _)| *ts == last_ts)
                .map(|(_, v)| *v)
                .next_back();
            out.push(RollupRow {
                metric_id: metric_id.clone(),
                entity_id: entity_id.clone(),
                bucket_utc_ms: bucket,
                count,
                ok_count: count,
                min: Some(min),
                max: Some(max),
                avg: Some(avg),
                last,
            });
        }
    }
    Ok(out)
}

/// Upsert rollup rows (idempotent recompute). `count` records the number of
/// raw samples seen; `ok_count` currently equals count because only
/// available numeric values are persisted — the columns exist so partial
/// intervals (with unavailable samples) can be represented once
/// unavailable-sample bookkeeping lands.
pub fn upsert_rollups(
    conn: &Connection,
    host_id: &str,
    granularity: RollupGranularity,
    rows: &[RollupRow],
) -> Result<usize, rusqlite::Error> {
    let table = granularity.table();
    let mut written = 0;
    for row in rows {
        conn.execute(
            &format!(
                "INSERT INTO {table} \
                 (host_id, metric_id, entity_id, bucket_utc_ms, count, ok_count, min, max, avg, last) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT (host_id, metric_id, entity_id, bucket_utc_ms) DO UPDATE SET \
                 count = ?5, ok_count = ?6, min = ?7, max = ?8, avg = ?9, last = ?10"
            ),
            rusqlite::params![
                host_id,
                row.metric_id,
                row.entity_id,
                row.bucket_utc_ms,
                row.count as i64,
                row.ok_count as i64,
                row.min,
                row.max,
                row.avg,
                row.last,
            ],
        )?;
        written += 1;
    }
    Ok(written)
}

/// Convenience used by the app's maintenance task: recompute rollups for
/// `[now - recompute_ms, now)`.
pub fn compute_rollups(
    conn: &Connection,
    host_id: &str,
    granularity: RollupGranularity,
    window_start_ms: i64,
    window_end_ms: i64,
) -> Result<usize, rusqlite::Error> {
    let rows = rollups_from_raw(conn, host_id, granularity, window_start_ms, window_end_ms)?;
    upsert_rollups(conn, host_id, granularity, &rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RetentionSummary {
    pub raw_deleted: u64,
    pub rollup_1m_deleted: u64,
    pub rollup_1h_deleted: u64,
}

/// Delete rows older than the configured horizons. Horizon 0 disables that
/// tier's deletion (kept for "keep everything" configurations).
pub fn retain(
    conn: &Connection,
    now_utc_ms: i64,
    raw_hours: u32,
    rollup_1m_days: u32,
    rollup_1h_days: u32,
) -> Result<RetentionSummary, rusqlite::Error> {
    let mut summary = RetentionSummary {
        raw_deleted: 0,
        rollup_1m_deleted: 0,
        rollup_1h_deleted: 0,
    };
    if raw_hours > 0 {
        let cutoff = now_utc_ms - (raw_hours as i64) * 3_600_000;
        summary.raw_deleted = conn.execute(
            "DELETE FROM samples_raw WHERE ts_utc_ms < ?1",
            rusqlite::params![cutoff],
        )? as u64;
    }
    if rollup_1m_days > 0 {
        let cutoff = now_utc_ms - (rollup_1m_days as i64) * 86_400_000;
        summary.rollup_1m_deleted = conn.execute(
            "DELETE FROM samples_rollup_1m WHERE bucket_utc_ms < ?1",
            rusqlite::params![cutoff],
        )? as u64;
    }
    if rollup_1h_days > 0 {
        let cutoff = now_utc_ms - (rollup_1h_days as i64) * 86_400_000;
        summary.rollup_1h_deleted = conn.execute(
            "DELETE FROM samples_rollup_1h WHERE bucket_utc_ms < ?1",
            rusqlite::params![cutoff],
        )? as u64;
    }
    Ok(summary)
}

/// Clear all samples (Settings → History → Clear). Audits and settings are
/// untouched; this is explicit, user-confirmed, and reported.
pub fn clear_samples(conn: &Connection) -> Result<RetentionSummary, rusqlite::Error> {
    Ok(RetentionSummary {
        raw_deleted: conn.execute("DELETE FROM samples_raw", [])? as u64,
        rollup_1m_deleted: conn.execute("DELETE FROM samples_rollup_1m", [])? as u64,
        rollup_1h_deleted: conn.execute("DELETE FROM samples_rollup_1h", [])? as u64,
    })
}
