//! Canonical metric reading types (`docs/metrics-data-contract.md` §2).

use crate::error::{ContractViolation, SchemaError};
use serde::{Deserialize, Serialize};

/// Schema version of the snapshot/reading contract this crate implements.
pub const SCHEMA_VERSION: u8 = 1;

/// Why a value is (or is not) present. This is the *only* honesty vocabulary
/// the UI is allowed to render; there is no "fake it" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unsupported,
    PermissionRequired,
    TemporarilyUnavailable,
    Stale,
    Error,
}

/// Canonical storage units. Conversion happens only at display/export
/// boundaries; collectors must emit these units directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    Percent,
    Bytes,
    BytesPerSecond,
    BitsPerSecond,
    Celsius,
    Rpm,
    Seconds,
    Count,
    Watts,
    Hertz,
    Boolean,
    Text,
}

/// A measured value. JSON-wise this is `number | string | boolean`; absence
/// is expressed as `Option::None` (serialized `null`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Number(f64),
    Text(String),
    Flag(bool),
}

/// One measurement of one metric for one entity at one collection instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricReading {
    pub metric_id: String,
    pub entity_id: String,
    /// `None` serializes as `null`. Non-available readings must be `None`.
    #[serde(default)]
    pub value: Option<MetricValue>,
    pub unit: MetricUnit,
    pub availability: Availability,
    /// Collection provenance, e.g. `sysinfo`, `linux-hwmon`, `windows-pdh`.
    pub source: String,
    /// ISO-8601 UTC collection timestamp.
    pub collected_at_utc: String,
    /// Monotonic per-host sequence number of the enclosing snapshot.
    pub monotonic_sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    /// i18n key resolved by the UI; never ship English prose in the payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_message_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
}

impl MetricReading {
    /// Enforce the contract invariants:
    ///
    /// * `available` readings must carry a finite value;
    /// * every other availability must carry `null` (no NaN, `-1`, `0`-fill
    ///   or stale value presented as live);
    /// * non-finite numbers are rejected outright.
    pub fn check_invariants(&self) -> Result<(), ContractViolation> {
        if self.availability == Availability::Available {
            let Some(value) = &self.value else {
                return Err(ContractViolation::AvailableRequiresValue);
            };
            if let MetricValue::Number(n) = value {
                if !n.is_finite() {
                    return Err(ContractViolation::NonFiniteNumber);
                }
            }
        } else if self.value.is_some() {
            return Err(ContractViolation::UnavailableRequiresNull);
        }
        Ok(())
    }

    /// Validate and return a descriptive schema error on failure.
    pub fn validate(&self) -> Result<(), SchemaError> {
        self.check_invariants()
            .map_err(|violation| SchemaError::InvalidReading {
                metric_id: self.metric_id.clone(),
                entity_id: self.entity_id.clone(),
                violation,
            })
    }
}
