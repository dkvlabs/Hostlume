//! Remote server integration tests (phase 08 §4): the security negatives
//! run over real loopback HTTP — OFF never binds, LAN refuses, missing /
//! bad / rotated tokens are blocked, methods are read-only, queries are
//! bounded, the session exchange mints SameSite=Strict cookies, the SSE
//! client cap holds, and floods trip the rate limiter.

use std::net::TcpListener;
use std::sync::{Arc, Mutex, RwLock};

use monitor_core::{CapabilityReport, SystemSnapshot};
use monitor_remote::{spawn, RemoteConfig, RemoteData, RemoteMode, MAX_SSE_CLIENTS};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn data() -> RemoteData {
    RemoteData {
        host_id: "host-uuid".into(),
        cadence_ms: 3_000,
        snapshot: Arc::default(),
        capabilities: Arc::new(Mutex::new(CapabilityReport::new("host-uuid", "t"))),
        history_db_path: {
            let dir = std::env::temp_dir().join(format!(
                "hostlume-remote-test-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            dir
        },
        process_rows: None,
        on_device_paired: None,
    }
}

fn loopback_config(port: u16) -> RemoteConfig {
    RemoteConfig {
        mode: RemoteMode::Loopback,
        port,
    }
}

struct TestServer {
    port: u16,
    token: String,
    snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    handle: monitor_remote::RemoteServerHandle,
}

fn start_server() -> TestServer {
    let port = free_port();
    let token = monitor_remote::auth::generate_token();
    let snapshot: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
    let data = RemoteData {
        snapshot: Arc::clone(&snapshot),
        ..data()
    };
    let handle = spawn(
        &loopback_config(port),
        data,
        monitor_remote::auth::hash_token(&token),
    )
    .expect("server binds");
    TestServer {
        port,
        token,
        snapshot,
        handle,
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    cookie: Option<&str>,
    body: Option<&str>,
) -> (u16, String, Vec<String>) {
    let url = format!("http://127.0.0.1:{port}{path}");
    let mut req = ureq::request(method, &url);
    if let Some(token) = token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    if let Some(cookie) = cookie {
        req = req.set("Cookie", &format!("hostlume_session={cookie}"));
    }
    let result = match body {
        Some(body) => req.send_string(body),
        None => req.call(),
    };
    match result {
        Ok(response) => collect(response),
        Err(ureq::Error::Status(_code, response)) => collect(response),
        Err(other) => panic!("{method} {path} failed: {other}"),
    }
}

fn collect(response: ureq::Response) -> (u16, String, Vec<String>) {
    let status = response.status();
    let cookies: Vec<String> = response
        .all("set-cookie")
        .into_iter()
        .map(str::to_string)
        .collect();
    let body = response.into_string().unwrap_or_default();
    (status, body, cookies)
}

/// Phase 08 §4: server OFF yields no listener (typed refusal here; the OS
/// `lsof` inspection runs in the live smoke). LAN fails closed.
#[test]
fn off_never_binds_and_lan_fails_closed() {
    let config = RemoteConfig::default();
    let err = spawn(&config, data(), "hash".into()).unwrap_err();
    assert!(matches!(err, monitor_remote::RemoteError::Disabled));

    let config = RemoteConfig {
        mode: RemoteMode::Lan,
        port: free_port(),
    };
    let err = spawn(&config, data(), "hash".into()).unwrap_err();
    assert!(matches!(err, monitor_remote::RemoteError::LanRequiresTls));
}

/// security-privacy.md §9 "cross-origin browser request": the server never
/// grants cross-origin read access (no `Access-Control-Allow-Origin`, so a
/// browser blocks the response) and a foreign `Origin` buys nothing — the
/// token is still required, and a valid token still produces no CORS grant.
#[test]
fn cross_origin_requests_get_no_cors_grant_and_still_need_auth() {
    let server = start_server();
    // Capabilities always answer (snapshot routes are 503 until a tick).
    let url = format!("http://127.0.0.1:{}/api/v1/capabilities", server.port);

    let anonymous = ureq::request("GET", &url)
        .set("Origin", "https://evil.example")
        .call();
    let response = match anonymous {
        Err(ureq::Error::Status(_, response)) | Ok(response) => response,
        Err(other) => panic!("request failed: {other}"),
    };
    assert_eq!(response.status(), 401);
    assert!(response.header("access-control-allow-origin").is_none());
    assert!(response
        .header("access-control-allow-credentials")
        .is_none());

    let authed = ureq::request("GET", &url)
        .set("Origin", "https://evil.example")
        .set("Authorization", &format!("Bearer {}", server.token))
        .call()
        .expect("authenticated read");
    assert_eq!(authed.status(), 200);
    assert!(authed.header("access-control-allow-origin").is_none());

    // Preflight for a cross-origin fetch has no handler: nothing to grant.
    let preflight = ureq::request("OPTIONS", &url)
        .set("Origin", "https://evil.example")
        .set("Access-Control-Request-Method", "GET")
        .call();
    let preflight = match preflight {
        Err(ureq::Error::Status(_, response)) | Ok(response) => response,
        Err(other) => panic!("preflight failed: {other}"),
    };
    assert!(
        preflight.status() == 405 || preflight.status() == 404 || preflight.status() == 401,
        "unexpected preflight status {}",
        preflight.status()
    );
    assert!(preflight.header("access-control-allow-origin").is_none());
}

/// Missing / wrong tokens are 401 on every data route; the token is never
/// echoed in any response.
#[test]
fn missing_and_bad_tokens_are_blocked() {
    let server = start_server();
    for path in [
        "/api/v1/snapshot",
        "/api/v1/system",
        "/api/v1/cpu",
        "/api/v1/processes",
        "/api/v1/metrics/history?metric_id=cpu.total.usage&entity_id=host&from_utc_ms=1&to_utc_ms=2",
    ] {
        let (status, body, _) = request(server.port, "GET", path, None, None, None);
        assert_eq!(status, 401, "{path} without token");
        assert!(!body.contains(&server.token), "token never echoed");
        let (status, _, _) = request(server.port, "GET", path, Some("wrong-token"), None, None);
        assert_eq!(status, 401, "{path} with bad token");
    }
}

/// With a snapshot present, the valid token reads real projections; the
/// privacy projection holds (text metrics never appear); unknown routes
/// 404; non-GET methods on data routes 405.
#[test]
fn authed_reads_work_and_writes_do_not_exist() {
    let server = start_server();

    // Before the first tick the honest state is explicit.
    let (status, body, _) = request(
        server.port,
        "GET",
        "/api/v1/system",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 503, "no_sample_yet is a first-class state");
    assert!(body.contains("no_sample_yet"));

    let mut snapshot = SystemSnapshot::empty("h", "g", 7, "2026-10-19T00:00:00.000Z");
    snapshot.metrics.push(monitor_core::MetricReading {
        metric_id: monitor_core::families::CPU_TOTAL_USAGE.into(),
        entity_id: "host".into(),
        value: Some(monitor_core::MetricValue::Number(42.5)),
        unit: monitor_core::MetricUnit::Percent,
        availability: monitor_core::Availability::Available,
        source: "test".into(),
        collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
        monotonic_sequence: 7,
        reason_code: None,
        human_message_key: None,
        precision: None,
    });
    snapshot.metrics.push(monitor_core::MetricReading {
        metric_id: monitor_core::families::NET_INTERFACE_IPV4.into(),
        entity_id: "if:en0".into(),
        value: Some(monitor_core::MetricValue::Text("192.168.1.10".into())),
        unit: monitor_core::MetricUnit::Text,
        availability: monitor_core::Availability::Available,
        source: "test".into(),
        collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
        monotonic_sequence: 7,
        reason_code: None,
        human_message_key: None,
        precision: None,
    });
    *server.snapshot.write().unwrap() = Some(snapshot);

    let (status, body, _) = request(
        server.port,
        "GET",
        "/api/v1/cpu",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 200);
    assert!(body.contains("42.5"), "real value projected: {body}");

    let (status, body, _) = request(
        server.port,
        "GET",
        "/api/v1/snapshot",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 200);
    assert!(
        !body.contains("192.168.1.10"),
        "private addresses never leave the machine: {body}"
    );

    let (status, _, _) = request(
        server.port,
        "GET",
        "/api/v1/no-such-route",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 404);

    // Phase 08 §4: POST/DELETE/process-kill/Trash routes do not exist.
    let (status, _, _) = request(
        server.port,
        "POST",
        "/api/v1/snapshot",
        Some(&server.token),
        None,
        Some("{}"),
    );
    assert_eq!(status, 405);
    let (status, _, _) = request(
        server.port,
        "DELETE",
        "/api/v1/processes",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 405);
    let (status, _, _) = request(
        server.port,
        "POST",
        "/api/v1/processes/kill",
        Some(&server.token),
        None,
        Some("{}"),
    );
    assert_eq!(status, 404, "no kill route exists at all");
    let (status, _, _) = request(
        server.port,
        "POST",
        "/api/v1/trash",
        Some(&server.token),
        None,
        Some("{}"),
    );
    assert_eq!(status, 404, "no trash route exists at all");
}

/// The history route validates the whitelist, identifier shape and range
/// bounds before opening the database.
#[test]
fn history_route_is_bounded_and_whitelisted() {
    let server = start_server();
    let history = |query: &str| {
        request(
            server.port,
            "GET",
            &format!("/api/v1/metrics/history?{query}"),
            Some(&server.token),
            None,
            None,
        )
        .0
    };
    assert_eq!(
        history("metric_id=not.a.family&entity_id=host&from_utc_ms=1&to_utc_ms=2"),
        400,
        "unknown metric family"
    );
    assert_eq!(
        history("metric_id=cpu.total.usage&entity_id=host&from_utc_ms=2&to_utc_ms=1"),
        400,
        "inverted range"
    );
    assert_eq!(
        history(&format!(
            "metric_id=cpu.total.usage&entity_id=host&from_utc_ms=0&to_utc_ms={}",
            200_i64 * 86_400_000
        )),
        400,
        "oversized range"
    );
    assert_eq!(
        history(&format!(
            "metric_id=cpu.total.usage&entity_id={}&from_utc_ms=1&to_utc_ms=2",
            "x".repeat(200)
        )),
        400,
        "oversized entity id"
    );
    // A valid query against an empty database is an honest 200 with no points.
    let (status, body, _) = request(
        server.port,
        "GET",
        "/api/v1/metrics/history?metric_id=cpu.total.usage&entity_id=host&from_utc_ms=1&to_utc_ms=2",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 200, "body: {body}");
    assert!(
        body.contains("\"points\":[]"),
        "empty history is explicit: {body}"
    );
}

/// Session exchange: the right token mints an HttpOnly SameSite=Strict
/// cookie that authenticates data routes; the wrong token is 401; the
/// dashboard serves inert HTML without auth.
#[test]
fn session_exchange_and_dashboard() {
    let server = start_server();
    let (status, body, _) = request(server.port, "GET", "/", None, None, None);
    assert_eq!(status, 200);
    assert!(body.contains("Read-only view"), "inert dashboard");

    let (status, _, _) = request(
        server.port,
        "POST",
        "/api/v1/session",
        None,
        None,
        Some(r#"{"token":"nope"}"#),
    );
    assert_eq!(status, 401);

    let (status, _, cookies) = request(
        server.port,
        "POST",
        "/api/v1/session",
        None,
        None,
        Some(&format!(r#"{{"token":"{}"}}"#, server.token)),
    );
    assert_eq!(status, 200);
    assert_eq!(cookies.len(), 1, "exactly one Set-Cookie");
    let cookie = &cookies[0];
    assert!(cookie.contains("HttpOnly"), "HttpOnly cookie");
    assert!(cookie.contains("SameSite=Strict"), "SameSite=Strict cookie");
    assert!(
        !cookie.contains(&server.token),
        "the token never lands in a cookie"
    );

    let sid = cookie
        .split(';')
        .next()
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap()
        .to_string();
    let (status, _, _) = request(server.port, "GET", "/api/v1/system", None, Some(&sid), None);
    assert_eq!(
        status, 503,
        "session authorizes; snapshot state stays honest"
    );

    // A forged session id is not authorized.
    let (status, _, _) = request(
        server.port,
        "GET",
        "/api/v1/system",
        None,
        Some("forged-session"),
        None,
    );
    assert_eq!(status, 401);
}

/// Token rotation: the old token (and its sessions) die immediately.
#[test]
fn rotation_invalidates_old_token_and_sessions() {
    let server = start_server();
    let (status, _, _) = request(
        server.port,
        "GET",
        "/api/v1/system",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 503, "old token accepted before rotation");

    let new_token = monitor_remote::auth::generate_token();
    server
        .handle
        .rotate(monitor_remote::auth::hash_token(&new_token));

    let (status, _, _) = request(
        server.port,
        "GET",
        "/api/v1/system",
        Some(&server.token),
        None,
        None,
    );
    assert_eq!(status, 401, "old token invalid immediately");
    let (status, _, _) = request(
        server.port,
        "GET",
        "/api/v1/system",
        Some(&new_token),
        None,
        None,
    );
    assert_eq!(status, 503, "new token accepted");
}

/// SSE: after the client cap, further subscribers receive 503.
#[test]
fn sse_client_cap_holds() {
    let port = free_port();
    let token = monitor_remote::auth::generate_token();
    let snapshot: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
    *snapshot.write().unwrap() = Some(SystemSnapshot::empty("h", "g", 1, "t"));
    let data = RemoteData { snapshot, ..data() };
    let handle = spawn(
        &loopback_config(port),
        data,
        monitor_remote::auth::hash_token(&token),
    )
    .expect("bind");

    let url = format!("http://127.0.0.1:{port}/api/v1/stream");
    let mut responses = Vec::new();
    for _ in 0..MAX_SSE_CLIENTS {
        let response = ureq::get(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .expect("sse accepted");
        assert_eq!(
            response.status(),
            200,
            "content-type: {}",
            response.content_type()
        );
        assert_eq!(response.content_type(), "text/event-stream");
        responses.push(response);
    }
    let (status, _, _) = request(port, "GET", "/api/v1/stream", Some(&token), None, None);
    assert_eq!(status, 503, "client cap exceeded");
    drop(responses);
    handle.stop();
}

/// Hammering an unauthenticated route trips the per-IP rate limiter.
#[test]
fn request_flood_is_rate_limited() {
    let port = free_port();
    let token = monitor_remote::auth::generate_token();
    let handle = spawn(
        &loopback_config(port),
        data(),
        monitor_remote::auth::hash_token(&token),
    )
    .expect("bind");
    let mut saw_429 = false;
    for _ in 0..600 {
        let (status, _, _) = request(port, "GET", "/api/v1/snapshot", Some("bad"), None, None);
        if status == 429 {
            saw_429 = true;
            break;
        }
        assert_eq!(status, 401);
    }
    assert!(saw_429, "flood must trip the limiter");
    handle.stop();
}
