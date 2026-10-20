//! monitor-remote — the optional, **read-only** remote monitor server
//! (phase 08, FX-019; security-privacy.md §6) and, since phase 10, the
//! paired-host client the console uses to read other machines (FX-022,
//! ADR-0009).
//!
//! Load-bearing properties, all test-enforced:
//!
//! * **Default OFF**: nothing constructs a listener unless the user opted
//!   in. Loopback mode binds `127.0.0.1` over plain HTTP (unchanged from
//!   phase 08). LAN mode binds all interfaces **only over TLS** with the
//!   agent's own pinned certificate; without a TLS identity it still fails
//!   closed with [`RemoteError::LanRequiresTls`].
//! * **Auth**: every data route requires the owner bearer token, a paired
//!   device's bearer token, or a dashboard session cookie exchanged for the
//!   owner token. Only SHA-256 hashes of tokens are stored or held in
//!   memory; rotation and per-device revocation take effect immediately.
//! * **Read-only**: GET routes plus exactly two POSTs — `/api/v1/session`
//!   (owner token → cookie) and `/api/v1/pair` (code-gated pairing). There
//!   is no write, kill, trash or shell route and no bridge into Tauri
//!   commands.
//! * **Bounded**: fixed point caps, range caps, a per-IP fixed-window rate
//!   limiter, a client cap on the SSE stream with slow-client drops, a
//!   single pairing window with one attempt per code, a device cap.
//! * **Private**: the projection layer emits numeric readings only; text
//!   metrics (interface addresses, filesystem names, device names) are
//!   excluded, `/system` projects a minimal identity block, the manifest
//!   carries a user-chosen display name, and the process route reuses the
//!   sanitized page (no paths, no command lines).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use monitor_core::{CapabilityReport, SystemSnapshot};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

pub mod auth;
pub mod client;
pub mod pairing;
pub mod projection;
pub mod protocol;
pub mod routes;
pub mod tls;

pub use protocol::PairedDevice;
pub use tls::TlsIdentity;

pub const DEFAULT_PORT: u16 = 45_454;
pub const PORT_MIN: u16 = 1024;
pub const PORT_MAX: u16 = 65_535;
/// SSE client cap; further clients receive an explicit 503.
pub const MAX_SSE_CLIENTS: usize = 8;
/// A slow client is dropped when it cannot accept an event in this window.
pub const SSE_SEND_TIMEOUT: Duration = Duration::from_secs(5);
pub const SSE_TICK: Duration = Duration::from_secs(2);
/// Bounded remote history: points and span caps (contract §7 boundedness).
pub const REMOTE_MAX_POINTS: u32 = 400;
pub const REMOTE_MAX_SPAN_MS: i64 = 90 * 86_400_000;
/// Session-cookie lifetime for the built-in dashboard.
pub const SESSION_TTL: Duration = Duration::from_secs(12 * 3_600);
pub const MAX_SESSIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteMode {
    /// No listener at all (the default).
    Off,
    /// Bound to 127.0.0.1 only, plain HTTP, token-authenticated.
    Loopback,
    /// All interfaces, **TLS only** (ADR-0009): requires a TLS identity,
    /// otherwise fails closed.
    Lan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteConfig {
    pub mode: RemoteMode,
    pub port: u16,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            mode: RemoteMode::Off,
            port: DEFAULT_PORT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("remote port {value} is outside {min}..={max}")]
    PortOutOfBounds { value: u16, min: u16, max: u16 },
}

impl RemoteConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(PORT_MIN..=PORT_MAX).contains(&self.port) {
            return Err(ConfigError::PortOutOfBounds {
                value: self.port,
                min: PORT_MIN,
                max: PORT_MAX,
            });
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(
        "LAN mode requires a TLS identity; refusing to bind a non-loopback address without one"
    )]
    LanRequiresTls,
    #[error("remote server is disabled")]
    Disabled,
    #[error("could not bind {bind}:{port}: {source}")]
    PortUnavailable {
        bind: IpAddr,
        port: u16,
        source: std::io::Error,
    },
    #[error("invalid remote configuration: {0}")]
    InvalidConfig(#[from] ConfigError),
    #[error("tls: {0}")]
    Tls(#[from] tls::TlsError),
}

/// Data seams the server reads. It never touches the collectors or Tauri
/// commands: a shared snapshot clone, a shared capability report clone and
/// the history database path for its own read-only WAL connections.
#[derive(Clone)]
pub struct RemoteData {
    pub host_id: String,
    pub cadence_ms: i64,
    pub snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    pub capabilities: Arc<Mutex<CapabilityReport>>,
    /// History database *directory* (the crate opens its own read-only
    /// WAL connection to `history.sqlite3` inside it, per query).
    pub history_db_path: PathBuf,
    /// Sanitized process rows supplied by the app's ProcessManager. `None`
    /// (or a paused manager) serves an honest empty list — never fake rows.
    pub process_rows: Option<Arc<dyn Fn() -> Vec<RemoteProcessRow> + Send + Sync>>,
    /// Invoked after a successful pairing so the app can persist the new
    /// device record (hash only). The server keeps the credential live in
    /// memory regardless.
    pub on_device_paired: Option<Arc<dyn Fn(PairedDevice) + Send + Sync>>,
}

/// A paired device's live credential (hash only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCredential {
    pub device_id: String,
    pub token_hash: String,
}

/// Snapshot of an open pairing window for the agent's own UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairingWindowInfo {
    pub code: String,
    pub secs_remaining: u64,
}

/// Shared server runtime state: token hashes (hot-swappable on rotation /
/// revocation), the per-IP rate limiters, the dashboard session map, the
/// pairing window and the per-device last-seen clock. `Debug` deliberately
/// skips contents — hashes and codes never reach logs.
pub struct ServerShared {
    pub token_hash: Mutex<String>,
    pub devices: Mutex<Vec<DeviceCredential>>,
    pub rate: Mutex<auth::RateLimiter>,
    pub auth_rate: Mutex<auth::RateLimiter>,
    pub sessions: Mutex<auth::SessionMap>,
    pub sse_clients: AtomicUsize,
    pub pairing: Mutex<Option<pairing::PairingWindow>>,
    pub display_name: Mutex<String>,
    /// `Some` in LAN mode: the fingerprint consoles pin (and the value the
    /// pairing proof is bound to).
    pub tls_fingerprint_hex: Option<String>,
    pub device_last_seen: Mutex<HashMap<String, Instant>>,
    /// Requests that reached an authentication or pairing gate (diagnostic;
    /// lets tests prove a pinned client never spoke to the wrong server).
    pub requests_seen: AtomicU64,
}

impl std::fmt::Debug for ServerShared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerShared")
            .field("sessions", &self.sessions.lock().map(|s| s.len()))
            .field("devices", &self.devices.lock().map(|d| d.len()))
            .field(
                "sse_clients",
                &self.sse_clients.load(std::sync::atomic::Ordering::SeqCst),
            )
            .field("tls", &self.tls_fingerprint_hex.is_some())
            .finish_non_exhaustive()
    }
}

/// Running server handle: bound address, an orderly shutdown trigger and
/// the shared auth state (token rotation goes through [`Self::rotate`]).
#[derive(Debug)]
pub struct RemoteServerHandle {
    pub addr: SocketAddr,
    pub mode: RemoteMode,
    pub shared: Arc<ServerShared>,
    shutdown_tx: watch::Sender<bool>,
}

impl RemoteServerHandle {
    /// Ask the server to stop accepting connections and drain; never
    /// blocks (the app awaits it via [`Self::join`] on quit).
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Rotate the accepted owner token. The old token — and every
    /// dashboard session minted from it — stops working immediately.
    pub fn rotate(&self, new_token_hash: String) {
        self.shared.rotate_token(new_token_hash);
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn tls_fingerprint_hex(&self) -> Option<&str> {
        self.shared.tls_fingerprint_hex.as_deref()
    }

    /// Open (or replace) the single pairing window and return its code for
    /// the agent's own UI. Only meaningful in LAN mode: without a TLS
    /// fingerprint there is nothing to bind the proof to, so `None`.
    pub fn open_pairing_window(&self) -> Option<PairingWindowInfo> {
        self.shared.tls_fingerprint_hex.as_ref()?;
        let window = pairing::PairingWindow::open(Instant::now());
        let info = PairingWindowInfo {
            code: window.code().to_string(),
            secs_remaining: pairing::CODE_TTL.as_secs(),
        };
        *self
            .shared
            .pairing
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(window);
        Some(info)
    }

    pub fn pairing_window(&self) -> Option<PairingWindowInfo> {
        let now = Instant::now();
        let guard = self
            .shared
            .pairing
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        guard
            .as_ref()
            .filter(|w| w.is_open(now))
            .map(|w| PairingWindowInfo {
                code: w.code().to_string(),
                secs_remaining: w.secs_remaining(now),
            })
    }

    pub fn close_pairing_window(&self) {
        *self
            .shared
            .pairing
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub fn set_devices(&self, devices: Vec<DeviceCredential>) {
        self.shared.set_devices(devices);
    }

    pub fn remove_device(&self, device_id: &str) {
        self.shared.remove_device(device_id);
    }

    pub fn set_display_name(&self, name: String) {
        *self
            .shared
            .display_name
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = name;
    }

    /// Seconds since each device's last authenticated request (this run).
    pub fn device_last_seen_secs(&self) -> Vec<(String, u64)> {
        let now = Instant::now();
        self.shared
            .device_last_seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(id, seen)| (id.clone(), now.saturating_duration_since(*seen).as_secs()))
            .collect()
    }

    pub fn requests_seen(&self) -> u64 {
        self.shared
            .requests_seen
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn join(self) {
        let _ = self.shutdown_tx.closed().await;
    }
}

/// Optional spawn inputs (phase 10). `Default` reproduces phase-08
/// behaviour exactly: loopback plain HTTP, no devices, no TLS.
#[derive(Default)]
pub struct SpawnOptions {
    /// Required for LAN mode; ignored for loopback.
    pub tls: Option<Arc<TlsIdentity>>,
    /// Test seam: bind LAN mode to this address instead of `0.0.0.0`
    /// (integration tests exercise TLS + pairing over loopback).
    pub bind_addr: Option<IpAddr>,
    pub devices: Vec<DeviceCredential>,
    pub display_name: Option<String>,
}

/// Start the server for `config`. `Off` is a typed [`RemoteError::Disabled`]
/// (callers check the mode first); `Lan` fails closed without TLS.
pub fn spawn(
    config: &RemoteConfig,
    data: RemoteData,
    token_hash: String,
) -> Result<RemoteServerHandle, RemoteError> {
    spawn_with(config, data, token_hash, SpawnOptions::default())
}

pub fn spawn_with(
    config: &RemoteConfig,
    data: RemoteData,
    token_hash: String,
    options: SpawnOptions,
) -> Result<RemoteServerHandle, RemoteError> {
    config.validate()?;
    let (bind_ip, tls_identity) = match config.mode {
        RemoteMode::Off => return Err(RemoteError::Disabled),
        RemoteMode::Lan => match options.tls.as_ref() {
            Some(identity) => (
                options.bind_addr.unwrap_or(IpAddr::from([0, 0, 0, 0])),
                Some(Arc::clone(identity)),
            ),
            None => return Err(RemoteError::LanRequiresTls),
        },
        RemoteMode::Loopback => (IpAddr::from([127, 0, 0, 1]), None),
    };
    let bind = SocketAddr::new(bind_ip, config.port);
    // Build the TLS server config before binding so a bad identity never
    // leaves a dangling socket.
    let server_config = match tls_identity.as_ref() {
        Some(identity) => Some(identity.server_config()?),
        None => None,
    };
    let port_error = |source: std::io::Error| RemoteError::PortUnavailable {
        bind: bind_ip,
        port: config.port,
        source,
    };
    let std_listener = std::net::TcpListener::bind(bind).map_err(port_error)?;
    // The dedicated thread's current-thread runtime needs a non-blocking FD
    // to take ownership.
    std_listener.set_nonblocking(true).map_err(port_error)?;
    let addr = bind;
    let shared = Arc::new(ServerShared {
        token_hash: Mutex::new(token_hash),
        devices: Mutex::new(options.devices),
        rate: Mutex::new(auth::RateLimiter::default()),
        auth_rate: Mutex::new(auth::RateLimiter::with_limits(
            auth::AUTH_MAX_PER_WINDOW,
            auth::DEFAULT_WINDOW,
        )),
        sessions: Mutex::new(auth::SessionMap::default()),
        sse_clients: AtomicUsize::new(0),
        pairing: Mutex::new(None),
        display_name: Mutex::new(
            options
                .display_name
                .filter(|name| protocol::display_name_is_valid(name))
                .unwrap_or_else(|| protocol::DEFAULT_DISPLAY_NAME.to_string()),
        ),
        tls_fingerprint_hex: tls_identity
            .as_ref()
            .map(|identity| identity.fingerprint_hex()),
        device_last_seen: Mutex::new(HashMap::new()),
        requests_seen: AtomicU64::new(0),
    });
    let app = routes::router(Arc::new(data), Arc::clone(&shared));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mode = config.mode;
    std::thread::Builder::new()
        .name("remote-server".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("remote server runtime");
            runtime.block_on(async move {
                let listener =
                    tokio::net::TcpListener::from_std(std_listener).expect("remote listener");
                let make_service = app.into_make_service_with_connect_info::<tls::ClientAddr>();
                let shutdown = async move {
                    let _ = shutdown_rx.clone().wait_for(|stopped| *stopped).await;
                };
                match server_config {
                    Some(server_config) => {
                        let tls_listener =
                            tls::TlsListener::spawn(listener, server_config).expect("tls listener");
                        let _ = axum::serve(tls_listener, make_service)
                            .with_graceful_shutdown(shutdown)
                            .await;
                    }
                    None => {
                        let _ = axum::serve(listener, make_service)
                            .with_graceful_shutdown(shutdown)
                            .await;
                    }
                }
            });
        })
        .expect("spawn remote server thread");
    Ok(RemoteServerHandle {
        addr,
        mode,
        shared: Arc::clone(&shared),
        shutdown_tx,
    })
}

/// The sanitized process page the remote projection re-serializes. Kept
/// structurally identical to the local UI's page minus warm-up internals —
/// defined here so the remote crate does not depend on the app crate.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteProcessRow {
    pub pid: u32,
    pub name: String,
    pub cpu_usage_host: Option<f64>,
    pub rss_bytes: u64,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> RemoteData {
        RemoteData {
            host_id: "h".into(),
            cadence_ms: 3_000,
            snapshot: Arc::default(),
            capabilities: Arc::new(Mutex::new(CapabilityReport::new("h", "t"))),
            history_db_path: std::env::temp_dir(),
            process_rows: None,
            on_device_paired: None,
        }
    }

    fn free_port() -> u16 {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        port
    }

    #[test]
    fn config_defaults_off_and_validates_port() {
        let config = RemoteConfig::default();
        assert_eq!(config.mode, RemoteMode::Off);
        assert_eq!(config.port, DEFAULT_PORT);
        config.validate().unwrap();

        let bad = RemoteConfig {
            mode: RemoteMode::Loopback,
            port: 80,
        };
        assert!(matches!(
            bad.validate(),
            Err(ConfigError::PortOutOfBounds { value: 80, .. })
        ));
    }

    /// Phase 08 gate, still true in phase 10: LAN without a TLS identity
    /// never binds anything.
    #[test]
    fn lan_mode_fails_closed_without_tls() {
        let config = RemoteConfig {
            mode: RemoteMode::Lan,
            port: DEFAULT_PORT,
        };
        let err = spawn(&config, data(), "hash".into()).unwrap_err();
        assert!(matches!(err, RemoteError::LanRequiresTls));
    }

    #[test]
    fn off_mode_never_binds() {
        let config = RemoteConfig::default();
        assert!(matches!(
            spawn(&config, data(), "hash".into()),
            Err(RemoteError::Disabled)
        ));
    }

    #[tokio::test]
    async fn loopback_binds_and_shuts_down() {
        let port = free_port();
        let config = RemoteConfig {
            mode: RemoteMode::Loopback,
            port,
        };
        let handle = spawn(&config, data(), "hash".into()).expect("loopback binds");
        assert_eq!(handle.addr.port(), port);
        assert!(handle.tls_fingerprint_hex().is_none());
        // Loopback has no fingerprint to bind a proof to: no pairing window.
        assert!(handle.open_pairing_window().is_none());
        handle.stop();
        handle.join().await;
    }

    /// LAN + TLS identity binds (test seam: loopback address), exposes the
    /// fingerprint and offers a single, replaceable pairing window.
    #[tokio::test]
    async fn lan_with_tls_binds_and_manages_pairing_window() {
        let port = free_port();
        let config = RemoteConfig {
            mode: RemoteMode::Lan,
            port,
        };
        let identity = Arc::new(TlsIdentity::generate().unwrap());
        let handle = spawn_with(
            &config,
            data(),
            "hash".into(),
            SpawnOptions {
                tls: Some(Arc::clone(&identity)),
                bind_addr: Some(IpAddr::from([127, 0, 0, 1])),
                devices: vec![],
                display_name: Some("Office Mac".into()),
            },
        )
        .expect("lan+tls binds on loopback");
        assert_eq!(handle.mode, RemoteMode::Lan);
        assert_eq!(
            handle.tls_fingerprint_hex(),
            Some(identity.fingerprint_hex().as_str())
        );
        assert!(handle.pairing_window().is_none());
        let first = handle.open_pairing_window().expect("window");
        assert_eq!(first.code.len(), pairing::CODE_LEN);
        let second = handle.open_pairing_window().expect("window");
        assert_ne!(first.code, second.code, "re-opening replaces the code");
        assert_eq!(handle.pairing_window().map(|w| w.code), Some(second.code));
        handle.close_pairing_window();
        assert!(handle.pairing_window().is_none());
        handle.stop();
        handle.join().await;
    }
}
