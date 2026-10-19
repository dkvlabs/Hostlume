//! Host identity collector (FX-001): hostname, OS, architecture, uptime,
//! CPU brand and core counts. Slow group — these facts change rarely.

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading,
    MetricUnit, MetricValue, PollGroup, SystemClock,
};
use sysinfo::System;

use super::{reading, HOST_ENTITY};
pub const COLLECTOR_ID: &str = "collector-host";

#[derive(Debug, Default)]
pub struct HostCollector {
    /// CPU brand needs a refreshed CPU list; it is static afterwards.
    cpu_list_ready: bool,
    system: System,
}

impl HostCollector {
    pub fn new() -> Self {
        Self::default()
    }

    fn family_capabilities() -> Vec<MetricCapability> {
        [
            families::HOST_HOSTNAME,
            families::HOST_OS_VERSION,
            families::HOST_OS_KERNEL,
            families::HOST_ARCH,
            families::HOST_UPTIME,
            families::HOST_CPU_BRAND,
            families::HOST_CPU_CORES_LOGICAL,
            families::HOST_CPU_CORES_PHYSICAL,
        ]
        .iter()
        .map(|id| MetricCapability {
            metric_id: (*id).to_string(),
            state: monitor_core::Availability::Available,
            source: COLLECTOR_ID.to_string(),
            explanation: None,
            reason_code: None,
            remediation: None,
            human_message_key: None,
            precision: None,
            freshness_budget_ms: Some(120_000),
            permissions_required: Vec::new(),
            optional_helper_required: false,
        })
        .collect()
    }
}

impl Collector for HostCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Slow
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        Self::family_capabilities()
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        if !self.cpu_list_ready {
            self.system.refresh_cpu_all();
            self.cpu_list_ready = true;
        }
        let host = |metric_id: &str, value: Option<MetricValue>, unit: MetricUnit| {
            reading(
                metric_id,
                HOST_ENTITY,
                value,
                unit,
                Availability::Available,
                COLLECTOR_ID,
                clock,
                None,
                None,
            )
        };
        let physical_cores = System::physical_core_count();
        let mut readings = vec![
            host(
                families::HOST_HOSTNAME,
                System::host_name().map(MetricValue::Text),
                MetricUnit::Text,
            ),
            host(
                families::HOST_OS_VERSION,
                System::long_os_version().map(MetricValue::Text),
                MetricUnit::Text,
            ),
            host(
                families::HOST_OS_KERNEL,
                System::kernel_version().map(MetricValue::Text),
                MetricUnit::Text,
            ),
            host(
                families::HOST_ARCH,
                Some(MetricValue::Text(System::cpu_arch())),
                MetricUnit::Text,
            ),
            host(
                families::HOST_UPTIME,
                Some(MetricValue::Number(System::uptime() as f64)),
                MetricUnit::Seconds,
            ),
            host(
                families::HOST_CPU_BRAND,
                self.system
                    .cpus()
                    .first()
                    .map(|cpu| MetricValue::Text(cpu.brand().trim().to_string())),
                MetricUnit::Text,
            ),
            host(
                families::HOST_CPU_CORES_LOGICAL,
                Some(MetricValue::Number(self.system.cpus().len() as f64)),
                MetricUnit::Count,
            ),
        ];
        readings.push(match physical_cores {
            Some(cores) => host(
                families::HOST_CPU_CORES_PHYSICAL,
                Some(MetricValue::Number(cores as f64)),
                MetricUnit::Count,
            ),
            None => reading(
                families::HOST_CPU_CORES_PHYSICAL,
                HOST_ENTITY,
                None,
                MetricUnit::Count,
                Availability::Unsupported,
                COLLECTOR_ID,
                &SystemClock,
                None,
                Some("states.unsupported"),
            ),
        });
        Ok(readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::{Availability, SystemClock};

    #[test]
    fn emits_host_facts_on_native_host() {
        let mut collector = HostCollector::new();
        let readings = collector.sample(&SystemClock).expect("host sample");
        let by_id = |id: &str| {
            readings
                .iter()
                .find(|r| r.metric_id == id)
                .unwrap_or_else(|| panic!("missing {id}"))
        };
        assert_eq!(
            by_id(families::HOST_ARCH).availability,
            Availability::Available
        );
        assert!(matches!(
            by_id(families::HOST_ARCH).value,
            Some(MetricValue::Text(_))
        ));
        // Uptime is a real, positive measurement on a booted machine.
        match by_id(families::HOST_UPTIME).value.clone() {
            Some(MetricValue::Number(seconds)) => assert!(seconds > 0.0),
            other => panic!("uptime must be a positive number, got {other:?}"),
        }
        // Logical cores exist and physical cores are reported or explicitly
        // unsupported on this platform — never missing and never zero.
        match by_id(families::HOST_CPU_CORES_LOGICAL).value.clone() {
            Some(MetricValue::Number(n)) => assert!(n >= 1.0),
            other => panic!("logical cores missing: {other:?}"),
        }
        match by_id(families::HOST_CPU_CORES_PHYSICAL).value.clone() {
            Some(MetricValue::Number(n)) => assert!(n >= 1.0),
            None => assert_eq!(
                by_id(families::HOST_CPU_CORES_PHYSICAL).availability,
                Availability::Unsupported
            ),
            other => panic!("physical cores wrong shape: {other:?}"),
        }
    }
}
