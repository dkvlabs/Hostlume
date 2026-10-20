//! Paired remote hosts — the console side (phase 10, FX-022; ADR-0009).
//!
//! * **No implicit networking**: nothing here connects until the user pairs
//!   a host (`pair_host`); an empty registry never opens a socket.
//! * **Secrets**: device tokens live in a dedicated restricted file
//!   (`remote-credentials.json`, 0600 on unix) — never in the settings
//!   SQLite, never serialized to the WebView, never in logs.
//! * **Bounded**: at most `client::MAX_HOSTS` workers, one thread each,
//!   bounded headline rings, request timeouts and exponential backoff with
//!   a 60 s ceiling.
//! * **Honest liveness**: status derives from the console's receipt clock;
//!   the agent's timestamp is carried for display and skew, never for the
//!   live/stale decision.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use monitor_core::Clock;
use monitor_remote::client::{self, AgentClient, AgentEndpoint, ClientError, HostStatus};
use monitor_remote::protocol;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::CommandError;
use crate::settings::PairedHostConfig;
use crate::state::AppState;

pub const CREDENTIALS_FILE: &str = "remote-credentials.json";
pub const HOSTS_EVENT: &str = "hosts-updated";
const HEADLINE_CAP: usize = 120;
/// Metrics tracked as per-host sparkline rings (local receipt time).
const HEADLINE_METRICS: &[&str] = &["cpu.total.usage", "memory.physical.used"];
/// Successful polls between manifest refreshes (families, agent version).
const MANIFEST_EVERY: u32 = 60;

// ---- credential store -------------------------------------------------------

/// Device tokens for paired hosts. `Debug` skips contents by design.
#[derive(Default, Serialize, Deserialize)]
struct CredentialFile {
    version: u32,
    tokens: BTreeMap<String, String>,
}

impl std::fmt::Debug for CredentialFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialFile")
            .field("version", &self.version)
            .field("hosts", &self.tokens.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    pub fn open(data_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Self {
            path: data_dir.join(CREDENTIALS_FILE),
        })
    }

    fn read(&self) -> CredentialFile {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write(&self, file: &CredentialFile) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(file)?;
        monitor_remote::tls::write_restricted(&self.path, &json)
    }

    pub fn get(&self, host_id: &str) -> Option<String> {
        self.read().tokens.get(host_id).cloned()
    }

    pub fn set(&self, host_id: &str, token: &str) -> std::io::Result<()> {
        let mut file = self.read();
        file.version = 1;
        file.tokens.insert(host_id.to_string(), token.to_string());
        self.write(&file)
    }

    pub fn remove(&self, host_id: &str) -> std::io::Result<()> {
        let mut file = self.read();
        file.tokens.remove(host_id);
        self.write(&file)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ---- live state -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeadlineSample {
    /// Local (console) receipt time — sparklines never render remote clocks.
    pub ts_utc_ms: i64,
    pub value: f64,
}

/// What the UI receives for one host. Every value is tagged with the host
/// id, and timestamps come in pairs: ours (`last_success_ms`, `age_ms`) and
/// the agent's (`remote_collected_at_utc`, `clock_skew_ms`).
#[derive(Debug, Clone, Serialize)]
pub struct HostLiveState {
    pub host_id: String,
    pub label: String,
    pub endpoint: String,
    pub fingerprint_sha256: String,
    pub device_id: String,
    pub protocol_version: u32,
    pub record_history: bool,
    pub status: HostStatus,
    /// Local epoch ms of the last accepted response.
    pub last_success_ms: Option<i64>,
    /// Recomputed at read time: ms since the last accepted response.
    pub age_ms: Option<u64>,
    pub last_error_code: Option<String>,
    pub last_error: Option<String>,
    pub remote_collected_at_utc: Option<String>,
    pub remote_sequence: Option<u64>,
    /// Remote timestamp − local receipt (ms); positive = agent clock ahead.
    /// Display-only: skew never drives the live/stale decision.
    pub clock_skew_ms: Option<i64>,
    pub no_sample_yet: bool,
    pub agent_version: Option<String>,
    pub agent_display_name: Option<String>,
    pub families: Vec<String>,
    pub headline: Vec<(String, Vec<HeadlineSample>)>,
    pub readings: Vec<client::RemoteReading>,
}

impl HostLiveState {
    fn from_config(config: &PairedHostConfig) -> Self {
        Self {
            host_id: config.host_id.clone(),
            label: config.label.clone(),
            endpoint: config.endpoint.clone(),
            fingerprint_sha256: config.fingerprint_sha256.clone(),
            device_id: config.device_id.clone(),
            protocol_version: config.protocol_version,
            record_history: config.record_history,
            status: HostStatus::NeverConnected,
            last_success_ms: None,
            age_ms: None,
            last_error_code: None,
            last_error: None,
            remote_collected_at_utc: None,
            remote_sequence: None,
            clock_skew_ms: None,
            no_sample_yet: false,
            agent_version: None,
            agent_display_name: None,
            families: Vec::new(),
            headline: Vec::new(),
            readings: Vec::new(),
        }
    }
}

pub fn error_code(error: &ClientError) -> &'static str {
    match error {
        ClientError::InvalidEndpoint => "endpoint",
        ClientError::InvalidCode => "code",
        ClientError::Transport(_) => "transport",
        ClientError::CertMismatch { .. } => "cert_mismatch",
        ClientError::Unauthorized => "unauthorized",
        ClientError::RateLimited => "rate_limited",
        ClientError::Incompatible { .. } => "incompatible",
        ClientError::NoSampleYet => "no_sample_yet",
        ClientError::PairingRefused(_) => "pairing_refused",
        ClientError::AgentProofInvalid => "agent_proof",
        ClientError::Protocol(_) => "protocol",
        ClientError::Status(_) => "status",
    }
}

/// Shared map entry. `last_success` is an `Instant` so reads can age the
/// status without waiting for a poll.
struct HostEntry {
    view: HostLiveState,
    last_success: Option<Instant>,
    last_error: Option<ClientError>,
}

// ---- ISO-8601 (the one shape monitor_core emits) ----------------------------

/// Parse `YYYY-MM-DDTHH:MM:SS.mmmZ` (or space-separated) to epoch ms.
/// Deliberately strict: anything else reports "unknown skew" instead of a
/// wrong number.
pub fn parse_iso8601_utc_ms(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 24
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || (bytes[10] != b'T' && bytes[10] != b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
        || bytes[23] != b'Z'
    {
        return None;
    }
    let num = |range: std::ops::Range<usize>| value.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second, millis) = (num(11..13)?, num(14..16)?, num(17..19)?, num(20..23)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    // Days from civil date (Howard Hinnant's algorithm), proleptic Gregorian.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second.min(59) * 1_000 + millis)
}

// ---- runtime ----------------------------------------------------------------

struct WorkerHandle {
    stop: Arc<AtomicBool>,
}

pub struct HostsRuntime {
    credentials: CredentialStore,
    states: Mutex<HashMap<String, HostEntry>>,
    workers: Mutex<HashMap<String, WorkerHandle>>,
}

impl std::fmt::Debug for HostsRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostsRuntime")
            .field(
                "workers",
                &self.workers.lock().map(|w| w.len()).unwrap_or(0),
            )
            .finish_non_exhaustive()
    }
}

impl HostsRuntime {
    pub fn open(data_dir: &Path) -> std::io::Result<Self> {
        Ok(Self {
            credentials: CredentialStore::open(data_dir)?,
            states: Mutex::new(HashMap::new()),
            workers: Mutex::new(HashMap::new()),
        })
    }

    pub fn credentials(&self) -> &CredentialStore {
        &self.credentials
    }

    /// Get-or-seed the entry for `config` and apply `f` to it.
    fn with_entry(&self, config: &PairedHostConfig, f: impl FnOnce(&mut HostEntry)) {
        let mut states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        let entry = states
            .entry(config.host_id.clone())
            .or_insert_with(|| HostEntry {
                view: HostLiveState::from_config(config),
                last_success: None,
                last_error: None,
            });
        f(entry);
    }

    /// Seed or refresh the registry view for one configured host (never
    /// touches a running worker).
    pub fn register(&self, config: &PairedHostConfig) {
        self.with_entry(config, |entry| {
            entry.view.label = config.label.clone();
            entry.view.endpoint = config.endpoint.clone();
            entry.view.fingerprint_sha256 = config.fingerprint_sha256.clone();
            entry.view.device_id = config.device_id.clone();
            entry.view.protocol_version = config.protocol_version;
            entry.view.record_history = config.record_history;
        });
    }

    fn note_result(
        &self,
        config: &PairedHostConfig,
        result: &Result<client::RemoteSnapshot, ClientError>,
        local_ms: i64,
    ) {
        self.with_entry(config, |entry| {
            let view = &mut entry.view;
            match result {
                Ok(snapshot) => {
                    entry.last_success = Some(Instant::now());
                    entry.last_error = None;
                    view.status = HostStatus::Online;
                    view.last_success_ms = Some(local_ms);
                    view.last_error_code = None;
                    view.last_error = None;
                    view.no_sample_yet = false;
                    view.remote_collected_at_utc = Some(snapshot.collected_at_utc.clone());
                    view.remote_sequence = Some(snapshot.sequence);
                    view.clock_skew_ms = parse_iso8601_utc_ms(&snapshot.collected_at_utc)
                        .map(|remote_ms| remote_ms - local_ms);
                    push_headline(&mut view.headline, &snapshot.readings, local_ms);
                    view.readings = snapshot.readings.clone();
                }
                Err(ClientError::NoSampleYet) => {
                    // Reachable agent still warming up: live for liveness
                    // purposes, with an honest "no sample yet" marker.
                    entry.last_success = Some(Instant::now());
                    entry.last_error = None;
                    view.status = HostStatus::Online;
                    view.last_success_ms = Some(local_ms);
                    view.last_error_code = None;
                    view.last_error = None;
                    view.no_sample_yet = true;
                    view.readings = Vec::new();
                }
                Err(error) => {
                    entry.last_error = Some(error.clone());
                    view.last_error_code = Some(error_code(error).to_string());
                    view.last_error = Some(error.to_string());
                    view.status = client::derive_status(
                        Instant::now(),
                        Duration::from_secs(1),
                        entry.last_success,
                        Some(error),
                    );
                }
            }
        });
    }

    fn note_manifest(&self, config: &PairedHostConfig, manifest: &protocol::Manifest) {
        self.with_entry(config, |entry| {
            entry.view.agent_version = Some(manifest.agent_version.clone());
            entry.view.agent_display_name = Some(manifest.display_name.clone());
            entry.view.families = manifest.families.clone();
        });
    }

    /// Status as of *now*: terminal states are sticky until the user acts,
    /// everything else ages against the console's receipt clock.
    fn effective_status(entry: &HostEntry, interval: Duration) -> HostStatus {
        match entry.last_error {
            Some(ClientError::Unauthorized) => HostStatus::Unauthorized,
            Some(ClientError::CertMismatch { .. }) => HostStatus::CertMismatch,
            Some(ClientError::Incompatible { .. }) => HostStatus::Incompatible,
            _ if entry.view.last_error_code.as_deref() == Some("credential_missing") => {
                HostStatus::Unauthorized
            }
            _ => client::derive_status(
                Instant::now(),
                interval,
                entry.last_success,
                entry.last_error.as_ref(),
            ),
        }
    }

    /// Serializable snapshot for the UI, sorted by label then id.
    pub fn snapshot(&self, interval_secs: u32) -> Vec<HostLiveState> {
        let interval = Duration::from_secs(interval_secs.max(1) as u64);
        let states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let mut hosts: Vec<HostLiveState> = states
            .values()
            .map(|entry| {
                let mut view = entry.view.clone();
                view.status = Self::effective_status(entry, interval);
                view.age_ms = entry
                    .last_success
                    .map(|at| now.saturating_duration_since(at).as_millis() as u64);
                view
            })
            .collect();
        hosts.sort_by(|a, b| a.label.cmp(&b.label).then(a.host_id.cmp(&b.host_id)));
        hosts
    }

    /// Launch one worker thread for a configured host, replacing any
    /// previous worker for the same host (stop is cooperative). Without a
    /// stored credential the host is shown as unauthorized, nothing polls.
    pub fn start_worker(
        self: &Arc<Self>,
        app: AppHandle,
        config: PairedHostConfig,
        interval_secs: u32,
    ) {
        self.register(&config);
        self.stop_worker(&config.host_id);
        let Some(token) = self.credentials.get(&config.host_id) else {
            self.with_entry(&config, |entry| {
                entry.last_error = None;
                entry.view.status = HostStatus::Unauthorized;
                entry.view.last_error_code = Some("credential_missing".into());
                entry.view.last_error =
                    Some("device token missing on this console; pair again".into());
            });
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let runtime = Arc::clone(self);
        let host_id = config.host_id.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("host-poll-{host_id}"))
            .spawn(move || run_worker(app, runtime, config, token, interval_secs, worker_stop));
        if spawned.is_ok() {
            self.workers
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(host_id, WorkerHandle { stop });
        }
    }

    pub fn stop_worker(&self, host_id: &str) {
        if let Some(worker) = self
            .workers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(host_id)
        {
            worker.stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn stop_all(&self) {
        for (_, worker) in self
            .workers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain()
        {
            worker.stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn worker_count(&self) -> usize {
        self.workers.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Stop polling and drop the live view (credential/settings removal is
    /// the command's job).
    pub fn forget(&self, host_id: &str) {
        self.stop_worker(host_id);
        self.states
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(host_id);
    }

    /// Startup: one worker per configured host. Hosts without a credential
    /// are listed as unauthorized; nothing connects for them.
    pub fn start_all(self: &Arc<Self>, app: &AppHandle) {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let Ok(settings) = state.store.load_settings() else {
            return;
        };
        for config in settings.remote_hosts {
            self.start_worker(app.clone(), config, settings.remote_poll_seconds);
        }
    }
}

fn push_headline(
    headline: &mut Vec<(String, Vec<HeadlineSample>)>,
    readings: &[client::RemoteReading],
    local_ms: i64,
) {
    for metric_id in HEADLINE_METRICS {
        let Some(reading) = readings.iter().find(|r| &r.metric_id == metric_id) else {
            continue;
        };
        let index = match headline.iter().position(|(id, _)| id == metric_id) {
            Some(index) => index,
            None => {
                headline.push(((*metric_id).to_string(), Vec::new()));
                headline.len() - 1
            }
        };
        let samples = &mut headline[index].1;
        samples.push(HeadlineSample {
            ts_utc_ms: local_ms,
            value: reading.value,
        });
        if samples.len() > HEADLINE_CAP {
            let excess = samples.len() - HEADLINE_CAP;
            samples.drain(0..excess);
        }
    }
}

fn sleep_interruptible(stop: &AtomicBool, total: Duration) {
    let slice = Duration::from_millis(250);
    let mut remaining = total;
    while !stop.load(Ordering::Relaxed) && !remaining.is_zero() {
        let step = remaining.min(slice);
        std::thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}

/// One poller thread per host: poll interval on success, exponential
/// backoff (60 s ceiling) on failure, cooperative stop.
fn run_worker(
    app: AppHandle,
    runtime: Arc<HostsRuntime>,
    config: PairedHostConfig,
    token: String,
    interval_secs: u32,
    stop: Arc<AtomicBool>,
) {
    use tauri::Emitter;
    let interval = Duration::from_secs(u64::from(interval_secs.max(2)));
    let Ok(endpoint) = AgentEndpoint::parse(&config.endpoint) else {
        runtime.note_result(&config, &Err(ClientError::InvalidEndpoint), 0);
        return;
    };
    let client = AgentClient::new(&endpoint, &config.fingerprint_sha256, token);
    let mut failures: u32 = 0;
    let mut polls_since_manifest: u32 = MANIFEST_EVERY;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let local_ms = monitor_core::SystemClock.now_epoch_ms();
        let result = match client.as_ref() {
            Ok(client) => client.snapshot(),
            Err(error) => Err(error.clone()),
        };
        let success = matches!(result, Ok(_) | Err(ClientError::NoSampleYet));
        failures = if success {
            0
        } else {
            failures.saturating_add(1)
        };
        runtime.note_result(&config, &result, local_ms);
        if config.record_history {
            if let (Ok(snapshot), Some(state)) = (result.as_ref(), app.try_state::<AppState>()) {
                for reading in &snapshot.readings {
                    state.record_remote_sample(&config.host_id, reading);
                }
            }
        }
        if success {
            polls_since_manifest = polls_since_manifest.saturating_add(1);
            if polls_since_manifest >= MANIFEST_EVERY {
                if let Ok(client) = client.as_ref() {
                    if let Ok(manifest) = client.manifest() {
                        runtime.note_manifest(&config, &manifest);
                        polls_since_manifest = 0;
                    }
                }
            }
        }
        let _ = app.emit(
            HOSTS_EVENT,
            serde_json::json!({ "host_id": config.host_id }),
        );
        let wait = if success {
            interval
        } else {
            client::backoff_for(failures)
        };
        sleep_interruptible(&stop, wait);
    }
}

// ---- commands ---------------------------------------------------------------

fn client_error(error: ClientError) -> CommandError {
    let (code, message_key) = match &error {
        ClientError::InvalidCode => ("HOST_CODE_INVALID", "errors.hostCodeInvalid"),
        ClientError::InvalidEndpoint => ("HOST_ENDPOINT_INVALID", "errors.hostEndpointInvalid"),
        ClientError::Unauthorized => ("HOST_UNAUTHORIZED", "errors.hostUnauthorized"),
        ClientError::CertMismatch { .. } => ("HOST_CERT_MISMATCH", "errors.hostCertMismatch"),
        ClientError::Incompatible { .. } => ("HOST_INCOMPATIBLE", "errors.hostIncompatible"),
        ClientError::PairingRefused(_) | ClientError::AgentProofInvalid => {
            ("HOST_PAIR_REFUSED", "errors.hostPairRefused")
        }
        _ => ("HOST_UNREACHABLE", "errors.hostUnreachable"),
    };
    CommandError {
        code,
        message_key,
        reason: Some(error.to_string()),
    }
}

fn not_found() -> CommandError {
    CommandError {
        code: "HOST_NOT_FOUND",
        message_key: "errors.hostNotFound",
        reason: None,
    }
}

fn io_error(error: std::io::Error) -> CommandError {
    CommandError::store(format!("credential store: {error}"))
}

#[tauri::command]
pub fn get_hosts_state(state: State<'_, AppState>) -> Result<Vec<HostLiveState>, CommandError> {
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    Ok(state.hosts.snapshot(settings.remote_poll_seconds))
}

/// Pair with an agent: runs the TLS + proof exchange off the main thread,
/// refuses duplicate host identities at a different endpoint (ADR-0009 A9),
/// then persists the token (restricted file) and the record (settings).
#[tauri::command]
pub async fn pair_host(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
    code: String,
    label: String,
) -> Result<HostLiveState, CommandError> {
    let label = label.trim().to_string();
    if !monitor_remote::pairing::label_is_valid(&label) {
        return Err(CommandError {
            code: "HOST_LABEL_INVALID",
            message_key: "errors.hostLabelInvalid",
            reason: None,
        });
    }
    let endpoint =
        AgentEndpoint::parse(&endpoint).map_err(|_| client_error(ClientError::InvalidEndpoint))?;
    let settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let already = settings
        .remote_hosts
        .iter()
        .any(|host| host.endpoint == endpoint.display());
    if !already && settings.remote_hosts.len() >= client::MAX_HOSTS {
        return Err(CommandError {
            code: "HOST_LIMIT",
            message_key: "errors.hostLimit",
            reason: Some(client::MAX_HOSTS.to_string()),
        });
    }
    // This console's identity at the agent is its installation UUID; the
    // label it presents is the user's display name for this machine.
    let device_id = state.host_id.clone();
    let console_label = settings.remote_display_name.clone();
    let pair_endpoint = endpoint.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        client::pair(&pair_endpoint, &code, &device_id, &console_label)
    })
    .await
    .map_err(|err| CommandError::store(format!("pairing task: {err}")))?
    .map_err(client_error)?;

    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    if let Some(existing) = settings
        .remote_hosts
        .iter()
        .find(|host| host.host_id == outcome.host_id)
    {
        if existing.endpoint != endpoint.display() {
            return Err(CommandError {
                code: "HOST_ALREADY_PAIRED",
                message_key: "errors.hostAlreadyPaired",
                reason: Some(existing.endpoint.clone()),
            });
        }
    }
    state
        .hosts
        .credentials()
        .set(&outcome.host_id, &outcome.device_token)
        .map_err(io_error)?;
    let config = PairedHostConfig {
        host_id: outcome.host_id.clone(),
        label,
        endpoint: endpoint.display(),
        fingerprint_sha256: outcome.fingerprint_hex.clone(),
        device_id: state.host_id.clone(),
        protocol_version: outcome.protocol_version,
        added_at_utc: monitor_core::SystemClock.now_utc_iso8601(),
        record_history: settings
            .remote_hosts
            .iter()
            .find(|host| host.host_id == outcome.host_id)
            .map(|host| host.record_history)
            .unwrap_or(false),
    };
    settings
        .remote_hosts
        .retain(|host| host.host_id != outcome.host_id);
    settings.remote_hosts.push(config.clone());
    settings
        .validate()
        .map_err(|err| CommandError::settings_invalid(err.to_string()))?;
    state
        .store
        .save_settings(&settings)
        .map_err(|err| CommandError::store(err.to_string()))?;
    state
        .hosts
        .start_worker(app, config, settings.remote_poll_seconds);
    state
        .hosts
        .snapshot(settings.remote_poll_seconds)
        .into_iter()
        .find(|host| host.host_id == outcome.host_id)
        .ok_or_else(not_found)
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveHostResult {
    pub host_id: String,
    pub history_purged: bool,
    pub history_deleted: Option<monitor_history::RetentionSummary>,
}

/// Remove a paired host: stop polling, drop the credential and the record,
/// and (only when asked) delete the rows stored under its host id.
#[tauri::command]
pub fn remove_host(
    state: State<'_, AppState>,
    host_id: String,
    purge_history: bool,
) -> Result<RemoveHostResult, CommandError> {
    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let before = settings.remote_hosts.len();
    settings.remote_hosts.retain(|host| host.host_id != host_id);
    if settings.remote_hosts.len() == before && !state.hosts.credentials().get(&host_id).is_some() {
        return Err(not_found());
    }
    state.hosts.forget(&host_id);
    state
        .hosts
        .credentials()
        .remove(&host_id)
        .map_err(io_error)?;
    state
        .store
        .save_settings(&settings)
        .map_err(|err| CommandError::store(err.to_string()))?;
    let history_deleted = if purge_history {
        // Never delete the local host's history through this path.
        if host_id == state.host_id {
            return Err(CommandError {
                code: "HOST_IS_LOCAL",
                message_key: "errors.hostIsLocal",
                reason: None,
            });
        }
        let dir = state
            .history_dir
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| CommandError::store("history directory unavailable".into()))?;
        let conn = rusqlite::Connection::open(dir.join("history.sqlite3"))
            .map_err(|err| CommandError::store(err.to_string()))?;
        Some(
            monitor_history::delete_host_samples(&conn, &host_id)
                .map_err(|err| CommandError::store(err.to_string()))?,
        )
    } else {
        None
    };
    Ok(RemoveHostResult {
        host_id,
        history_purged: purge_history,
        history_deleted,
    })
}

fn update_host_config(
    state: &AppState,
    host_id: &str,
    apply: impl FnOnce(&mut PairedHostConfig),
) -> Result<(PairedHostConfig, u32), CommandError> {
    let mut settings = state
        .store
        .load_settings()
        .map_err(|err| CommandError::store(err.to_string()))?;
    let host = settings
        .remote_hosts
        .iter_mut()
        .find(|host| host.host_id == host_id)
        .ok_or_else(not_found)?;
    apply(host);
    let updated = host.clone();
    settings
        .validate()
        .map_err(|err| CommandError::settings_invalid(err.to_string()))?;
    state
        .store
        .save_settings(&settings)
        .map_err(|err| CommandError::store(err.to_string()))?;
    Ok((updated, settings.remote_poll_seconds))
}

#[tauri::command]
pub fn rename_host(
    state: State<'_, AppState>,
    host_id: String,
    label: String,
) -> Result<HostLiveState, CommandError> {
    let label = label.trim().to_string();
    if !monitor_remote::pairing::label_is_valid(&label) {
        return Err(CommandError {
            code: "HOST_LABEL_INVALID",
            message_key: "errors.hostLabelInvalid",
            reason: None,
        });
    }
    let (config, interval) = update_host_config(&state, &host_id, |host| host.label = label)?;
    state.hosts.register(&config);
    state
        .hosts
        .snapshot(interval)
        .into_iter()
        .find(|host| host.host_id == host_id)
        .ok_or_else(not_found)
}

/// Toggle local history recording for a host; the worker restarts so the
/// new flag applies from the next poll.
#[tauri::command]
pub fn set_host_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    host_id: String,
    enabled: bool,
) -> Result<HostLiveState, CommandError> {
    let (config, interval) =
        update_host_config(&state, &host_id, |host| host.record_history = enabled)?;
    state.hosts.start_worker(app, config, interval);
    state
        .hosts
        .snapshot(interval)
        .into_iter()
        .find(|host| host.host_id == host_id)
        .ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(host_id: &str) -> PairedHostConfig {
        PairedHostConfig {
            host_id: host_id.into(),
            label: "Office Mac".into(),
            endpoint: "192.168.1.5:45454".into(),
            fingerprint_sha256: "ab".repeat(32),
            device_id: "console-uuid".into(),
            protocol_version: protocol::PROTOCOL_VERSION,
            added_at_utc: "2026-10-19T00:00:00.000Z".into(),
            record_history: false,
        }
    }

    fn reading(metric_id: &str, value: f64) -> client::RemoteReading {
        client::RemoteReading {
            metric_id: metric_id.into(),
            entity_id: "host".into(),
            value,
            unit: "percent".into(),
            collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
            sequence: 1,
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("hostlume-hosts-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn iso8601_parser_round_trips_core_format_and_rejects_junk() {
        for ms in [0i64, 1_000, 1_760_000_000_000, 4_102_444_800_123] {
            let text = monitor_core::clock::format_utc_iso8601(ms);
            assert_eq!(parse_iso8601_utc_ms(&text), Some(ms), "{text}");
        }
        assert_eq!(
            parse_iso8601_utc_ms("2026-10-19T00:00:00.000Z"),
            Some(1_792_368_000_000)
        );
        assert_eq!(parse_iso8601_utc_ms("2026-10-19T00:00:00Z"), None);
        assert_eq!(parse_iso8601_utc_ms("not a date at all!!!!!!"), None);
        assert_eq!(parse_iso8601_utc_ms("2026-13-19T00:00:00.000Z"), None);
    }

    /// ADR-0009 A5: tokens live only in the restricted file.
    #[test]
    fn credential_store_is_owner_only_and_never_in_settings_shape() {
        let dir = temp_dir("cred");
        let store = CredentialStore::open(&dir).unwrap();
        assert_eq!(store.get("h1"), None);
        store.set("h1", "secret-token").unwrap();
        store.set("h2", "other").unwrap();
        assert_eq!(store.get("h1").as_deref(), Some("secret-token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(store.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        store.remove("h1").unwrap();
        assert_eq!(store.get("h1"), None);
        assert_eq!(store.get("h2").as_deref(), Some("other"));
        // Debug output never leaks token material.
        let rendered = format!("{:?}", store.read());
        assert!(!rendered.contains("other"));
        // The settings record type has no token field at all (compile-time
        // shape): serializing a config contains no "token" key.
        let json = serde_json::to_string(&config("h2")).unwrap();
        assert!(!json.contains("token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Exit gate: stale data is never presented as live — status ages from
    /// the console clock; remote timestamps only feed the skew field.
    #[test]
    fn liveness_ages_from_local_clock_and_skew_is_display_only() {
        let dir = temp_dir("live");
        let runtime = HostsRuntime::open(&dir).unwrap();
        let cfg = config("remote-1");
        runtime.register(&cfg);
        let local_ms = 1_792_368_000_000; // 2026-10-19T00:00:00.000Z
        let snapshot = client::RemoteSnapshot {
            schema_version: 1,
            sequence: 7,
            // Agent clock 5 minutes ahead.
            collected_at_utc: "2026-10-19T00:05:00.000Z".into(),
            readings: vec![reading("cpu.total.usage", 42.5)],
        };
        runtime.note_result(&cfg, &Ok(snapshot), local_ms);
        let view = &runtime.snapshot(5)[0];
        assert_eq!(view.status, HostStatus::Online);
        assert_eq!(view.clock_skew_ms, Some(300_000));
        assert_eq!(view.remote_sequence, Some(7));
        assert_eq!(view.headline[0].0, "cpu.total.usage");
        assert_eq!(view.headline[0].1[0].value, 42.5);
        assert_eq!(
            view.headline[0].1[0].ts_utc_ms, local_ms,
            "ring uses local time"
        );
        assert!(view.age_ms.unwrap() < 5_000);

        // Backdate the receipt: the same entry must read stale, then offline.
        {
            let mut states = runtime.states.lock().unwrap();
            states.get_mut("remote-1").unwrap().last_success =
                Some(Instant::now() - Duration::from_secs(20));
        }
        assert_eq!(runtime.snapshot(5)[0].status, HostStatus::Stale);
        {
            let mut states = runtime.states.lock().unwrap();
            states.get_mut("remote-1").unwrap().last_success =
                Some(Instant::now() - Duration::from_secs(120));
        }
        assert_eq!(runtime.snapshot(5)[0].status, HostStatus::Offline);

        // Terminal errors stick even after a success in the past.
        runtime.note_result(&cfg, &Err(ClientError::Unauthorized), local_ms);
        assert_eq!(runtime.snapshot(5)[0].status, HostStatus::Unauthorized);
        assert_eq!(
            runtime.snapshot(5)[0].last_error_code.as_deref(),
            Some("unauthorized")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_sample_yet_counts_as_reachable_and_headline_ring_is_bounded() {
        let dir = temp_dir("ring");
        let runtime = HostsRuntime::open(&dir).unwrap();
        let cfg = config("remote-2");
        runtime.note_result(&cfg, &Err(ClientError::NoSampleYet), 1);
        let view = &runtime.snapshot(5)[0];
        assert_eq!(view.status, HostStatus::Online);
        assert!(view.no_sample_yet);
        for i in 0..(HEADLINE_CAP as i64 + 40) {
            let snapshot = client::RemoteSnapshot {
                schema_version: 1,
                sequence: i as u64,
                collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
                readings: vec![reading("cpu.total.usage", i as f64)],
            };
            runtime.note_result(&cfg, &Ok(snapshot), i);
        }
        let view = &runtime.snapshot(5)[0];
        assert_eq!(view.headline[0].1.len(), HEADLINE_CAP);
        assert_eq!(
            view.headline[0].1.last().unwrap().value,
            (HEADLINE_CAP as i64 + 39) as f64
        );
        assert!(!view.no_sample_yet);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No implicit networking: a registry with no credential starts no
    /// worker and reports the host as unauthorized.
    #[test]
    fn missing_credential_starts_no_worker() {
        let dir = temp_dir("nocred");
        let runtime = Arc::new(HostsRuntime::open(&dir).unwrap());
        let cfg = config("remote-3");
        runtime.register(&cfg);
        assert_eq!(runtime.worker_count(), 0);
        // start_worker needs an AppHandle, which tests cannot build; the
        // credential gate it relies on is asserted directly.
        assert!(runtime.credentials().get(&cfg.host_id).is_none());
        runtime.forget(&cfg.host_id);
        assert!(runtime.snapshot(5).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
