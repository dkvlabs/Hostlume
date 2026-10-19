//! Shared application state managed by Tauri.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};

use monitor_core::{CapabilityReport, Clock, SystemSnapshot};
use monitor_history::BatchWriter;

use crate::processes::ProcessManager;
use crate::scheduler::{DiagnosticsMap, SchedulerHandle};
use crate::settings_store::SettingsStore;

pub struct AppState {
    pub host_id: String,
    /// Collector-set generation; stamped onto persisted samples so restarts
    /// and collector resets surface as gaps in history queries.
    pub generation: String,
    /// Latest published snapshot. Until the scheduler's first tick this is
    /// `None`: the UI renders its honest "no sample yet" state, never zeros.
    pub latest: Arc<RwLock<Option<SystemSnapshot>>>,
    /// Merged capability report. Behind a mutex so the explicit re-probe
    /// command can refresh it; consumers get a consistent clone.
    pub capabilities: Mutex<CapabilityReport>,
    /// Shared with the scheduler; bumped when the report is re-probed so
    /// listeners re-render capability-dependent UI.
    pub capabilities_revision: Arc<AtomicU64>,
    /// Sampling paused (tray toggle, phase 06 §2.9).
    pub paused: Arc<AtomicBool>,
    /// Tray controller; `None` only if construction failed (typed error in
    /// diagnostics — the app keeps running without the tray).
    pub tray: Option<std::sync::Arc<crate::tray::TrayController>>,
    pub store: Arc<SettingsStore>,
    /// Settings-backed base polling seconds, read by the scheduler each tick.
    pub polling_secs: Arc<AtomicU32>,
    pub diagnostics: DiagnosticsMap,
    pub scheduler: SchedulerHandle,
    /// Bounded history writer (single thread; drops counted under load),
    /// shared with the scheduler's snapshot tap. `None` after flush at exit.
    pub history_writer: Mutex<Option<Arc<BatchWriter>>>,
    /// Process manager for the Applications page and guarded actions.
    pub processes: Mutex<ProcessManager>,
    /// Active storage scans (FX-008).
    pub scans: crate::storage::ScanRegistry,
    /// History database directory (for read-side query connections).
    pub history_dir: Mutex<Option<std::path::PathBuf>>,
}

impl AppState {
    /// Enqueue one available numeric reading for persistence. No-op after
    /// the writer closed at shutdown.
    pub fn record_sample(&self, reading: &monitor_core::MetricReading) {
        let Some(monitor_core::MetricValue::Number(value)) = reading.value else {
            return; // only available numeric values are persisted
        };
        let guard = self
            .history_writer
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(writer) = guard.as_ref() {
            writer.enqueue(monitor_history::WriteItem::Sample {
                host_id: self.host_id.clone(),
                metric_id: reading.metric_id.clone(),
                entity_id: reading.entity_id.clone(),
                ts_utc_ms: monitor_core::SystemClock.now_epoch_ms(),
                value_real: value,
                source: reading.source.clone(),
                generation: self.generation.clone(),
                sequence: reading.monotonic_sequence,
            });
        }
    }

    /// Write one process-action audit row (privacy: pid + start time +
    /// mode + result; no command lines, no paths).
    pub fn record_audit(
        &self,
        pid: u32,
        start_time_sec: u64,
        mode: &str,
        result: &str,
        detail: Option<String>,
    ) {
        let guard = self
            .history_writer
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(writer) = guard.as_ref() {
            writer.enqueue(monitor_history::WriteItem::Audit {
                ts_utc_ms: monitor_core::SystemClock.now_epoch_ms(),
                target_pid: pid as i32,
                target_start_time: start_time_sec,
                mode: mode.to_string(),
                result: result.to_string(),
                detail,
            });
        }
    }
}
