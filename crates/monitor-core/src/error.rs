//! Typed errors for collectors and schema validation.

use crate::metric::SCHEMA_VERSION;

/// A reading that violates the data contract invariants.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractViolation {
    #[error("available reading must carry a value")]
    AvailableRequiresValue,
    #[error("non-available reading must carry null value (no sentinel, zero or stale fill)")]
    UnavailableRequiresNull,
    #[error("numeric value must be finite (NaN and infinities are forbidden)")]
    NonFiniteNumber,
}

/// Snapshot/schema validation failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    #[error("unsupported schema version {found} (this build supports {supported})")]
    UnsupportedSchemaVersion { found: u8, supported: u8 },
    #[error("invalid snapshot: {0}")]
    InvalidSnapshot(String),
    #[error("invalid reading {metric_id}/{entity_id}: {violation}")]
    InvalidReading {
        metric_id: String,
        entity_id: String,
        violation: ContractViolation,
    },
}

/// Failures a single collector can report. A failing collector must never
/// poison the whole snapshot; the scheduler converts these to explicit
/// availability states.
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("collector {collector}: io failure: {message}")]
    Io {
        collector: &'static str,
        message: String,
    },
    #[error("collector {collector}: parse failure: {message}")]
    Parse {
        collector: &'static str,
        message: String,
    },
    #[error("collector {collector}: unsupported on this platform/hardware: {reason}")]
    Unsupported {
        collector: &'static str,
        reason: String,
    },
    #[error("collector {collector}: permission required: {reason}")]
    PermissionDenied {
        collector: &'static str,
        reason: String,
    },
    #[error("collector {collector}: temporarily unavailable: {reason}")]
    TemporarilyUnavailable {
        collector: &'static str,
        reason: String,
    },
    #[error("collector {collector}: internal error: {message}")]
    Other {
        collector: &'static str,
        message: String,
    },
}

impl CollectorError {
    pub fn collector_id(&self) -> &'static str {
        match self {
            CollectorError::Io { collector, .. }
            | CollectorError::Parse { collector, .. }
            | CollectorError::Unsupported { collector, .. }
            | CollectorError::PermissionDenied { collector, .. }
            | CollectorError::TemporarilyUnavailable { collector, .. }
            | CollectorError::Other { collector, .. } => collector,
        }
    }
}

/// Guard used when deserializing payloads from any boundary (IPC, storage).
pub fn check_schema_version(found: u8) -> Result<(), SchemaError> {
    if found == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(SchemaError::UnsupportedSchemaVersion {
            found,
            supported: SCHEMA_VERSION,
        })
    }
}
