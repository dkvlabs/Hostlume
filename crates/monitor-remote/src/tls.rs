//! TLS for LAN mode (phase 10, ADR-0009 §2).
//!
//! * The agent identity is a self-signed ECDSA P-256 certificate generated
//!   with `rcgen` at first LAN enable and persisted as DER; the private key
//!   file is created with mode 0600 on unix.
//! * Trust is **pinning**: consoles pin the SHA-256 fingerprint of the DER
//!   certificate at pairing time (ADR-0009 A2/A3). Validity dates and
//!   subject names are therefore not part of the trust decision.
//! * [`TlsListener`] adapts an accepted TCP stream into a TLS stream for
//!   `axum::serve`, handshaking off the accept loop with a timeout and a
//!   concurrency cap so a slow or hostile peer cannot stall new connections.

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

pub const CERT_FILE: &str = "cert.der";
pub const KEY_FILE: &str = "key.der";
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_CONCURRENT_HANDSHAKES: usize = 32;
const SAN_NAME: &str = "hostlume-agent";

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("certificate generation failed: {0}")]
    Generate(String),
    #[error("tls identity file error: {0}")]
    Io(#[from] io::Error),
    #[error("tls configuration rejected: {0}")]
    Config(String),
}

/// Certificate + PKCS#8 private key, both DER. `Debug` prints only the
/// fingerprint: the key never reaches logs.
#[derive(Clone)]
pub struct TlsIdentity {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
}

impl std::fmt::Debug for TlsIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsIdentity")
            .field("fingerprint_sha256", &self.fingerprint_hex())
            .finish_non_exhaustive()
    }
}

pub fn fingerprint_of(cert_der: &[u8]) -> [u8; 32] {
    Sha256::digest(cert_der).into()
}

pub fn fingerprint_hex_of(cert_der: &[u8]) -> String {
    crate::pairing::hex(&fingerprint_of(cert_der))
}

impl TlsIdentity {
    pub fn generate() -> Result<Self, TlsError> {
        let key_pair = rcgen::KeyPair::generate().map_err(|e| TlsError::Generate(e.to_string()))?;
        let mut params = rcgen::CertificateParams::new(vec![SAN_NAME.to_string()])
            .map_err(|e| TlsError::Generate(e.to_string()))?;
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Hostlume agent");
        params.distinguished_name = dn;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| TlsError::Generate(e.to_string()))?;
        Ok(Self {
            cert_der: cert.der().to_vec(),
            key_der: key_pair.serialize_der(),
        })
    }

    pub fn from_der(cert_der: Vec<u8>, key_der: Vec<u8>) -> Self {
        Self { cert_der, key_der }
    }

    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        fingerprint_of(&self.cert_der)
    }

    pub fn fingerprint_hex(&self) -> String {
        fingerprint_hex_of(&self.cert_der)
    }

    /// Build the rustls server configuration. rustls checks that the key
    /// matches the certificate, so this doubles as validation of stored
    /// material.
    pub fn server_config(&self) -> Result<Arc<rustls::ServerConfig>, TlsError> {
        let certs = vec![CertificateDer::from(self.cert_der.clone())];
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone()));
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TlsError::Config(e.to_string()))?;
        Ok(Arc::new(config))
    }

    /// Read a persisted identity; `Ok(None)` when no files exist yet.
    pub fn load(dir: &Path) -> Result<Option<Self>, TlsError> {
        let cert_path = dir.join(CERT_FILE);
        let key_path = dir.join(KEY_FILE);
        if !cert_path.exists() || !key_path.exists() {
            return Ok(None);
        }
        Ok(Some(Self {
            cert_der: std::fs::read(cert_path)?,
            key_der: std::fs::read(key_path)?,
        }))
    }

    pub fn save(&self, dir: &Path) -> Result<(), TlsError> {
        std::fs::create_dir_all(dir)?;
        restrict_dir(dir)?;
        std::fs::write(dir.join(CERT_FILE), &self.cert_der)?;
        write_restricted(&dir.join(KEY_FILE), &self.key_der)?;
        Ok(())
    }

    /// Load the stored identity, or generate and persist a fresh one when
    /// nothing is stored **or the stored material is unusable** (a
    /// corrupted key must not permanently disable LAN mode; the new
    /// fingerprint makes paired consoles fail closed with `cert_mismatch`
    /// until re-paired, which is the visible, safe outcome).
    pub fn load_or_generate(dir: &Path) -> Result<Self, TlsError> {
        if let Some(stored) = Self::load(dir)? {
            if stored.server_config().is_ok() {
                return Ok(stored);
            }
        }
        let fresh = Self::generate()?;
        fresh.save(dir)?;
        Ok(fresh)
    }
}

#[cfg(unix)]
/// Write a secret-bearing file owner-only (0600 on unix; per-user AppData
/// elsewhere). Shared with the app's credential store.
pub fn write_restricted(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    // An existing file keeps its old mode on open; enforce it explicitly.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
/// Write a secret-bearing file owner-only (0600 on unix; per-user AppData
/// elsewhere). Shared with the app's credential store.
pub fn write_restricted(path: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(unix)]
fn restrict_dir(dir: &Path) -> io::Result<()> {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// TLS-terminating listener for `axum::serve`. Handshakes run in their own
/// tasks (bounded by a semaphore, each with a timeout); only completed TLS
/// streams reach the server's accept loop.
pub struct TlsListener {
    rx: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
    local_addr: SocketAddr,
}

impl TlsListener {
    /// Must be called from inside a tokio runtime (it spawns the acceptor task).
    pub fn spawn(listener: TcpListener, config: Arc<rustls::ServerConfig>) -> io::Result<Self> {
        let local_addr = listener.local_addr()?;
        let acceptor = TlsAcceptor::from(config);
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_HANDSHAKES));
            loop {
                let (stream, addr) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(_) => {
                        // Transient accept errors (EMFILE, aborted) must not
                        // spin the loop.
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                };
                let Ok(permit) = Arc::clone(&permits).acquire_owned().await else {
                    break;
                };
                let acceptor = acceptor.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Ok(Ok(tls)) =
                        tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await
                    {
                        let _ = tx.send((tls, addr)).await;
                    }
                });
            }
        });
        Ok(Self { rx, local_addr })
    }
}

impl Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.rx.recv().await {
            Some(connection) => connection,
            // The acceptor task is gone (listener closed): park here so the
            // graceful-shutdown branch of `axum::serve` still resolves.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local_addr)
    }
}

/// Peer address usable with both the plain and the TLS listener (axum has
/// no blanket `Connected` impl for custom listeners).
#[derive(Debug, Clone, Copy)]
pub struct ClientAddr(pub SocketAddr);

impl Connected<IncomingStream<'_, TcpListener>> for ClientAddr {
    fn connect_info(target: IncomingStream<'_, TcpListener>) -> Self {
        ClientAddr(*target.remote_addr())
    }
}

impl Connected<IncomingStream<'_, TlsListener>> for ClientAddr {
    fn connect_info(target: IncomingStream<'_, TlsListener>) -> Self {
        ClientAddr(*target.remote_addr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_generates_validates_and_round_trips_through_disk() {
        let identity = TlsIdentity::generate().expect("generate");
        assert_eq!(identity.fingerprint_hex().len(), 64);
        identity.server_config().expect("usable server config");

        let dir = std::env::temp_dir().join(format!("hostlume-tls-{}", uuid::Uuid::new_v4()));
        identity.save(&dir).expect("save");
        let loaded = TlsIdentity::load(&dir).expect("load").expect("present");
        assert_eq!(loaded.fingerprint_hex(), identity.fingerprint_hex());
        #[cfg(unix)]
        {
            let mode = std::fs::metadata(dir.join(KEY_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "private key must be owner-only");
        }
        // load_or_generate keeps a valid stored identity (same fingerprint).
        let again = TlsIdentity::load_or_generate(&dir).expect("load_or_generate");
        assert_eq!(again.fingerprint_hex(), identity.fingerprint_hex());
        // Corrupted key → regenerate rather than fail forever; fingerprint changes.
        std::fs::write(dir.join(KEY_FILE), b"garbage").unwrap();
        let regenerated = TlsIdentity::load_or_generate(&dir).expect("regenerate");
        assert_ne!(regenerated.fingerprint_hex(), identity.fingerprint_hex());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn debug_output_never_contains_key_material() {
        let identity = TlsIdentity::generate().expect("generate");
        let rendered = format!("{identity:?}");
        assert!(rendered.contains(&identity.fingerprint_hex()));
        let key_hex = crate::pairing::hex(&identity.key_der);
        assert!(!rendered.contains(&key_hex[..16]));
    }
}
