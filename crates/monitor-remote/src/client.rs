//! Console-side agent client (phase 10, ADR-0009 §2/§3/§5).
//!
//! * TLS trust is **pinning** (`PinnedVerifier`): the SHA-256 of the agent's
//!   DER certificate, learned at pairing time. A different certificate
//!   fails the handshake before any HTTP bytes — hence before the bearer
//!   token — leave the console (A3).
//! * Pairing uses an observing verifier that learns the presented
//!   certificate on the first handshake and pins it for the rest of the
//!   exchange, so the proof is bound to exactly the certificate the pairing
//!   request travels over (A2).
//! * Host status is derived from the console's **own** receipt clock; the
//!   remote timestamp is carried along for display but never decides
//!   live/stale (A10).

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::pairing::{self, constant_time_eq};
use crate::protocol::{self, Manifest, PairRequest, PairResponse};
use crate::tls::fingerprint_of;

/// Paired hosts per console — "a few user-owned machines".
pub const MAX_HOSTS: usize = 8;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const BACKOFF_MIN: Duration = Duration::from_secs(2);
pub const BACKOFF_MAX: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("endpoint must be host:port with a port in 1024..=65535")]
    InvalidEndpoint,
    #[error("pairing code is not well-formed")]
    InvalidCode,
    #[error("transport: {0}")]
    Transport(String),
    #[error("certificate does not match the pinned fingerprint")]
    CertMismatch { presented: String },
    #[error("agent refused the credential")]
    Unauthorized,
    #[error("agent is rate limiting this console")]
    RateLimited,
    #[error("agent speaks protocol {remote}, this console speaks {local}")]
    Incompatible { remote: u32, local: u32 },
    #[error("agent has no sample yet")]
    NoSampleYet,
    #[error("agent refused pairing: {0}")]
    PairingRefused(String),
    #[error("agent could not prove it knows the pairing code")]
    AgentProofInvalid,
    #[error("unexpected agent response: {0}")]
    Protocol(String),
    #[error("agent answered HTTP {0}")]
    Status(u16),
}

/// `host:port`, `[v6]:port` or `name.local:port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEndpoint {
    pub host: String,
    pub port: u16,
}

impl AgentEndpoint {
    pub fn parse(input: &str) -> Result<Self, ClientError> {
        let input = input.trim();
        let (host, port) = input.rsplit_once(':').ok_or(ClientError::InvalidEndpoint)?;
        let port: u16 = port.parse().map_err(|_| ClientError::InvalidEndpoint)?;
        if !(crate::PORT_MIN..=crate::PORT_MAX).contains(&port) {
            return Err(ClientError::InvalidEndpoint);
        }
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let valid_host = !host.is_empty()
            && host.len() <= 253
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
        if !valid_host {
            return Err(ClientError::InvalidEndpoint);
        }
        Ok(Self {
            host: host.to_string(),
            port,
        })
    }

    pub fn base_url(&self) -> String {
        if self.host.parse::<IpAddr>().is_ok_and(|ip| ip.is_ipv6()) {
            format!("https://[{}]:{}", self.host, self.port)
        } else {
            format!("https://{}:{}", self.host, self.port)
        }
    }

    pub fn display(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn algorithms() -> WebPkiSupportedAlgorithms {
    rustls::crypto::ring::default_provider().signature_verification_algorithms
}

/// Pins one certificate fingerprint. Records the presented fingerprint on
/// a mismatch so the caller can report `cert_mismatch` precisely.
#[derive(Debug)]
pub struct PinnedVerifier {
    expected: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
    mismatch: Mutex<Option<String>>,
}

impl PinnedVerifier {
    pub fn new(expected: [u8; 32]) -> Self {
        Self {
            expected,
            algorithms: algorithms(),
            mismatch: Mutex::new(None),
        }
    }

    pub fn take_mismatch(&self) -> Option<String> {
        self.mismatch
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = fingerprint_of(end_entity.as_ref());
        if constant_time_eq(&presented, &self.expected) {
            Ok(ServerCertVerified::assertion())
        } else {
            *self.mismatch.lock().unwrap_or_else(|p| p.into_inner()) =
                Some(pairing::hex(&presented));
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// Trust-on-first-handshake for the pairing exchange only: learns the
/// certificate once, then behaves like a pin for every later connection
/// of the same agent (a swapped certificate mid-exchange is refused).
#[derive(Debug)]
pub struct ObservingVerifier {
    observed: Mutex<Option<[u8; 32]>>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl Default for ObservingVerifier {
    fn default() -> Self {
        Self {
            observed: Mutex::new(None),
            algorithms: algorithms(),
        }
    }
}

impl ObservingVerifier {
    pub fn observed_hex(&self) -> Option<String> {
        self.observed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .map(|fp| pairing::hex(&fp))
    }
}

impl ServerCertVerifier for ObservingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = fingerprint_of(end_entity.as_ref());
        let mut observed = self.observed.lock().unwrap_or_else(|p| p.into_inner());
        match *observed {
            None => {
                *observed = Some(presented);
                Ok(ServerCertVerified::assertion())
            }
            Some(first) if constant_time_eq(&first, &presented) => {
                Ok(ServerCertVerified::assertion())
            }
            Some(_) => Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            )),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

fn build_agent(verifier: Arc<dyn ServerCertVerifier>) -> ureq::Agent {
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    ureq::AgentBuilder::new()
        .tls_config(Arc::new(config))
        .timeout_connect(REQUEST_TIMEOUT)
        .timeout_read(REQUEST_TIMEOUT)
        .timeout_write(REQUEST_TIMEOUT)
        .build()
}

fn error_code(response: ureq::Response) -> String {
    response
        .into_json::<serde_json::Value>()
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_default()
}

fn map_status(code: u16, response: ureq::Response) -> ClientError {
    match code {
        401 => ClientError::Unauthorized,
        429 => ClientError::RateLimited,
        409 => {
            let body: serde_json::Value = response.into_json().unwrap_or_default();
            match body.get("error").and_then(|e| e.as_str()) {
                Some("incompatible") => ClientError::Incompatible {
                    remote: body
                        .get("protocol_version")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    local: protocol::PROTOCOL_VERSION,
                },
                Some(other) => ClientError::PairingRefused(other.to_string()),
                None => ClientError::Status(code),
            }
        }
        503 => match error_code(response).as_str() {
            "no_sample_yet" => ClientError::NoSampleYet,
            _ => ClientError::Status(code),
        },
        403 => ClientError::PairingRefused(error_code(response)),
        other => ClientError::Status(other),
    }
}

/// Result of a successful pairing: everything the console persists (the
/// token goes to the restricted credential store, the rest to settings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairOutcome {
    pub host_id: String,
    pub display_name: String,
    pub protocol_version: u32,
    pub fingerprint_hex: String,
    pub device_token: String,
}

/// Observe the certificate an endpoint presents (first handshake, no
/// credential sent). Returns the agent handle used for the observation so
/// the caller keeps talking to the **same** pinned certificate.
fn observe(endpoint: &AgentEndpoint) -> Result<(ureq::Agent, String), ClientError> {
    let observer = Arc::new(ObservingVerifier::default());
    let agent = build_agent(Arc::clone(&observer) as Arc<dyn ServerCertVerifier>);
    // The dashboard route is public and data-free, so it serves purely to
    // observe the certificate.
    agent
        .get(&format!("{}/", endpoint.base_url()))
        .call()
        .map_err(|err| map_transport(err, None))?;
    let fingerprint_hex = observer
        .observed_hex()
        .ok_or_else(|| ClientError::Transport("no certificate observed".into()))?;
    Ok((agent, fingerprint_hex))
}

/// Fingerprint an endpoint presents right now (shown to the user before or
/// during pairing; never trusted on its own).
pub fn probe_fingerprint(endpoint: &AgentEndpoint) -> Result<String, ClientError> {
    observe(endpoint).map(|(_, fingerprint)| fingerprint)
}

/// Send an arbitrary pairing request and return the raw status + body.
/// Test seam for the negative paths (wrong proof, wrong version); `pair`
/// is the production path.
pub fn send_pair_request(
    endpoint: &AgentEndpoint,
    request: &PairRequest,
) -> Result<(u16, serde_json::Value), ClientError> {
    let (agent, _) = observe(endpoint)?;
    match agent
        .post(&format!("{}/api/v1/pair", endpoint.base_url()))
        .send_json(request)
    {
        Ok(response) => {
            let status = response.status();
            Ok((status, response.into_json().unwrap_or_default()))
        }
        Err(ureq::Error::Status(status, response)) => {
            Ok((status, response.into_json().unwrap_or_default()))
        }
        Err(other) => Err(map_transport(other, None)),
    }
}

/// Run the pairing exchange against `endpoint` with the code shown on the
/// agent. Never stores anything; the caller decides what to persist.
pub fn pair(
    endpoint: &AgentEndpoint,
    code: &str,
    device_id: &str,
    device_label: &str,
) -> Result<PairOutcome, ClientError> {
    let code = pairing::normalize_code(code).ok_or(ClientError::InvalidCode)?;
    if !pairing::identifier_is_valid(device_id, pairing::MAX_DEVICE_ID_LEN)
        || !pairing::label_is_valid(device_label)
    {
        return Err(ClientError::Protocol("device identity".into()));
    }
    let (agent, fingerprint_hex) = observe(endpoint)?;
    let base = endpoint.base_url();
    let nonce = pairing::generate_nonce();
    let request = PairRequest {
        protocol_version: protocol::PROTOCOL_VERSION,
        device_id: device_id.to_string(),
        device_label: device_label.trim().to_string(),
        nonce: nonce.clone(),
        proof: pairing::proof(&code, device_id, &nonce, &fingerprint_hex),
    };
    let response = agent
        .post(&format!("{base}/api/v1/pair"))
        .send_json(&request)
        .map_err(|err| map_transport(err, None))?;
    let parsed: PairResponse = response
        .into_json()
        .map_err(|err| ClientError::Protocol(err.to_string()))?;
    let expected_ack = pairing::ack(&code, device_id, &nonce, &fingerprint_hex);
    if !constant_time_eq(expected_ack.as_bytes(), parsed.ack.as_bytes()) {
        return Err(ClientError::AgentProofInvalid);
    }
    if !protocol::is_compatible(parsed.protocol_version) {
        return Err(ClientError::Incompatible {
            remote: parsed.protocol_version,
            local: protocol::PROTOCOL_VERSION,
        });
    }
    if !pairing::identifier_is_valid(&parsed.host_id, 64) {
        return Err(ClientError::Protocol("host_id".into()));
    }
    Ok(PairOutcome {
        host_id: parsed.host_id,
        display_name: if protocol::display_name_is_valid(&parsed.display_name) {
            parsed.display_name
        } else {
            protocol::DEFAULT_DISPLAY_NAME.to_string()
        },
        protocol_version: parsed.protocol_version,
        fingerprint_hex,
        device_token: parsed.device_token,
    })
}

fn map_transport(err: ureq::Error, pinned: Option<&PinnedVerifier>) -> ClientError {
    match err {
        ureq::Error::Status(code, response) => map_status(code, response),
        ureq::Error::Transport(transport) => {
            if let Some(presented) = pinned.and_then(PinnedVerifier::take_mismatch) {
                return ClientError::CertMismatch { presented };
            }
            ClientError::Transport(transport.to_string())
        }
    }
}

/// A numeric reading as received from an agent (owned mirror of
/// `projection::RemoteReading`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteReading {
    pub metric_id: String,
    pub entity_id: String,
    pub value: f64,
    pub unit: String,
    pub collected_at_utc: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteSnapshot {
    pub schema_version: u8,
    pub sequence: u64,
    pub collected_at_utc: String,
    pub readings: Vec<RemoteReading>,
}

/// Authenticated, certificate-pinned client for one paired agent.
pub struct AgentClient {
    agent: ureq::Agent,
    base: String,
    token: String,
    verifier: Arc<PinnedVerifier>,
}

impl std::fmt::Debug for AgentClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentClient")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

pub fn parse_fingerprint(hex: &str) -> Option<[u8; 32]> {
    if !pairing::fingerprint_hex_is_valid(hex) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

impl AgentClient {
    pub fn new(
        endpoint: &AgentEndpoint,
        fingerprint_hex: &str,
        token: String,
    ) -> Result<Self, ClientError> {
        let expected = parse_fingerprint(fingerprint_hex)
            .ok_or_else(|| ClientError::Protocol("fingerprint".into()))?;
        let verifier = Arc::new(PinnedVerifier::new(expected));
        let agent = build_agent(Arc::clone(&verifier) as Arc<dyn ServerCertVerifier>);
        Ok(Self {
            agent,
            base: endpoint.base_url(),
            token,
            verifier,
        })
    }

    fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let _ = self.verifier.take_mismatch();
        let response = self
            .agent
            .get(&format!("{}{}", self.base, path))
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map_err(|err| map_transport(err, Some(&self.verifier)))?;
        response
            .into_json::<T>()
            .map_err(|err| ClientError::Protocol(err.to_string()))
    }

    pub fn manifest(&self) -> Result<Manifest, ClientError> {
        let manifest: Manifest = self.get_json("/api/v1/manifest")?;
        if !protocol::is_compatible(manifest.protocol_version) {
            return Err(ClientError::Incompatible {
                remote: manifest.protocol_version,
                local: protocol::PROTOCOL_VERSION,
            });
        }
        Ok(manifest)
    }

    pub fn snapshot(&self) -> Result<RemoteSnapshot, ClientError> {
        self.get_json("/api/v1/snapshot")
    }
}

// ---- host liveness (pure, clock-injected) -----------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostStatus {
    NeverConnected,
    Online,
    Stale,
    Offline,
    Unauthorized,
    CertMismatch,
    Incompatible,
}

/// Live within two poll intervals (one missed poll tolerated), stale up to
/// six, offline beyond. Credential, certificate and protocol failures are
/// terminal until the user acts, whatever the last success.
pub fn derive_status(
    now: Instant,
    poll_interval: Duration,
    last_success: Option<Instant>,
    last_error: Option<&ClientError>,
) -> HostStatus {
    match last_error {
        Some(ClientError::Unauthorized) => return HostStatus::Unauthorized,
        Some(ClientError::CertMismatch { .. }) => return HostStatus::CertMismatch,
        Some(ClientError::Incompatible { .. }) => return HostStatus::Incompatible,
        _ => {}
    }
    match last_success {
        None if last_error.is_some() => HostStatus::Offline,
        None => HostStatus::NeverConnected,
        Some(at) => {
            let age = now.saturating_duration_since(at);
            if age <= poll_interval * 2 {
                HostStatus::Online
            } else if age <= poll_interval * 6 {
                HostStatus::Stale
            } else {
                HostStatus::Offline
            }
        }
    }
}

pub fn backoff_for(consecutive_failures: u32) -> Duration {
    let factor = 1u64 << consecutive_failures.min(5);
    (BACKOFF_MIN * factor as u32).min(BACKOFF_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parsing_accepts_v4_v6_names_and_rejects_bad_ports() {
        let v4 = AgentEndpoint::parse("192.168.1.5:45454").unwrap();
        assert_eq!(v4.base_url(), "https://192.168.1.5:45454");
        let v6 = AgentEndpoint::parse("[fe80::1]:45454").unwrap();
        assert_eq!(v6.base_url(), "https://[fe80::1]:45454");
        let name = AgentEndpoint::parse(" office-mac.local:2000 ").unwrap();
        assert_eq!(name.display(), "office-mac.local:2000");
        assert_eq!(
            AgentEndpoint::parse("host:80"),
            Err(ClientError::InvalidEndpoint)
        );
        assert_eq!(
            AgentEndpoint::parse("no-port"),
            Err(ClientError::InvalidEndpoint)
        );
        assert_eq!(
            AgentEndpoint::parse("bad host:45454"),
            Err(ClientError::InvalidEndpoint)
        );
    }

    #[test]
    fn fingerprint_parsing_round_trips() {
        let bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        let hex = pairing::hex(&bytes);
        assert_eq!(parse_fingerprint(&hex), Some(bytes));
        assert_eq!(parse_fingerprint("zz"), None);
    }

    /// ADR-0009 A10: liveness is a function of the console's receipt clock.
    #[test]
    fn status_is_derived_from_local_receipt_time() {
        let now = Instant::now();
        let interval = Duration::from_secs(5);
        assert_eq!(
            derive_status(now, interval, None, None),
            HostStatus::NeverConnected
        );
        assert_eq!(
            derive_status(now, interval, Some(now - Duration::from_secs(3)), None),
            HostStatus::Online
        );
        assert_eq!(
            derive_status(now, interval, Some(now - Duration::from_secs(20)), None),
            HostStatus::Stale
        );
        assert_eq!(
            derive_status(now, interval, Some(now - Duration::from_secs(61)), None),
            HostStatus::Offline
        );
        // A transport error with no success yet: offline, not "never".
        assert_eq!(
            derive_status(
                now,
                interval,
                None,
                Some(&ClientError::Transport("x".into()))
            ),
            HostStatus::Offline
        );
        // Terminal states win even over a fresh success.
        assert_eq!(
            derive_status(now, interval, Some(now), Some(&ClientError::Unauthorized)),
            HostStatus::Unauthorized
        );
        assert_eq!(
            derive_status(
                now,
                interval,
                Some(now),
                Some(&ClientError::CertMismatch {
                    presented: "ab".into()
                })
            ),
            HostStatus::CertMismatch
        );
        assert_eq!(
            derive_status(
                now,
                interval,
                Some(now),
                Some(&ClientError::Incompatible {
                    remote: 2,
                    local: 1
                })
            ),
            HostStatus::Incompatible
        );
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_for(0), Duration::from_secs(2));
        assert_eq!(backoff_for(1), Duration::from_secs(4));
        assert_eq!(backoff_for(3), Duration::from_secs(16));
        assert_eq!(backoff_for(10), BACKOFF_MAX);
    }

    #[test]
    fn pinned_verifier_accepts_match_and_records_mismatch() {
        let identity = crate::TlsIdentity::generate().unwrap();
        let other = crate::TlsIdentity::generate().unwrap();
        let verifier = PinnedVerifier::new(identity.fingerprint());
        let name = ServerName::try_from("localhost").unwrap();
        let ok = verifier.verify_server_cert(
            &CertificateDer::from(identity.cert_der().to_vec()),
            &[],
            &name,
            &[],
            UnixTime::now(),
        );
        assert!(ok.is_ok());
        assert!(verifier.take_mismatch().is_none());
        let bad = verifier.verify_server_cert(
            &CertificateDer::from(other.cert_der().to_vec()),
            &[],
            &name,
            &[],
            UnixTime::now(),
        );
        assert!(bad.is_err());
        assert_eq!(verifier.take_mismatch(), Some(other.fingerprint_hex()));
    }

    #[test]
    fn observing_verifier_pins_after_first_sight() {
        let first = crate::TlsIdentity::generate().unwrap();
        let second = crate::TlsIdentity::generate().unwrap();
        let verifier = ObservingVerifier::default();
        let name = ServerName::try_from("localhost").unwrap();
        let cert = |id: &crate::TlsIdentity| CertificateDer::from(id.cert_der().to_vec());
        assert!(verifier
            .verify_server_cert(&cert(&first), &[], &name, &[], UnixTime::now())
            .is_ok());
        assert_eq!(verifier.observed_hex(), Some(first.fingerprint_hex()));
        assert!(verifier
            .verify_server_cert(&cert(&first), &[], &name, &[], UnixTime::now())
            .is_ok());
        assert!(verifier
            .verify_server_cert(&cert(&second), &[], &name, &[], UnixTime::now())
            .is_err());
    }
}
