//! monitor-history — the persisted telemetry substrate (phase 03 §3A).
//!
//! Schema per docs/metrics-data-contract.md §6; storage decisions per
//! ADR-0002 (bundled rusqlite, checksummed ordered migrations, one writer).
//! The load-bearing properties, all test-enforced:
//!
//! * UTC epoch-millisecond timestamps only; UTC bucketing for rollups
//!   (renderers convert to local time — never the other way around).
//! * Writes go through a bounded batch writer; overload is *counted* and
//!   surfaced, never silent.
//! * Queries are bounded (`max_points`) and report gaps and missing ratios
//!   instead of interpolating across them.

pub mod alerts;
pub mod migrations;
pub mod query;
pub mod retention;
pub mod store;
pub mod writer;

pub use alerts::{
    alert_rule_exists, delete_alert_rule, last_fire_per_rule, list_alert_rules,
    list_recent_alert_events, upsert_alert_rule, AlertEventRow, AlertRuleRow,
};
pub use migrations::{apply_migrations, checksum_of, Migration, StoreError, MIGRATIONS};
pub use query::{
    get_history, HistoryPoint, HistoryQuery, HistoryResponse, QueryError, Resolution,
    DEFAULT_MAX_POINTS, MAX_MAX_POINTS,
};
pub use retention::{
    clear_samples, compute_rollups, retain, RetentionSummary, RollupGranularity, RollupRow,
    DEFAULT_RAW_HOURS, DEFAULT_ROLLUP_1H_DAYS, DEFAULT_ROLLUP_1M_DAYS, ROLLUP_BUCKET_1H_MS,
    ROLLUP_BUCKET_1M_MS,
};
pub use store::HistoryStore;
pub use writer::{BatchWriter, WriteItem, WriterStats};
