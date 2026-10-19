//! CSV export support (phase 08 §2.4): bounded paged raw rows so large
//! exports stream instead of building huge arrays, plus the documented CSV
//! encoding — header `timestamp_utc,host_id,entity_id,metric_id,value,unit,
//! availability,source`, RFC 4180 quoting, and formula-leading escape for
//! spreadsheet safety. Import is intentionally not implemented; exports are
//! for tools, not for re-ingestion.

use rusqlite::Connection;
use serde::Serialize;

/// Upper bound per paging round-trip; callers loop until a short page.
pub const EXPORT_PAGE_ROWS: i64 = 5_000;
/// Hard ceiling per export (rows); guards unbounded ranges.
pub const MAX_EXPORT_ROWS: i64 = 2_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportRow {
    pub ts_utc_ms: i64,
    pub entity_id: String,
    pub value: f64,
    pub source: String,
}

/// One page of raw available rows for `(host, metric, entity)` strictly
/// after `after_ts` (exclusive), ascending. Missing data is expressed by
/// the absence of rows (documented gaps), never interpolated values.
pub fn export_page(
    conn: &Connection,
    host_id: &str,
    metric_id: &str,
    entity_id: &str,
    window: &ExportWindow,
    after_ts_utc_ms: i64,
    limit: i64,
) -> Result<Vec<ExportRow>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT ts_utc_ms, entity_id, value_real, source FROM samples_raw \
         WHERE host_id = ?1 AND metric_id = ?2 AND entity_id = ?3 \
           AND ts_utc_ms >= ?4 AND ts_utc_ms <= ?5 AND ts_utc_ms > ?6 \
         ORDER BY ts_utc_ms LIMIT ?7",
    )?;
    let rows = statement
        .query_map(
            rusqlite::params![
                host_id,
                metric_id,
                entity_id,
                window.from_utc_ms,
                window.to_utc_ms,
                after_ts_utc_ms,
                limit
            ],
            |row| {
                Ok(ExportRow {
                    ts_utc_ms: row.get(0)?,
                    entity_id: row.get(1)?,
                    value: row.get(2)?,
                    source: row.get(3)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The inclusive export window (UTC epoch ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportWindow {
    pub from_utc_ms: i64,
    pub to_utc_ms: i64,
}

/// CSV header, documented and stable (contract §7).
pub const CSV_HEADER: &str =
    "timestamp_utc,host_id,entity_id,metric_id,value,unit,availability,source";

/// RFC 4180 field quoting plus formula-leading escape: values beginning
/// with `=`, `+`, `-`, `@` (or tab/CR) are prefixed with `'` so a
/// spreadsheet cannot evaluate them as formulas (phase 08 §4).
pub fn csv_field(raw: &str) -> String {
    let needs_quoting =
        raw.contains(',') || raw.contains('"') || raw.contains('\n') || raw.contains('\r');
    let mut value = raw.to_string();
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        value.insert(0, '\'');
    }
    if needs_quoting {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// One CSV line (without newline) from already-formatted parts.
pub fn csv_line(parts: &[String]) -> String {
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::HistoryStore;

    fn store() -> HistoryStore {
        let conn = Connection::open_in_memory().unwrap();
        HistoryStore::with_connection(conn).unwrap()
    }

    fn insert(conn: &Connection, ts: i64, value: f64, source: &str) {
        conn.execute(
            "INSERT INTO samples_raw \
             (host_id, metric_id, entity_id, ts_utc_ms, value_real, source, generation, sequence) \
             VALUES ('h', 'cpu.total.usage', 'host', ?1, ?2, ?3, 'g', 1)",
            rusqlite::params![ts, value, source],
        )
        .unwrap();
    }

    #[test]
    fn pages_are_ordered_bounded_and_resume_after_cursor() {
        let store = store();
        let conn = store.connection();
        for ts in [1_000, 2_000, 3_000, 4_000] {
            insert(conn, ts, ts as f64, "sysinfo");
        }
        let window = ExportWindow {
            from_utc_ms: 0,
            to_utc_ms: 10_000,
        };
        let page1 = export_page(conn, "h", "cpu.total.usage", "host", &window, 0, 2).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].ts_utc_ms, 1_000);
        let page2 = export_page(
            conn,
            "h",
            "cpu.total.usage",
            "host",
            &window,
            page1[1].ts_utc_ms,
            2,
        )
        .unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].ts_utc_ms, 3_000);
        // Range bounds hold on every page.
        let late = ExportWindow {
            from_utc_ms: 4_500,
            to_utc_ms: 10_000,
        };
        assert!(
            export_page(conn, "h", "cpu.total.usage", "host", &late, 0, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn csv_escaping_quotes_and_neutralizes_formulas() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        // Formula-leading content cannot execute in a spreadsheet.
        assert_eq!(csv_field("=cmd"), "'=cmd");
        assert_eq!(csv_field("+1"), "'+1");
        assert_eq!(csv_field("-1"), "'-1");
        assert_eq!(csv_field("@x"), "'@x");
        // Quoted fields that also lead with a formula keep both treatments.
        assert_eq!(csv_field("=a,b"), "\"'=a,b\"");
        assert_eq!(csv_line(&[csv_field("a"), csv_field("b,c")]), "a,\"b,c\"");
    }
}
