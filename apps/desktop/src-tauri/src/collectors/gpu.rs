//! GPU utilization / dedicated-memory collector (FX-010, phase 05 §2.3).
//!
//! "Implement narrowly per tested vendor/device" applies here in full:
//! * **Linux:** DRM sysfs driver files — `gpu_busy_percent` and
//!   `mem_info_vram_used/total` (amdgpu-class interface, also provided by
//!   some other DRM drivers). A card exposes live readings **only** when
//!   its kernel driver publishes those files; NVIDIA's proprietary stack
//!   does not, and no vendor-specific tool is bundled.
//! * **macOS:** no public, non-root GPU busy-time API exists (Apple's
//!   `powermetrics` sampler requires root, which the app refuses by
//!   design — Phase 00 cross-check 6). Utilization is an explicit
//!   `unsupported`, and Apple Silicon has *no dedicated VRAM*: shared
//!   unified memory is never reported as VRAM.
//! * **Windows:** adapter/display identity is served by the DXGI metadata
//!   provider; live counters are `unsupported` until validated on real
//!   hardware (`NOT_VALIDATED_ON_HARDWARE`) rather than shipped unproven.
//!
//! Static adapter/display identity lives in `monitor-core::gpu` and is
//! served through the `get_gpu_info` command, not this polled stream.

use monitor_core::{
    families, reasons, Availability, Clock, Collector, CollectorError, MetricCapability,
    MetricReading, PollGroup,
};

#[cfg(target_os = "linux")]
use super::reading;

#[cfg(target_os = "linux")]
use monitor_core::{MetricUnit, MetricValue};

pub const COLLECTOR_ID: &str = "collector-gpu";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveGpuSource {
    /// `gpu:cardN` — stable while the kernel keeps the card index.
    pub entity: String,
    pub busy_path: std::path::PathBuf,
    pub vram_used_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Default)]
pub struct GpuCollector {
    #[cfg(target_os = "linux")]
    sources: Vec<LiveGpuSource>,
}

/// `gpu_busy_percent` files contain a single integer 0–100.
pub fn parse_busy_percent(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    value.is_finite().then_some(value.clamp(0.0, 100.0))
}

/// `mem_info_vram_*` files contain a byte count.
pub fn parse_mem_bytes(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok()
}

#[cfg(target_os = "linux")]
fn probe_live_sources() -> Vec<LiveGpuSource> {
    let mut sources = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return sources;
    };
    let mut cards: Vec<(String, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            // Plain card nodes only — card0-DP-1 connectors are children.
            let is_card = name.starts_with("card")
                && name["card".len()..].chars().all(|c| c.is_ascii_digit());
            is_card.then(|| (name, entry.path()))
        })
        .collect();
    cards.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, path) in cards {
        let device = path.join("device");
        let busy_path = device.join("gpu_busy_percent");
        if !busy_path.is_file() {
            continue;
        }
        let vram_used_path = {
            let candidate = device.join("mem_info_vram_used");
            candidate.is_file().then_some(candidate)
        };
        let index = name.trim_start_matches("card").to_string();
        sources.push(LiveGpuSource {
            entity: format!("gpu:card{index}"),
            busy_path,
            vram_used_path,
        });
    }
    sources
}

impl GpuCollector {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self {
                sources: probe_live_sources(),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self {}
        }
    }

    #[cfg(target_os = "linux")]
    fn live_source_count(&self) -> usize {
        self.sources.len()
    }

    #[cfg(not(target_os = "linux"))]
    fn live_source_count(&self) -> usize {
        0
    }
}

impl Collector for GpuCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Slow
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        let (util_state, util_reason, util_explanation): (
            Availability,
            Option<&'static str>,
            String,
        ) = if cfg!(target_os = "linux") {
            if self.live_source_count() > 0 {
                (
                    Availability::Available,
                    None,
                    format!(
                        "{} GPU adapter(s) expose kernel busy-time counters \
                         (/sys/class/drm/*/device/gpu_busy_percent).",
                        self.live_source_count()
                    ),
                )
            } else {
                (
                    Availability::Unsupported,
                    Some(reasons::NOT_EXPOSED_BY_OS_API),
                    "No DRM adapter on this machine publishes gpu_busy_percent (common for \
                     NVIDIA's proprietary driver; only the open kernel interfaces are read)."
                        .to_string(),
                )
            }
        } else if cfg!(target_os = "macos") {
            (
                Availability::Unsupported,
                Some(reasons::NO_PUBLIC_READABLE_GPU),
                "macOS exposes no public non-root GPU busy-time API. Apple's sampler \
                 (powermetrics) requires root, which this app does not request; showing 0% \
                 would be fabricated, so utilization stays unsupported."
                    .to_string(),
            )
        } else {
            (
                Availability::Unsupported,
                Some(reasons::NOT_VALIDATED_ON_HARDWARE),
                "Windows exposes GPU engine counters through driver interfaces that could \
                 not be validated on real hardware in this build; they are not reported \
                 until that validation exists."
                    .to_string(),
            )
        };
        let (vram_state, vram_reason, vram_explanation): (
            Availability,
            Option<&'static str>,
            String,
        ) = if cfg!(target_os = "linux") {
            if self.live_source_count() > 0 {
                (
                    Availability::Available,
                    None,
                    "Dedicated VRAM usage from the kernel's mem_info_vram_used counters \
                     where the driver provides them."
                        .to_string(),
                )
            } else {
                (
                    Availability::Unsupported,
                    Some(reasons::NOT_EXPOSED_BY_OS_API),
                    "No adapter publishes a dedicated-VRAM counter on this machine.".to_string(),
                )
            }
        } else if cfg!(target_os = "macos") {
            if std::env::consts::ARCH == "aarch64" {
                (
                    Availability::Unsupported,
                    Some(reasons::UNIFIED_MEMORY),
                    "Apple Silicon shares system memory with the GPU; there is no \
                     dedicated VRAM and this app never relabels unified memory as one."
                        .to_string(),
                )
            } else {
                (
                    Availability::Unsupported,
                    Some(reasons::NOT_EXPOSED_BY_OS_API),
                    "This Mac's GPU driver exposes no public live dedicated-VRAM usage \
                     counter."
                        .to_string(),
                )
            }
        } else {
            (
                Availability::Unsupported,
                Some(reasons::NOT_EXPOSED_BY_OS_API),
                "Windows publicly exposes dedicated-VRAM capacity (DXGI), not live usage; \
                 usage is not reported rather than approximated."
                    .to_string(),
            )
        };
        vec![
            MetricCapability {
                metric_id: families::GPU_ENGINE_UTILIZATION.to_string(),
                state: util_state,
                source: COLLECTOR_ID.to_string(),
                explanation: Some(util_explanation),
                reason_code: util_reason.map(str::to_string),
                remediation: None,
                human_message_key: None,
                precision: Some(0),
                freshness_budget_ms: Some(15_000),
                permissions_required: Vec::new(),
                optional_helper_required: false,
            },
            MetricCapability {
                metric_id: families::GPU_MEMORY_DEDICATED_USED.to_string(),
                state: vram_state,
                source: COLLECTOR_ID.to_string(),
                explanation: Some(vram_explanation),
                reason_code: vram_reason.map(str::to_string),
                remediation: None,
                human_message_key: None,
                precision: None,
                freshness_budget_ms: Some(15_000),
                permissions_required: Vec::new(),
                optional_helper_required: false,
            },
        ]
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        #[cfg(target_os = "linux")]
        {
            let mut readings = Vec::new();
            for source in &self.sources {
                match std::fs::read_to_string(&source.busy_path)
                    .ok()
                    .and_then(|text| parse_busy_percent(&text))
                {
                    Some(percent) => readings.push(reading(
                        families::GPU_ENGINE_UTILIZATION,
                        &source.entity,
                        Some(MetricValue::Number(percent)),
                        MetricUnit::Percent,
                        Availability::Available,
                        COLLECTOR_ID,
                        clock,
                        None,
                        None,
                    )),
                    None => readings.push(reading(
                        families::GPU_ENGINE_UTILIZATION,
                        &source.entity,
                        None,
                        MetricUnit::Percent,
                        Availability::TemporarilyUnavailable,
                        COLLECTOR_ID,
                        clock,
                        Some(reasons::NO_SAMPLE),
                        None,
                    )),
                }
                if let Some(vram_path) = &source.vram_used_path {
                    match std::fs::read_to_string(vram_path)
                        .ok()
                        .and_then(|text| parse_mem_bytes(&text))
                    {
                        Some(bytes) => readings.push(reading(
                            families::GPU_MEMORY_DEDICATED_USED,
                            &source.entity,
                            Some(MetricValue::Number(bytes as f64)),
                            MetricUnit::Bytes,
                            Availability::Available,
                            COLLECTOR_ID,
                            clock,
                            None,
                            None,
                        )),
                        None => readings.push(reading(
                            families::GPU_MEMORY_DEDICATED_USED,
                            &source.entity,
                            None,
                            MetricUnit::Bytes,
                            Availability::TemporarilyUnavailable,
                            COLLECTOR_ID,
                            clock,
                            Some(reasons::NO_SAMPLE),
                            None,
                        )),
                    }
                }
            }
            return Ok(readings);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = clock;
            // Explicit unsupported capability; nothing is invented.
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::FakeClock;

    #[test]
    fn busy_percent_parse_and_clamp() {
        assert_eq!(parse_busy_percent("42\n"), Some(42.0));
        assert_eq!(parse_busy_percent("0"), Some(0.0));
        assert_eq!(parse_busy_percent("100"), Some(100.0));
        // Out-of-range driver values must never escape the 0–100 contract.
        assert_eq!(parse_busy_percent("255"), Some(100.0));
        assert_eq!(parse_busy_percent(""), None);
        assert_eq!(parse_busy_percent("garbage"), None);
    }

    #[test]
    fn mem_bytes_parse() {
        assert_eq!(parse_mem_bytes("123456789\n"), Some(123_456_789));
        assert_eq!(parse_mem_bytes("0"), Some(0));
        assert_eq!(parse_mem_bytes("-5"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_rows_are_explicitly_unsupported_with_reasons() {
        let collector = GpuCollector::new();
        let caps = collector.capabilities();
        assert_eq!(caps.len(), 2);
        let utilization = caps
            .iter()
            .find(|c| c.metric_id == families::GPU_ENGINE_UTILIZATION)
            .expect("utilization row");
        assert_eq!(utilization.state, Availability::Unsupported);
        assert_eq!(
            utilization.reason_code.as_deref(),
            Some(reasons::NO_PUBLIC_READABLE_GPU)
        );
        let vram = caps
            .iter()
            .find(|c| c.metric_id == families::GPU_MEMORY_DEDICATED_USED)
            .expect("vram row");
        assert_eq!(vram.state, Availability::Unsupported);
        // Apple Silicon: the unified-memory reason is the honest answer.
        assert_eq!(vram.reason_code.as_deref(), Some(reasons::UNIFIED_MEMORY));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sample_emits_nothing() {
        let mut collector = GpuCollector::new();
        let readings = collector.sample(&FakeClock::default()).expect("ok");
        assert!(readings.is_empty(), "no fabricated GPU values on macOS");
    }
}
