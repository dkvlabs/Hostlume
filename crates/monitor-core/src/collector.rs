//! Collector behavior contract (architecture.md §4). The core defines the
//! seam; platform crates and the app runtime implement it with real native
//! sources.

use crate::capability::MetricCapability;
use crate::clock::Clock;
use crate::error::CollectorError;
use crate::metric::MetricReading;

/// Sampling cadence a collector wants (architecture.md §5 poll groups).
/// The scheduler owns exactly one timer per group — never one per card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollGroup {
    /// Live counters that need frequent deltas (CPU, network). Default 3 s,
    /// settings-driven.
    Fast,
    /// Moderately expensive enumeration (memory detail). 10 s.
    Medium,
    /// Static-ish discovery (host identity, volumes). 60 s.
    Slow,
}

/// One source of metrics (e.g. the portable sysinfo collector, the macOS
/// IOKit battery collector). Implementations own their warm-up baseline:
/// interval-derived metrics returned before a second sample must be
/// `TemporarilyUnavailable` with reason `WARMUP`, never zero.
pub trait Collector: Send {
    /// Stable collector identifier used as the `source` label.
    fn id(&self) -> &'static str;

    /// Which poll group drives this collector's cadence.
    fn poll_group(&self) -> PollGroup;

    /// What this collector can deliver on *this* host right now. Cheap and
    /// safe to call repeatedly; re-probed after sleep/resume and device
    /// changes.
    fn capabilities(&self) -> Vec<MetricCapability>;

    /// Produce readings for the current tick. A failing collector returns a
    /// typed [`CollectorError`]; it must not fabricate values and must not
    /// poison other collectors' work.
    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError>;
}
