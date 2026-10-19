//! Tauri commands. Every input is validated in Rust; the WebView is
//! untrusted input (security-privacy.md §2). Errors are structured payloads
//! with i18n keys so the UI renders its own localized messages.

use std::sync::atomic::Ordering;

use std::path::PathBuf;
use std::sync::Arc;

use monitor_core::{CapabilityReport, Clock, SystemSnapshot};
use monitor_history::{HistoryQuery, HistoryResponse};
use serde::Serialize;
use tauri::State;

use crate::processes::{ProcessActionRequest, ProcessActionResult, ProcessPage};
use crate::settings::SettingsPatch;
use crate::state::AppState;
use crate::storage::{ChildEntry, ScanChildren, ScanStatus};

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    #[serde(rename = "messageKey")]
    pub message_key: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl CommandError {
    fn no_scan() -> Self {
        Self {
            code: "SCAN_NOT_FOUND",
            message_key: "errors.scanNotFound",
            reason: None,
        }
    }

    pub(crate) fn no_snapshot_yet() -> Self {
        Self {
            code: "NO_SNAPSHOT_YET",
            message_key: "errors.noSnapshotYet",
            reason: None,
        }
    }

    fn settings_invalid(reason: String) -> Self {
        Self {
            code: "SETTINGS_INVALID",
            message_key: "errors.settingsInvalid",
            reason: Some(reason),
        }
    }

    pub(crate) fn store(reason: String) -> Self {
        Self {
            code: "STORE_ERROR",
            message_key: "errors.store",
            reason: Some(reason),
        }
    }
}

/// Latest normalized snapshot. Before the first tick this is a typed
/// NO_SNAPSHOT_YET error — the UI shows "Collecting…", never zeros.
#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<SystemSnapshot, CommandError> {
    let guard = state
        .latest
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clone().ok_or_else(CommandError::no_snapshot_yet)
}

#[tauri::command]
pub fn get_capabilities(state: State<'_, AppState>) -> CapabilityReport {
    state
        .capabilities
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Re-run every capability probe on demand (phase 05 §2.1/§2.8): the UI's
/// "re-probe" affordance after a driver/permission/device change. Replaces
/// the stored report and bumps the capabilities revision.
#[tauri::command]
pub fn reprobe_capabilities(state: State<'_, AppState>) -> CapabilityReport {
    let report =
        crate::build_capability_report(&state.host_id, monitor_core::SystemClock.now_utc_iso8601());
    let next_revision = state
        .capabilities_revision
        .load(Ordering::Relaxed)
        .wrapping_add(1);
    state
        .capabilities_revision
        .store(next_revision, Ordering::Relaxed);
    let mut guard = state
        .capabilities
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = report;
    guard.clone()
}

/// Graphics adapter and display identity (FX-010). Static metadata probed
/// on demand — separate from the polled metric stream, so live-utilization
/// availability stays honest in the snapshot.
#[tauri::command]
pub fn get_gpu_info() -> monitor_core::GpuInfoReport {
    let adapters = if cfg!(target_os = "macos") {
        monitor_platform_macos::gpu_adapters::adapters().unwrap_or_default()
    } else if cfg!(target_os = "windows") {
        monitor_platform_windows::gpu_adapters::adapters().unwrap_or_default()
    } else if cfg!(target_os = "linux") {
        monitor_platform_linux::gpu_adapters::adapters().unwrap_or_default()
    } else {
        Vec::new()
    };
    monitor_core::GpuInfoReport::new(monitor_core::SystemClock.now_utc_iso8601(), adapters)
}

/// Per-collector health: tick counts, errors, last success and duration.
/// Contains no host-identifying information.
#[tauri::command]
pub fn get_diagnostics(state: State<'_, AppState>) -> crate::scheduler::DiagnosticsReport {
    let map = state
        .diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut collectors: Vec<crate::scheduler::CollectorDiagnostic> =
        map.values().cloned().collect();
    collectors.sort_by(|a, b| a.id.cmp(&b.id));
    crate::scheduler::DiagnosticsReport { collectors }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<crate::settings::Settings, CommandError> {
    state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))
}

/// Validate-then-persist an all-or-nothing settings patch. Invalid values
/// are rejected with the current settings left untouched.
#[tauri::command]
pub fn update_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<crate::settings::Settings, CommandError> {
    let launch_at_login_changed = patch.launch_at_login.is_some();
    let launch_at_login_value = patch.launch_at_login;
    let tray_changed = patch.tray.is_some();
    let alerts_changed = patch.alerts.is_some();
    let store = &state.store;
    let mut updated = store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    updated.apply_patch(patch);
    updated
        .validate()
        .map_err(|err| CommandError::settings_invalid(err.to_string()))?;
    store
        .save_settings(&updated)
        .map_err(|err| CommandError::store(err.to_string()))?;
    state
        .polling_secs
        .store(updated.polling_base_seconds, Ordering::Relaxed);
    // Phase 06: tray configuration applies live (visibility follows
    // `enabled`; value rows follow the toggles on the next snapshot).
    if tray_changed {
        if let Some(tray) = state.tray.as_ref() {
            tray.apply_config(
                updated.tray,
                updated.rate_unit == crate::settings::RateUnit::Bits,
            );
        }
    }
    // Phase 07: alert settings apply live (notification switch, quiet
    // hours, and the action master gate — the runner reads the mirror on
    // every fire, so disabling takes effect immediately).
    if alerts_changed {
        *state
            .alerts
            .config
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = updated.alerts;
    }
    // Launch-at-login: the OS login item follows the persisted setting
    // only when the user changes it — never applied silently at startup.
    if launch_at_login_changed {
        use tauri_plugin_autostart::ManagerExt;
        let autostart = app.autolaunch();
        let outcome = if launch_at_login_value.unwrap_or(false) {
            autostart.enable()
        } else {
            autostart.disable()
        };
        if let Err(error) = outcome {
            return Err(CommandError::store(format!("autostart: {error}")));
        }
    }
    Ok(updated)
}

/// Process table for the Applications page. Polled by the frontend while
/// the page is visible (single bounded source — never per-row timers).
#[tauri::command]
pub fn get_process_page(state: State<'_, AppState>) -> ProcessPage {
    let mut manager = state
        .processes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let logical_cores = state
        .latest
        .read()
        .ok()
        .and_then(|guard| {
            guard.as_ref().and_then(|snapshot| {
                snapshot
                    .metrics
                    .iter()
                    .find(|r| r.metric_id == monitor_core::families::HOST_CPU_CORES_LOGICAL)
                    .and_then(|r| match r.value {
                        Some(monitor_core::MetricValue::Number(n)) => Some(n as usize),
                        _ => None,
                    })
            })
        })
        .unwrap_or(0);
    manager.page(&monitor_core::SystemClock, logical_cores)
}

/// Guarded process termination. The Rust identity recheck (PID + start
/// time) is the security boundary; the result is audited locally.
#[tauri::command]
pub fn request_process_action(
    state: State<'_, AppState>,
    request: ProcessActionRequest,
) -> ProcessActionResult {
    let outcome = {
        let mut manager = state
            .processes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        manager.request_action(&request)
    };
    state.record_audit(
        request.pid,
        request.start_time_sec,
        &request.mode,
        &format!("{:?}", outcome.outcome),
        None,
    );
    outcome
}

/// Bounded history query. Metric ids are checked against the family
/// registry; range and point count validated inside monitor-history.
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    metric_id: String,
    entity_id: String,
    from_utc_ms: i64,
    to_utc_ms: i64,
    max_points: Option<u32>,
) -> Result<HistoryResponse, CommandError> {
    if !monitor_core::families::ALL_FAMILIES.contains(&metric_id.as_str()) {
        return Err(CommandError {
            code: "HISTORY_INVALID",
            message_key: "errors.historyInvalid",
            reason: Some("unknown metric_id".to_string()),
        });
    }
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let query = HistoryQuery {
        host_id: state.host_id.clone(),
        metric_id,
        entity_id,
        from_utc_ms,
        to_utc_ms,
        cadence_ms: (settings.polling_base_seconds.max(1) as i64) * 1_000,
        max_points: max_points.unwrap_or(monitor_history::DEFAULT_MAX_POINTS),
    };
    // The writer owns the DB connection; queries need their own handle.
    let query_conn = open_history_query_connection(&state)?;
    monitor_history::get_history(query_conn.connection(), &query)
        .map_err(|err| CommandError::store(err.to_string()))
}

/// Explicit, user-confirmed history wipe (samples only; settings, audits
/// and schema untouched).
#[tauri::command]
pub fn clear_history(
    state: State<'_, AppState>,
    confirmation_id: String,
) -> Result<monitor_history::RetentionSummary, CommandError> {
    if confirmation_id.trim().is_empty() {
        return Err(CommandError {
            code: "HISTORY_CONFIRMATION_REQUIRED",
            message_key: "errors.historyConfirmation",
            reason: None,
        });
    }
    let conn = open_history_query_connection(&state)?;
    monitor_history::clear_samples(conn.connection())
        .map_err(|err| CommandError::store(err.to_string()))
}

pub(crate) fn open_history_query_connection(
    state: &AppState,
) -> Result<monitor_history::HistoryStore, CommandError> {
    let dir = state
        .history_dir
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let Some(dir) = dir else {
        return Err(CommandError::store("history directory not set".into()));
    };
    monitor_history::HistoryStore::open(&dir.join("history.sqlite3"))
        .map_err(|err| CommandError::store(err.to_string()))
}

/// Flush and stop the history writer (called before app exit).
pub fn shutdown_history(state: &AppState) {
    let mut guard = state
        .history_writer
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some(mut writer) = guard.take() {
        let _ = Arc::get_mut(&mut writer).map(|w| w.shutdown());
    }
}

// ---- Storage analyzer + safe cleanup (FX-008, phase 04) --------------------

/// Start a scan of a user-chosen root. The root must exist and be a
/// directory; anything else is a typed rejection.
#[tauri::command]
pub fn start_scan(state: State<'_, AppState>, root: String) -> Result<ScanStatus, CommandError> {
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(CommandError {
            code: "SCAN_ROOT_INVALID",
            message_key: "errors.scanRootInvalid",
            reason: Some(root.clone()),
        });
    }
    let scan_id = uuid::Uuid::new_v4().to_string();
    let status_root = root.clone();
    state.scans.start(scan_id.clone(), path);
    let progress = {
        let handle = state.scans.get(&scan_id).expect("just registered");
        let progress = handle.progress.lock().unwrap_or_else(|p| p.into_inner());
        progress.clone()
    };
    Ok(ScanStatus {
        scan_id,
        root: status_root,
        progress,
    })
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>, scan_id: String) -> bool {
    state.scans.cancel(&scan_id)
}

#[tauri::command]
pub fn get_scan_status(state: State<'_, AppState>, scan_id: String) -> Option<ScanStatus> {
    let handle = state.scans.get(&scan_id)?;
    let progress = {
        let progress = handle.progress.lock().unwrap_or_else(|p| p.into_inner());
        progress.clone()
    };
    let root = handle.root.to_string_lossy().to_string();
    Some(ScanStatus {
        scan_id,
        root,
        progress,
    })
}

/// Paginated children of one node, size-descending — the UI never receives
/// the whole tree (architecture.md §10).
#[tauri::command]
pub fn get_scan_children(
    state: State<'_, AppState>,
    scan_id: String,
    node_index: usize,
    offset: usize,
    limit: usize,
) -> Result<ScanChildren, CommandError> {
    if limit == 0 || limit > 1_000 {
        return Err(CommandError {
            code: "SCAN_PAGE_INVALID",
            message_key: "errors.scanPageInvalid",
            reason: Some("limit must be within 1..=1000".into()),
        });
    }
    let handle = state.scans.get(&scan_id).ok_or(CommandError::no_scan())?;
    let tree = handle.tree.lock().unwrap_or_else(|p| p.into_inner());
    let Some(node) = tree.nodes.get(node_index) else {
        return Err(CommandError {
            code: "SCAN_NODE_INVALID",
            message_key: "errors.scanNodeInvalid",
            reason: None,
        });
    };
    let mut order: Vec<usize> = node.children.clone();
    order.sort_by(|a, b| {
        tree.nodes[*b]
            .logical_size
            .cmp(&tree.nodes[*a].logical_size)
            .then_with(|| tree.nodes[*a].name.cmp(&tree.nodes[*b].name))
    });
    let children = order
        .iter()
        .skip(offset)
        .take(limit)
        .map(|idx| {
            let child = &tree.nodes[*idx];
            ChildEntry {
                index: *idx,
                name: child.name.clone(),
                is_dir: child.is_dir,
                is_symlink: child.is_symlink,
                is_mount_boundary: child.is_mount_boundary,
                logical_size: child.logical_size,
                allocated_size: child.allocated_size,
                error_count: child.error_count,
            }
        })
        .collect();
    Ok(ScanChildren {
        children,
        total_children: order.len(),
        end_of_list: offset + limit >= order.len(),
        node_index,
        errors: node.error_count,
        skipped_count: tree.skipped.len(),
    })
}

/// Move explicitly selected items to the OS trash. Scope: paths must live
/// inside a root scanned this session. Fail-closed, audited.
#[tauri::command]
pub fn move_to_trash(
    state: State<'_, AppState>,
    paths: Vec<String>,
    confirmation_id: String,
) -> Result<Vec<TrashPathResult>, CommandError> {
    if confirmation_id.trim().is_empty() {
        return Err(CommandError {
            code: "TRASH_CONFIRMATION_REQUIRED",
            message_key: "errors.trashConfirmation",
            reason: None,
        });
    }
    if paths.is_empty() || paths.len() > 100 {
        return Err(CommandError {
            code: "TRASH_SELECTION_INVALID",
            message_key: "errors.trashSelection",
            reason: Some(format!("{} paths", paths.len())),
        });
    }
    let scan_roots = state.scans.roots();
    let mut results = Vec::with_capacity(paths.len());
    for raw in &paths {
        let path = PathBuf::from(raw);
        let outcome = match monitor_storage::move_to_trash(&path, &scan_roots) {
            Ok(()) => "ok".to_string(),
            Err(err) => {
                // Detail kept short; full paths stay in the local UI only.
                state.record_audit(0, 0, "trash", "failed", Some(err.to_string()));
                err.to_string()
            }
        };
        let ok = outcome == "ok";
        results.push(TrashPathResult {
            path: raw.clone(),
            ok,
            detail: if ok { None } else { Some(outcome) },
        });
    }
    Ok(results)
}

/// Open a path in the platform file manager. Named binary + argument
/// array — no shell interpolation (security-privacy.md §5).
#[tauri::command]
pub fn open_in_manager(path: String) -> Result<(), CommandError> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(CommandError {
            code: "OPEN_PATH_INVALID",
            message_key: "errors.openPathInvalid",
            reason: Some(path),
        });
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(windows)]
    let program = "explorer";
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map_err(|err| CommandError::store(err.to_string()))?;
    Ok(())
}

/// External IP lookup: **opt-in only**. The default build has no approved
/// HTTP transport wired; with the setting enabled this still reports a
/// typed unavailable instead of performing an unreviewed network call.
#[tauri::command]
pub fn lookup_external_ip(state: State<'_, AppState>) -> Result<String, CommandError> {
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    if !settings.external_lookup.enabled {
        return Err(CommandError {
            code: "EXTERNAL_LOOKUP_DISABLED",
            message_key: "errors.externalLookupDisabled",
            reason: None,
        });
    }
    Err(CommandError {
        code: "EXTERNAL_LOOKUP_TRANSPORT_UNAVAILABLE",
        message_key: "errors.externalLookupTransport",
        reason: Some(settings.external_lookup.provider),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct TrashPathResult {
    pub path: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
