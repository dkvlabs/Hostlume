//! Alert rule and alert event persistence (phase 07 §3.3).
//!
//! The tables already exist in migration V1 (`alert_rules`,
//! `alert_events`). Hot-path event writes go through the bounded batch
//! writer; rule CRUD and event queries open their own short-lived
//! connection (WAL + busy timeout make that safe next to the single
//! writer thread). Storage is generic: rows carry the rule's serialized
//! condition JSON; monitor-alerts owns its schema.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};

/// One `alert_rules` row. `condition_json` is the serialized
/// `monitor_alerts::AlertRule`.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRuleRow {
    pub id: String,
    pub condition_json: String,
    pub enabled: bool,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
}

pub fn upsert_alert_rule(conn: &Connection, row: &AlertRuleRow) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO alert_rules (id, condition_json, enabled, created_at_utc, updated_at_utc) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(id) DO UPDATE SET \
           condition_json = ?2, enabled = ?3, updated_at_utc = ?5",
        rusqlite::params![
            row.id,
            row.condition_json,
            row.enabled,
            row.created_at_utc_ms,
            row.updated_at_utc_ms
        ],
    )?;
    Ok(())
}

/// Returns whether a row was actually deleted.
pub fn delete_alert_rule(conn: &Connection, id: &str) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute("DELETE FROM alert_rules WHERE id = ?1", [id])?;
    Ok(changed > 0)
}

pub fn list_alert_rules(conn: &Connection) -> Result<Vec<AlertRuleRow>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, condition_json, enabled, created_at_utc, updated_at_utc \
         FROM alert_rules ORDER BY created_at_utc, id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(AlertRuleRow {
                id: row.get(0)?,
                condition_json: row.get(1)?,
                enabled: row.get::<_, i64>(2)? != 0,
                created_at_utc_ms: row.get(3)?,
                updated_at_utc_ms: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// One `alert_events` row: an episode from fire to resolve.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AlertEventRow {
    pub id: String,
    pub rule_id: String,
    /// `firing` while the episode is open, `resolved` once closed.
    pub state: String,
    pub triggered_at_utc_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at_utc_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_json: Option<String>,
}

/// Most recent episodes first, bounded by `limit` (caller clamps).
pub fn list_recent_alert_events(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<AlertEventRow>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, rule_id, state, triggered_at_utc, resolved_at_utc, summary_json \
         FROM alert_events ORDER BY triggered_at_utc DESC, id LIMIT ?1",
    )?;
    let rows = statement
        .query_map([limit], |row| {
            Ok(AlertEventRow {
                id: row.get(0)?,
                rule_id: row.get(1)?,
                state: row.get(2)?,
                triggered_at_utc_ms: row.get(3)?,
                resolved_at_utc_ms: row.get(4)?,
                summary_json: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Last fire time per rule — the restart-safety seed for
/// `AlertEngine::restore_from_events` (phase 07 §2: no double firing after
/// a restart unless the condition is freshly proven).
pub fn last_fire_per_rule(conn: &Connection) -> Result<HashMap<String, i64>, rusqlite::Error> {
    let mut statement =
        conn.prepare("SELECT rule_id, MAX(triggered_at_utc) FROM alert_events GROUP BY rule_id")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

/// Whether a rule id exists (the update path must not resurrect deleted
/// rules).
pub fn alert_rule_exists(conn: &Connection, id: &str) -> Result<bool, rusqlite::Error> {
    let found: Option<i64> = conn
        .query_row("SELECT 1 FROM alert_rules WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(found.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::HistoryStore;

    fn store() -> HistoryStore {
        let conn = Connection::open_in_memory().unwrap();
        HistoryStore::with_connection(conn).unwrap()
    }

    fn row(id: &str, created: i64) -> AlertRuleRow {
        AlertRuleRow {
            id: id.to_string(),
            condition_json: format!(r#"{{"id":"{id}"}}"#),
            enabled: true,
            created_at_utc_ms: created,
            updated_at_utc_ms: created,
        }
    }

    #[test]
    fn rules_round_trip_and_delete() {
        let store = store();
        upsert_alert_rule(store.connection(), &row("a", 100)).unwrap();
        upsert_alert_rule(store.connection(), &row("b", 50)).unwrap();
        // Order is by creation time.
        let rules = list_alert_rules(store.connection()).unwrap();
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a"]);
        // Update overwrites condition and bump time, keeps creation.
        let mut updated = row("a", 100);
        updated.condition_json = r#"{"id":"a","threshold":95}"#.to_string();
        updated.updated_at_utc_ms = 200;
        upsert_alert_rule(store.connection(), &updated).unwrap();
        let rules = list_alert_rules(store.connection()).unwrap();
        assert_eq!(rules.len(), 2);
        let a = rules.iter().find(|r| r.id == "a").unwrap();
        assert_eq!(a.updated_at_utc_ms, 200);
        assert_eq!(a.created_at_utc_ms, 100);
        assert!(alert_rule_exists(store.connection(), "a").unwrap());
        assert!(delete_alert_rule(store.connection(), "a").unwrap());
        assert!(!delete_alert_rule(store.connection(), "a").unwrap());
        assert!(!alert_rule_exists(store.connection(), "a").unwrap());
    }

    #[test]
    fn events_round_trip_close_and_last_fire() {
        let store = store();
        let insert = |id: &str, rule: &str, at: i64| {
            conn_insert_event(store.connection(), id, rule, at);
        };
        insert("e1", "r1", 1_000);
        insert("e2", "r1", 5_000);
        insert("e3", "r2", 6_000);
        // Close e2: resolved with resolved_at.
        conn_close_event(store.connection(), "e2", 7_000);
        let events = list_recent_alert_events(store.connection(), 10).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, "e3", "most recent first");
        let e2 = events.iter().find(|e| e.id == "e2").unwrap();
        assert_eq!(e2.state, "resolved");
        assert_eq!(e2.resolved_at_utc_ms, Some(7_000));
        let last = last_fire_per_rule(store.connection()).unwrap();
        assert_eq!(last.get("r1"), Some(&5_000));
        assert_eq!(last.get("r2"), Some(&6_000));
        // Bounded queries.
        assert_eq!(
            list_recent_alert_events(store.connection(), 1)
                .unwrap()
                .len(),
            1
        );
    }

    fn conn_insert_event(conn: &Connection, id: &str, rule: &str, at: i64) {
        conn.execute(
            "INSERT INTO alert_events (id, rule_id, state, triggered_at_utc) \
             VALUES (?1, ?2, 'firing', ?3)",
            rusqlite::params![id, rule, at],
        )
        .unwrap();
    }

    fn conn_close_event(conn: &Connection, id: &str, resolved_at: i64) {
        conn.execute(
            "UPDATE alert_events SET state = 'resolved', resolved_at_utc = ?2 WHERE id = ?1",
            rusqlite::params![id, resolved_at],
        )
        .unwrap();
    }
}
