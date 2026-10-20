//! The read-only HTTP surface (phase 08 §3.7–§3.11, phase 10 ADR-0009).
//!
//! * `GET /api/v1/{capabilities,system,snapshot,cpu,memory,disks,network,
//!   gpu,sensors,battery,bluetooth,processes,metrics/history,manifest}` —
//!   token- or session-authenticated, privacy-projected, bounded.
//! * `POST /api/v1/session` — exchanges the owner token for a short-lived
//!   HttpOnly SameSite=Strict cookie so the built-in dashboard's
//!   EventSource needs no URL token (forbidden).
//! * `POST /api/v1/pair` — the pairing exchange (phase 10): unauthenticated
//!   by necessity, but code-gated (one attempt per code), rate-limited, and
//!   only answered while the agent user holds a pairing window open.
//! * `GET /` — the embedded read-only dashboard (no data without auth).
//! * Everything else: 404; wrong methods: 405 by method routing — there is
//!   no write, kill, trash or shell handler and no Tauri bridge.
//! * No CORS headers are emitted, so browsers block cross-origin reads.

#![allow(clippy::result_large_err)]

use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Query as QueryExtractor, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use monitor_core::{families, Clock};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::{self, Principal, RateDecision};
use crate::pairing::{self, PairError};
use crate::protocol::{self, Manifest, PairRequest, PairResponse, PairedDevice};
use crate::tls::ClientAddr;
use crate::{
    projection, DeviceCredential, RemoteData, ServerShared, MAX_SSE_CLIENTS, REMOTE_MAX_POINTS,
    REMOTE_MAX_SPAN_MS, SSE_SEND_TIMEOUT, SSE_TICK,
};

pub const SESSION_COOKIE: &str = "hostlume_session";
const MAX_BODY: usize = 4_096;

pub struct AppShared {
    pub data: Arc<RemoteData>,
    pub shared: Arc<ServerShared>,
}

pub fn router(data: Arc<RemoteData>, shared: Arc<ServerShared>) -> Router {
    let state = Arc::new(AppShared { data, shared });
    Router::new()
        .route("/", get(dashboard))
        .route("/api/v1/capabilities", get(capabilities))
        .route("/api/v1/manifest", get(manifest))
        .route("/api/v1/system", get(system))
        .route("/api/v1/snapshot", get(snapshot))
        .route("/api/v1/cpu", get(cpu))
        .route("/api/v1/memory", get(memory))
        .route("/api/v1/disks", get(disks))
        .route("/api/v1/network", get(network))
        .route("/api/v1/gpu", get(gpu))
        .route("/api/v1/sensors", get(sensors))
        .route("/api/v1/battery", get(battery))
        .route("/api/v1/bluetooth", get(bluetooth))
        .route("/api/v1/processes", get(processes))
        .route("/api/v1/metrics/history", get(metrics_history))
        .route("/api/v1/stream", get(stream))
        .route("/api/v1/session", post(create_session))
        .route("/api/v1/pair", post(pair))
        .fallback(fallback_404)
        .with_state(state)
}

// ---- shared helpers ---------------------------------------------------------

// Cold reject paths (401/405/429/5xx) keep unboxed `Response` errors;
// boxing would add allocation for no functional benefit.
#[allow(clippy::result_large_err)]
fn api_response(status: StatusCode, body: serde_json::Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

fn error_body(code: &str) -> serde_json::Value {
    json!({ "error": code })
}

/// Authentication + rate-limit gate shared by every data route.
fn authorize(
    state: &AppShared,
    ip: ClientAddr,
    headers: &HeaderMap,
) -> Result<Principal, Response> {
    state.shared.requests_seen.fetch_add(1, Ordering::SeqCst);
    let limited = {
        let mut rate = state.shared.rate.lock().unwrap_or_else(|p| p.into_inner());
        rate.check(ip.0.ip())
    };
    if limited == RateDecision::Limit {
        return Err(api_response(
            StatusCode::TOO_MANY_REQUESTS,
            error_body("rate_limited"),
        ));
    }
    principal(state, headers)
        .ok_or_else(|| api_response(StatusCode::UNAUTHORIZED, error_body("unauthorized")))
}

/// Bearer token (owner or paired device) or dashboard session cookie.
/// Neither the token nor the session id is ever logged or echoed back.
fn principal(state: &AppShared, headers: &HeaderMap) -> Option<Principal> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = value.strip_prefix("Bearer ") {
            return state.shared.authenticate_bearer(token);
        }
    }
    if let Some(cookies) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for pair in cookies.split(';') {
            let pair = pair.trim();
            if let Some(sid) = pair.strip_prefix(SESSION_COOKIE) {
                let sid = sid.strip_prefix('=').unwrap_or(sid);
                let valid = state
                    .shared
                    .sessions
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .is_valid(sid, Instant::now());
                if valid {
                    return Some(Principal::Owner);
                }
            }
        }
    }
    None
}

fn latest_snapshot(state: &AppShared) -> Result<monitor_core::SystemSnapshot, Response> {
    state
        .data
        .snapshot
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .ok_or_else(|| api_response(StatusCode::SERVICE_UNAVAILABLE, error_body("no_sample_yet")))
}

// ---- data routes ------------------------------------------------------------

async fn capabilities(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    let report = state
        .data
        .capabilities
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    Ok(api_response(
        StatusCode::OK,
        serde_json::to_value(report).unwrap_or_else(|_| error_body("serialize")),
    ))
}

/// Versioned capability handshake (ADR-0009 §1). Authenticated: the
/// display name and host id are not broadcast to unpaired peers.
async fn manifest(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    let families = {
        let report = state
            .data
            .capabilities
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        protocol::families_of(&report)
    };
    let manifest = Manifest {
        protocol_version: protocol::PROTOCOL_VERSION,
        agent_version: protocol::AGENT_VERSION.to_string(),
        host_id: state.data.host_id.clone(),
        display_name: state
            .shared
            .display_name
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone(),
        cadence_ms: state.data.cadence_ms,
        families,
        tls_fingerprint_sha256: state.shared.tls_fingerprint_hex.clone(),
    };
    Ok(api_response(
        StatusCode::OK,
        serde_json::to_value(manifest).unwrap_or_else(|_| error_body("serialize")),
    ))
}

async fn system(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    let snapshot = latest_snapshot(&state)?;
    Ok(api_response(
        StatusCode::OK,
        serde_json::to_value(projection::system_payload(&snapshot).0)
            .unwrap_or_else(|_| error_body("serialize")),
    ))
}

async fn snapshot(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    let snapshot = latest_snapshot(&state)?;
    Ok(api_response(
        StatusCode::OK,
        serde_json::to_value(projection::snapshot_payload(&snapshot).0)
            .unwrap_or_else(|_| error_body("serialize")),
    ))
}

macro_rules! family_route {
    ($name:ident, $family:literal) => {
        async fn $name(
            State(state): State<Arc<AppShared>>,
            ConnectInfo(ip): ConnectInfo<ClientAddr>,
            headers: HeaderMap,
        ) -> Result<Response, Response> {
            authorize(&state, ip, &headers)?;
            let snapshot = latest_snapshot(&state)?;
            Ok(api_response(
                StatusCode::OK,
                serde_json::to_value(projection::family_payload(&snapshot, $family).0)
                    .unwrap_or_else(|_| error_body("serialize")),
            ))
        }
    };
}

family_route!(cpu, "cpu.");
family_route!(memory, "memory.");
family_route!(disks, "disk.");
family_route!(network, "net.");
family_route!(gpu, "gpu.");
family_route!(sensors, "sensor.");
family_route!(battery, "battery.");
family_route!(bluetooth, "bluetooth.");

async fn processes(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    let rows = state
        .data
        .process_rows
        .as_ref()
        .map(|fetch| fetch())
        .unwrap_or_default();
    Ok(api_response(
        StatusCode::OK,
        json!({ "rows": rows, "note": "sanitized projection: no paths, no command lines" }),
    ))
}

/// Bounded, whitelisted history (contract §7). `max_points` is fixed for
/// remote callers; range and identifiers are validated before any query.
#[allow(clippy::too_many_arguments)]
async fn metrics_history(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
    QueryExtractor(params): QueryExtractor<std::collections::HashMap<String, String>>,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    let invalid = |reason: &str| {
        api_response(
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid_query", "detail": reason }),
        )
    };
    let metric_id = params
        .get("metric_id")
        .ok_or_else(|| invalid("metric_id required"))?;
    if !families::ALL_FAMILIES.contains(&metric_id.as_str()) {
        return Err(invalid("metric_id is not a known family"));
    }
    let entity_id = params
        .get("entity_id")
        .ok_or_else(|| invalid("entity_id required"))?;
    if entity_id.is_empty() || entity_id.len() > 128 {
        return Err(invalid("entity_id length"));
    }
    let from_utc_ms: i64 = params
        .get("from_utc_ms")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| invalid("from_utc_ms required"))?;
    let to_utc_ms: i64 = params
        .get("to_utc_ms")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| invalid("to_utc_ms required"))?;
    if to_utc_ms <= from_utc_ms || to_utc_ms - from_utc_ms > REMOTE_MAX_SPAN_MS {
        return Err(invalid("range"));
    }
    let query = monitor_history::HistoryQuery {
        host_id: state.data.host_id.clone(),
        metric_id: metric_id.clone(),
        entity_id: entity_id.clone(),
        from_utc_ms,
        to_utc_ms,
        cadence_ms: state.data.cadence_ms.max(1),
        max_points: REMOTE_MAX_POINTS,
    };
    // The seam carries the history *directory*; each query opens its own
    // WAL connection to history.sqlite3 (same pattern as local IPC).
    let store =
        monitor_history::HistoryStore::open(&state.data.history_db_path.join("history.sqlite3"))
            .map_err(|err| {
                api_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "error": "store", "detail": err.to_string() }),
                )
            })?;
    let response = monitor_history::get_history(store.connection(), &query).map_err(|err| {
        api_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": "query", "detail": err.to_string() }),
        )
    })?;
    Ok(api_response(
        StatusCode::OK,
        serde_json::to_value(response).unwrap_or_else(|_| error_body("serialize")),
    ))
}

// ---- SSE stream (ADR-0008) --------------------------------------------------

async fn stream(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    authorize(&state, ip, &headers)?;
    // Client cap: an explicit 503 rather than unbounded connections.
    if state.shared.sse_clients.load(Ordering::SeqCst) >= MAX_SSE_CLIENTS {
        return Err(api_response(
            StatusCode::SERVICE_UNAVAILABLE,
            error_body("client_limit"),
        ));
    }
    state.shared.sse_clients.fetch_add(1, Ordering::SeqCst);

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(4);
    let snapshot_source = Arc::clone(&state.data.snapshot);
    let shared = Arc::clone(&state.shared);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(SSE_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            // Slow-client guard: a bounded buffer that cannot drain within
            // the send budget ends this client's stream (phase 08 §3.8).
            let event = {
                let guard = snapshot_source.read().unwrap_or_else(|p| p.into_inner());
                match guard.as_ref() {
                    Some(snapshot) => {
                        let payload = projection::snapshot_payload(snapshot).0;
                        Event::default()
                            .event("snapshot")
                            .json_data(payload)
                            .unwrap_or_else(|_| Event::default().event("tick"))
                    }
                    None => Event::default().event("wait"),
                }
            };
            match tokio::time::timeout(SSE_SEND_TIMEOUT, tx.send(Ok(event))).await {
                Ok(Ok(())) => {}
                _ => break,
            }
        }
        shared.sse_clients.fetch_sub(1, Ordering::SeqCst);
    });

    let stream = ReceiverStream::new(rx);
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_TICK).text("keep-alive")),
    )
        .into_response())
}

// ---- session exchange, pairing, dashboard -----------------------------------

/// Owner token → HttpOnly SameSite=Strict session cookie. Auth failures
/// are rate-limited per IP (brute-force budget). Device tokens cannot mint
/// dashboard sessions: the dashboard is the owner's surface.
async fn create_session(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    if auth_rate_limited(&state, ip) {
        return api_response(StatusCode::TOO_MANY_REQUESTS, error_body("rate_limited"));
    }
    if body.len() > MAX_BODY {
        return api_response(StatusCode::PAYLOAD_TOO_LARGE, error_body("payload"));
    }
    let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&body);
    let token = parsed.ok().and_then(|value| {
        value
            .get("token")
            .and_then(|t| t.as_str())
            .map(str::to_string)
    });
    let Some(token) = token else {
        return api_response(StatusCode::BAD_REQUEST, error_body("token_required"));
    };
    if !auth::token_matches(&token, &state.shared.token_hash()) {
        return api_response(StatusCode::UNAUTHORIZED, error_body("unauthorized"));
    }
    let sid = state
        .shared
        .sessions
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .create(crate::SESSION_TTL, Instant::now());
    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        SESSION_COOKIE,
        sid,
        crate::SESSION_TTL.as_secs()
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

fn auth_rate_limited(state: &AppShared, ip: ClientAddr) -> bool {
    let mut auth_rate = state
        .shared
        .auth_rate
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    auth_rate.check(ip.0.ip()) == RateDecision::Limit
}

/// Pairing exchange (ADR-0009 §3). Order of checks is deliberate: cheap
/// shape/version rejections happen **without** touching the window, so a
/// malformed probe cannot burn a code the real console is about to use;
/// every attempt that reaches the proof check consumes the window.
async fn pair(
    State(state): State<Arc<AppShared>>,
    ConnectInfo(ip): ConnectInfo<ClientAddr>,
    body: Bytes,
) -> Response {
    state.shared.requests_seen.fetch_add(1, Ordering::SeqCst);
    if auth_rate_limited(&state, ip) {
        return api_response(StatusCode::TOO_MANY_REQUESTS, error_body("rate_limited"));
    }
    if body.len() > MAX_BODY {
        return api_response(StatusCode::PAYLOAD_TOO_LARGE, error_body("payload"));
    }
    let Ok(request) = serde_json::from_slice::<PairRequest>(&body) else {
        return api_response(StatusCode::BAD_REQUEST, error_body("malformed"));
    };
    if !protocol::is_compatible(request.protocol_version) {
        return api_response(
            StatusCode::CONFLICT,
            json!({ "error": "incompatible", "protocol_version": protocol::PROTOCOL_VERSION }),
        );
    }
    if !pairing::identifier_is_valid(&request.device_id, pairing::MAX_DEVICE_ID_LEN)
        || !pairing::identifier_is_valid(&request.nonce, pairing::MAX_NONCE_LEN)
        || !pairing::label_is_valid(&request.device_label)
    {
        return api_response(StatusCode::BAD_REQUEST, error_body("malformed"));
    }
    let Some(fingerprint) = state.shared.tls_fingerprint_hex.clone() else {
        // Loopback (plain HTTP) has no certificate to bind the proof to.
        return api_response(StatusCode::CONFLICT, error_body("pairing_unavailable"));
    };
    if state.shared.device_count() >= protocol::MAX_DEVICES {
        return api_response(StatusCode::CONFLICT, error_body("device_limit"));
    }

    let verified = {
        let mut guard = state
            .shared
            .pairing
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(window) = guard.as_mut() else {
            return api_response(StatusCode::FORBIDDEN, error_body("no_pairing_window"));
        };
        let result = window.verify(
            Instant::now(),
            &request.device_id,
            &request.nonce,
            &fingerprint,
            &request.proof,
        );
        // Whatever happened, this window is spent.
        *guard = None;
        result
    };
    let ack = match verified {
        Ok(ack) => ack,
        Err(PairError::Expired) => {
            return api_response(StatusCode::FORBIDDEN, error_body("expired"));
        }
        Err(PairError::Burned) | Err(PairError::NoWindow) => {
            return api_response(StatusCode::FORBIDDEN, error_body("no_pairing_window"));
        }
        Err(PairError::InvalidProof) => {
            return api_response(StatusCode::FORBIDDEN, error_body("invalid_proof"));
        }
        Err(PairError::Malformed) => {
            return api_response(StatusCode::BAD_REQUEST, error_body("malformed"));
        }
    };

    let device_token = auth::generate_token();
    let token_hash = auth::hash_token(&device_token);
    let device = PairedDevice {
        device_id: request.device_id.clone(),
        label: request.device_label.trim().to_string(),
        token_hash: token_hash.clone(),
        created_at_utc: monitor_core::SystemClock.now_utc_iso8601(),
        protocol_version: protocol::PROTOCOL_VERSION,
    };
    state.shared.add_device(DeviceCredential {
        device_id: device.device_id.clone(),
        token_hash,
    });
    if let Some(persist) = state.data.on_device_paired.as_ref() {
        persist(device);
    }
    let response = PairResponse {
        device_token,
        host_id: state.data.host_id.clone(),
        display_name: state
            .shared
            .display_name
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone(),
        protocol_version: protocol::PROTOCOL_VERSION,
        ack,
    };
    api_response(
        StatusCode::OK,
        serde_json::to_value(response).unwrap_or_else(|_| error_body("serialize")),
    )
}

async fn dashboard() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        include_str!("dashboard.html"),
    )
        .into_response()
}

async fn fallback_404() -> Response {
    api_response(StatusCode::NOT_FOUND, error_body("not_found"))
}
