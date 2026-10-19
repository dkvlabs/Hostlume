//! macOS platform adapter — Phase 01 seam.
//!
//! Real native collectors land with the telemetry phases (validated sources
//! per ADR-0004: sysinfo baseline, IOKit `AppleSmartBattery`, IOKit HID
//! temperature sensors; see `docs/evidence/FEASIBILITY_SPIKES.md`). This
//! crate must compile on every OS so CI catches divergence immediately; on
//! non-macOS targets everything reports explicit `WRONG_PLATFORM` and no
//! values are ever fabricated.

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading,
};

pub const COLLECTOR_ID: &str = "platform-macos";

#[cfg(target_os = "macos")]
fn is_native() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
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
                    "This build of the macOS adapter is running on a different operating system."
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
        assert!(caps.iter().all(
            |c| c.reason_code.as_deref() == Some("COLLECTOR_NOT_IMPLEMENTED")
                || c.reason_code.as_deref() == Some("WRONG_PLATFORM")
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_sample_is_empty_not_fake() {
        let mut c = PlatformCollector::new();
        let out = c.sample(&SystemClock).expect("native sample succeeds");
        assert!(out.is_empty());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_native_sample_is_typed_unsupported() {
        let mut c = PlatformCollector::new();
        let err = c.sample(&SystemClock).expect_err("must refuse");
        assert!(matches!(err, CollectorError::Unsupported { .. }));
    }
}
