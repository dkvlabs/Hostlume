//! Shared application state managed by Tauri.

use std::sync::atomic::AtomicU32;
use std::sync::{Arc, RwLock};

use monitor_core::{CapabilityReport, SystemSnapshot};

use crate::scheduler::{DiagnosticsMap, SchedulerHandle};
use crate::settings_store::SettingsStore;

pub struct AppState {
    /// Latest published snapshot. Until the scheduler's first tick this is
    /// `None`: the UI renders its honest "no sample yet" state, never zeros.
    pub latest: Arc<RwLock<Option<SystemSnapshot>>>,
    pub capabilities: CapabilityReport,
    pub store: Arc<SettingsStore>,
    /// Settings-backed base polling seconds, read by the scheduler each tick.
    pub polling_secs: Arc<AtomicU32>,
    pub diagnostics: DiagnosticsMap,
    pub scheduler: SchedulerHandle,
}
