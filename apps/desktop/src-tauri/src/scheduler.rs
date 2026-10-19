//! The collection scheduler (phase 02 §2A): one loop drives the fast /
//! medium / slow poll groups — never one timer per card. Group ticks merge
//! their readings into a keyed latest-value map; every published snapshot
//! is the full merged state with a strictly increasing sequence.
//!
//! Error isolation: a failing collector's previously owned readings are
//! replaced by explicit `error` states (null values) — other collectors'
//! data is untouched, and the loop keeps running.
//!
//! Sleep/resume: rate collectors detect wall-vs-monotonic skew themselves
//! and report `SLEEP_GAP`; the UI additionally staleness-marks when ticks
//! stop arriving.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use monitor_core::{
    reasons, Availability, Clock, Collector, MetricReading, MetricUnit, PollGroup, SystemClock,
    SystemSnapshot,
};
use serde::Serialize;
use tokio::sync::watch;

use crate::collectors;

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

/// Called with each published snapshot right after it is stored (used by
/// the history writer; the snapshot carries host_id and generation).
pub type SnapshotTap = Option<Arc<dyn Fn(&SystemSnapshot) + Send + Sync>>;

/// Interval source seam: production reads the settings-backed atomic; tests
/// inject tiny intervals to drive many ticks quickly.
pub type IntervalSource = Arc<dyn Fn() -> Duration + Send + Sync>;

pub const TELEMETRY_EVENT: &str = "telemetry://snapshot";

/// Default cadences for the non-settings poll groups (architecture.md §5).
pub const MEDIUM_INTERVAL: Duration = Duration::from_secs(10);
pub const SLOW_INTERVAL: Duration = Duration::from_secs(60);

/// Per-collector health, surfaced through `get_diagnostics`. Host
/// identifiers are deliberately not included (security-privacy.md §2).
#[derive(Debug, Clone, Serialize)]
pub struct CollectorDiagnostic {
    pub id: String,
    pub group: &'static str,
    pub ticks_total: u64,
    pub error_count: u64,
    #[serde(rename = "lastOkAtUtc")]
    pub last_ok_at_utc: Option<String>,
    #[serde(rename = "lastDurationMs")]
    pub last_duration_ms: Option<u64>,
    #[serde(rename = "lastError", skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

pub type DiagnosticsMap = Arc<Mutex<HashMap<String, CollectorDiagnostic>>>;

#[derive(Debug, Clone, Serialize, Default)]
pub struct DiagnosticsReport {
    pub collectors: Vec<CollectorDiagnostic>,
}

struct Registered {
    collector: Box<dyn Collector>,
    /// (metric_id, entity_id) keys this collector emitted last — used to
    /// swap in error states when it fails.
    owned: HashSet<(String, String)>,
}

pub struct SchedulerHandle {
    stop: watch::Sender<bool>,
}

impl SchedulerHandle {
    /// Ask the loop to finish after the current tick; never blocks.
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

/// Production entry point: registers the portable collectors and drives the
/// groups with settings-backed fast cadence.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    host_id: String,
    generation: String,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
    polling_secs: Arc<AtomicU32>,
    capabilities_revision: Arc<AtomicU64>,
    diagnostics: DiagnosticsMap,
    emit: TelemetryEmitter,
    snapshot_tap: SnapshotTap,
) -> SchedulerHandle {
    let collectors_list: Vec<Box<dyn Collector>> = vec![
        Box::new(collectors::CpuCollector::new()),
        Box::new(collectors::NetworkCollector::new()),
        Box::new(collectors::MemoryCollector::new()),
        Box::new(collectors::HostCollector::new()),
        Box::new(collectors::DisksCollector::new()),
    ];
    let fast_interval: IntervalSource =
        Arc::new(move || Duration::from_secs(polling_secs.load(Ordering::Relaxed).max(1) as u64));
    spawn_engine(
        host_id,
        generation,
        collectors_list,
        latest,
        capabilities_revision,
        diagnostics,
        emit,
        snapshot_tap,
        fast_interval,
        MEDIUM_INTERVAL,
        SLOW_INTERVAL,
    )
}

/// Fully injectable entry point (production `spawn` delegates here; tests
/// register fake collectors with tiny intervals).
#[allow(clippy::too_many_arguments)]
pub fn spawn_engine(
    host_id: String,
    generation: String,
    collectors_list: Vec<Box<dyn Collector>>,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
    capabilities_revision: Arc<AtomicU64>,
    diagnostics: DiagnosticsMap,
    emit: TelemetryEmitter,
    snapshot_tap: SnapshotTap,
    fast_interval: IntervalSource,
    medium_interval: Duration,
    slow_interval: Duration,
) -> SchedulerHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    tauri::async_runtime::spawn(run_loop(
        Engine {
            host_id,
            generation,
            collectors: collectors_list
                .into_iter()
                .map(|collector| Registered {
                    collector,
                    owned: HashSet::new(),
                })
                .collect(),
            latest,
            capabilities_revision,
            diagnostics,
            emit,
            snapshot_tap,
            current: HashMap::new(),
            sequence: 0,
            clock: SystemClock,
            fast_interval,
            medium_interval,
            slow_interval,
        },
        stop_rx,
    ));
    SchedulerHandle { stop: stop_tx }
}

struct Engine {
    host_id: String,
    generation: String,
    collectors: Vec<Registered>,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
    capabilities_revision: Arc<AtomicU64>,
    diagnostics: DiagnosticsMap,
    emit: TelemetryEmitter,
    snapshot_tap: SnapshotTap,
    current: HashMap<(String, String), MetricReading>,
    sequence: u64,
    clock: SystemClock,
    fast_interval: IntervalSource,
    medium_interval: Duration,
    slow_interval: Duration,
}

impl Engine {
    fn tick(&mut self, group: PollGroup) {
        for registered in &mut self.collectors {
            if registered.collector.poll_group() != group {
                continue;
            }
            let collector_id = registered.collector.id();
            let started = std::time::Instant::now();
            let outcome = registered.collector.sample(&self.clock);
            let duration_ms = started.elapsed().as_millis() as u64;
            match outcome {
                Ok(readings) => {
                    registered.owned.clear();
                    for reading in readings {
                        registered
                            .owned
                            .insert((reading.metric_id.clone(), reading.entity_id.clone()));
                        self.current.insert(
                            (reading.metric_id.clone(), reading.entity_id.clone()),
                            reading,
                        );
                    }
                    Self::record_ok(
                        &self.diagnostics,
                        collector_id,
                        group,
                        duration_ms,
                        &self.clock,
                    );
                }
                Err(error) => {
                    // Isolate: replace this collector's previous readings
                    // with explicit error states (null values).
                    let error_reading = |key: (String, String)| MetricReading {
                        metric_id: key.0,
                        entity_id: key.1,
                        value: None,
                        unit: MetricUnit::Text,
                        availability: Availability::Error,
                        source: collector_id.to_string(),
                        collected_at_utc: self.clock.now_utc_iso8601(),
                        monotonic_sequence: 0,
                        reason_code: Some(reasons::COLLECTOR_ERROR.to_string()),
                        human_message_key: Some("states.error".to_string()),
                        precision: None,
                    };
                    for key in registered.owned.drain() {
                        self.current.insert(key.clone(), error_reading(key));
                    }
                    Self::record_error(
                        &self.diagnostics,
                        collector_id,
                        group,
                        duration_ms,
                        &error.to_string(),
                    );
                }
            }
        }
    }

    fn publish(&mut self) -> TelemetryEventPayload {
        self.sequence += 1;
        let collected_at_utc = self.clock.now_utc_iso8601();
        let mut metrics: Vec<MetricReading> = self.current.values().cloned().collect();
        for reading in &mut metrics {
            reading.monotonic_sequence = self.sequence;
        }
        let mut snapshot = SystemSnapshot::empty(
            self.host_id.clone(),
            self.generation.clone(),
            self.sequence,
            collected_at_utc.clone(),
        );
        snapshot.metrics = metrics;
        snapshot.capabilities_revision = self.capabilities_revision.load(Ordering::Relaxed);
        if let Ok(mut guard) = self.latest.write() {
            *guard = Some(snapshot);
        }
        if let Some(tap) = &self.snapshot_tap {
            if let Ok(guard) = self.latest.read() {
                if let Some(published) = guard.as_ref() {
                    tap(published);
                }
            }
        }
        TelemetryEventPayload {
            schema_version: monitor_core::SCHEMA_VERSION,
            host_id: self.host_id.clone(),
            generation: self.generation.clone(),
            sequence: self.sequence,
            collected_at_utc,
            capabilities_revision: self.capabilities_revision.load(Ordering::Relaxed),
        }
    }

    fn record_ok(
        diagnostics: &DiagnosticsMap,
        id: &str,
        group: PollGroup,
        duration_ms: u64,
        clock: &dyn Clock,
    ) {
        let mut map = diagnostics.lock().unwrap_or_else(|p| p.into_inner());
        let entry = map
            .entry(id.to_string())
            .or_insert_with(|| CollectorDiagnostic {
                id: id.to_string(),
                group: group_name(group),
                ticks_total: 0,
                error_count: 0,
                last_ok_at_utc: None,
                last_duration_ms: None,
                last_error: None,
            });
        entry.ticks_total += 1;
        entry.last_ok_at_utc = Some(clock.now_utc_iso8601());
        entry.last_duration_ms = Some(duration_ms);
    }

    fn record_error(
        diagnostics: &DiagnosticsMap,
        id: &str,
        group: PollGroup,
        duration_ms: u64,
        error: &str,
    ) {
        let mut map = diagnostics.lock().unwrap_or_else(|p| p.into_inner());
        let entry = map
            .entry(id.to_string())
            .or_insert_with(|| CollectorDiagnostic {
                id: id.to_string(),
                group: group_name(group),
                ticks_total: 0,
                error_count: 0,
                last_ok_at_utc: None,
                last_duration_ms: None,
                last_error: None,
            });
        entry.ticks_total += 1;
        entry.error_count += 1;
        entry.last_duration_ms = Some(duration_ms);
        // Bound the stored error text; diagnostics must stay small.
        let mut text = error.to_string();
        text.truncate(200);
        entry.last_error = Some(text);
    }
}

fn group_name(group: PollGroup) -> &'static str {
    match group {
        PollGroup::Fast => "fast",
        PollGroup::Medium => "medium",
        PollGroup::Slow => "slow",
    }
}

async fn run_loop(mut engine: Engine, mut stop_rx: watch::Receiver<bool>) {
    let now = std::time::Instant::now();
    let mut next_fast = now; // fast fires immediately: warm-up states first
    let mut next_medium = now + engine.medium_interval;
    let mut next_slow = now + engine.slow_interval;
    loop {
        if *stop_rx.borrow() {
            break;
        }
        let now = std::time::Instant::now();
        let mut published = false;
        if now >= next_fast {
            engine.tick(PollGroup::Fast);
            next_fast = std::time::Instant::now() + (engine.fast_interval)();
            published = true;
        }
        if now >= next_medium {
            engine.tick(PollGroup::Medium);
            next_medium = std::time::Instant::now() + engine.medium_interval;
            published = true;
        }
        if now >= next_slow {
            engine.tick(PollGroup::Slow);
            next_slow = std::time::Instant::now() + engine.slow_interval;
            published = true;
        }
        if published {
            let payload = engine.publish();
            (engine.emit)(&payload);
        }
        let next_due = next_fast.min(next_medium).min(next_slow);
        tokio::select! {
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(next_due)) => {}
            _ = stop_rx.changed() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::{CollectorError, MetricCapability, MetricValue};
    use std::sync::atomic::AtomicU64;

    struct StubCollector {
        id: &'static str,
        group: PollGroup,
        /// Fail on every tick once this many successful samples elapsed;
        /// `None` fails immediately (the collector never produces a row).
        fail_after: Option<u32>,
        value: u64,
        ticks: u32,
    }

    impl Collector for StubCollector {
        fn id(&self) -> &'static str {
            self.id
        }
        fn poll_group(&self) -> PollGroup {
            self.group
        }
        fn capabilities(&self) -> Vec<MetricCapability> {
            Vec::new()
        }
        fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
            self.ticks += 1;
            if self.fail_after.is_none_or(|after| self.ticks > after) {
                return Err(CollectorError::Other {
                    collector: self.id,
                    message: "injected failure".to_string(),
                });
            }
            Ok(vec![reading_stub(
                &format!("{}.value", self.id),
                "entity-1",
                self.value,
                clock,
            )])
        }
    }

    fn reading_stub(
        metric_id: &str,
        entity_id: &str,
        value: u64,
        clock: &dyn Clock,
    ) -> MetricReading {
        MetricReading {
            metric_id: metric_id.to_string(),
            entity_id: entity_id.to_string(),
            value: Some(MetricValue::Number(value as f64)),
            unit: MetricUnit::Count,
            availability: Availability::Available,
            source: "stub".to_string(),
            collected_at_utc: clock.now_utc_iso8601(),
            monotonic_sequence: 0,
            reason_code: None,
            human_message_key: None,
            precision: None,
        }
    }

    #[tokio::test]
    async fn groups_merge_isolate_errors_and_publish_diagnostics() {
        let latest: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
        let caps_rev = Arc::new(AtomicU64::new(1));
        let diagnostics: DiagnosticsMap = Arc::default();
        let emitted = Arc::new(AtomicU64::new(0));
        let emitted_for_closure = emitted.clone();
        let emit: TelemetryEmitter = Arc::new(move |_| {
            emitted_for_closure.fetch_add(1, Ordering::Relaxed);
        });

        let handle = spawn_engine(
            "test-host".into(),
            "gen".into(),
            vec![
                Box::new(StubCollector {
                    id: "ok-fast",
                    group: PollGroup::Fast,
                    fail_after: Some(u32::MAX),
                    value: 7,
                    ticks: 0,
                }),
                // Succeeds once, then fails: its owned key must flip to an
                // explicit error state on the next tick.
                Box::new(StubCollector {
                    id: "broken-fast",
                    group: PollGroup::Fast,
                    fail_after: Some(1),
                    value: 0,
                    ticks: 0,
                }),
                Box::new(StubCollector {
                    id: "ok-slow",
                    group: PollGroup::Slow,
                    fail_after: Some(u32::MAX),
                    value: 99,
                    ticks: 0,
                }),
                // Never succeeds: contributes nothing (no invented rows).
                Box::new(StubCollector {
                    id: "dead-fast",
                    group: PollGroup::Fast,
                    fail_after: None,
                    value: 0,
                    ticks: 0,
                }),
            ],
            latest.clone(),
            caps_rev.clone(),
            diagnostics.clone(),
            emit,
            None,
            Arc::new(|| Duration::from_millis(10)),
            Duration::from_millis(20),
            Duration::from_millis(30),
        );

        // Let several rounds of every group run.
        tokio::time::sleep(Duration::from_millis(150)).await;
        handle.stop();
        tokio::time::sleep(Duration::from_millis(40)).await;

        let snapshot = latest.read().unwrap().clone().expect("snapshot published");
        // Merged state contains values from healthy collectors...
        let ok_value = snapshot
            .metrics
            .iter()
            .find(|r| r.metric_id == "ok-fast.value" && r.entity_id == "entity-1")
            .expect("fast collector value present");
        assert_eq!(ok_value.availability, Availability::Available);
        assert!(snapshot
            .metrics
            .iter()
            .any(|r| r.metric_id == "ok-slow.value" && r.availability == Availability::Available));
        // ...and explicit error states for the broken one — no gap, no zero.
        let failed = snapshot
            .metrics
            .iter()
            .find(|r| r.metric_id == "broken-fast.value" && r.entity_id == "entity-1")
            .expect("broken collector still owns its keys");
        assert_eq!(failed.availability, Availability::Error);
        assert_eq!(failed.value, None);
        assert_eq!(
            failed.reason_code.as_deref(),
            Some(reasons::COLLECTOR_ERROR)
        );
        // Sequence strictly increasing and stamped onto readings.
        assert!(snapshot.sequence >= 2);
        assert!(snapshot
            .metrics
            .iter()
            .all(|r| r.monotonic_sequence == snapshot.sequence));

        // Diagnostics: healthy ticks recorded; errors counted and bounded.
        let map = diagnostics.lock().unwrap();
        let ok = map.get("ok-fast").expect("ok-fast diag");
        assert!(ok.ticks_total >= 2);
        assert!(ok.last_ok_at_utc.is_some());
        let broken = map.get("broken-fast").expect("broken diag");
        assert!(broken.error_count >= 2);
        assert_eq!(
            broken.last_error.as_deref(),
            Some("collector broken-fast: internal error: injected failure")
        );
        assert!(emitted.load(Ordering::Relaxed) >= 2, "each round published");
        // A collector that never succeeded contributes no rows at all —
        // absence, not a fabricated zero or placeholder.
        assert!(snapshot
            .metrics
            .iter()
            .all(|r| r.source != "stub" || !r.metric_id.starts_with("dead-fast")));
    }

    #[tokio::test]
    async fn live_collectors_produce_real_readings_through_the_engine() {
        // End-to-end (still headless): the production portable collectors
        // feed the engine; within ~1.5 s the snapshot must contain a real,
        // available CPU usage and memory figures for this machine.
        use crate::collectors::{CpuCollector, MemoryCollector};

        let latest: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
        let caps_rev = Arc::new(AtomicU64::new(1));
        let diagnostics: DiagnosticsMap = Arc::default();
        let handle = spawn_engine(
            "live-host".into(),
            "gen-live".into(),
            vec![
                Box::new(CpuCollector::new()),
                Box::new(MemoryCollector::new()),
            ],
            latest.clone(),
            caps_rev.clone(),
            diagnostics,
            Arc::new(|_| {}),
            None,
            Arc::new(|| Duration::from_millis(300)),
            Duration::from_millis(400),
            Duration::from_secs(60),
        );
        tokio::time::sleep(Duration::from_millis(1_600)).await;
        handle.stop();

        let snapshot = latest.read().unwrap().clone().expect("live snapshot");
        let cpu = snapshot
            .metrics
            .iter()
            .find(|r| r.metric_id == monitor_core::families::CPU_TOTAL_USAGE)
            .expect("cpu usage present");
        assert_eq!(cpu.availability, Availability::Available, "past warm-up");
        assert!(matches!(cpu.value, Some(MetricValue::Number(n)) if (0.0..=100.0).contains(&n)));
        let mem = snapshot
            .metrics
            .iter()
            .find(|r| r.metric_id == monitor_core::families::MEMORY_PHYSICAL_TOTAL)
            .expect("memory total present");
        assert!(matches!(mem.value, Some(MetricValue::Number(n)) if n > 0.0));
    }

    #[tokio::test]
    async fn stop_is_graceful_and_quiets_the_loop() {
        let latest: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
        let caps_rev = Arc::new(AtomicU64::new(1));
        let diagnostics: DiagnosticsMap = Arc::default();
        let emitted = Arc::new(AtomicU64::new(0));
        let emitted_for_closure = emitted.clone();
        let emit: TelemetryEmitter = Arc::new(move |_| {
            emitted_for_closure.fetch_add(1, Ordering::Relaxed);
        });
        let handle = spawn_engine(
            "h".into(),
            "g".into(),
            vec![Box::new(StubCollector {
                id: "fast",
                group: PollGroup::Fast,
                fail_after: Some(u32::MAX),
                value: 1,
                ticks: 0,
            })],
            latest.clone(),
            caps_rev.clone(),
            diagnostics,
            emit,
            None,
            Arc::new(|| Duration::from_millis(10)),
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(emitted.load(Ordering::Relaxed) >= 2);
        handle.stop();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let after_stop = emitted.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(emitted.load(Ordering::Relaxed), after_stop);
    }
}
