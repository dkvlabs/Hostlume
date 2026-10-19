//! Checksummed ordered migrations (ADR-0002). Same policy as the settings
//! store: a database written by a different build is refused, a database
//! from the future is refused, each migration runs in its own transaction.

use monitor_core::{Clock, SystemClock};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub fn checksum_of(migration: &Migration) -> String {
    let mut hasher = Sha256::new();
    hasher.update(migration.name.as_bytes());
    hasher.update(b"\n");
    hasher.update(migration.sql.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "history_core",
        sql: "CREATE TABLE metric_definitions (\n\
          \x20 metric_id TEXT PRIMARY KEY,\n\
          \x20 description TEXT NOT NULL,\n\
          \x20 unit TEXT NOT NULL,\n\
          \x20 entity_type TEXT NOT NULL,\n\
          \x20 semantic_version INTEGER NOT NULL DEFAULT 1\n\
          );\n\
          CREATE TABLE samples_raw (\n\
          \x20 host_id TEXT NOT NULL,\n\
          \x20 metric_id TEXT NOT NULL,\n\
          \x20 entity_id TEXT NOT NULL,\n\
          \x20 ts_utc_ms INTEGER NOT NULL,\n\
          \x20 value_real REAL NOT NULL,\n\
          \x20 source TEXT NOT NULL,\n\
          \x20 generation TEXT NOT NULL,\n\
          \x20 sequence INTEGER NOT NULL\n\
          );\n\
          CREATE INDEX idx_samples_raw_lookup\n\
          \x20 ON samples_raw (host_id, metric_id, entity_id, ts_utc_ms);\n\
          CREATE INDEX idx_samples_raw_ts ON samples_raw (ts_utc_ms);\n\
          CREATE TABLE samples_rollup_1m (\n\
          \x20 host_id TEXT NOT NULL,\n\
          \x20 metric_id TEXT NOT NULL,\n\
          \x20 entity_id TEXT NOT NULL,\n\
          \x20 bucket_utc_ms INTEGER NOT NULL,\n\
          \x20 count INTEGER NOT NULL,\n\
          \x20 ok_count INTEGER NOT NULL,\n\
          \x20 min REAL,\n\
          \x20 max REAL,\n\
          \x20 avg REAL,\n\
          \x20 last REAL,\n\
          \x20 PRIMARY KEY (host_id, metric_id, entity_id, bucket_utc_ms)\n\
          );\n\
          CREATE TABLE samples_rollup_1h (\n\
          \x20 host_id TEXT NOT NULL,\n\
          \x20 metric_id TEXT NOT NULL,\n\
          \x20 entity_id TEXT NOT NULL,\n\
          \x20 bucket_utc_ms INTEGER NOT NULL,\n\
          \x20 count INTEGER NOT NULL,\n\
          \x20 ok_count INTEGER NOT NULL,\n\
          \x20 min REAL,\n\
          \x20 max REAL,\n\
          \x20 avg REAL,\n\
          \x20 last REAL,\n\
          \x20 PRIMARY KEY (host_id, metric_id, entity_id, bucket_utc_ms)\n\
          );\n\
          CREATE TABLE recording_sessions (\n\
          \x20 id TEXT PRIMARY KEY,\n\
          \x20 started_at_utc INTEGER NOT NULL,\n\
          \x20 ended_at_utc INTEGER,\n\
          \x20 metric_ids_json TEXT NOT NULL,\n\
          \x20 label TEXT\n\
          );\n\
          CREATE TABLE alert_rules (\n\
          \x20 id TEXT PRIMARY KEY,\n\
          \x20 condition_json TEXT NOT NULL,\n\
          \x20 enabled INTEGER NOT NULL DEFAULT 1,\n\
          \x20 created_at_utc INTEGER NOT NULL,\n\
          \x20 updated_at_utc INTEGER NOT NULL\n\
          );\n\
          CREATE TABLE alert_events (\n\
          \x20 id TEXT PRIMARY KEY,\n\
          \x20 rule_id TEXT NOT NULL,\n\
          \x20 state TEXT NOT NULL,\n\
          \x20 triggered_at_utc INTEGER NOT NULL,\n\
          \x20 resolved_at_utc INTEGER,\n\
          \x20 summary_json TEXT\n\
          );\n\
          CREATE TABLE action_audit (\n\
          \x20 id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
          \x20 ts_utc_ms INTEGER NOT NULL,\n\
          \x20 target_pid INTEGER NOT NULL,\n\
          \x20 target_start_time INTEGER NOT NULL,\n\
          \x20 mode TEXT NOT NULL,\n\
          \x20 result TEXT NOT NULL,\n\
          \x20 detail TEXT\n\
          );",
    },
    Migration {
        version: 2,
        name: "recording_session_status",
        sql: "ALTER TABLE recording_sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'stopped';\n\
          \x20 UPDATE recording_sessions SET status = 'stopped';",
    },
];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "migration checksum mismatch for V{version}: database was written by a different build \
         (stored {stored}, expected {expected}); refusing to touch it"
    )]
    ChecksumMismatch {
        version: i64,
        expected: String,
        stored: String,
    },
    #[error("database schema version {db} is newer than this build supports ({app})")]
    DatabaseTooNew { db: i64, app: i64 },
}

/// Apply `migrations` in order with checksum verification of applied steps.
pub fn apply_migrations(conn: &mut Connection, migrations: &[Migration]) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
         \x20 version INTEGER PRIMARY KEY,\
         \x20 name TEXT NOT NULL,\
         \x20 checksum TEXT NOT NULL,\
         \x20 applied_at_utc TEXT NOT NULL\
         );",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    let latest = migrations.iter().map(|m| m.version).max().unwrap_or(0);
    if current > latest {
        return Err(StoreError::DatabaseTooNew {
            db: current,
            app: latest,
        });
    }
    for migration in migrations.iter().filter(|m| m.version <= current) {
        let expected = checksum_of(migration);
        let stored: Option<String> = conn
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                rusqlite::params![migration.version],
                |row| row.get(0),
            )
            .optional()?;
        if stored.as_deref() != Some(expected.as_str()) {
            return Err(StoreError::ChecksumMismatch {
                version: migration.version,
                expected,
                stored: stored.unwrap_or_else(|| "<missing>".to_string()),
            });
        }
    }
    for migration in migrations.iter().filter(|m| m.version > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at_utc) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                migration.version,
                migration.name,
                checksum_of(migration),
                SystemClock.now_utc_iso8601()
            ],
        )?;
        tx.commit()?;
    }
    Ok(())
}

use rusqlite::OptionalExtension;
