//! Capability reporting: what this host/OS/driver combination can actually
//! deliver, with reasons. `unsupported` is a first-class successful
//! diagnostic result, never an error to hide (architecture.md §9).

use crate::metric::{Availability, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricCapability {
    pub metric_id: String,
    pub state: Availability,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    /// Neutral, actionable guidance. UI-localized via message keys when
    /// present; never "install random tool" advice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_message_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness_budget_ms: Option<u64>,
    #[serde(default)]
    pub permissions_required: Vec<String>,
    #[serde(default)]
    pub optional_helper_required: bool,
}

impl MetricCapability {
    /// A capability entry for a metric family this build cannot measure yet.
    /// Used by the Phase 01 skeletons; honest about being unimplemented.
    pub fn not_implemented_yet(metric_id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            metric_id: metric_id.into(),
            state: Availability::Unsupported,
            source: source.into(),
            explanation: Some(
                "Metric collection for this family is not implemented in this build yet."
                    .to_string(),
            ),
            reason_code: Some("COLLECTOR_NOT_IMPLEMENTED".to_string()),
            remediation: None,
            human_message_key: Some("capabilities.collectorNotImplemented".to_string()),
            precision: None,
            freshness_budget_ms: None,
            permissions_required: Vec::new(),
            optional_helper_required: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityReport {
    pub schema_version: u8,
    pub host_id: String,
    pub generated_at_utc: String,
    /// Bumped when the set of capabilities changes (driver/permission/hotplug).
    pub revision: u64,
    pub capabilities: Vec<MetricCapability>,
}

impl CapabilityReport {
    pub fn new(host_id: impl Into<String>, generated_at_utc: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            host_id: host_id.into(),
            generated_at_utc: generated_at_utc.into(),
            revision: 1,
            capabilities: Vec::new(),
        }
    }
}
