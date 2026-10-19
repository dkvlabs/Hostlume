//! CPU collector (FX-002): total and per-core usage plus per-core
//! frequency. First tick is an explicit `WARMUP` — sysinfo computes usage
//! from the delta between refreshes, so sample one is not a measurement
//! (contract §4: never present it as live).

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading,
    MetricUnit, MetricValue, PollGroup,
};
use sysinfo::System;

use super::{reading, HOST_ENTITY};
pub const COLLECTOR_ID: &str = "collector-cpu";

#[derive(Debug, Default)]
pub struct CpuCollector {
    system: System,
    warmed_up: bool,
}

impl CpuCollector {
    pub fn new() -> Self {
        Self::default()
    }

    fn warmup_reading(&self, clock: &dyn Clock, metric_id: &str, entity_id: &str) -> MetricReading {
        reading(
            metric_id,
            entity_id,
            None,
            MetricUnit::Percent,
            Availability::TemporarilyUnavailable,
            COLLECTOR_ID,
            clock,
            Some(monitor_core::reasons::WARMUP),
            Some("states.collecting"),
        )
    }

    fn family_capabilities(_core_count: usize) -> Vec<MetricCapability> {
        // Capabilities stay at family granularity; per-core entities appear
        // in readings.
        [
            families::CPU_TOTAL_USAGE,
            families::CPU_CORE_USAGE,
            families::CPU_CORE_FREQUENCY,
            // Per-process CPU is implemented behind the process page
            // (pid + start-time identity, phase 03) rather than the polled
            // snapshot; the capability report must still cover the family.
            families::CPU_PROCESS_USAGE_CORES,
            families::CPU_PROCESS_USAGE_HOST,
        ]
        .iter()
        .map(|id| MetricCapability {
            metric_id: (*id).to_string(),
            state: Availability::Available,
            source: COLLECTOR_ID.to_string(),
            explanation: None,
            reason_code: None,
            remediation: None,
            human_message_key: None,
            precision: Some(0),
            freshness_budget_ms: Some(9_000),
            permissions_required: Vec::new(),
            optional_helper_required: false,
        })
        .collect()
    }
}

impl Collector for CpuCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Fast
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        Self::family_capabilities(self.system.cpus().len())
    }

    fn reset(&mut self) {
        // Re-enter warm-up so the resumed baseline is fresh, not averaged
        // across the paused interval.
        self.warmed_up = false;
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        self.system.refresh_cpu_all();
        if !self.warmed_up {
            self.warmed_up = true;
            let mut readings =
                vec![self.warmup_reading(clock, families::CPU_TOTAL_USAGE, HOST_ENTITY)];
            for index in 0..self.system.cpus().len() {
                readings.push(self.warmup_reading(
                    clock,
                    families::CPU_CORE_USAGE,
                    &format!("core:{index}"),
                ));
            }
            return Ok(readings);
        }
        let mut readings = vec![reading(
            families::CPU_TOTAL_USAGE,
            HOST_ENTITY,
            Some(MetricValue::Number(
                self.system.global_cpu_usage().clamp(0.0, 100.0) as f64,
            )),
            MetricUnit::Percent,
            Availability::Available,
            COLLECTOR_ID,
            clock,
            None,
            None,
        )];
        for (index, cpu) in self.system.cpus().iter().enumerate() {
            readings.push(reading(
                families::CPU_CORE_USAGE,
                &format!("core:{index}"),
                Some(MetricValue::Number(cpu.cpu_usage().clamp(0.0, 100.0) as f64)),
                MetricUnit::Percent,
                Availability::Available,
                COLLECTOR_ID,
                clock,
                None,
                None,
            ));
            readings.push(reading(
                families::CPU_CORE_FREQUENCY,
                &format!("core:{index}"),
                Some(MetricValue::Number(cpu.frequency() as f64 * 1_000_000.0)),
                MetricUnit::Hertz,
                Availability::Available,
                COLLECTOR_ID,
                clock,
                None,
                None,
            ));
        }
        Ok(readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::SystemClock;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    #[cfg(debug_assertions)]
    fn first_sample_warms_up_second_is_valid_percent() {
        let mut collector = CpuCollector::new();
        let first = collector.sample(&SystemClock).expect("first sample");
        let total = first
            .iter()
            .find(|r| r.metric_id == families::CPU_TOTAL_USAGE)
            .expect("total usage reading");
        assert_eq!(total.availability, Availability::TemporarilyUnavailable);
        assert_eq!(total.value, None, "warm-up must carry null");
        assert_eq!(
            total.reason_code.as_deref(),
            Some(monitor_core::reasons::WARMUP)
        );

        // sysinfo needs MINIMUM_CPU_UPDATE_INTERVAL (200 ms on Apple) between
        // refreshes for a valid delta; 400 ms is comfortably above it.
        sleep(Duration::from_millis(400));
        let second = collector.sample(&SystemClock).expect("second sample");
        let total = second
            .iter()
            .find(|r| r.metric_id == families::CPU_TOTAL_USAGE)
            .expect("total usage reading");
        assert_eq!(total.availability, Availability::Available);
        match total.value.clone() {
            Some(MetricValue::Number(percent)) => {
                assert!(
                    (0.0..=100.0).contains(&percent),
                    "percent {percent} out of range"
                );
            }
            other => panic!("second sample must carry a number, got {other:?}"),
        }
        // Per-core usage present for every core, all in 0..=100.
        let core_usages: Vec<f64> = second
            .iter()
            .filter(|r| r.metric_id == families::CPU_CORE_USAGE)
            .filter_map(|r| match r.value.clone() {
                Some(MetricValue::Number(n)) => Some(n),
                _ => None,
            })
            .collect();
        assert!(!core_usages.is_empty());
        assert!(core_usages.iter().all(|v| (0.0..=100.0).contains(v)));
    }
}
