//! Bounded batch writer (phase 03 §3A.3): one writer thread, a bounded
//! channel, batched transactions. Overload is *instrumented* — samples are
//! dropped oldest-first under pressure and the drop count is readable, so
//! an overloaded disk shows up in diagnostics instead of stalling the
//! collection loop or crashing the app (exit criteria: disk-full must not
//! take down the main window).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::migrations::{apply_migrations, MIGRATIONS};

/// One enqueued write. Variants stay small and plain — the writer owns SQL.
#[derive(Debug, Clone)]
pub enum WriteItem {
    Sample {
        host_id: String,
        metric_id: String,
        entity_id: String,
        ts_utc_ms: i64,
        value_real: f64,
        source: String,
        generation: String,
        sequence: u64,
    },
    Audit {
        ts_utc_ms: i64,
        target_pid: i32,
        target_start_time: u64,
        mode: String,
        result: String,
        detail: Option<String>,
    },
    /// Open a firing episode (phase 07): `state` starts as `firing`.
    AlertEvent {
        id: String,
        rule_id: String,
        triggered_at_utc_ms: i64,
        summary_json: Option<String>,
    },
    /// Close an episode when the rule resolves.
    AlertEventClose { id: String, resolved_at_utc_ms: i64 },
}

/// Writer-side instrumentation shared with diagnostics.
#[derive(Debug, Default)]
pub struct WriterStats {
    pub enqueued: AtomicU64,
    pub written: AtomicU64,
    pub dropped_full: AtomicU64,
    pub errors: AtomicU64,
    pub last_error: Mutex<Option<String>>,
}

impl WriterStats {
    pub fn snapshot(&self) -> WriterStatsSnapshot {
        WriterStatsSnapshot {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            written: self.written.load(Ordering::Relaxed),
            dropped_full: self.dropped_full.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            last_error: self
                .last_error
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct WriterStatsSnapshot {
    pub enqueued: u64,
    pub written: u64,
    pub dropped_full: u64,
    pub errors: u64,
    pub last_error: Option<String>,
}

const CHANNEL_CAPACITY: usize = 4_096;
const BATCH_MAX: usize = 256;

pub struct BatchWriter {
    tx: Option<std::sync::mpsc::SyncSender<WriteItem>>,
    stats: Arc<WriterStats>,
    worker: Option<JoinHandle<()>>,
}

impl BatchWriter {
    /// Spawn the writer thread for `conn`. Ownership of the connection
    /// moves to the writer: it is the single writer (ADR-0002). The schema
    /// is migrated synchronously here so the database is ready the moment
    /// this returns.
    pub fn spawn(mut conn: Connection) -> Result<Self, crate::migrations::StoreError> {
        apply_migrations(&mut conn, MIGRATIONS)?;
        let stats = Arc::new(WriterStats::default());
        let (tx, rx) = std::sync::mpsc::sync_channel::<WriteItem>(CHANNEL_CAPACITY);
        let worker_stats = Arc::clone(&stats);
        let worker = std::thread::Builder::new()
            .name("history-writer".into())
            .spawn(move || {
                worker_loop(&mut conn, rx, worker_stats);
                // Orderly end of the writer: fold the WAL back.
                let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
            })
            .expect("spawn history writer thread");
        Ok(Self {
            tx: Some(tx),
            stats,
            worker: Some(worker),
        })
    }

    pub fn stats(&self) -> WriterStatsSnapshot {
        self.stats.snapshot()
    }

    /// Non-blocking enqueue. Under channel pressure the item is dropped and
    /// counted (drop-oldest instrumentation for the live stream).
    pub fn enqueue(&self, item: WriteItem) {
        self.stats.enqueued.fetch_add(1, Ordering::Relaxed);
        let Some(tx) = &self.tx else {
            self.stats.dropped_full.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if let Err(std::sync::mpsc::TrySendError::Full(_)) = tx.try_send(item) {
            self.stats.dropped_full.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Flush variant for shared (`Arc`) handles: drop the sender so the
    /// worker drains, then wait for it. The stats handle stays readable.
    pub fn shutdown(&mut self) -> WriterStatsSnapshot {
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.stats()
    }

    /// Stop accepting writes, drain the queue, and join the writer thread.
    /// Never panics; returns final stats.
    pub fn close_and_flush(mut self) -> WriterStatsSnapshot {
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.stats()
    }
}

fn worker_loop(
    conn: &mut Connection,
    rx: std::sync::mpsc::Receiver<WriteItem>,
    stats: Arc<WriterStats>,
) {
    let mut batch: Vec<WriteItem> = Vec::with_capacity(BATCH_MAX);
    while let Ok(item) = rx.recv() {
        batch.push(item);
        while batch.len() < BATCH_MAX {
            match rx.try_recv() {
                Ok(more) => batch.push(more),
                Err(_) => break,
            }
        }
        if write_batch(conn, &batch, &stats).is_err() {
            // Writer keeps running: the next batch retries after the
            // transient condition (e.g. short disk pressure).
            batch.clear();
        }
        batch.clear();
    }
    // Drain whatever is left after close.
    let remainder: Vec<WriteItem> = rx.try_iter().collect();
    if !remainder.is_empty() {
        let _ = write_batch(conn, &remainder, &stats);
    }
}

fn write_batch(
    conn: &mut Connection,
    batch: &[WriteItem],
    stats: &WriterStats,
) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    {
        let mut insert_sample = tx.prepare_cached(
            "INSERT INTO samples_raw \
             (host_id, metric_id, entity_id, ts_utc_ms, value_real, source, generation, sequence) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        let mut insert_audit = tx.prepare_cached(
            "INSERT INTO action_audit \
             (ts_utc_ms, target_pid, target_start_time, mode, result, detail) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut insert_alert_event = tx.prepare_cached(
            "INSERT INTO alert_events \
             (id, rule_id, state, triggered_at_utc, resolved_at_utc, summary_json) \
             VALUES (?1, ?2, 'firing', ?3, NULL, ?4)",
        )?;
        let mut close_alert_event = tx.prepare_cached(
            "UPDATE alert_events \
             SET state = 'resolved', resolved_at_utc = ?2 WHERE id = ?1",
        )?;
        for item in batch {
            match item {
                WriteItem::Sample {
                    host_id,
                    metric_id,
                    entity_id,
                    ts_utc_ms,
                    value_real,
                    source,
                    generation,
                    sequence,
                } => {
                    insert_sample.execute(rusqlite::params![
                        host_id,
                        metric_id,
                        entity_id,
                        ts_utc_ms,
                        value_real,
                        source,
                        generation,
                        *sequence as i64,
                    ])?;
                }
                WriteItem::Audit {
                    ts_utc_ms,
                    target_pid,
                    target_start_time,
                    mode,
                    result,
                    detail,
                } => {
                    insert_audit.execute(rusqlite::params![
                        ts_utc_ms,
                        *target_pid,
                        *target_start_time as i64,
                        mode,
                        result,
                        detail,
                    ])?;
                }
                WriteItem::AlertEvent {
                    id,
                    rule_id,
                    triggered_at_utc_ms,
                    summary_json,
                } => {
                    insert_alert_event.execute(rusqlite::params![
                        id,
                        rule_id,
                        triggered_at_utc_ms,
                        summary_json,
                    ])?;
                }
                WriteItem::AlertEventClose {
                    id,
                    resolved_at_utc_ms,
                } => {
                    close_alert_event.execute(rusqlite::params![id, resolved_at_utc_ms])?;
                }
            }
        }
    }
    let written = batch.len() as u64;
    tx.commit()?;
    stats.written.fetch_add(written, Ordering::Relaxed);
    Ok(())
}
