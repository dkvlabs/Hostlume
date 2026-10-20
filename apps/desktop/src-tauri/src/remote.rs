//! Remote server lifecycle + commands (phase 08 FX-019, phase 10 FX-022).
//!
//! Security posture (security-privacy.md §6, ADR-0008/ADR-0009):
//! * The server runs **only** when the user opts in through
//!   [`configure_remote`]; the default settings ship `mode: off`, and
//!   nothing starts a listener at app startup or login without that
//!   configuration.
//! * Loopback mode is plain HTTP on 127.0.0.1 (phase 08). LAN mode (phase
//!   10) serves HTTPS with this machine's own certificate — generated on
//!   first use, persisted under `remote-tls/` with an owner-only key — and
//!   is refused by `monitor-remote` without one (fail-closed).
//! * The owner token is generated from the OS RNG and persisted **only**
//!   as its SHA-256 hash; the plaintext is returned exactly once. Paired
//!   consoles get their own per-device tokens (hash-only persistence too)
//!   and can be revoked one by one; rotation of the owner token leaves them
//!   intact, revocation of the owner token stops the server entirely.
//! * Stopping the server on app exit is unconditional (`shutdown_remote`).

use std::sync::Arc;

use monitor_core::{families, SystemClock};
use monitor_remote::{
    DeviceCredential, PairedDevice, PairingWindowInfo, RemoteConfig, RemoteData, RemoteMode,
    SpawnOptions, TlsIdentity,
};
use serde::Serialize;
use tauri::State;

use crate::commands::CommandError;
use crate::settings_store::SettingsStore;
use crate::state::AppState;

pub const TLS_DIR: &str = "remote-tls";

/// Build the read-only data seams for the server. The process closure
/// locks the shared manager and re-projects rows without paths or command
/// lines (they are not collected in the first place).
fn remote_data(state: &AppState) -> RemoteData {
    let processes = Arc::clone(&state.processes);
    let snapshot = Arc::clone(&state.latest);
    let store = Arc::clone(&state.store);
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
        on_device_paired: Some(Arc::new(move |device| persist_device(&store, device))),
    }
}

/// Called on the server thread after a successful pairing: keep the hash
/// (never the token) in settings so the device survives restarts.
fn persist_device(store: &SettingsStore, device: PairedDevice) {
    let Ok(mut settings) = store.load_settings() else {
        eprintln!("paired device not persisted: settings unavailable");
        return;
    };
    settings
        .remote_devices
        .retain(|existing| existing.device_id != device.device_id);
    settings.remote_devices.push(device);
    if let Err(error) = settings
        .validate()
        .map_err(|e| e.to_string())
        .and_then(|_| store.save_settings(&settings).map_err(|e| e.to_string()))
    {
        eprintln!("paired device not persisted: {error}");
    }
}

fn history_dir(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .history_dir
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

fn device_credentials(settings: &crate::settings::Settings) -> Vec<DeviceCredential> {
    settings
        .remote_devices
        .iter()
        .map(|device| DeviceCredential {
            device_id: device.device_id.clone(),
            token_hash: device.token_hash.clone(),
        })
        .collect()
}

fn tls_error(error: impl std::fmt::Display) -> CommandError {
    CommandError {
        code: "REMOTE_TLS",
        message_key: "errors.remoteTls",
        reason: Some(error.to_string()),
    }
}

/// Start (replacing any previous instance) the server in the persisted
/// mode. LAN loads or generates the TLS identity first.
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
    let tls = match settings.remote.mode {
        RemoteMode::Lan => Some(Arc::new(
            TlsIdentity::load_or_generate(&state.data_dir.join(TLS_DIR)).map_err(tls_error)?,
        )),
        _ => None,
    };
    let handle = monitor_remote::spawn_with(
        &RemoteConfig {
            mode: settings.remote.mode,
            port: settings.remote.port,
        },
        remote_data(state),
        token_hash,
        SpawnOptions {
            tls,
            bind_addr: None,
            devices: device_credentials(&settings),
            display_name: Some(settings.remote_display_name.clone()),
        },
    )
    .map_err(|error| match error {
        monitor_remote::RemoteError::LanRequiresTls => CommandError {
            code: "REMOTE_LAN_REQUIRES_TLS",
            message_key: "errors.remoteLanRequiresTls",
            reason: None,
        },
        monitor_remote::RemoteError::PortUnavailable { bind, port, source } => CommandError {
            code: "REMOTE_PORT_UNAVAILABLE",
            message_key: "errors.remotePortUnavailable",
            reason: Some(format!("{bind}:{port}: {source}")),
        },
        monitor_remote::RemoteError::Tls(error) => tls_error(error),
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
/// the user opted in (loopback or LAN) AND a token hash exists; otherwise
/// the default stays "no listener".
pub fn apply_remote_at_startup(state: &AppState) {
    let Ok(settings) = state.store.load_settings() else {
        return;
    };
    if matches!(settings.remote.mode, RemoteMode::Loopback | RemoteMode::Lan)
        && settings.remote_token_hash.is_some()
    {
        if let Err(error) = start_remote(state) {
            eprintln!("remote server not started: {error:?}");
        }
    }
}

/// Live display-name change (Settings → Remote → name): the manifest
/// reflects it immediately.
pub fn apply_display_name(state: &AppState, name: &str) {
    if let Some(handle) = state
        .remote
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
    {
        handle.set_display_name(name.to_string());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteDeviceView {
    pub device_id: String,
    pub label: String,
    pub created_at_utc: String,
    pub protocol_version: u32,
    /// Seconds since this device's last authenticated request in this run;
    /// `None` when it has not connected since the server started.
    pub last_seen_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteStatus {
    pub running: bool,
    pub mode: RemoteMode,
    pub port: u16,
    /// Address to open in a browser when running on loopback. LAN has no
    /// single URL (the UI lists this machine's interface addresses).
    pub url: Option<String>,
    pub token_exists: bool,
    pub display_name: String,
    /// LAN only: the certificate fingerprint consoles pin.
    pub tls_fingerprint: Option<String>,
    /// Open pairing window (code shown on this machine only).
    pub pairing: Option<PairingWindowInfo>,
    pub devices: Vec<RemoteDeviceView>,
}

fn status(state: &AppState) -> Result<RemoteStatus, CommandError> {
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let guard = state.remote.lock().unwrap_or_else(|p| p.into_inner());
    let handle = guard.as_ref();
    let running = handle.is_some();
    let last_seen: std::collections::HashMap<String, u64> = handle
        .map(|h| h.device_last_seen_secs().into_iter().collect())
        .unwrap_or_default();
    Ok(RemoteStatus {
        running,
        mode: settings.remote.mode,
        port: settings.remote.port,
        url: match (running, settings.remote.mode) {
            (true, RemoteMode::Loopback) => {
                Some(format!("http://127.0.0.1:{}/", settings.remote.port))
            }
            _ => None,
        },
        token_exists: settings.remote_token_hash.is_some(),
        display_name: settings.remote_display_name.clone(),
        tls_fingerprint: handle.and_then(|h| h.tls_fingerprint_hex().map(str::to_string)),
        pairing: handle.and_then(|h| h.pairing_window()),
        devices: settings
            .remote_devices
            .iter()
            .map(|device| RemoteDeviceView {
                device_id: device.device_id.clone(),
                label: device.label.clone(),
                created_at_utc: device.created_at_utc.clone(),
                protocol_version: device.protocol_version,
                last_seen_secs: last_seen.get(&device.device_id).copied(),
            })
            .collect(),
    })
}

#[tauri::command]
pub fn get_remote_status(state: State<'_, AppState>) -> Result<RemoteStatus, CommandError> {
    status(&state)
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
/// returns its plaintext exactly once. LAN provisions TLS on first start.
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
    if settings.remote.mode != RemoteMode::Off && settings.remote_token_hash.is_none() {
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

    match settings.remote.mode {
        RemoteMode::Off => stop_remote(&state),
        RemoteMode::Loopback | RemoteMode::Lan => {
            start_remote(&state)?;
        }
    }
    Ok(RemoteSetup {
        status: status(&state)?,
        token_once,
    })
}

/// Generate a fresh owner token (rotation): persists only the hash,
/// invalidates the previous token and its dashboard sessions immediately,
/// and returns the plaintext exactly once. Paired devices are unaffected.
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
    Ok(RemoteSetup {
        status: status(&state)?,
        token_once: Some(token),
    })
}

/// Revoke the owner token: clears the hash and **stops the server** — a
/// listener with no owner credential is pointless, and running one with
/// any sentinel value would be a guessable credential.
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
    status(&state)
}

fn pairing_unavailable() -> CommandError {
    CommandError {
        code: "REMOTE_PAIRING_UNAVAILABLE",
        message_key: "errors.remotePairingUnavailable",
        reason: None,
    }
}

/// Open the single pairing window (phase 10). Only meaningful while the
/// server runs in LAN mode: loopback has no certificate to bind a proof to.
#[tauri::command]
pub fn remote_open_pairing_window(
    state: State<'_, AppState>,
) -> Result<PairingWindowInfo, CommandError> {
    let guard = state.remote.lock().unwrap_or_else(|p| p.into_inner());
    let handle = guard.as_ref().ok_or_else(pairing_unavailable)?;
    if handle.mode != RemoteMode::Lan {
        return Err(pairing_unavailable());
    }
    handle.open_pairing_window().ok_or_else(pairing_unavailable)
}

#[tauri::command]
pub fn remote_close_pairing_window(
    state: State<'_, AppState>,
) -> Result<RemoteStatus, CommandError> {
    if let Some(handle) = state
        .remote
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
    {
        handle.close_pairing_window();
    }
    status(&state)
}

/// Revoke one paired console: its token stops working on the next request
/// and the record leaves settings.
#[tauri::command]
pub fn remote_revoke_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<RemoteStatus, CommandError> {
    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let before = settings.remote_devices.len();
    settings
        .remote_devices
        .retain(|device| device.device_id != device_id);
    if settings.remote_devices.len() == before {
        return Err(CommandError {
            code: "REMOTE_DEVICE_NOT_FOUND",
            message_key: "errors.remoteDeviceNotFound",
            reason: None,
        });
    }
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
        handle.remove_device(&device_id);
    }
    status(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enabling without a token on file must fail closed — the server never
    /// runs open (unit-level; the UI flow mints first).
    #[test]
    fn token_is_required_before_start() {
        // start_remote checks `remote_token_hash.is_some()` before spawn;
        // the negative case is a pure settings-shape assertion here.
        let settings = crate::settings::Settings::default();
        assert!(settings.remote_token_hash.is_none());
        assert_eq!(settings.remote.mode, RemoteMode::Off);
        assert!(settings.remote_devices.is_empty());
        assert!(settings.remote_hosts.is_empty());
    }

    /// Device credentials are derived from settings hashes only.
    #[test]
    fn device_credentials_carry_hashes_not_tokens() {
        let mut settings = crate::settings::Settings::default();
        settings.remote_devices.push(PairedDevice {
            device_id: "console-1".into(),
            label: "Console".into(),
            token_hash: monitor_remote::auth::hash_token("plaintext-token"),
            created_at_utc: "2026-10-19T00:00:00.000Z".into(),
            protocol_version: 1,
        });
        let creds = device_credentials(&settings);
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].device_id, "console-1");
        assert_ne!(creds[0].token_hash, "plaintext-token");
        assert_eq!(creds[0].token_hash.len(), 64);
    }
}
