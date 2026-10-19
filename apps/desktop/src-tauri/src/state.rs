//! Shared application state managed by Tauri. Phase 01 keeps this to what
//! the four commands and the lifecycle actually consume; fields arrive with
//! the features that read them, not ahead of them.

use std::sync::atomic::AtomicU32;
use std::sync::{Arc, RwLock};

use monitor_core::{CapabilityReport, SystemSnapshot};

use crate::scheduler::SchedulerHandle;
use crate::settings_store::SettingsStore;

pub struct AppState {
    /// Latest published snapshot. `None` until the scheduler's first tick:
    /// the UI renders its honest "no sample yet" state, never zeros.
    pub latest: Arc<RwLock<Option<SystemSnapshot>>>,
    pub capabilities: CapabilityReport,
    pub store: Arc<SettingsStore>,
    /// Settings-backed base polling seconds, read by the scheduler each tick.
    pub polling_secs: Arc<AtomicU32>,
    pub scheduler: SchedulerHandle,
}
