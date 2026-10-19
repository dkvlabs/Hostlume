//! Network collector (FX-006): per-interface cumulative counters, rates on
//! our own monotonic clock, address lists, and stable entity keys
//! (`if:<name>:<generation>`) that survive renames/recreations.
//!
//! Sleep/suspend handling: if wall time advanced substantially more than
//! monotonic time between ticks, every rate tracker is reset and this
//! cycle's rates are reported `SLEEP_GAP` instead of a bogus spike.

use std::collections::HashMap;

use monitor_core::{
    families, reasons, sampling::RateOutcome, sampling::RateTracker, Availability, Clock,
    Collector, CollectorError, MetricCapability, MetricReading, MetricUnit, MetricValue, PollGroup,
};
use sysinfo::Networks;

use super::reading;
pub const COLLECTOR_ID: &str = "collector-network";

/// Wall-vs-monotonic skew tolerated between ticks before declaring a sleep
/// gap (ms). Scheduling jitter is far below this; real sleep is not.
const SLEEP_GAP_TOLERANCE_MS: i64 = 5_000;

#[derive(Debug)]
struct InterfaceState {
    generation: u64,
    rx: RateTracker,
    tx: RateTracker,
}

#[derive(Debug, Default)]
pub struct NetworkCollector {
    networks: Networks,
    states: HashMap<String, InterfaceState>,
    next_generation: u64,
    last_wall_ms: Option<i64>,
    last_mono_ms: Option<u64>,
}

impl NetworkCollector {
    pub fn new() -> Self {
        Self::default()
    }

    fn entity_id(name: &str, generation: u64) -> String {
        format!("if:{name}:{generation}")
    }

    fn rate_availability(
        outcome: RateOutcome,
    ) -> (Availability, Option<&'static str>, Option<f64>) {
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
            RateOutcome::Rate(value) => (Availability::Available, None, Some(value)),
        }
    }
}

impl Collector for NetworkCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Fast
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        [
            families::NET_INTERFACE_RX_TOTAL,
            families::NET_INTERFACE_TX_TOTAL,
            families::NET_INTERFACE_RX_RATE,
            families::NET_INTERFACE_TX_RATE,
            families::NET_INTERFACE_IPV4,
            families::NET_INTERFACE_IPV6,
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

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        self.networks.refresh(true);
        let wall_ms = clock.now_epoch_ms();
        let mono_ms = clock.monotonic_ns() / 1_000_000;
        // Sleep gap detection: wall clock jumped far beyond monotonic time.
        let sleep_gap = match (self.last_wall_ms, self.last_mono_ms) {
            (Some(prev_wall), Some(prev_mono)) => {
                (wall_ms - prev_wall).saturating_sub((mono_ms - prev_mono) as i64)
                    > SLEEP_GAP_TOLERANCE_MS
            }
            _ => false,
        };
        self.last_wall_ms = Some(wall_ms);
        self.last_mono_ms = Some(mono_ms);

        let current_names: Vec<String> = self.networks.list().keys().cloned().collect();
        let mut readings = Vec::new();
        for name in &current_names {
            let state = self.states.entry(name.clone()).or_insert_with(|| {
                let generation = self.next_generation;
                self.next_generation += 1;
                InterfaceState {
                    generation,
                    rx: RateTracker::new(),
                    tx: RateTracker::new(),
                }
            });
            let entity = Self::entity_id(name, state.generation);
            let Some(data) = self.networks.list().get(name) else {
                continue;
            };
            if sleep_gap {
                state.rx.reset();
                state.tx.reset();
            }
            let rx_total = data.total_received();
            let tx_total = data.total_transmitted();
            let rx_outcome = state.rx.record(rx_total, mono_ms);
            let tx_outcome = state.tx.record(tx_total, mono_ms);

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
                // After a sleep gap every tracker was reset, so the outcome
                // is a Warmup — but the truthful reason for the missing rate
                // this cycle is the gap itself, not warm-up.
                let (availability, reason, value) = if sleep_gap {
                    (
                        Availability::TemporarilyUnavailable,
                        Some(reasons::SLEEP_GAP),
                        None,
                    )
                } else {
                    Self::rate_availability(outcome)
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

            readings.push(counter(families::NET_INTERFACE_RX_TOTAL, rx_total));
            readings.push(counter(families::NET_INTERFACE_TX_TOTAL, tx_total));
            readings.push(rate(families::NET_INTERFACE_RX_RATE, rx_outcome));
            readings.push(rate(families::NET_INTERFACE_TX_RATE, tx_outcome));

            let mut ipv4 = Vec::new();
            let mut ipv6 = Vec::new();
            for network in data.ip_networks() {
                match network.addr {
                    std::net::IpAddr::V4(_) => ipv4.push(network.to_string()),
                    std::net::IpAddr::V6(_) => ipv6.push(network.to_string()),
                }
            }
            if !ipv4.is_empty() {
                readings.push(reading(
                    families::NET_INTERFACE_IPV4,
                    &entity,
                    Some(MetricValue::Text(ipv4.join(", "))),
                    MetricUnit::Text,
                    Availability::Available,
                    COLLECTOR_ID,
                    clock,
                    None,
                    None,
                ));
            }
            if !ipv6.is_empty() {
                readings.push(reading(
                    families::NET_INTERFACE_IPV6,
                    &entity,
                    Some(MetricValue::Text(ipv6.join(", "))),
                    MetricUnit::Text,
                    Availability::Available,
                    COLLECTOR_ID,
                    clock,
                    None,
                    None,
                ));
            }
        }
        // Interfaces that vanished stop owning state; their entities simply
        // disappear from the next snapshot (no invented zeros).
        self.states.retain(|name, _| current_names.contains(name));
        Ok(readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::FakeClock;
    use monitor_core::SystemClock;

    #[test]
    fn enumerates_interfaces_with_counters_and_warmup_rates() {
        let mut collector = NetworkCollector::new();
        let first = collector.sample(&SystemClock).expect("first sample");
        assert!(
            !first.is_empty(),
            "a machine has at least a loopback interface"
        );
        let rx_total = first
            .iter()
            .find(|r| r.metric_id == families::NET_INTERFACE_RX_TOTAL)
            .expect("rx total");
        assert_eq!(rx_total.availability, Availability::Available);
        match rx_total.value.clone() {
            Some(MetricValue::Number(bytes)) => assert!(bytes >= 0.0),
            other => panic!("counter wrong shape: {other:?}"),
        }
        let rx_rate = first
            .iter()
            .find(|r| r.metric_id == families::NET_INTERFACE_RX_RATE)
            .expect("rx rate");
        assert_eq!(rx_rate.availability, Availability::TemporarilyUnavailable);
        assert_eq!(rx_rate.reason_code.as_deref(), Some(reasons::WARMUP));

        std::thread::sleep(std::time::Duration::from_millis(300));
        let second = collector.sample(&SystemClock).expect("second sample");
        let rx_rate = second
            .iter()
            .find(|r| r.metric_id == families::NET_INTERFACE_RX_RATE)
            .expect("rx rate");
        assert_eq!(rx_rate.availability, Availability::Available);
        match rx_rate.value.clone() {
            Some(MetricValue::Number(rate)) => assert!(rate >= 0.0, "rate must never be negative"),
            other => panic!("rate wrong shape: {other:?}"),
        }
    }

    #[test]
    fn wall_clock_jump_reports_sleep_gap_not_a_rate() {
        // Deterministic sleep-gap test: real interfaces for shape, fake
        // clock for timing so a 1h wall jump with 0 monotonic movement is
        // reproducible.
        let mut collector = NetworkCollector::new();
        let clock = FakeClock::default();
        let _ = collector.sample(&clock).expect("warmup");
        // Normal tick: 3 s wall, 3 s mono.
        clock.advance(3_000, 3_000);
        let _ = collector.sample(&clock).expect("normal tick");
        // Sleep: +3 s wall, +0 mono → skew 3000 < 5000 tolerance: no gap.
        clock.advance(3_000, 0);
        let no_gap = collector.sample(&clock).expect("small jump");
        assert!(no_gap
            .iter()
            .filter(|r| r.metric_id == families::NET_INTERFACE_RX_RATE)
            .all(|r| r.reason_code.as_deref() != Some(reasons::SLEEP_GAP)));
        // Real sleep: +1h wall, +0 mono.
        clock.advance(3_600_000, 0);
        let after_sleep = collector.sample(&clock).expect("sleep tick");
        let gap_rates = after_sleep
            .iter()
            .filter(|r| r.metric_id == families::NET_INTERFACE_RX_RATE)
            .count();
        assert!(gap_rates > 0, "expected interfaces to report");
        assert!(after_sleep
            .iter()
            .filter(|r| r.metric_id == families::NET_INTERFACE_RX_RATE)
            .all(|r| r.reason_code.as_deref() == Some(reasons::SLEEP_GAP) && r.value.is_none()));
    }
}
