//! Recording sessions (phase 08 §2.3/§2.5): named annotation windows over
//! the continuously persisted telemetry. A session does not change what is
//! sampled — it marks a UTC window so exports and the Compare view can be
//! scoped to it. Crash recovery: sessions left open by a previous run are
//! marked `interrupted` at startup, never silently closed.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

pub const STATUS_RECORDING: &str = "recording";
pub const STATUS_STOPPED: &str = "stopped";
pub const STATUS_INTERRUPTED: &str = "interrupted";

const MAX_METRIC_IDS: usize = 16;
const MAX_LABEL_LEN: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordingSession {
    pub id: String,
    pub label: String,
    /// Selected metric ids (bounded); empty means "everything persisted".
    pub metric_ids: Vec<String>,
    pub started_at_utc_ms: i64,
    pub ended_at_utc_ms: Option<i64>,
    /// `recording` while open, `stopped` after a clean stop, `interrupted`
    /// when a previous app run ended without stopping it.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("session label must be at most {max} characters")]
    LabelTooLong { max: usize },
    #[error("a recording accepts at most {max} metric ids")]
    TooManyMetrics { max: usize },
    #[error("session not found")]
    NotFound,
    #[error("session is not open")]
    NotOpen,
}

/// Validate and open a session. The caller supplies the id (UUID) and the
/// clock reading; storage stays generic.
pub fn start_session(
    conn: &Connection,
    id: &str,
    label: &str,
    metric_ids: &[String],
    started_at_utc_ms: i64,
) -> Result<RecordingSession, SessionError> {
    if label.len() > MAX_LABEL_LEN {
        return Err(SessionError::LabelTooLong { max: MAX_LABEL_LEN });
    }
    if metric_ids.len() > MAX_METRIC_IDS {
        return Err(SessionError::TooManyMetrics {
            max: MAX_METRIC_IDS,
        });
    }
    let metrics_json = serde_json::to_string(metric_ids).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO recording_sessions \
         (id, started_at_utc, ended_at_utc, metric_ids_json, label, status) \
         VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
        rusqlite::params![id, started_at_utc_ms, metrics_json, label, STATUS_RECORDING],
    )
    .map_err(|_| SessionError::NotFound)?;
    Ok(RecordingSession {
        id: id.to_string(),
        label: label.to_string(),
        metric_ids: metric_ids.to_vec(),
        started_at_utc_ms,
        ended_at_utc_ms: None,
        status: STATUS_RECORDING.to_string(),
    })
}

/// Cleanly close an open session. Closing an already-closed session is a
/// typed error (no silent double-stop).
pub fn stop_session(conn: &Connection, id: &str, ended_at_utc_ms: i64) -> Result<(), SessionError> {
    let changed = conn
        .execute(
            "UPDATE recording_sessions \
             SET ended_at_utc = ?2, status = ?3 \
             WHERE id = ?1 AND status = ?4",
            rusqlite::params![id, ended_at_utc_ms, STATUS_STOPPED, STATUS_RECORDING],
        )
        .map_err(|_| SessionError::NotFound)?;
    if changed == 0 {
        return Err(SessionError::NotOpen);
    }
    Ok(())
}

/// Startup crash recovery: any session still `recording` belongs to a run
/// that ended without a clean stop. Returns how many were re-marked.
pub fn mark_interrupted_sessions(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "UPDATE recording_sessions SET status = ?1, ended_at_utc = NULL \
         WHERE status = ?2",
        rusqlite::params![STATUS_INTERRUPTED, STATUS_RECORDING],
    )
}

pub fn get_session(
    conn: &Connection,
    id: &str,
) -> Result<Option<RecordingSession>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, label, metric_ids_json, started_at_utc, ended_at_utc, status \
         FROM recording_sessions WHERE id = ?1",
        [id],
        row_to_session,
    )
    .optional()
}

pub fn list_sessions(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<RecordingSession>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, label, metric_ids_json, started_at_utc, ended_at_utc, status \
         FROM recording_sessions ORDER BY started_at_utc DESC, id LIMIT ?1",
    )?;
    let rows = statement
        .query_map([limit.clamp(1, 100)], row_to_session)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn row_to_session(row: &rusqlite::Row<'_>) -> Result<RecordingSession, rusqlite::Error> {
    let metrics_json: String = row.get(2)?;
    let metric_ids: Vec<String> = serde_json::from_str(&metrics_json).unwrap_or_default();
    Ok(RecordingSession {
        id: row.get(0)?,
        label: row.get(1)?,
        metric_ids,
        started_at_utc_ms: row.get(3)?,
        ended_at_utc_ms: row.get(4)?,
        status: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::HistoryStore;

    fn store() -> HistoryStore {
        let conn = Connection::open_in_memory().unwrap();
        HistoryStore::with_connection(conn).unwrap()
    }

    #[test]
    fn session_lifecycle_and_validation() {
        let store = store();
        let conn = store.connection();
        assert!(matches!(
            start_session(conn, "s1", &"x".repeat(200), &[], 1),
            Err(SessionError::LabelTooLong { .. })
        ));
        let many: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
        assert!(matches!(
            start_session(conn, "s1", "ok", &many, 1),
            Err(SessionError::TooManyMetrics { .. })
        ));

        let session = start_session(
            conn,
            "s1",
            "compile load",
            &["cpu.total.usage".into()],
            1_000,
        )
        .unwrap();
        assert_eq!(session.status, STATUS_RECORDING);
        assert_eq!(session.ended_at_utc_ms, None);

        // Double-stop is a typed error.
        stop_session(conn, "s1", 5_000).unwrap();
        assert!(matches!(
            stop_session(conn, "s1", 6_000),
            Err(SessionError::NotOpen)
        ));

        let listed = list_sessions(conn, 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, STATUS_STOPPED);
        assert_eq!(listed[0].ended_at_utc_ms, Some(5_000));
        assert_eq!(listed[0].metric_ids, vec!["cpu.total.usage"]);
    }

    /// Phase 08 §2.3: power loss recovers an interrupted recording state.
    #[test]
    fn open_sessions_are_marked_interrupted_on_startup() {
        let store = store();
        let conn = store.connection();
        start_session(conn, "open-1", "lost", &[], 1).unwrap();
        start_session(conn, "open-2", "also lost", &[], 2).unwrap();
        stop_session(conn, "open-2", 3).unwrap();

        assert_eq!(mark_interrupted_sessions(conn).unwrap(), 1);
        let open_1 = get_session(conn, "open-1").unwrap().unwrap();
        assert_eq!(open_1.status, STATUS_INTERRUPTED);
        assert_eq!(open_1.ended_at_utc_ms, None);
        let open_2 = get_session(conn, "open-2").unwrap().unwrap();
        assert_eq!(open_2.status, STATUS_STOPPED);
        // Idempotent.
        assert_eq!(mark_interrupted_sessions(conn).unwrap(), 0);
    }

    #[test]
    fn list_is_bounded_and_ordered() {
        let store = store();
        let conn = store.connection();
        for i in 0..5 {
            start_session(conn, &format!("s{i}"), "l", &[], i).unwrap();
        }
        let listed = list_sessions(conn, 3).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].id, "s4", "newest first");
    }
}
