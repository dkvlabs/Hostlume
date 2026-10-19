//! monitor-remote — the optional, **read-only** remote monitor server
//! (phase 08, FX-019; security-privacy.md §6).
//!
//! Load-bearing properties, all test-enforced:
//!
//! * **Default OFF**: nothing constructs a listener unless the user opted
//!   in; the only supported bind this build is `127.0.0.1` (loopback). LAN
//!   mode fails closed with [`RemoteError::LanRequiresTls`] because no TLS
//!   provisioning has been validated yet (phase 08 §3.10 — LAN ships only
//!   after TLS + auth validation).
//! * **Auth**: every data route requires the bearer token (or a session
//!   cookie exchanged for it). Only the SHA-256 hash of the token is ever
//!   stored or held in memory; rotation invalidates the old token at once.
//! * **Read-only**: GET routes plus exactly one POST (`/api/v1/session`,
//!   the token→cookie exchange). There is no write, kill, trash or shell
//!   route and no bridge into Tauri commands.
//! * **Bounded**: fixed point caps, range caps, a per-IP fixed-window rate
//!   limiter, a client cap on the SSE stream with slow-client drops.
//! * **Private**: the projection layer emits numeric readings only; text
//!   metrics (interface addresses, filesystem names, device names) are
//!   excluded, `/system` projects a minimal identity block, and the
//!   process route reuses the sanitized page (no paths, no command lines).

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use monitor_core::{CapabilityReport, SystemSnapshot};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

pub mod auth;
pub mod projection;
pub mod routes;

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
    /// Bound to 127.0.0.1 only, token-authenticated.
    Loopback,
    /// Explicitly **not supported in this build**: fails closed until TLS
    /// provisioning exists and is validated (ADR-0008).
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
    #[error("LAN mode requires TLS, which is not provisioned in this build; refusing to bind a non-loopback address")]
    LanRequiresTls,
    #[error("remote server is disabled")]
    Disabled,
    #[error("could not bind 127.0.0.1:{port}: {source}")]
    PortUnavailable { port: u16, source: std::io::Error },
    #[error("invalid remote configuration: {0}")]
    InvalidConfig(#[from] ConfigError),
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
}

/// Shared server runtime state: the token hash (hot-swappable on
/// rotation), the per-IP rate limiter and the dashboard session map.
/// `Debug` deliberately skips contents — token hashes never reach logs.
pub struct ServerShared {
    pub token_hash: Mutex<String>,
    pub rate: Mutex<auth::RateLimiter>,
    pub auth_rate: Mutex<auth::RateLimiter>,
    pub sessions: Mutex<auth::SessionMap>,
    pub sse_clients: AtomicUsize,
}

impl std::fmt::Debug for ServerShared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerShared")
            .field("sessions", &self.sessions.lock().map(|s| s.len()))
            .field(
                "sse_clients",
                &self.sse_clients.load(std::sync::atomic::Ordering::SeqCst),
            )
            .finish_non_exhaustive()
    }
}

/// Running server handle: bound address, an orderly shutdown trigger and
/// the shared auth state (token rotation goes through [`Self::rotate`]).
#[derive(Debug)]
pub struct RemoteServerHandle {
    pub addr: SocketAddr,
    pub shared: Arc<ServerShared>,
    shutdown_tx: watch::Sender<bool>,
}

impl RemoteServerHandle {
    /// Ask the server to stop accepting connections and drain; never
    /// blocks (the app awaits it via [`Self::join`] on quit).
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Rotate the accepted token. The old token — and every dashboard
    /// session minted from it — stops working immediately.
    pub fn rotate(&self, new_token_hash: String) {
        self.shared.rotate_token(new_token_hash);
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub async fn join(self) {
        let _ = self.shutdown_tx.closed().await;
    }
}

/// Start the server for `config`. `Off` is a typed [`RemoteError::Disabled`]
/// (callers check the mode first); `Lan` fails closed.
pub fn spawn(
    config: &RemoteConfig,
    data: RemoteData,
    token_hash: String,
) -> Result<RemoteServerHandle, RemoteError> {
    config.validate()?;
    match config.mode {
        RemoteMode::Off => Err(RemoteError::Disabled),
        RemoteMode::Lan => Err(RemoteError::LanRequiresTls),
        RemoteMode::Loopback => {
            let bind = SocketAddr::new(IpAddr::from([127, 0, 0, 1]), config.port);
            let std_listener = std::net::TcpListener::bind(bind).map_err(|source| {
                RemoteError::PortUnavailable {
                    port: config.port,
                    source,
                }
            })?;
            // The dedicated thread's current-thread runtime needs a
            // non-blocking FD to take ownership.
            std_listener
                .set_nonblocking(true)
                .map_err(|source| RemoteError::PortUnavailable {
                    port: config.port,
                    source,
                })?;
            let addr = bind;
            let shared = Arc::new(ServerShared {
                token_hash: Mutex::new(token_hash),
                rate: Mutex::new(auth::RateLimiter::default()),
                auth_rate: Mutex::new(auth::RateLimiter::with_limits(
                    auth::AUTH_MAX_PER_WINDOW,
                    auth::DEFAULT_WINDOW,
                )),
                sessions: Mutex::new(auth::SessionMap::default()),
                sse_clients: AtomicUsize::new(0),
            });
            let app = routes::router(Arc::new(data), Arc::clone(&shared));
            let (shutdown_tx, shutdown_rx) = watch::channel(false);
            std::thread::Builder::new()
                .name("remote-server".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("remote server runtime");
                    runtime.block_on(async move {
                        let listener = tokio::net::TcpListener::from_std(std_listener)
                            .expect("remote listener");
                        let server = axum::serve(
                            listener,
                            app.into_make_service_with_connect_info::<SocketAddr>(),
                        )
                        .with_graceful_shutdown(async move {
                            let _ = shutdown_rx.clone().wait_for(|stopped| *stopped).await;
                        });
                        let _ = server.await;
                    });
                })
                .expect("spawn remote server thread");
            Ok(RemoteServerHandle {
                addr,
                shared: Arc::clone(&shared),
                shutdown_tx,
            })
        }
    }
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

    #[test]
    fn lan_mode_fails_closed_even_with_valid_port() {
        let config = RemoteConfig {
            mode: RemoteMode::Lan,
            port: DEFAULT_PORT,
        };
        let data = RemoteData {
            host_id: "h".into(),
            cadence_ms: 3_000,
            snapshot: Arc::default(),
            capabilities: Arc::new(Mutex::new(CapabilityReport::new("h", "t"))),
            history_db_path: std::env::temp_dir(),
            process_rows: None,
        };
        let err = spawn(&config, data, "hash".into()).unwrap_err();
        assert!(matches!(err, RemoteError::LanRequiresTls));
    }

    #[test]
    fn off_mode_never_binds() {
        let config = RemoteConfig::default();
        let data = RemoteData {
            host_id: "h".into(),
            cadence_ms: 3_000,
            snapshot: Arc::default(),
            capabilities: Arc::new(Mutex::new(CapabilityReport::new("h", "t"))),
            history_db_path: std::env::temp_dir(),
            process_rows: None,
        };
        assert!(matches!(
            spawn(&config, data, "hash".into()),
            Err(RemoteError::Disabled)
        ));
    }

    #[tokio::test]
    async fn loopback_binds_and_shuts_down() {
        // Pick a free high port via an ephemeral bind probe.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let config = RemoteConfig {
            mode: RemoteMode::Loopback,
            port,
        };
        let data = RemoteData {
            host_id: "h".into(),
            cadence_ms: 3_000,
            snapshot: Arc::default(),
            capabilities: Arc::new(Mutex::new(CapabilityReport::new("h", "t"))),
            history_db_path: std::env::temp_dir(),
            process_rows: None,
        };
        let handle = spawn(&config, data, "hash".into()).expect("loopback binds");
        assert_eq!(handle.addr.port(), port);
        handle.stop();
        handle.join().await;
    }
}
