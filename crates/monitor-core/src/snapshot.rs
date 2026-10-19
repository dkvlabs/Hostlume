//! System snapshot envelope — the single normalized payload consumed by IPC,
//! history storage, alerts and (later) the optional remote server.

use crate::error::SchemaError;
use crate::metric::{MetricReading, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub schema_version: u8,
    /// Random installation UUID — never MAC address or hardware serial
    /// (contract §8).
    pub host_id: String,
    /// Changes when the collector set resets (app restart, collector
    /// reload); consumers must resubscribe and re-baseline rates.
    pub generation: String,
    /// Strictly increasing per generation.
    pub sequence: u64,
    pub collected_at_utc: String,
    pub metrics: Vec<MetricReading>,
    /// Bumped whenever the capability report changes.
    pub capabilities_revision: u64,
}

impl SystemSnapshot {
    /// Empty, honest snapshot: collection is alive but has produced no
    /// readings yet (Phase 01 scheduler). Not a placeholder for real data.
    pub fn empty(
        host_id: impl Into<String>,
        generation: impl Into<String>,
        sequence: u64,
        collected_at_utc: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            host_id: host_id.into(),
            generation: generation.into(),
            sequence,
            collected_at_utc: collected_at_utc.into(),
            metrics: Vec::new(),
            capabilities_revision: 1,
        }
    }

    /// Full validation: schema version plus every reading's invariants.
    pub fn validate(&self) -> Result<(), SchemaError> {
        crate::error::check_schema_version(self.schema_version)?;
        if self.host_id.is_empty() {
            return Err(SchemaError::InvalidSnapshot(
                "host_id must not be empty".into(),
            ));
        }
        if self.generation.is_empty() {
            return Err(SchemaError::InvalidSnapshot(
                "generation must not be empty".into(),
            ));
        }
        for metric in &self.metrics {
            metric.validate()?;
        }
        Ok(())
    }
}
