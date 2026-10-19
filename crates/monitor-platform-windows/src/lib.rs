//! Windows platform adapter — Phase 01 seam.
//!
//! Real collectors land with the telemetry phases. Per ADR-0004 the verified
//! Windows guidance is: direct Win32 APIs (GetSystemTimes,
//! GlobalMemoryStatusEx, psapi) preferred over PDH, whose performance
//! counters Microsoft documents as "not designed to be collected more than
//! once per second". Nothing here is implemented yet; this crate must
//! compile on every OS (cfg-gated) and must never fabricate values.

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading,
    PollGroup,
};

pub const COLLECTOR_ID: &str = "platform-windows";

#[cfg(target_os = "windows")]
fn is_native() -> bool {
    true
}

#[cfg(not(target_os = "windows"))]
fn is_native() -> bool {
    false
}

/// Phase 01 skeleton collector: capabilities are honest, samples are empty.
#[derive(Debug, Default)]
pub struct PlatformCollector;

impl PlatformCollector {
    pub fn new() -> Self {
        Self
    }

    fn capability_for(&self, metric_id: &str) -> MetricCapability {
        if is_native() {
            MetricCapability::not_implemented_yet(metric_id.to_string(), COLLECTOR_ID)
        } else {
            MetricCapability {
                metric_id: metric_id.to_string(),
                state: Availability::Unsupported,
                source: COLLECTOR_ID.to_string(),
                explanation: Some(
                    "This build of the Windows adapter is running on a different operating system."
                        .to_string(),
                ),
                reason_code: Some("WRONG_PLATFORM".to_string()),
                remediation: None,
                human_message_key: Some("capabilities.wrongPlatform".to_string()),
                precision: None,
                freshness_budget_ms: None,
                permissions_required: Vec::new(),
                optional_helper_required: false,
            }
        }
    }
}

impl Collector for PlatformCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Slow
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        families::ALL_FAMILIES
            .iter()
            .map(|id| self.capability_for(id))
            .collect()
    }

    fn sample(&mut self, _clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        if !is_native() {
            return Err(CollectorError::Unsupported {
                collector: COLLECTOR_ID,
                reason: "compiled for a different target OS".to_string(),
            });
        }
        // Real collectors arrive with the telemetry phases; until then an
        // empty set is the honest output (never placeholder values).
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::{families, SystemClock};

    #[test]
    fn capabilities_cover_every_family() {
        let caps = PlatformCollector::new().capabilities();
        assert_eq!(caps.len(), families::ALL_FAMILIES.len());
        assert!(caps.iter().all(|c| c.state == Availability::Unsupported));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_sample_is_empty_not_fake() {
        let mut c = PlatformCollector::new();
        let out = c.sample(&SystemClock).expect("native sample succeeds");
        assert!(out.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_native_sample_is_typed_unsupported() {
        let mut c = PlatformCollector::new();
        let err = c.sample(&SystemClock).expect_err("must refuse");
        assert!(matches!(err, CollectorError::Unsupported { .. }));
    }
}
