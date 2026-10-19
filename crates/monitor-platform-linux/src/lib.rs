//! Linux platform adapter (Phase 05 role).
//!
//! Per ADR-0006 the *reading* collectors live in the app runtime crate
//! (cfg-split, unit-testable); this crate contributes what is genuinely
//! OS-bound and app-independent:
//!
//! * `gpu_adapters` — the DRM-sysfs adapter/display metadata provider
//!   behind the `get_gpu_info` IPC command (FX-010).
//! * A cross-compile guard collector: on non-Linux targets it reports
//!   explicit `WRONG_PLATFORM` rows so CI catches divergence, and on a
//!   native Linux host it claims nothing — metric families are owned by
//!   the app collectors, which probe the real host state.

use monitor_core::{
    Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading, PollGroup,
};

pub mod gpu_adapters;

pub const COLLECTOR_ID: &str = "platform-linux";

#[cfg(target_os = "linux")]
fn is_native() -> bool {
    true
}

#[cfg(not(target_os = "linux"))]
fn is_native() -> bool {
    false
}

#[derive(Debug, Default)]
pub struct PlatformCollector;

impl PlatformCollector {
    pub fn new() -> Self {
        Self
    }
}

impl Collector for PlatformCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Slow
    }

    /// Native: no rows — every metric family is owned by an app-runtime
    /// collector whose capability row reflects the *probed* host state.
    /// Cross-compiled: explicit `WRONG_PLATFORM` rows (filtered out by the
    /// report builder) so nothing silently claims Linux capabilities.
    fn capabilities(&self) -> Vec<MetricCapability> {
        if is_native() {
            Vec::new()
        } else {
            monitor_core::families::ALL_FAMILIES
                .iter()
                .map(|metric_id| MetricCapability {
                    metric_id: (*metric_id).to_string(),
                    state: Availability::Unsupported,
                    source: COLLECTOR_ID.to_string(),
                    explanation: Some(
                        "This build of the Linux adapter is running on a different operating \
                         system."
                            .to_string(),
                    ),
                    reason_code: Some(monitor_core::reasons::WRONG_PLATFORM.to_string()),
                    remediation: None,
                    human_message_key: Some("capabilities.wrongPlatform".to_string()),
                    precision: None,
                    freshness_budget_ms: None,
                    permissions_required: Vec::new(),
                    optional_helper_required: false,
                })
                .collect()
        }
    }

    fn sample(&mut self, _clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        if !is_native() {
            return Err(CollectorError::Unsupported {
                collector: COLLECTOR_ID,
                reason: "compiled for a different target OS".to_string(),
            });
        }
        // Reading collectors live in the app runtime; nothing to emit here.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::SystemClock;

    #[cfg(target_os = "linux")]
    #[test]
    fn native_capabilities_claim_nothing() {
        assert!(PlatformCollector::new().capabilities().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_sample_is_empty_not_fake() {
        let mut c = PlatformCollector::new();
        let out = c.sample(&SystemClock).expect("native sample succeeds");
        assert!(out.is_empty());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_native_rows_are_marked_wrong_platform() {
        let caps = PlatformCollector::new().capabilities();
        assert_eq!(caps.len(), monitor_core::families::ALL_FAMILIES.len());
        assert!(caps
            .iter()
            .all(|c| c.reason_code.as_deref() == Some("WRONG_PLATFORM")));
        let mut c = PlatformCollector::new();
        let err = c.sample(&SystemClock).expect_err("must refuse");
        assert!(matches!(err, CollectorError::Unsupported { .. }));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gpu_provider_reads_drm_or_reports_absence() {
        match gpu_adapters::adapters() {
            Ok(adapters) => {
                for adapter in adapters {
                    assert!(adapter.id.starts_with("gpu:card"));
                }
            }
            Err(message) => assert!(message.contains("no DRM tree")),
        }
    }
}
