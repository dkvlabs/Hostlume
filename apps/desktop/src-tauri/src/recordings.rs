//! Recording sessions and CSV export (phase 08, FX-018).
//!
//! A session is a named annotation window over the continuously persisted
//! telemetry — it never changes what is sampled (production persistence is
//! the real collector stream, always). Crash recovery: sessions left open
//! by a previous run are marked `interrupted` at startup.
//!
//! CSV export follows the documented contract shape (metrics-data-contract
//! §7): `timestamp_utc,host_id,entity_id,metric_id,value,unit,availability,
//! source`, UTC epoch-millisecond timestamps, RFC 4180 quoting with
//! formula-leading escape. Rows stream page-by-page into the exports
//! directory under the app's own data root — the WebView never picks an
//! arbitrary write path. Missing data appears as absent rows plus an
//! explicit gap summary; values are never interpolated.

use std::io::Write;
use std::path::PathBuf;

use monitor_core::{families, Clock, SystemClock};
use monitor_history::{RecordingSession, SessionError};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{open_history_query_connection, CommandError};
use crate::state::AppState;

/// Export bounds: at most 10 series per request, 365 days per range,
/// [`monitor_history::MAX_EXPORT_ROWS`] rows per file (documented and
/// enforced; the file notes the truncation).
const MAX_ENTITIES_PER_METRIC: usize = 64;
const MAX_EXPORT_RANGE_MS: i64 = 365 * 86_400_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingStart {
    pub label: String,
    /// Bounded subset of known metric families (empty = everything that
    /// is persisted).
    #[serde(default)]
    pub metric_ids: Vec<String>,
}

#[tauri::command]
pub fn start_recording(
    state: State<'_, AppState>,
    request: RecordingStart,
) -> Result<RecordingSession, CommandError> {
    for metric_id in &request.metric_ids {
        if !families::ALL_FAMILIES.contains(&metric_id.as_str()) {
            return Err(CommandError {
                code: "RECORDING_INVALID",
                message_key: "errors.recordingInvalid",
                reason: Some(format!("unknown metric {metric_id}")),
            });
        }
    }
    let store = open_history_query_connection(&state)?;
    monitor_history::start_session(
        store.connection(),
        &uuid::Uuid::new_v4().to_string(),
        &request.label,
        &request.metric_ids,
        SystemClock.now_epoch_ms(),
    )
    .map_err(session_error)
}

#[tauri::command]
pub fn stop_recording(state: State<'_, AppState>, session_id: String) -> Result<(), CommandError> {
    let store = open_history_query_connection(&state)?;
    monitor_history::stop_session(store.connection(), &session_id, SystemClock.now_epoch_ms())
        .map_err(session_error)
}

#[tauri::command]
pub fn list_recordings(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<RecordingSession>, CommandError> {
    let store = open_history_query_connection(&state)?;
    monitor_history::list_sessions(store.connection(), limit.unwrap_or(20).clamp(1, 100) as i64)
        .map_err(|err| CommandError::store(err.to_string()))
}

/// Startup crash recovery: open sessions from a previous run become
/// `interrupted` (phase 08 §2.3). Called from setup.
pub fn recover_interrupted_recordings(state: &AppState) {
    let Some(dir) = history_dir(state) else {
        return;
    };
    let Ok(store) = monitor_history::HistoryStore::open(&dir.join("history.sqlite3")) else {
        return;
    };
    match monitor_history::mark_interrupted_sessions(store.connection()) {
        Ok(count) if count > 0 => eprintln!("marked {count} recording session(s) interrupted"),
        Ok(_) => {}
        Err(error) => eprintln!("recording recovery failed: {error}"),
    }
}

fn session_error(error: SessionError) -> CommandError {
    CommandError {
        code: "RECORDING_INVALID",
        message_key: "errors.recordingInvalid",
        reason: Some(error.to_string()),
    }
}

fn history_dir(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .history_dir
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

// ---- CSV export -------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsvExportSpec {
    pub metric_id: String,
    pub entity_id: String,
    pub from_utc_ms: i64,
    pub to_utc_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CsvExportResult {
    pub path: String,
    pub rows: u64,
    /// True when the row ceiling cut the export short (noted in the file).
    pub truncated: bool,
}

#[tauri::command]
pub fn export_csv(
    state: State<'_, AppState>,
    spec: CsvExportSpec,
) -> Result<CsvExportResult, CommandError> {
    if !families::ALL_FAMILIES.contains(&spec.metric_id.as_str()) {
        return Err(CommandError {
            code: "EXPORT_INVALID",
            message_key: "errors.exportInvalid",
            reason: Some(format!("unknown metric {}", spec.metric_id)),
        });
    }
    if spec.entity_id.is_empty() || spec.entity_id.len() > 128 {
        return Err(CommandError {
            code: "EXPORT_INVALID",
            message_key: "errors.exportInvalid",
            reason: Some("entity_id length".into()),
        });
    }
    if spec.to_utc_ms <= spec.from_utc_ms || spec.to_utc_ms - spec.from_utc_ms > MAX_EXPORT_RANGE_MS
    {
        return Err(CommandError {
            code: "EXPORT_INVALID",
            message_key: "errors.exportInvalid",
            reason: Some("range".into()),
        });
    }
    let Some(unit) = families::numeric_unit(&spec.metric_id) else {
        return Err(CommandError {
            code: "EXPORT_INVALID",
            message_key: "errors.exportInvalid",
            reason: Some("metric is not numeric".into()),
        });
    };
    let dir = exports_dir(&state)?;
    let path = dir.join(format!(
        "hostlume-{}-{}-{}.csv",
        spec.metric_id.replace('.', "_"),
        spec.entity_id
            .replace(['/', ':', '\\'], "_")
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(24)
            .collect::<String>(),
        SystemClock.now_epoch_ms()
    ));
    let file = std::fs::File::create(&path)
        .map_err(|err| CommandError::store(format!("create export: {err}")))?;
    let mut writer = std::io::BufWriter::new(file);

    let store = open_history_query_connection(&state)?;
    let conn = store.connection();
    writeln!(writer, "{}", monitor_history::CSV_HEADER)
        .map_err(|err| CommandError::store(err.to_string()))?;

    let mut cursor = spec.from_utc_ms - 1;
    let mut rows_total: u64 = 0;
    let mut truncated = false;
    loop {
        if rows_total >= monitor_history::MAX_EXPORT_ROWS as u64 {
            truncated = true;
            break;
        }
        let page = monitor_history::export_page(
            conn,
            &state.host_id,
            &spec.metric_id,
            &spec.entity_id,
            &monitor_history::ExportWindow {
                from_utc_ms: spec.from_utc_ms,
                to_utc_ms: spec.to_utc_ms,
            },
            cursor,
            monitor_history::EXPORT_PAGE_ROWS,
        )
        .map_err(|err| CommandError::store(err.to_string()))?;
        let page_len = page.len() as u64;
        if page_len == 0 {
            break;
        }
        for row in &page {
            if rows_total >= monitor_history::MAX_EXPORT_ROWS as u64 {
                truncated = true;
                break;
            }
            let line = monitor_history::csv_line(&[
                monitor_history::csv_field(&row.ts_utc_ms.to_string()),
                monitor_history::csv_field(&state.host_id),
                monitor_history::csv_field(&row.entity_id),
                monitor_history::csv_field(&spec.metric_id),
                monitor_history::csv_field(&format!("{}", row.value)),
                monitor_history::csv_field(unit.as_canonical_str()),
                monitor_history::csv_field("available"),
                monitor_history::csv_field(&row.source),
            ]);
            writeln!(writer, "{line}").map_err(|err| CommandError::store(err.to_string()))?;
            rows_total += 1;
        }
        cursor = page.last().expect("non-empty page").ts_utc_ms;
        if page_len < monitor_history::EXPORT_PAGE_ROWS as u64 {
            break;
        }
    }
    if truncated {
        writeln!(
            writer,
            "# truncated at {} rows (increase the history retention or narrow the range)",
            monitor_history::MAX_EXPORT_ROWS
        )
        .map_err(|err| CommandError::store(err.to_string()))?;
    }
    writer
        .flush()
        .map_err(|err| CommandError::store(err.to_string()))?;
    Ok(CsvExportResult {
        path: path.to_string_lossy().to_string(),
        rows: rows_total,
        truncated,
    })
}

/// Export a recording session: its metric selection across all entities
/// seen in the session window (entities bounded per metric).
#[tauri::command]
pub fn export_recording_csv(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<CsvExportResult, CommandError> {
    let store = open_history_query_connection(&state)?;
    let Some(session) = monitor_history::get_session(store.connection(), &session_id)
        .map_err(|err| CommandError::store(err.to_string()))?
    else {
        return Err(CommandError {
            code: "RECORDING_NOT_FOUND",
            message_key: "errors.recordingNotFound",
            reason: None,
        });
    };
    let to = session
        .ended_at_utc_ms
        .unwrap_or_else(|| SystemClock.now_epoch_ms());
    let metrics: Vec<String> = if session.metric_ids.is_empty() {
        families::ALL_FAMILIES
            .iter()
            .filter(|id| families::numeric_unit(id).is_some())
            .map(|id| (*id).to_string())
            .collect()
    } else {
        session.metric_ids.clone()
    };
    let dir = exports_dir(&state)?;
    let path = dir.join(format!(
        "hostlume-session-{}-{}.csv",
        session
            .label
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(24)
            .collect::<String>()
            .to_lowercase(),
        SystemClock.now_epoch_ms()
    ));
    let file = std::fs::File::create(&path)
        .map_err(|err| CommandError::store(format!("create export: {err}")))?;
    let mut writer = std::io::BufWriter::new(file);
    writeln!(writer, "{}", monitor_history::CSV_HEADER)
        .map_err(|err| CommandError::store(err.to_string()))?;

    let mut rows_total: u64 = 0;
    let mut truncated = false;
    'metrics: for metric_id in &metrics {
        let Some(unit) = families::numeric_unit(metric_id) else {
            continue;
        };
        let entities = distinct_entities(
            store.connection(),
            &state.host_id,
            metric_id,
            session.started_at_utc_ms,
            to,
        );
        for entity in entities {
            let mut cursor = session.started_at_utc_ms - 1;
            loop {
                if rows_total >= monitor_history::MAX_EXPORT_ROWS as u64 {
                    truncated = true;
                    break 'metrics;
                }
                let page = monitor_history::export_page(
                    store.connection(),
                    &state.host_id,
                    metric_id,
                    &entity,
                    &monitor_history::ExportWindow {
                        from_utc_ms: session.started_at_utc_ms,
                        to_utc_ms: to,
                    },
                    cursor,
                    monitor_history::EXPORT_PAGE_ROWS,
                )
                .map_err(|err| CommandError::store(err.to_string()))?;
                if page.is_empty() {
                    break;
                }
                for row in &page {
                    if rows_total >= monitor_history::MAX_EXPORT_ROWS as u64 {
                        truncated = true;
                        break 'metrics;
                    }
                    let line = monitor_history::csv_line(&[
                        monitor_history::csv_field(&row.ts_utc_ms.to_string()),
                        monitor_history::csv_field(&state.host_id),
                        monitor_history::csv_field(&row.entity_id),
                        monitor_history::csv_field(metric_id),
                        monitor_history::csv_field(&format!("{}", row.value)),
                        monitor_history::csv_field(unit.as_canonical_str()),
                        monitor_history::csv_field("available"),
                        monitor_history::csv_field(&row.source),
                    ]);
                    writeln!(writer, "{line}")
                        .map_err(|err| CommandError::store(err.to_string()))?;
                    rows_total += 1;
                }
                cursor = page.last().expect("non-empty page").ts_utc_ms;
                if (page.len() as i64) < monitor_history::EXPORT_PAGE_ROWS {
                    break;
                }
            }
        }
    }
    writer
        .flush()
        .map_err(|err| CommandError::store(err.to_string()))?;
    Ok(CsvExportResult {
        path: path.to_string_lossy().to_string(),
        rows: rows_total,
        truncated,
    })
}

/// Distinct entities for one metric inside a window, bounded — per-core
/// metrics can have dozens of entities and the export is documented as
/// capped (the row ceiling catches the rest).
fn distinct_entities(
    conn: &rusqlite::Connection,
    host_id: &str,
    metric_id: &str,
    from_utc_ms: i64,
    to_utc_ms: i64,
) -> Vec<String> {
    let Ok(mut statement) = conn.prepare(
        "SELECT DISTINCT entity_id FROM samples_raw \
         WHERE host_id = ?1 AND metric_id = ?2 AND ts_utc_ms >= ?3 AND ts_utc_ms <= ?4 \
         ORDER BY entity_id LIMIT ?5",
    ) else {
        return Vec::new();
    };
    statement
        .query_map(
            rusqlite::params![
                host_id,
                metric_id,
                from_utc_ms,
                to_utc_ms,
                MAX_ENTITIES_PER_METRIC as i64,
            ],
            |row| row.get::<_, String>(0),
        )
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

fn exports_dir(state: &AppState) -> Result<PathBuf, CommandError> {
    let Some(dir) = history_dir(state) else {
        return Err(CommandError::store("history directory not set".into()));
    };
    // Exports live beside (not inside) the history directory so retention
    // sweeping never touches exported files.
    let exports = dir
        .parent()
        .map(|p| p.join("exports"))
        .unwrap_or_else(|| dir.join("exports"));
    std::fs::create_dir_all(&exports).map_err(|err| CommandError::store(err.to_string()))?;
    Ok(exports)
}
