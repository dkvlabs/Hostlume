//! Open/upgrade/migrate a history database and hand out connections.

use std::path::Path;

use rusqlite::Connection;

pub use crate::migrations::StoreError;
use crate::migrations::{apply_migrations, MIGRATIONS};

pub struct HistoryStore {
    conn: Connection,
}

impl std::fmt::Debug for HistoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryStore").finish_non_exhaustive()
    }
}

impl HistoryStore {
    /// Open (creating parent directories) with WAL and the migration set.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        Self::configure(&mut conn);
        apply_migrations(&mut conn, MIGRATIONS)?;
        Ok(Self { conn })
    }

    /// Open an existing connection (tests, in-memory databases).
    pub fn with_connection(mut conn: Connection) -> Result<Self, StoreError> {
        Self::configure(&mut conn);
        apply_migrations(&mut conn, MIGRATIONS)?;
        Ok(Self { conn })
    }

    fn configure(conn: &mut Connection) {
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.busy_timeout(std::time::Duration::from_millis(2_000));
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// WAL checkpoint on orderly shutdown so the main DB file is complete.
    pub fn checkpoint_on_close(self) -> Result<(), StoreError> {
        self.conn
            .pragma_update(None, "wal_checkpoint", "TRUNCATE")
            .map_err(|err| err.into())
    }
}
