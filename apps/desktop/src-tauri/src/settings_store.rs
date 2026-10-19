//! Durable settings + host identity in SQLite (ADR-0002):
//! bundled rusqlite, checksummed ordered migrations, single connection
//! behind a mutex (the app has exactly one writer). Every statement uses
//! parameter binding — no string-assembled SQL anywhere.

use std::path::Path;
use std::sync::Mutex;

use monitor_core::{Clock, SystemClock};
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::settings::{Settings, SettingsError, SettingsV1, SETTINGS_SCHEMA_VERSION};

/// One ordered schema migration. Checksums pin content: a database created
/// by a different build is refused instead of silently diverged.
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

pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "settings_and_host_identity",
    sql: "CREATE TABLE app_settings (\n\
          \x20 key TEXT PRIMARY KEY,\n\
          \x20 schema_version INTEGER NOT NULL,\n\
          \x20 json_value TEXT NOT NULL\n\
          );\n\
          CREATE TABLE host_identity (\n\
          \x20 id TEXT PRIMARY KEY,\n\
          \x20 created_at_utc TEXT NOT NULL\n\
          );",
}];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("settings error: {0}")]
    Settings(#[from] SettingsError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
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

pub struct SettingsStore {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SettingsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsStore").finish_non_exhaustive()
    }
}

impl SettingsStore {
    /// Open (creating parent directories), apply pragmas and migrations.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        Self::configure(&mut conn);
        Self::with_migrations(conn, MIGRATIONS)
    }

    /// Test/extension seam: initialize from an explicit migration list.
    pub fn with_migrations(
        mut conn: Connection,
        migrations: &[Migration],
    ) -> Result<Self, StoreError> {
        Self::configure(&mut conn);
        apply_migrations(&mut conn, migrations)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn configure(conn: &mut Connection) {
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.busy_timeout(std::time::Duration::from_millis(2_000));
    }

    /// Installation-stable host identity: a random UUID created once and
    /// persisted. Never a MAC address or hardware serial (contract §8).
    pub fn host_id(&self) -> Result<String, StoreError> {
        let conn = self.lock_conn();
        let existing: Option<String> = conn
            .query_row("SELECT id FROM host_identity LIMIT 1", [], |row| row.get(0))
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = SystemClock.now_utc_iso8601();
        conn.execute(
            "INSERT INTO host_identity (id, created_at_utc) VALUES (?1, ?2)",
            rusqlite::params![id, created_at],
        )?;
        Ok(id)
    }

    pub fn load_settings(&self) -> Result<Settings, StoreError> {
        let row: Option<(i64, String)> = {
            let conn = self.lock_conn();
            conn.query_row(
                "SELECT schema_version, json_value FROM app_settings WHERE key = 'app'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
        };
        let Some((version, json)) = row else {
            return Ok(Settings::default());
        };
        match version {
            1 => {
                // Load-time settings migration v1 -> v2 (FX-005 layout): the
                // stored preferences carry over and defaults are appended.
                // The migrated row is persisted so migration happens once.
                let v1: SettingsV1 = serde_json::from_str(&json)?;
                let migrated = v1.into_v2();
                self.save_settings(&migrated)?;
                Ok(migrated)
            }
            v if v == SETTINGS_SCHEMA_VERSION as i64 => {
                let settings: Settings = serde_json::from_str(&json)?;
                settings.validate()?;
                Ok(settings)
            }
            found => Err(StoreError::Settings(
                SettingsError::UnsupportedSchemaVersion {
                    found: found as u32,
                    supported: SETTINGS_SCHEMA_VERSION,
                },
            )),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), StoreError> {
        settings.validate()?;
        let json = serde_json::to_string(settings)?;
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO app_settings (key, schema_version, json_value) \
             VALUES ('app', ?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET schema_version = ?1, json_value = ?2",
            rusqlite::params![settings.schema_version as i64, json],
        )?;
        Ok(())
    }

    fn lock_conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned mutex means a previous statement panicked mid-transaction;
        // recovering the connection is safe (SQLite is crash-consistent).
        match self.conn.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Apply `migrations` in order. Already-applied versions are checksum-
/// verified; newer ones run each inside its own transaction. A database
/// claiming a version newer than the known list is refused.
pub(crate) fn apply_migrations(
    conn: &mut Connection,
    migrations: &[Migration],
) -> Result<(), StoreError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A second, test-only migration proving ordered upgrades work and
    /// preserve data. Not shipped: production currently stops at V1.
    const MIGRATIONS_WITH_V2: &[Migration] = &[
        MIGRATIONS[0],
        Migration {
            version: 2,
            name: "test_only_add_note_column",
            sql: "ALTER TABLE app_settings ADD COLUMN note TEXT;",
        },
    ];

    fn temp_db_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir();
        dir.join(format!(
            "hostlume-test-{}-{}-{}.sqlite3",
            std::process::id(),
            tag,
            uuid::Uuid::new_v4()
        ))
    }

    fn cleanup(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.as_os_str().to_owned();
            p.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(p));
        }
    }

    #[test]
    fn fresh_database_reaches_latest_and_loads_defaults() {
        let path = temp_db_path("fresh");
        let store = SettingsStore::open(&path).expect("open fresh");
        let settings = store.load_settings().expect("defaults load");
        assert_eq!(settings, Settings::default());
        let host = store.host_id().expect("host id");
        assert!(
            uuid::Uuid::parse_str(&host).is_ok(),
            "host id must be a UUID"
        );
        cleanup(&path);
    }

    #[test]
    fn ordered_migration_preserves_data() {
        let path = temp_db_path("upgrade");
        {
            let store =
                SettingsStore::with_migrations(Connection::open(&path).unwrap(), &MIGRATIONS[..1])
                    .unwrap();
            let s = Settings {
                appearance: crate::settings::Appearance::Dark,
                polling_base_seconds: 7,
                ..Settings::default()
            };
            store.save_settings(&s).unwrap();
        }
        {
            let store = SettingsStore::with_migrations(
                Connection::open(&path).unwrap(),
                MIGRATIONS_WITH_V2,
            )
            .unwrap();
            let s = store.load_settings().unwrap();
            assert_eq!(s.appearance, crate::settings::Appearance::Dark);
            assert_eq!(s.polling_base_seconds, 7, "data must survive migration");
        }
        // The migration ledger recorded both steps exactly once.
        let conn = Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        cleanup(&path);
    }

    #[test]
    fn checksum_mismatch_is_refused() {
        let path = temp_db_path("drift");
        SettingsStore::open(&path).unwrap();
        let drifted = &[Migration {
            version: 1,
            name: MIGRATIONS[0].name,
            sql: "-- tampered\nCREATE TABLE other (id INTEGER);",
        }];
        let err = SettingsStore::with_migrations(Connection::open(&path).unwrap(), drifted)
            .expect_err("drifted database must be refused");
        assert!(matches!(
            err,
            StoreError::ChecksumMismatch { version: 1, .. }
        ));
        cleanup(&path);
    }

    #[test]
    fn database_from_the_future_is_refused() {
        let path = temp_db_path("future");
        {
            let mut conn = Connection::open(&path).unwrap();
            apply_migrations(&mut conn, MIGRATIONS).unwrap();
            conn.execute_batch(
                "INSERT INTO schema_migrations (version, name, checksum, applied_at_utc) \
                 VALUES (9, 'from_the_future', 'x', '2026-01-01T00:00:00.000Z');",
            )
            .unwrap();
        }
        let err = SettingsStore::open(&path).expect_err("future database must be refused");
        assert!(matches!(err, StoreError::DatabaseTooNew { db: 9, app: 1 }));
        cleanup(&path);
    }

    #[test]
    fn host_id_is_stable_across_reopens() {
        let path = temp_db_path("host");
        let first = SettingsStore::open(&path).unwrap().host_id().unwrap();
        let second = SettingsStore::open(&path).unwrap().host_id().unwrap();
        assert_eq!(first, second, "installation UUID must not rotate");
        cleanup(&path);
    }

    #[test]
    fn invalid_settings_never_persist() {
        let path = temp_db_path("invalid");
        let store = SettingsStore::open(&path).unwrap();
        let s = Settings {
            polling_base_seconds: 500,
            ..Settings::default()
        };
        assert!(store.save_settings(&s).is_err(), "out-of-bounds rejected");
        assert_eq!(store.load_settings().unwrap(), Settings::default());
        cleanup(&path);
    }

    #[test]
    fn v1_row_migrates_to_v2_on_load_and_persists() {
        let path = temp_db_path("settings-v1-migration");
        let store = SettingsStore::open(&path).unwrap();
        // Simulate a v1-era row: version 1, JSON without overview_cards.
        let v1_json = r#"{"schema_version":1,"appearance":"dark","compact_mode":true,"polling_base_seconds":5,"close_to_tray":false,"locale":"ro"}"#;
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO app_settings (key, schema_version, json_value) VALUES ('app', 1, ?1)",
                rusqlite::params![v1_json],
            )
            .unwrap();
        }
        let migrated = store.load_settings().unwrap();
        assert_eq!(migrated.schema_version, 2);
        assert_eq!(migrated.appearance, crate::settings::Appearance::Dark);
        assert_eq!(migrated.polling_base_seconds, 5);
        assert_eq!(migrated.locale, "ro");
        assert_eq!(
            migrated.overview_cards,
            crate::settings::default_overview_cards(),
            "defaults appended by migration"
        );
        // Migration persisted: reloads return v2 directly.
        let again = store.load_settings().unwrap();
        assert_eq!(again, migrated);
        let conn = store.conn.lock().unwrap();
        let version: i64 = conn
            .query_row(
                "SELECT schema_version FROM app_settings WHERE key = 'app'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 2);
        drop(conn);
        cleanup(&path);
    }
}
