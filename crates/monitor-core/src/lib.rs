//! monitor-core — the telemetry contract shared by collectors, IPC and UI.
//!
//! Types here mirror `docs/metrics-data-contract.md` (adopted normatively by
//! ADR-0001). The load-bearing rule: a missing reading is `null`, never zero,
//! NaN or a sentinel. `MetricReading::check_invariants` enforces this and is
//! exercised by the serialization fixtures under `tests/`.

pub mod capability;
pub mod clock;
pub mod collector;
pub mod error;
pub mod families;
pub mod metric;
pub mod reasons;
pub mod sampling;
pub mod snapshot;
pub mod units;

pub use capability::{CapabilityReport, MetricCapability};
pub use clock::{Clock, SystemClock};
pub use collector::{Collector, PollGroup};
pub use error::{check_schema_version, CollectorError, ContractViolation, SchemaError};
pub use metric::{Availability, MetricReading, MetricUnit, MetricValue, SCHEMA_VERSION};
pub use snapshot::SystemSnapshot;
