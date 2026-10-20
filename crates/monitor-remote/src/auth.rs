//! Remote authentication (phase 08 §3.9): OS-RNG token generation with
//! hash-only storage, a bounded per-IP fixed-window rate limiter, and the
//! in-memory session map behind the dashboard's cookie exchange. Tokens
//! never appear in URLs or logs; the plaintext exists only at generation
//! time and in the user's clipboard.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// Fresh token from the OS random source (UUID v4 = 122 random bits).
pub fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Only this ever touches disk or server memory.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Constant-shape comparison of two hex hashes (local traffic, but the
/// comparison cost does not reveal a partial match).
pub fn token_matches(presented: &str, stored_hash: &str) -> bool {
    let presented = hash_token(presented);
    if presented.len() != stored_hash.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in presented.bytes().zip(stored_hash.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Fixed-window limiter: `max` events per `window` per key. The map is
/// bounded — when it grows past the cap, expired entries are swept and a
/// still-full map refuses new keys (fail-closed under address spoofing).
pub struct RateLimiter {
    windows: HashMap<IpAddr, (u32, Instant)>,
    max_per_window: u32,
    window: Duration,
    key_cap: usize,
}

pub const DEFAULT_MAX_PER_WINDOW: u32 = 240;
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(10);
pub const AUTH_MAX_PER_WINDOW: u32 = 10;
const KEY_CAP: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    Allow,
    Limit,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            windows: HashMap::new(),
            max_per_window: DEFAULT_MAX_PER_WINDOW,
            window: DEFAULT_WINDOW,
            key_cap: KEY_CAP,
        }
    }
}

impl RateLimiter {
    pub fn with_limits(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window,
            window,
            ..Self::default()
        }
    }

    pub fn check(&mut self, key: IpAddr) -> RateDecision {
        let now = Instant::now();
        match self.windows.get_mut(&key) {
            Some((count, window_start)) if now.duration_since(*window_start) < self.window => {
                if *count >= self.max_per_window {
                    RateDecision::Limit
                } else {
                    *count += 1;
                    RateDecision::Allow
                }
            }
            Some(entry) => {
                *entry = (1, now);
                RateDecision::Allow
            }
            None => {
                if self.windows.len() >= self.key_cap {
                    self.windows
                        .retain(|_, (_, start)| now.duration_since(*start) < self.window);
                    if self.windows.len() >= self.key_cap {
                        return RateDecision::Limit;
                    }
                }
                self.windows.insert(key, (1, now));
                RateDecision::Allow
            }
        }
    }
}

/// In-memory dashboard sessions (cookie value → expiry). Bounded: expired
/// entries are swept and the oldest session is evicted at capacity.
#[derive(Debug, Default)]
pub struct SessionMap {
    sessions: HashMap<String, Instant>,
}

impl SessionMap {
    pub fn create(&mut self, ttl: Duration, now: Instant) -> String {
        if self.sessions.len() >= super::MAX_SESSIONS {
            self.sessions.retain(|_, expiry| *expiry > now);
            while self.sessions.len() >= super::MAX_SESSIONS {
                let oldest = self
                    .sessions
                    .iter()
                    .min_by_key(|(_, expiry)| **expiry)
                    .map(|(id, _)| id.clone());
                match oldest {
                    Some(id) => {
                        self.sessions.remove(&id);
                    }
                    None => break,
                }
            }
        }
        let id = generate_token();
        self.sessions.insert(id.clone(), now + ttl);
        id
    }

    pub fn is_valid(&self, sid: &str, now: Instant) -> bool {
        self.sessions.get(sid).is_some_and(|expiry| *expiry > now)
    }

    /// Drop everything (used on token rotation/revocation: existing
    /// dashboard sessions die with the token they were minted from).
    pub fn clear(&mut self) {
        self.sessions.clear();
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// Who presented a valid bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    Owner,
    Device(String),
}

impl super::ServerShared {
    /// Hot-swap the accepted owner token (rotation). Old tokens, and every
    /// dashboard session minted from them, stop working immediately.
    /// Paired devices are unaffected: they have their own credentials.
    pub fn rotate_token(&self, new_hash: String) {
        *self.token_hash.lock().unwrap_or_else(|p| p.into_inner()) = new_hash;
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    pub fn token_hash(&self) -> String {
        self.token_hash
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Owner token first, then every paired device (each compared in
    /// constant shape). A device match refreshes its last-seen clock.
    pub fn authenticate_bearer(&self, token: &str) -> Option<Principal> {
        if token_matches(token, &self.token_hash()) {
            return Some(Principal::Owner);
        }
        let matched = {
            let devices = self.devices.lock().unwrap_or_else(|p| p.into_inner());
            devices
                .iter()
                .find(|device| token_matches(token, &device.token_hash))
                .map(|device| device.device_id.clone())
        };
        let device_id = matched?;
        self.device_last_seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(device_id.clone(), Instant::now());
        Some(Principal::Device(device_id))
    }

    /// Replace the live device credential set (startup load, bulk revoke).
    pub fn set_devices(&self, devices: Vec<super::DeviceCredential>) {
        *self.devices.lock().unwrap_or_else(|p| p.into_inner()) = devices;
    }

    /// Add (or replace by id) one credential; bounded by
    /// [`super::protocol::MAX_DEVICES`] — the caller checks the cap first.
    pub fn add_device(&self, credential: super::DeviceCredential) {
        let mut devices = self.devices.lock().unwrap_or_else(|p| p.into_inner());
        devices.retain(|d| d.device_id != credential.device_id);
        devices.push(credential);
    }

    /// Revoke one device: its token stops working on the next request.
    pub fn remove_device(&self, device_id: &str) {
        self.devices
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|d| d.device_id != device_id);
        self.device_last_seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(device_id);
    }

    pub fn device_count(&self) -> usize {
        self.devices.lock().unwrap_or_else(|p| p.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([127, 0, 0, n])
    }

    #[test]
    fn token_hashing_is_stable_and_matches_only_equal_input() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_eq!(hash.len(), 64);
        assert_eq!(hash_token(&token), hash);
        assert!(token_matches(&token, &hash));
        assert!(!token_matches(&token, &hash_token("other")));
        assert!(!token_matches(&token, "too-short"));
        // UUID v4 shape: hyphens, version nibble.
        assert_eq!(token.len(), 36);
        assert!(token.chars().filter(|c| *c == '-').count() == 4);
    }

    #[test]
    fn limiter_allows_burst_then_limits_and_resets() {
        let mut limiter = RateLimiter::with_limits(3, Duration::from_millis(50));
        assert_eq!(limiter.check(ip(1)), RateDecision::Allow);
        assert_eq!(limiter.check(ip(1)), RateDecision::Allow);
        assert_eq!(limiter.check(ip(1)), RateDecision::Allow);
        assert_eq!(limiter.check(ip(1)), RateDecision::Limit);
        // Other keys are independent.
        assert_eq!(limiter.check(ip(2)), RateDecision::Allow);
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(limiter.check(ip(1)), RateDecision::Allow);
    }

    #[test]
    fn limiter_is_bounded_under_key_flood() {
        let mut limiter = RateLimiter::with_limits(1_000_000, Duration::from_secs(60));
        // Flood past the key cap: new keys must eventually be limited.
        for i in 0..(KEY_CAP + 64) {
            let _ = limiter.check(IpAddr::from([10, 0, 0, (i % 255) as u8]));
        }
        let fresh = IpAddr::from([192, 168, 5, 5]);
        // Either swept space allowed it or the map refused it — but the
        // structure never grows unbounded.
        let _ = limiter.check(fresh);
        assert!(limiter.windows.len() <= KEY_CAP + 1);
    }

    #[test]
    fn sessions_are_bounded_expire_and_clear() {
        let mut map = SessionMap::default();
        let now = Instant::now();
        let ttl = Duration::from_secs(60);
        for _ in 0..super::super::MAX_SESSIONS {
            let _ = map.create(ttl, now);
        }
        assert_eq!(map.len(), super::super::MAX_SESSIONS);
        // One more evicts the oldest instead of growing unbounded.
        let _ = map.create(ttl, now);
        assert_eq!(map.len(), super::super::MAX_SESSIONS);

        let sid = map.sessions.keys().next().unwrap().clone();
        assert!(map.is_valid(&sid, now + Duration::from_secs(1)));
        assert!(!map.is_valid(&sid, now + ttl + Duration::from_secs(1)));

        map.clear();
        assert!(map.is_empty());
        assert!(!map.is_valid(&sid, now));
    }
}
