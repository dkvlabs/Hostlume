//! Multi-host integration tests (phase 10, ADR-0009): TLS, pairing,
//! revocation and MITM over a real loopback HTTPS agent, plus the lifecycle
//! cases the phase names — reconnect, revoked key, clock skew, duplicate
//! device, rename, compromised LAN device, network partition.

use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use monitor_core::{
    Availability, CapabilityReport, MetricCapability, MetricReading, MetricUnit, MetricValue,
    SystemSnapshot,
};
use monitor_remote::client::{self, AgentClient, AgentEndpoint, ClientError, HostStatus};
use monitor_remote::protocol::{self, PairRequest, PairedDevice};
use monitor_remote::{
    pairing, spawn, spawn_with, DeviceCredential, RemoteConfig, RemoteData, RemoteMode,
    RemoteServerHandle, SpawnOptions, TlsIdentity,
};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn endpoint(port: u16) -> AgentEndpoint {
    AgentEndpoint::parse(&format!("127.0.0.1:{port}")).unwrap()
}

fn fixture_snapshot(collected_at_utc: &str) -> SystemSnapshot {
    let mut snapshot = SystemSnapshot::empty("host-uuid", "gen", 9, collected_at_utc);
    snapshot.metrics.push(MetricReading {
        metric_id: "cpu.total.usage".into(),
        entity_id: "host".into(),
        value: Some(MetricValue::Number(42.5)),
        unit: MetricUnit::Percent,
        availability: Availability::Available,
        source: "test".into(),
        collected_at_utc: collected_at_utc.into(),
        monotonic_sequence: 9,
        reason_code: None,
        human_message_key: None,
        precision: None,
    });
    snapshot
}

struct Agent {
    handle: RemoteServerHandle,
    port: u16,
    snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    paired: Arc<Mutex<Vec<PairedDevice>>>,
}

impl Agent {
    fn endpoint(&self) -> AgentEndpoint {
        endpoint(self.port)
    }

    /// Stop and wait until the port actually refuses connections.
    fn stop(self) {
        let port = self.port;
        self.handle.stop();
        drop(self.handle);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("port {port} still accepting after stop");
    }
}

fn tls_agent(
    identity: Arc<TlsIdentity>,
    port: u16,
    devices: Vec<DeviceCredential>,
    display_name: &str,
) -> Agent {
    let snapshot: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
    let paired: Arc<Mutex<Vec<PairedDevice>>> = Arc::default();
    let sink = Arc::clone(&paired);
    let mut capabilities = CapabilityReport::new("host-uuid", "t");
    let mut cap = MetricCapability::not_implemented_yet("cpu.total.usage", "test");
    cap.state = Availability::Available;
    capabilities.capabilities.push(cap);
    let data = RemoteData {
        host_id: "host-uuid".into(),
        cadence_ms: 3_000,
        snapshot: Arc::clone(&snapshot),
        capabilities: Arc::new(Mutex::new(capabilities)),
        history_db_path: std::env::temp_dir(),
        process_rows: None,
        on_device_paired: Some(Arc::new(move |device| {
            sink.lock().unwrap().push(device);
        })),
    };
    let handle = spawn_with(
        &RemoteConfig {
            mode: RemoteMode::Lan,
            port,
        },
        data,
        monitor_remote::auth::hash_token("owner-token"),
        SpawnOptions {
            tls: Some(Arc::clone(&identity)),
            bind_addr: Some(IpAddr::from([127, 0, 0, 1])),
            devices,
            display_name: Some(display_name.into()),
        },
    )
    .expect("lan+tls agent binds on loopback");
    Agent {
        handle,
        port,
        snapshot,
        paired,
    }
}

fn pair_ok(agent: &Agent, device_id: &str) -> client::PairOutcome {
    let window = agent.handle.open_pairing_window().expect("window");
    client::pair(&agent.endpoint(), &window.code, device_id, "Console").expect("pairing succeeds")
}

/// Exit-gate core: pair over TLS, persist the hash, read the manifest and a
/// snapshot with the device token; the console pinned exactly the agent's
/// certificate.
#[test]
fn pair_over_tls_end_to_end_then_read_manifest_and_snapshot() {
    let identity = Arc::new(TlsIdentity::generate().unwrap());
    let agent = tls_agent(Arc::clone(&identity), free_port(), vec![], "Office Mac");
    let before = agent.handle.pairing_window();
    assert!(before.is_none(), "no window until the user opens one");

    let outcome = pair_ok(&agent, "console-1");
    assert_eq!(outcome.fingerprint_hex, identity.fingerprint_hex());
    assert_eq!(outcome.host_id, "host-uuid");
    assert_eq!(outcome.display_name, "Office Mac");
    assert_eq!(outcome.protocol_version, protocol::PROTOCOL_VERSION);
    assert!(
        agent.handle.pairing_window().is_none(),
        "a successful pairing consumes the window"
    );
    {
        let paired = agent.paired.lock().unwrap();
        assert_eq!(paired.len(), 1);
        assert_eq!(paired[0].device_id, "console-1");
        assert_eq!(paired[0].label, "Console");
        assert_eq!(
            paired[0].token_hash,
            monitor_remote::auth::hash_token(&outcome.device_token),
            "the agent persists the hash, never the token"
        );
    }

    let client = AgentClient::new(
        &agent.endpoint(),
        &outcome.fingerprint_hex,
        outcome.device_token.clone(),
    )
    .unwrap();
    let manifest = client.manifest().unwrap();
    assert_eq!(manifest.protocol_version, protocol::PROTOCOL_VERSION);
    assert_eq!(manifest.display_name, "Office Mac");
    assert_eq!(manifest.host_id, "host-uuid");
    assert_eq!(manifest.families, vec!["cpu".to_string()]);
    assert_eq!(
        manifest.tls_fingerprint_sha256.as_deref(),
        Some(identity.fingerprint_hex().as_str())
    );

    assert_eq!(client.snapshot(), Err(ClientError::NoSampleYet));
    *agent.snapshot.write().unwrap() = Some(fixture_snapshot("2026-10-19T00:00:00.000Z"));
    let snapshot = client.snapshot().unwrap();
    assert_eq!(snapshot.readings.len(), 1);
    assert_eq!(snapshot.readings[0].metric_id, "cpu.total.usage");
    assert_eq!(snapshot.readings[0].value, 42.5);
    assert_eq!(snapshot.readings[0].unit, "percent");
    assert!(agent
        .handle
        .device_last_seen_secs()
        .iter()
        .any(|(id, _)| id == "console-1"));
    assert!(agent.handle.requests_seen() >= 3);
}

/// ADR-0009 A4: a wrong code burns the window — the correct code no longer
/// works until the agent user opens a new one.
#[test]
fn wrong_code_burns_the_window() {
    let agent = tls_agent(
        Arc::new(TlsIdentity::generate().unwrap()),
        free_port(),
        vec![],
        "Office Mac",
    );
    let window = agent.handle.open_pairing_window().unwrap();
    let wrong = if window.code == "AAAAAAAA" {
        "BBBBBBBB"
    } else {
        "AAAAAAAA"
    };
    assert_eq!(
        client::pair(&agent.endpoint(), wrong, "console-1", "Console"),
        Err(ClientError::PairingRefused("invalid_proof".into()))
    );
    assert_eq!(
        client::pair(&agent.endpoint(), &window.code, "console-1", "Console"),
        Err(ClientError::PairingRefused("no_pairing_window".into()))
    );
    assert!(agent.paired.lock().unwrap().is_empty());
    assert_eq!(
        client::pair(&agent.endpoint(), "not a code", "console-1", "Console"),
        Err(ClientError::InvalidCode)
    );
}

/// ADR-0009 A2: a proof bound to any fingerprint other than the agent's
/// own (what a relaying MITM would have to send) is refused and burns the
/// code, so the attacker cannot replay the real console's attempt either.
#[test]
fn mitm_at_pairing_fails_and_burns_code() {
    let agent = tls_agent(
        Arc::new(TlsIdentity::generate().unwrap()),
        free_port(),
        vec![],
        "Office Mac",
    );
    let window = agent.handle.open_pairing_window().unwrap();
    let mitm_fingerprint = TlsIdentity::generate().unwrap().fingerprint_hex();
    let nonce = pairing::generate_nonce();
    let request = PairRequest {
        protocol_version: protocol::PROTOCOL_VERSION,
        device_id: "console-1".into(),
        device_label: "Console".into(),
        nonce: nonce.clone(),
        proof: pairing::proof(&window.code, "console-1", &nonce, &mitm_fingerprint),
    };
    let (status, body) = client::send_pair_request(&agent.endpoint(), &request).unwrap();
    assert_eq!(status, 403);
    assert_eq!(body["error"], "invalid_proof");
    // The real console, with the right code, now gets nothing.
    assert_eq!(
        client::pair(&agent.endpoint(), &window.code, "console-1", "Console"),
        Err(ClientError::PairingRefused("no_pairing_window".into()))
    );
    assert!(agent.paired.lock().unwrap().is_empty());
}

/// Cheap rejections (wrong protocol version, malformed identity) must not
/// burn the window: a probe cannot deny the real console its attempt.
#[test]
fn version_and_shape_rejections_do_not_burn_the_window() {
    let agent = tls_agent(
        Arc::new(TlsIdentity::generate().unwrap()),
        free_port(),
        vec![],
        "Office Mac",
    );
    let window = agent.handle.open_pairing_window().unwrap();
    let nonce = pairing::generate_nonce();
    let mut request = PairRequest {
        protocol_version: protocol::PROTOCOL_VERSION + 1,
        device_id: "console-1".into(),
        device_label: "Console".into(),
        nonce: nonce.clone(),
        proof: "00".repeat(32),
    };
    let (status, body) = client::send_pair_request(&agent.endpoint(), &request).unwrap();
    assert_eq!(status, 409);
    assert_eq!(body["error"], "incompatible");
    assert_eq!(body["protocol_version"], protocol::PROTOCOL_VERSION);

    request.protocol_version = protocol::PROTOCOL_VERSION;
    request.device_id = "bad id!".into();
    let (status, body) = client::send_pair_request(&agent.endpoint(), &request).unwrap();
    assert_eq!(status, 400);
    assert_eq!(body["error"], "malformed");

    assert!(
        agent.handle.pairing_window().is_some(),
        "window survives probes"
    );
    let outcome = client::pair(&agent.endpoint(), &window.code, "console-1", "Console");
    assert!(outcome.is_ok());
}

/// Revoked key: the device token stops working on the very next request
/// and the derived status is the terminal `unauthorized`.
#[test]
fn revoked_device_is_unauthorized_immediately() {
    let agent = tls_agent(
        Arc::new(TlsIdentity::generate().unwrap()),
        free_port(),
        vec![],
        "Office Mac",
    );
    *agent.snapshot.write().unwrap() = Some(fixture_snapshot("2026-10-19T00:00:00.000Z"));
    let outcome = pair_ok(&agent, "console-1");
    let client = AgentClient::new(
        &agent.endpoint(),
        &outcome.fingerprint_hex,
        outcome.device_token,
    )
    .unwrap();
    assert!(client.snapshot().is_ok());

    agent.handle.remove_device("console-1");
    let err = client.snapshot().unwrap_err();
    assert_eq!(err, ClientError::Unauthorized);
    assert_eq!(
        client::derive_status(
            Instant::now(),
            Duration::from_secs(5),
            Some(Instant::now()),
            Some(&err)
        ),
        HostStatus::Unauthorized
    );
    assert!(agent
        .handle
        .device_last_seen_secs()
        .iter()
        .all(|(id, _)| id != "console-1"));
}

/// ADR-0009 A3: a pinned client pointed at a different (compromised or
/// impostor) device fails the handshake — that server never sees an HTTP
/// request, so the token never leaves the console.
#[test]
fn compromised_lan_device_never_receives_the_token() {
    let real_identity = Arc::new(TlsIdentity::generate().unwrap());
    let real = tls_agent(
        Arc::clone(&real_identity),
        free_port(),
        vec![],
        "Office Mac",
    );
    let outcome = pair_ok(&real, "console-1");

    let impostor_identity = Arc::new(TlsIdentity::generate().unwrap());
    let impostor = tls_agent(
        Arc::clone(&impostor_identity),
        free_port(),
        vec![],
        "Office Mac",
    );
    let client = AgentClient::new(
        &impostor.endpoint(),
        &outcome.fingerprint_hex,
        outcome.device_token,
    )
    .unwrap();
    let err = client.snapshot().unwrap_err();
    assert_eq!(
        err,
        ClientError::CertMismatch {
            presented: impostor_identity.fingerprint_hex()
        }
    );
    assert_eq!(
        impostor.handle.requests_seen(),
        0,
        "the handshake failed before any request reached the impostor"
    );
    assert!(impostor.handle.device_last_seen_secs().is_empty());
    assert_eq!(
        client::derive_status(Instant::now(), Duration::from_secs(5), None, Some(&err)),
        HostStatus::CertMismatch
    );
}

/// Reconnect: the agent goes away and comes back on the same port with the
/// same identity and persisted devices; the pinned client recovers without
/// re-pairing.
#[test]
fn reconnect_after_agent_restart() {
    let identity = Arc::new(TlsIdentity::generate().unwrap());
    let port = free_port();
    let agent = tls_agent(Arc::clone(&identity), port, vec![], "Office Mac");
    *agent.snapshot.write().unwrap() = Some(fixture_snapshot("2026-10-19T00:00:00.000Z"));
    let outcome = pair_ok(&agent, "console-1");
    let persisted: Vec<DeviceCredential> = agent
        .paired
        .lock()
        .unwrap()
        .iter()
        .map(|d| DeviceCredential {
            device_id: d.device_id.clone(),
            token_hash: d.token_hash.clone(),
        })
        .collect();
    let client = AgentClient::new(
        &agent.endpoint(),
        &outcome.fingerprint_hex,
        outcome.device_token,
    )
    .unwrap();
    assert!(client.snapshot().is_ok());

    agent.stop();
    let err = client.snapshot().unwrap_err();
    assert!(matches!(err, ClientError::Transport(_)), "{err:?}");
    let last_success = Instant::now() - Duration::from_secs(120);
    assert_eq!(
        client::derive_status(
            Instant::now(),
            Duration::from_secs(5),
            Some(last_success),
            Some(&err)
        ),
        HostStatus::Offline
    );

    let restarted = tls_agent(Arc::clone(&identity), port, persisted, "Office Mac");
    *restarted.snapshot.write().unwrap() = Some(fixture_snapshot("2026-10-19T00:01:00.000Z"));
    let snapshot = client
        .snapshot()
        .expect("same identity + devices → recovers");
    assert_eq!(snapshot.collected_at_utc, "2026-10-19T00:01:00.000Z");
}

/// Clock skew: a remote timestamp far in the future (or past) is carried
/// verbatim and never decides liveness — the console's receipt clock does.
#[test]
fn clock_skew_does_not_affect_liveness() {
    let agent = tls_agent(
        Arc::new(TlsIdentity::generate().unwrap()),
        free_port(),
        vec![],
        "Office Mac",
    );
    *agent.snapshot.write().unwrap() = Some(fixture_snapshot("2099-01-01T00:00:00.000Z"));
    let outcome = pair_ok(&agent, "console-1");
    let client = AgentClient::new(
        &agent.endpoint(),
        &outcome.fingerprint_hex,
        outcome.device_token,
    )
    .unwrap();
    let snapshot = client.snapshot().unwrap();
    assert_eq!(snapshot.collected_at_utc, "2099-01-01T00:00:00.000Z");
    let received = Instant::now();
    assert_eq!(
        client::derive_status(received, Duration::from_secs(5), Some(received), None),
        HostStatus::Online
    );
}

/// Duplicate device id re-pairs (one credential per device, the old token
/// dies); renaming the agent changes the manifest's display name while the
/// host identity stays put.
#[test]
fn duplicate_device_replaces_credential_and_rename_keeps_identity() {
    let agent = tls_agent(
        Arc::new(TlsIdentity::generate().unwrap()),
        free_port(),
        vec![],
        "Office Mac",
    );
    *agent.snapshot.write().unwrap() = Some(fixture_snapshot("2026-10-19T00:00:00.000Z"));
    let first = pair_ok(&agent, "console-1");
    let second = pair_ok(&agent, "console-1");
    assert_ne!(first.device_token, second.device_token);
    assert_eq!(agent.handle.shared.device_count(), 1);

    let stale = AgentClient::new(
        &agent.endpoint(),
        &first.fingerprint_hex,
        first.device_token,
    )
    .unwrap();
    assert_eq!(stale.snapshot(), Err(ClientError::Unauthorized));
    let fresh = AgentClient::new(
        &agent.endpoint(),
        &second.fingerprint_hex,
        second.device_token,
    )
    .unwrap();
    assert!(fresh.snapshot().is_ok());

    agent.handle.set_display_name("Renamed Mac".into());
    let manifest = fresh.manifest().unwrap();
    assert_eq!(manifest.display_name, "Renamed Mac");
    assert_eq!(manifest.host_id, "host-uuid");
}

/// Network partition: nothing listening → transport error, offline, and no
/// panic in the client.
#[test]
fn network_partition_is_offline_not_a_crash() {
    let fingerprint = TlsIdentity::generate().unwrap().fingerprint_hex();
    let client = AgentClient::new(&endpoint(free_port()), &fingerprint, "token".into()).unwrap();
    let err = client.snapshot().unwrap_err();
    assert!(matches!(err, ClientError::Transport(_)), "{err:?}");
    assert_eq!(
        client::derive_status(Instant::now(), Duration::from_secs(5), None, Some(&err)),
        HostStatus::Offline
    );
}

/// Loopback (plain HTTP) has no certificate to bind a proof to: pairing is
/// explicitly unavailable there rather than silently weaker.
#[test]
fn pairing_is_unavailable_on_plain_loopback() {
    let port = free_port();
    let data = RemoteData {
        host_id: "host-uuid".into(),
        cadence_ms: 3_000,
        snapshot: Arc::default(),
        capabilities: Arc::new(Mutex::new(CapabilityReport::new("host-uuid", "t"))),
        history_db_path: std::env::temp_dir(),
        process_rows: None,
        on_device_paired: None,
    };
    let handle = spawn(
        &RemoteConfig {
            mode: RemoteMode::Loopback,
            port,
        },
        data,
        monitor_remote::auth::hash_token("owner-token"),
    )
    .unwrap();
    let request = PairRequest {
        protocol_version: protocol::PROTOCOL_VERSION,
        device_id: "console-1".into(),
        device_label: "Console".into(),
        nonce: pairing::generate_nonce(),
        proof: "00".repeat(32),
    };
    let result = ureq::post(&format!("http://127.0.0.1:{port}/api/v1/pair")).send_json(&request);
    let (status, body) = match result {
        Err(ureq::Error::Status(status, response)) => {
            (status, response.into_json::<serde_json::Value>().unwrap())
        }
        other => panic!("expected a 409, got {other:?}"),
    };
    assert_eq!(status, 409);
    assert_eq!(body["error"], "pairing_unavailable");
    handle.stop();
}
