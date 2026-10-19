//! The (currently empty) collection scheduler.
//!
//! One loop, one timer — never one timer per card (architecture.md §5). In
//! Phase 01 it produces honest *empty* snapshots: collection is alive, no
//! readings exist yet, and nothing is fabricated. Telemetry-phase collectors
//! will plug into the tick body. Cancellation is graceful: the loop exits
//! between ticks when told to stop.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use monitor_core::{Clock, SystemClock, SystemSnapshot};
use serde::Serialize;
use tokio::sync::watch;

/// Lightweight push notification (ADR-0003). Carries identity/sequence only;
/// consumers fetch the full snapshot over `get_snapshot`.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryEventPayload {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u8,
    #[serde(rename = "hostId")]
    pub host_id: String,
    pub generation: String,
    pub sequence: u64,
    #[serde(rename = "collectedAtUtc")]
    pub collected_at_utc: String,
    #[serde(rename = "capabilitiesRevision")]
    pub capabilities_revision: u64,
}

/// Emitter seam so the loop is unit-testable without a Tauri AppHandle.
pub type TelemetryEmitter = Arc<dyn Fn(&TelemetryEventPayload) + Send + Sync>;

/// Interval source seam: production reads the settings-backed atomic; tests
/// inject tiny intervals to drive many ticks quickly.
pub type IntervalSource = Arc<dyn Fn() -> Duration + Send + Sync>;

pub const TELEMETRY_EVENT: &str = "telemetry://snapshot";

#[derive(Clone)]
pub struct SchedulerHandle {
    stop: watch::Sender<bool>,
}

impl SchedulerHandle {
    /// Ask the loop to finish after the current tick; never blocks.
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

pub fn spawn(
    host_id: String,
    generation: String,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
    polling_secs: Arc<AtomicU32>,
    capabilities_revision: Arc<AtomicU64>,
    emit: TelemetryEmitter,
) -> SchedulerHandle {
    let interval_source: IntervalSource =
        Arc::new(move || Duration::from_secs(polling_secs.load(Ordering::Relaxed).max(1) as u64));
    spawn_with_interval(
        host_id,
        generation,
        latest,
        capabilities_revision,
        emit,
        interval_source,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_with_interval(
    host_id: String,
    generation: String,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
    capabilities_revision: Arc<AtomicU64>,
    emit: TelemetryEmitter,
    interval_source: IntervalSource,
) -> SchedulerHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    tauri::async_runtime::spawn(run_loop(
        host_id,
        generation,
        latest,
        capabilities_revision,
        emit,
        interval_source,
        stop_rx,
    ));
    SchedulerHandle { stop: stop_tx }
}

async fn run_loop(
    host_id: String,
    generation: String,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
    capabilities_revision: Arc<AtomicU64>,
    emit: TelemetryEmitter,
    interval_source: IntervalSource,
    mut stop_rx: watch::Receiver<bool>,
) {
    let clock = SystemClock;
    let mut sequence: u64 = 0;
    loop {
        if *stop_rx.borrow() {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(interval_source()) => {}
            _ = stop_rx.changed() => {}
        }
        if *stop_rx.borrow() {
            break;
        }
        sequence += 1;
        let collected_at_utc = clock.now_utc_iso8601();
        let mut snapshot = SystemSnapshot::empty(
            host_id.clone(),
            generation.clone(),
            sequence,
            collected_at_utc.clone(),
        );
        snapshot.capabilities_revision = capabilities_revision.load(Ordering::Relaxed);
        if let Ok(mut guard) = latest.write() {
            *guard = Some(snapshot);
        }
        emit(&TelemetryEventPayload {
            schema_version: monitor_core::SCHEMA_VERSION,
            host_id: host_id.clone(),
            generation: generation.clone(),
            sequence,
            collected_at_utc,
            capabilities_revision: capabilities_revision.load(Ordering::Relaxed),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ticks_store_empty_snapshots_then_stop_gracefully() {
        let latest: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
        let caps_rev = Arc::new(AtomicU64::new(1));
        let emitted = Arc::new(AtomicU64::new(0));
        let emitted_for_closure = emitted.clone();
        let emit: TelemetryEmitter = Arc::new(move |_| {
            emitted_for_closure.fetch_add(1, Ordering::Relaxed);
        });

        let handle = spawn_with_interval(
            "test-host".into(),
            "gen-test".into(),
            latest.clone(),
            caps_rev.clone(),
            emit,
            Arc::new(|| Duration::from_millis(10)),
        );

        // Let several ticks run.
        tokio::time::sleep(Duration::from_millis(80)).await;
        let count_before_stop = emitted.load(Ordering::Relaxed);
        assert!(
            count_before_stop >= 2,
            "expected multiple ticks, got {count_before_stop}"
        );

        {
            let guard = latest.read().unwrap();
            let snapshot = guard.as_ref().expect("scheduler must publish snapshots");
            assert_eq!(snapshot.sequence, count_before_stop);
            assert_eq!(snapshot.host_id, "test-host");
            assert_eq!(snapshot.generation, "gen-test");
            assert!(
                snapshot.metrics.is_empty(),
                "Phase 01 must not fabricate metrics"
            );
            assert_eq!(snapshot.schema_version, monitor_core::SCHEMA_VERSION);
        }

        handle.stop();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let count_after_stop = emitted.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(
            emitted.load(Ordering::Relaxed),
            count_after_stop,
            "loop must be fully stopped after stop() (graceful cancellation)"
        );
        // Sequences only ever move forward within a generation.
        let guard = latest.read().unwrap();
        assert_eq!(guard.as_ref().unwrap().sequence, count_after_stop);
    }
}
