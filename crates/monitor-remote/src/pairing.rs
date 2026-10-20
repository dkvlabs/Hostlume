//! Pairing (phase 10, ADR-0009 §3): short-lived single-use codes and
//! HMAC-SHA256 proofs bound to the TLS certificate fingerprint the console
//! observed, so a man-in-the-middle presenting its own certificate cannot
//! complete pairing even while relaying every byte.
//!
//! Properties, all test-enforced below:
//! * a code is 8 symbols from a 32-symbol alphabet (40 bits) drawn from the
//!   OS random source, valid for [`CODE_TTL`] and consumed by **one**
//!   verification attempt — success or failure;
//! * the proof message is domain-separated and length-delimited, so no two
//!   `(device, nonce, fingerprint)` triples share a message;
//! * the acknowledgement uses a distinct domain string so an agent cannot
//!   be used as an oracle to produce a console-side proof.

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

pub const CODE_LEN: usize = 8;
/// No `0/O` and no `1/I`: codes are read aloud or typed from another screen.
pub const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_TTL: Duration = Duration::from_secs(120);
pub const MAX_DEVICE_ID_LEN: usize = 64;
pub const MAX_LABEL_LEN: usize = 48;
pub const MAX_NONCE_LEN: usize = 64;

const PAIR_DOMAIN: &[u8] = b"hostlume-pair-v1";
const ACK_DOMAIN: &[u8] = b"hostlume-pair-ack-v1";

/// HMAC-SHA256 (RFC 2104) over the workspace's existing `sha2` crate.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    outer.finalize().into()
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Constant-shape comparison: the loop always runs over the full length.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// A fresh code. A UUID v4 supplies 122 OS-random bits; the eight bytes
/// used here avoid the version/variant bytes, and `byte % 32` is unbiased
/// because 256 is a multiple of 32.
pub fn generate_code() -> String {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    let chosen = [
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[9], bytes[10],
    ];
    chosen
        .iter()
        .map(|byte| CODE_ALPHABET[(byte % 32) as usize] as char)
        .collect()
}

/// A console-side nonce (hex, 32 chars) from the OS random source.
pub fn generate_nonce() -> String {
    hex(uuid::Uuid::new_v4().as_bytes())
}

/// Normalize user input: uppercase, strip whitespace and dashes. Returns
/// `None` when the result is not a well-formed code.
pub fn normalize_code(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if code.len() != CODE_LEN || !code.bytes().all(|b| CODE_ALPHABET.contains(&b)) {
        return None;
    }
    Some(code)
}

fn message(domain: &[u8], device_id: &str, nonce: &str, fingerprint_hex: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + device_id.len() + nonce.len() + 80);
    for part in [
        domain,
        device_id.as_bytes(),
        nonce.as_bytes(),
        fingerprint_hex.as_bytes(),
    ] {
        out.extend_from_slice(&(part.len() as u32).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// Console → agent proof (hex).
pub fn proof(code: &str, device_id: &str, nonce: &str, fingerprint_hex: &str) -> String {
    hex(&hmac_sha256(
        code.as_bytes(),
        &message(PAIR_DOMAIN, device_id, nonce, fingerprint_hex),
    ))
}

/// Agent → console acknowledgement (hex).
pub fn ack(code: &str, device_id: &str, nonce: &str, fingerprint_hex: &str) -> String {
    hex(&hmac_sha256(
        code.as_bytes(),
        &message(ACK_DOMAIN, device_id, nonce, fingerprint_hex),
    ))
}

/// Identifier hygiene shared by both sides: printable ASCII only, bounded.
pub fn identifier_is_valid(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Labels are user-visible text: bounded, no control characters.
pub fn label_is_valid(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_LABEL_LEN
        && !trimmed.chars().any(char::is_control)
}

pub fn fingerprint_hex_is_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    #[error("no pairing window is open")]
    NoWindow,
    #[error("the pairing window has expired")]
    Expired,
    #[error("the pairing code was already used")]
    Burned,
    #[error("the pairing proof is invalid")]
    InvalidProof,
    #[error("the pairing request is malformed")]
    Malformed,
}

/// One pairing window: a code and its lifetime. Verification consumes the
/// window whatever the outcome, so each code allows exactly one guess.
#[derive(Debug)]
pub struct PairingWindow {
    code: String,
    expires_at: Instant,
    burned: bool,
}

impl PairingWindow {
    pub fn open(now: Instant) -> Self {
        Self::with_code(generate_code(), now)
    }

    pub fn with_code(code: String, now: Instant) -> Self {
        Self {
            code,
            expires_at: now + CODE_TTL,
            burned: false,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn is_open(&self, now: Instant) -> bool {
        !self.burned && now < self.expires_at
    }

    pub fn secs_remaining(&self, now: Instant) -> u64 {
        if self.burned || now >= self.expires_at {
            0
        } else {
            (self.expires_at - now).as_secs()
        }
    }

    /// Verify a presented proof against this window's code and the
    /// **agent's own** fingerprint. The window is consumed before the
    /// comparison; a valid proof yields the acknowledgement for the console.
    pub fn verify(
        &mut self,
        now: Instant,
        device_id: &str,
        nonce: &str,
        own_fingerprint_hex: &str,
        presented_proof_hex: &str,
    ) -> Result<String, PairError> {
        if self.burned {
            return Err(PairError::Burned);
        }
        if now >= self.expires_at {
            self.burned = true;
            return Err(PairError::Expired);
        }
        // Consume first: a malformed or wrong attempt must not leave the
        // code guessable a second time.
        self.burned = true;
        if !identifier_is_valid(device_id, MAX_DEVICE_ID_LEN)
            || !identifier_is_valid(nonce, MAX_NONCE_LEN)
            || !fingerprint_hex_is_valid(presented_proof_hex)
        {
            return Err(PairError::Malformed);
        }
        let expected = proof(&self.code, device_id, nonce, own_fingerprint_hex);
        if !constant_time_eq(expected.as_bytes(), presented_proof_hex.as_bytes()) {
            return Err(PairError::InvalidProof);
        }
        Ok(ack(&self.code, device_id, nonce, own_fingerprint_hex))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
    #[test]
    fn hmac_matches_rfc4231_vector() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    /// RFC 4231 test case 6: a key longer than the block size is hashed first.
    #[test]
    fn hmac_handles_long_keys() {
        let key = [0xaau8; 131];
        let mac = hmac_sha256(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn codes_are_well_formed_and_random() {
        let a = generate_code();
        let b = generate_code();
        assert_eq!(a.len(), CODE_LEN);
        assert!(a.bytes().all(|c| CODE_ALPHABET.contains(&c)));
        assert_ne!(a, b, "two fresh codes must not collide in practice");
        assert_eq!(
            normalize_code(" ab-cd ef23 ").as_deref(),
            Some("ABCDEF23"),
            "whitespace/dashes are stripped and case folded"
        );
        assert_eq!(normalize_code("abcd-efgh").as_deref(), Some("ABCDEFGH"));
        assert_eq!(normalize_code("ABCDEFG0"), None, "0 is not in the alphabet");
        assert_eq!(normalize_code("ABCDEFG"), None);
    }

    #[test]
    fn proof_round_trip_and_ack() {
        let fp = "ab".repeat(32);
        let now = Instant::now();
        let mut window = PairingWindow::with_code("ABCDEFGH".into(), now);
        let presented = proof("ABCDEFGH", "device-1", "nonce1", &fp);
        let ack_value = window
            .verify(now, "device-1", "nonce1", &fp, &presented)
            .expect("valid proof");
        assert_eq!(ack_value, ack("ABCDEFGH", "device-1", "nonce1", &fp));
        assert_ne!(ack_value, presented, "ack uses a distinct domain");
        assert!(
            !window.is_open(now),
            "a successful attempt consumes the code"
        );
    }

    /// ADR-0009 A2: a proof computed over the attacker's certificate
    /// fingerprint never verifies against the agent's own — and burns the code.
    #[test]
    fn proof_bound_to_another_fingerprint_is_rejected_and_burns() {
        let agent_fp = "ab".repeat(32);
        let mitm_fp = "cd".repeat(32);
        let now = Instant::now();
        let mut window = PairingWindow::with_code("ABCDEFGH".into(), now);
        let presented = proof("ABCDEFGH", "device-1", "nonce1", &mitm_fp);
        assert_eq!(
            window.verify(now, "device-1", "nonce1", &agent_fp, &presented),
            Err(PairError::InvalidProof)
        );
        // Even the correct proof is now useless: one attempt per code.
        let correct = proof("ABCDEFGH", "device-1", "nonce1", &agent_fp);
        assert_eq!(
            window.verify(now, "device-1", "nonce1", &agent_fp, &correct),
            Err(PairError::Burned)
        );
    }

    #[test]
    fn expiry_and_malformed_inputs_consume_the_window() {
        let fp = "ab".repeat(32);
        let now = Instant::now();
        let mut window = PairingWindow::with_code("ABCDEFGH".into(), now);
        assert_eq!(window.secs_remaining(now), CODE_TTL.as_secs());
        let later = now + CODE_TTL + Duration::from_secs(1);
        assert_eq!(window.secs_remaining(later), 0);
        let presented = proof("ABCDEFGH", "device-1", "nonce1", &fp);
        assert_eq!(
            window.verify(later, "device-1", "nonce1", &fp, &presented),
            Err(PairError::Expired)
        );

        let mut window = PairingWindow::with_code("ABCDEFGH".into(), now);
        assert_eq!(
            window.verify(now, "bad id!", "nonce1", &fp, &presented),
            Err(PairError::Malformed)
        );
        assert!(!window.is_open(now));
    }

    #[test]
    fn validators_bound_and_reject_control_characters() {
        assert!(identifier_is_valid("device-1_a", 16));
        assert!(!identifier_is_valid("", 16));
        assert!(!identifier_is_valid("x".repeat(17).as_str(), 16));
        assert!(!identifier_is_valid("dev ice", 16));
        assert!(label_is_valid("Office Mac"));
        assert!(!label_is_valid("bad\u{7}label"));
        assert!(!label_is_valid("x".repeat(MAX_LABEL_LEN + 1).as_str()));
        assert!(fingerprint_hex_is_valid(&"0a".repeat(32)));
        assert!(!fingerprint_hex_is_valid(&"0g".repeat(32)));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
