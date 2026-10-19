//! Tauri commands. Every input is validated in Rust; the WebView is
//! untrusted input (security-privacy.md §2). Errors are structured payloads
//! with i18n keys so the UI renders its own localized messages.

use std::sync::atomic::Ordering;

use monitor_core::{CapabilityReport, SystemSnapshot};
use serde::Serialize;
use tauri::State;

use crate::settings::SettingsPatch;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    #[serde(rename = "messageKey")]
    pub message_key: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl CommandError {
    fn no_snapshot_yet() -> Self {
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

    fn store(reason: String) -> Self {
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
    state.capabilities.clone()
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
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<crate::settings::Settings, CommandError> {
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
    Ok(updated)
}
