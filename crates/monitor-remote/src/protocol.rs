//! Protocol v1 (phase 10, ADR-0009 §1): versioning, the agent manifest and
//! the pairing wire types shared by the server and the console client.
//!
//! The manifest is the versioned capability handshake: a console refuses a
//! different major protocol version with a typed `incompatible` state
//! instead of guessing at payload shapes. Nothing in it identifies
//! hardware — `display_name` is user-chosen; serials, MACs and the OS
//! hostname are never sent.

use monitor_core::{Availability, CapabilityReport};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Paired consoles per agent — a few user-owned machines, not a fleet.
pub const MAX_DEVICES: usize = 16;
pub const DEFAULT_DISPLAY_NAME: &str = "APP_NAME host";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub protocol_version: u32,
    pub agent_version: String,
    pub host_id: String,
    pub display_name: String,
    pub cadence_ms: i64,
    /// Metric families (`cpu`, `memory`, …) the agent currently reports as
    /// available — the console's capability-difference signal.
    pub families: Vec<String>,
    /// Present in LAN (TLS) mode so a console can show what it pinned.
    pub tls_fingerprint_sha256: Option<String>,
}

/// Agent-side record of a paired console. Only the token **hash** is kept
/// (ADR-0009 A6); the plaintext token is returned once, in the pairing
/// response, and never again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub label: String,
    pub token_hash: String,
    pub created_at_utc: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairRequest {
    pub protocol_version: u32,
    pub device_id: String,
    pub device_label: String,
    pub nonce: String,
    pub proof: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairResponse {
    /// The console's bearer token — the only time it exists in transit.
    pub device_token: String,
    pub host_id: String,
    pub display_name: String,
    pub protocol_version: u32,
    /// Proves the agent also knew the code (ADR-0009 A8).
    pub ack: String,
}

/// Distinct family prefixes of the capabilities reported as available,
/// sorted for stable output.
pub fn families_of(report: &CapabilityReport) -> Vec<String> {
    let mut families: Vec<String> = report
        .capabilities
        .iter()
        .filter(|cap| cap.state == Availability::Available)
        .filter_map(|cap| cap.metric_id.split('.').next())
        .map(str::to_string)
        .collect();
    families.sort();
    families.dedup();
    families
}

pub fn display_name_is_valid(value: &str) -> bool {
    crate::pairing::label_is_valid(value)
}

/// Major-version compatibility: v1 consoles only talk to v1 agents.
pub fn is_compatible(remote_protocol_version: u32) -> bool {
    remote_protocol_version == PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::MetricCapability;

    #[test]
    fn families_are_distinct_sorted_and_available_only() {
        let mut report = CapabilityReport::new("h", "t");
        for (id, state) in [
            ("memory.physical.used", Availability::Available),
            ("cpu.total.usage", Availability::Available),
            ("cpu.core.usage", Availability::Available),
            ("gpu.usage", Availability::Unsupported),
        ] {
            let mut cap = MetricCapability::not_implemented_yet(id, "test");
            cap.state = state;
            report.capabilities.push(cap);
        }
        assert_eq!(families_of(&report), vec!["cpu", "memory"]);
    }

    #[test]
    fn manifest_round_trips_and_version_gate_is_exact() {
        let manifest = Manifest {
            protocol_version: PROTOCOL_VERSION,
            agent_version: AGENT_VERSION.into(),
            host_id: "host-1".into(),
            display_name: "Office Mac".into(),
            cadence_ms: 3_000,
            families: vec!["cpu".into()],
            tls_fingerprint_sha256: Some("ab".repeat(32)),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, manifest);
        assert!(is_compatible(PROTOCOL_VERSION));
        assert!(!is_compatible(PROTOCOL_VERSION + 1));
        assert!(display_name_is_valid("Office Mac"));
        assert!(!display_name_is_valid(""));
    }
}
