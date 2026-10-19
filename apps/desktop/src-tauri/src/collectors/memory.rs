//! Memory collector (FX-003): RAM total/available/used and swap.
//! `memory.physical.used` is derived as `total - available` (sysinfo
//! semantics: "available" is the OS-specific estimate of memory freeable
//! without swapping — contract §4 requires documenting the provider).
//! Swap values are real measurements: a machine with no swap currently in
//! use reports genuine zeros, which is a measurement, not a sentinel.

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading,
    MetricUnit, MetricValue, PollGroup,
};
use sysinfo::System;

use super::{reading, HOST_ENTITY};
pub const COLLECTOR_ID: &str = "collector-memory";

#[derive(Debug, Default)]
pub struct MemoryCollector {
    system: System,
}

impl MemoryCollector {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Collector for MemoryCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Medium
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        [
            families::MEMORY_PHYSICAL_TOTAL,
            families::MEMORY_PHYSICAL_USED,
            families::MEMORY_AVAILABLE,
            families::MEMORY_SWAP_USED,
            families::MEMORY_SWAP_TOTAL,
        ]
        .iter()
        .map(|id| MetricCapability {
            metric_id: (*id).to_string(),
            state: Availability::Available,
            source: COLLECTOR_ID.to_string(),
            explanation: Some(
                "used = total - available; available is the OS-specific freeable estimate."
                    .to_string(),
            ),
            reason_code: None,
            remediation: None,
            human_message_key: Some("capabilities.memorySemantics".to_string()),
            precision: Some(0),
            freshness_budget_ms: Some(30_000),
            permissions_required: Vec::new(),
            optional_helper_required: false,
        })
        .collect()
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        self.system.refresh_memory();
        let total = self.system.total_memory();
        let available = self.system.available_memory();
        let used = total.saturating_sub(available);
        let swap_total = self.system.total_swap();
        let swap_used = swap_total.saturating_sub(self.system.free_swap());
        let mem = |metric_id: &str, value: u64| {
            reading(
                metric_id,
                HOST_ENTITY,
                Some(MetricValue::Number(value as f64)),
                MetricUnit::Bytes,
                Availability::Available,
                COLLECTOR_ID,
                clock,
                None,
                None,
            )
        };
        Ok(vec![
            mem(families::MEMORY_PHYSICAL_TOTAL, total),
            mem(families::MEMORY_PHYSICAL_USED, used),
            mem(families::MEMORY_AVAILABLE, available),
            mem(families::MEMORY_SWAP_TOTAL, swap_total),
            mem(families::MEMORY_SWAP_USED, swap_used),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::SystemClock;

    #[test]
    fn memory_values_are_sane_and_never_inverted() {
        let mut collector = MemoryCollector::new();
        let readings = collector.sample(&SystemClock).expect("memory sample");
        let value_of = |id: &str| match readings.iter().find(|r| r.metric_id == id) {
            Some(r) => match r.value.clone() {
                Some(MetricValue::Number(n)) => n,
                other => panic!("{id} wrong shape: {other:?}"),
            },
            None => panic!("{id} missing"),
        };
        let total = value_of(families::MEMORY_PHYSICAL_TOTAL);
        let available = value_of(families::MEMORY_AVAILABLE);
        let used = value_of(families::MEMORY_PHYSICAL_USED);
        assert!(total > 0.0, "a booted machine has RAM");
        assert!(used >= 0.0 && used <= total);
        assert!(available <= total);
        let swap_total = value_of(families::MEMORY_SWAP_TOTAL);
        let swap_used = value_of(families::MEMORY_SWAP_USED);
        assert!(swap_used <= swap_total, "swap used must never exceed total");
    }
}
