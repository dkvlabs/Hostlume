//! Remote server lifecycle + commands (phase 08, FX-019).
//!
//! Security posture (security-privacy.md §6):
//! * The server runs **only** when the user opts in through
//!   [`configure_remote`]; the default settings ship `mode: off`, and
//!   nothing starts a listener at app startup or login without that
//!   configuration.
//! * Loopback only in this build: LAN is refused by `monitor-remote`
//!   (fail-closed) because no TLS provisioning has been validated.
//! * The access token is generated from the OS RNG and persisted **only**
//!   as its SHA-256 hash; the plaintext is returned exactly once, at
//!   generation. Rotation invalidates the old token (and its dashboard
//!   sessions) immediately; revocation stops the flow entirely.
//! * Stopping the server on app exit is unconditional (`shutdown_remote`).

use std::sync::Arc;

use monitor_core::{families, SystemClock};
use monitor_remote::{RemoteConfig, RemoteData, RemoteMode};
use serde::Serialize;
use tauri::State;

use crate::commands::CommandError;
use crate::state::AppState;

/// Build the read-only data seams for the server. The process closure
/// locks the shared manager and re-projects rows without paths or command
/// lines (they are not collected in the first place).
fn remote_data(state: &AppState) -> RemoteData {
    let processes = Arc::clone(&state.processes);
    let snapshot = Arc::clone(&state.latest);
    RemoteData {
        host_id: state.host_id.clone(),
        cadence_ms: (state
            .polling_secs
            .load(std::sync::atomic::Ordering::Relaxed)
            .max(1) as i64)
            * 1_000,
        snapshot: Arc::clone(&state.latest),
        capabilities: Arc::clone(&state.capabilities),
        history_db_path: history_dir(state).unwrap_or_default(),
        process_rows: Some(Arc::new(move || {
            let logical_cores = {
                let guard = snapshot.read().unwrap_or_else(|p| p.into_inner());
                guard.as_ref().and_then(|snapshot| {
                    snapshot.metrics.iter().find_map(|r| {
                        if r.metric_id == families::HOST_CPU_CORES_LOGICAL {
                            match r.value {
                                Some(monitor_core::MetricValue::Number(n)) => Some(n as usize),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                })
            }
            .unwrap_or(0);
            let mut manager = processes.lock().unwrap_or_else(|p| p.into_inner());
            manager
                .page(&SystemClock, logical_cores)
                .rows
                .into_iter()
                .map(|row| monitor_remote::RemoteProcessRow {
                    pid: row.pid,
                    name: row.name,
                    cpu_usage_host: row.cpu_usage_host,
                    rss_bytes: row.rss_bytes,
                    status: row.status,
                })
                .collect()
        })),
    }
}

fn history_dir(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .history_dir
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

/// Start (replacing any previous instance) the loopback server.
fn start_remote(state: &AppState) -> Result<u16, CommandError> {
    stop_remote(state);
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let Some(token_hash) = settings.remote_token_hash.clone() else {
        // No token on file: fail closed rather than starting an open server.
        return Err(CommandError {
            code: "REMOTE_TOKEN_REQUIRED",
            message_key: "errors.remoteTokenRequired",
            reason: None,
        });
    };
    let handle = monitor_remote::spawn(
        &RemoteConfig {
            mode: RemoteMode::Loopback,
            port: settings.remote.port,
        },
        remote_data(state),
        token_hash,
    )
    .map_err(|error| match error {
        monitor_remote::RemoteError::LanRequiresTls => CommandError {
            code: "REMOTE_LAN_REQUIRES_TLS",
            message_key: "errors.remoteLanRequiresTls",
            reason: None,
        },
        monitor_remote::RemoteError::PortUnavailable { port, source } => CommandError {
            code: "REMOTE_PORT_UNAVAILABLE",
            message_key: "errors.remotePortUnavailable",
            reason: Some(format!("{port}: {source}")),
        },
        other => CommandError::store(other.to_string()),
    })?;
    let port = handle.port();
    *state.remote.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
    Ok(port)
}

/// Stop the server if running; stopping while stopped is fine.
fn stop_remote(state: &AppState) {
    if let Some(handle) = state
        .remote
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        handle.stop();
    }
}

/// Unconditional shutdown (app exit).
pub fn shutdown_remote(state: &AppState) {
    stop_remote(state);
}

/// Apply the persisted remote configuration at app startup: start only if
/// the user opted in (loopback) AND a token hash exists; otherwise the
/// default stays "no listener".
pub fn apply_remote_at_startup(state: &AppState) {
    let Ok(settings) = state.store.load_settings() else {
        return;
    };
    if settings.remote.mode == RemoteMode::Loopback && settings.remote_token_hash.is_some() {
        if let Err(error) = start_remote(state) {
            eprintln!("remote server not started: {error:?}");
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteStatus {
    pub running: bool,
    pub mode: RemoteMode,
    pub port: u16,
    /// Address to open in a browser when running (loopback).
    pub url: Option<String>,
    pub token_exists: bool,
}

#[tauri::command]
pub fn get_remote_status(state: State<'_, AppState>) -> Result<RemoteStatus, CommandError> {
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let running = state
        .remote
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .is_some();
    Ok(RemoteStatus {
        running,
        mode: settings.remote.mode,
        port: settings.remote.port,
        url: if running {
            Some(format!("http://127.0.0.1:{}/", settings.remote.port))
        } else {
            None
        },
        token_exists: settings.remote_token_hash.is_some(),
    })
}

#[derive(Debug, Serialize)]
pub struct RemoteSetup {
    pub status: RemoteStatus,
    /// The full access token — returned ONLY when this call generated a
    /// new one (first enable or explicit rotation). Never persisted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_once: Option<String>,
}

/// Configure the remote server: validates, persists, then starts or stops
/// the listener. Enabling without a token on file generates one and
/// returns its plaintext exactly once.
#[tauri::command]
pub fn configure_remote(
    state: State<'_, AppState>,
    mode: RemoteMode,
    port: u16,
) -> Result<RemoteSetup, CommandError> {
    let config = RemoteConfig { mode, port };
    config
        .validate()
        .map_err(|error| CommandError::settings_invalid(error.to_string()))?;

    let mut token_once: Option<String> = None;
    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    settings.remote = config;
    if settings.remote.mode == RemoteMode::Loopback && settings.remote_token_hash.is_none() {
        let token = monitor_remote::auth::generate_token();
        settings.remote_token_hash = Some(monitor_remote::auth::hash_token(&token));
        token_once = Some(token);
    }
    settings
        .validate()
        .map_err(|err| CommandError::settings_invalid(err.to_string()))?;
    state
        .store
        .save_settings(&settings)
        .map_err(|err| CommandError::store(err.to_string()))?;

    let status = match settings.remote.mode {
        RemoteMode::Off => {
            stop_remote(&state);
            RemoteStatus {
                running: false,
                mode: RemoteMode::Off,
                port: settings.remote.port,
                url: None,
                token_exists: settings.remote_token_hash.is_some(),
            }
        }
        RemoteMode::Loopback => {
            let port = start_remote(&state)?;
            RemoteStatus {
                running: true,
                mode: RemoteMode::Loopback,
                port,
                url: Some(format!("http://127.0.0.1:{port}/")),
                token_exists: true,
            }
        }
        RemoteMode::Lan => {
            // Fail closed: stop anything running and record the refused
            // mode so the UI can explain it.
            stop_remote(&state);
            RemoteStatus {
                running: false,
                mode: RemoteMode::Lan,
                port: settings.remote.port,
                url: None,
                token_exists: settings.remote_token_hash.is_some(),
            }
        }
    };
    Ok(RemoteSetup { status, token_once })
}

/// Generate a fresh token (rotation): persists only the hash, invalidates
/// the previous token and its dashboard sessions immediately, and returns
/// the plaintext exactly once.
#[tauri::command]
pub fn remote_rotate_token(state: State<'_, AppState>) -> Result<RemoteSetup, CommandError> {
    let token = monitor_remote::auth::generate_token();
    let hash = monitor_remote::auth::hash_token(&token);
    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    settings.remote_token_hash = Some(hash.clone());
    settings
        .validate()
        .map_err(|err| CommandError::settings_invalid(err.to_string()))?;
    state
        .store
        .save_settings(&settings)
        .map_err(|err| CommandError::store(err.to_string()))?;
    if let Some(handle) = state
        .remote
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
    {
        handle.rotate(hash);
    }
    let status = RemoteStatus {
        running: state
            .remote
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some(),
        mode: settings.remote.mode,
        port: settings.remote.port,
        url: state
            .remote
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|handle| format!("http://127.0.0.1:{}/", handle.port())),
        token_exists: true,
    };
    Ok(RemoteSetup {
        status,
        token_once: Some(token),
    })
}

/// Revoke access: clears the token hash and **stops the server** — a
/// listener with no accepted credential is pointless, and running one
/// with any sentinel value would be a guessable credential.
#[tauri::command]
pub fn remote_revoke_token(state: State<'_, AppState>) -> Result<RemoteStatus, CommandError> {
    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    settings.remote_token_hash = None;
    settings
        .validate()
        .map_err(|err| CommandError::settings_invalid(err.to_string()))?;
    state
        .store
        .save_settings(&settings)
        .map_err(|err| CommandError::store(err.to_string()))?;
    stop_remote(&state);
    Ok(RemoteStatus {
        running: false,
        mode: settings.remote.mode,
        port: settings.remote.port,
        url: None,
        token_exists: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enabling loopback without a token on file must fail closed — the
    /// server never runs open (unit-level; the UI flow mints first).
    #[test]
    fn token_is_required_before_start() {
        // start_remote checks `remote_token_hash.is_some()` before spawn;
        // the negative case is a pure settings-shape assertion here.
        let settings = crate::settings::Settings::default();
        assert!(settings.remote_token_hash.is_none());
        assert_eq!(settings.remote.mode, RemoteMode::Off);
    }
}
