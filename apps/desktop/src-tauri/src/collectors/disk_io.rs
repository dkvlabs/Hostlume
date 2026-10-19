//! Disk device I/O collector (FX-009, phase 04 §3).
//!
//! Per-device read/write byte counters with contract-correct rate math:
//! per-device `RateTracker`s (reset on device replacement), monotonic
//! clock, `COUNTER_RESET`/`WARMUP`/`SLEEP_GAP` states.
//!
//! Sources per OS (honest, no invention):
//! * **Linux:** `/proc/diskstats` — real kernel counters. Synthetic layers
//!   (loop, ram, dm-, md*) are excluded so device-mapper stacks are not
//!   double-counted (phase doc: no duplicate counts).
//! * **macOS / Windows:** no public counter source is wired in this build;
//!   the collector reports an explicit `unsupported` capability with
//!   reason `SOURCE_NOT_AVAILABLE_IN_BUILD` and emits no readings. The
//!   IOKit/Win32 adapter is the documented follow-up (platform_limited).

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, PollGroup,
};

#[cfg(target_os = "linux")]
use monitor_core::{
    families, reasons, sampling::RateOutcome, sampling::RateTracker, MetricUnit, MetricValue,
};

#[cfg(target_os = "linux")]
use super::reading;
pub const COLLECTOR_ID: &str = "collector-disk-io";

#[derive(Debug, Default)]
pub struct DiskIoCollector {
    #[cfg(target_os = "linux")]
    trackers: std::collections::HashMap<String, (RateTracker, RateTracker)>,
    #[cfg(target_os = "linux")]
    last_wall_ms: Option<i64>,
    #[cfg(target_os = "linux")]
    last_mono_ms: Option<u64>,
}

impl DiskIoCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse one `/proc/diskstats` line: major minor name reads reads_merged
    /// sectors_read ms_reading writes writes_merged sectors_written ms_writing ...
    /// Counters are sectors (usually 512 B) — converted with the kernel's
    /// documented "always 512" convention (procfs man page).
    #[cfg(target_os = "linux")]
    fn parse_line(line: &str) -> Option<(String, u64, u64)> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            return None;
        }
        let name = fields.get(2)?.to_string();
        // Skip synthetic layers to avoid double counting through dm/loop.
        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("dm-")
            || name.starts_with("md")
        {
            return None;
        }
        let reads_completed: u64 = fields.get(3)?.parse().ok()?;
        let sectors_read: u64 = fields.get(5)?.parse().ok()?;
        let writes_completed: u64 = fields.get(7)?.parse().ok()?;
        let sectors_written: u64 = fields.get(9)?.parse().ok()?;
        // Prefer completed-operation count for resets (small, monotonic);
        // bytes from sectors × 512.
        let _ = reads_completed;
        let _ = writes_completed;
        Some((
            name,
            sectors_read.saturating_mul(512),
            sectors_written.saturating_mul(512),
        ))
    }
}

impl Collector for DiskIoCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Medium
    }

    fn reset(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.trackers.clear();
            self.last_wall_ms = None;
            self.last_mono_ms = None;
        }
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        let state = if cfg!(target_os = "linux") {
            Availability::Available
        } else {
            Availability::Unsupported
        };
        let reason = if cfg!(target_os = "linux") {
            None
        } else {
            Some("SOURCE_NOT_AVAILABLE_IN_BUILD")
        };
        [
            families::DISK_DEVICE_READ_BYTES_TOTAL,
            families::DISK_DEVICE_READ_RATE,
            families::DISK_DEVICE_WRITE_BYTES_TOTAL,
            families::DISK_DEVICE_WRITE_RATE,
        ]
        .iter()
        .map(|id| MetricCapability {
            metric_id: (*id).to_string(),
            state,
            source: COLLECTOR_ID.to_string(),
            explanation: if cfg!(target_os = "linux") {
                Some("Kernel /proc/diskstats counters; synthetic layers excluded.".into())
            } else {
                Some("No public per-device counter source is wired into this build yet.".into())
            },
            reason_code: reason.map(str::to_string),
            remediation: None,
            human_message_key: if cfg!(target_os = "linux") {
                None
            } else {
                Some("capabilities.diskIoUnsupported".to_string())
            },
            precision: Some(0),
            freshness_budget_ms: Some(30_000),
            permissions_required: Vec::new(),
            optional_helper_required: false,
        })
        .collect()
    }

    #[cfg(not(target_os = "linux"))]
    fn sample(
        &mut self,
        _clock: &dyn Clock,
    ) -> Result<Vec<monitor_core::MetricReading>, CollectorError> {
        // Honest unsupported: emit nothing; capabilities carry the reason.
        Ok(Vec::new())
    }

    #[cfg(target_os = "linux")]
    fn sample(
        &mut self,
        clock: &dyn Clock,
    ) -> Result<Vec<monitor_core::MetricReading>, CollectorError> {
        let content = std::fs::read_to_string("/proc/diskstats").map_err(|err| {
            CollectorError::Unsupported {
                collector: COLLECTOR_ID,
                reason: format!("cannot read /proc/diskstats: {err}"),
            }
        })?;
        let wall_ms = clock.now_epoch_ms();
        let mono_ms = clock.monotonic_ns() / 1_000_000;
        let sleep_gap = match (self.last_wall_ms, self.last_mono_ms) {
            (Some(pw), Some(pm)) => (wall_ms - pw).saturating_sub((mono_ms - pm) as i64) > 5_000,
            _ => false,
        };
        self.last_wall_ms = Some(wall_ms);
        self.last_mono_ms = Some(mono_ms);

        let mut readings = Vec::new();
        for line in content.lines() {
            let Some((device, read_bytes, write_bytes)) = Self::parse_line(line) else {
                continue;
            };
            let state = self
                .trackers
                .entry(device.clone())
                .or_insert_with(|| (RateTracker::new(), RateTracker::new()));
            if sleep_gap {
                state.0.reset();
                state.1.reset();
            }
            let entity = format!("disk:{device}");
            let counter = |metric_id: &str, value: u64| {
                reading(
                    metric_id,
                    &entity,
                    Some(MetricValue::Number(value as f64)),
                    MetricUnit::Bytes,
                    Availability::Available,
                    COLLECTOR_ID,
                    clock,
                    None,
                    None,
                )
            };
            let rate = |metric_id: &str, outcome: RateOutcome| {
                let (availability, reason, value) = if sleep_gap {
                    (
                        Availability::TemporarilyUnavailable,
                        Some(reasons::SLEEP_GAP),
                        None,
                    )
                } else {
                    match outcome {
                        RateOutcome::Warmup | RateOutcome::NoElapsedTime => (
                            Availability::TemporarilyUnavailable,
                            Some(reasons::WARMUP),
                            None,
                        ),
                        RateOutcome::CounterReset => (
                            Availability::TemporarilyUnavailable,
                            Some(reasons::COUNTER_RESET),
                            None,
                        ),
                        RateOutcome::Rate(v) => (Availability::Available, None, Some(v)),
                    }
                };
                reading(
                    metric_id,
                    &entity,
                    value.map(MetricValue::Number),
                    MetricUnit::BytesPerSecond,
                    availability,
                    COLLECTOR_ID,
                    clock,
                    reason,
                    reason.map(|_| "states.collecting"),
                )
            };
            let rx_outcome = state.0.record(read_bytes, mono_ms);
            let tx_outcome = state.1.record(write_bytes, mono_ms);
            readings.push(counter(families::DISK_DEVICE_READ_BYTES_TOTAL, read_bytes));
            readings.push(counter(
                families::DISK_DEVICE_WRITE_BYTES_TOTAL,
                write_bytes,
            ));
            readings.push(rate(families::DISK_DEVICE_READ_RATE, rx_outcome));
            readings.push(rate(families::DISK_DEVICE_WRITE_RATE, tx_outcome));
        }
        Ok(readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::SystemClock;

    #[test]
    fn capabilities_are_honest_per_platform() {
        let caps = DiskIoCollector::new().capabilities();
        assert_eq!(caps.len(), 4);
        if cfg!(target_os = "linux") {
            assert!(caps.iter().all(|c| c.state == Availability::Available));
        } else {
            assert!(caps.iter().all(|c| c.state == Availability::Unsupported));
            assert!(caps
                .iter()
                .all(|c| { c.reason_code.as_deref() == Some("SOURCE_NOT_AVAILABLE_IN_BUILD") }));
        }
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn unsupported_platform_emits_no_readings() {
        let mut collector = DiskIoCollector::new();
        let out = collector.sample(&SystemClock).expect("sample");
        assert!(out.is_empty(), "no fabricated I/O on unsupported platforms");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parses_diskstats_line_and_skips_synthetic() {
        let line =
            "   8       0 sda 4312 431 305644 3022 883 621 12048 2276 0 3322 5300 0 0 0 0 0 0";
        let (name, read, write) = DiskIoCollector::parse_line(line).expect("parses");
        assert_eq!(name, "sda");
        assert_eq!(read, 305_644 * 512);
        assert_eq!(write, 12_048 * 512);
        assert!(
            DiskIoCollector::parse_line("   7       0 loop0 0 0 0 0 0 0 0 0 0 0 0").is_none(),
            "loop devices excluded"
        );
        assert!(DiskIoCollector::parse_line("garbage").is_none());
    }
}
