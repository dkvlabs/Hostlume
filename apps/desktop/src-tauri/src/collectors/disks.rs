//! Volumes collector (FX-004): mount points, filesystem, capacity and
//! available bytes, removable/read-only flags. A full re-enumeration each
//! tick handles mount/unmount naturally: entities that disappear simply
//! stop being emitted. Eject remains a guarded Phase 04 enhancement.

use monitor_core::{
    families, Availability, Clock, Collector, CollectorError, MetricCapability, MetricReading,
    MetricUnit, MetricValue, PollGroup,
};
use sysinfo::Disks;

use super::reading;
pub const COLLECTOR_ID: &str = "collector-disks";

#[derive(Debug, Default)]
pub struct DisksCollector;

impl DisksCollector {
    pub fn new() -> Self {
        Self
    }

    fn volume_entity(mount: &std::path::Path) -> String {
        format!("vol:{}", mount.to_string_lossy())
    }
}

impl Collector for DisksCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        PollGroup::Slow
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        [
            families::DISK_VOLUME_CAPACITY,
            families::DISK_VOLUME_AVAILABLE,
            families::DISK_VOLUME_FILESYSTEM,
            families::DISK_VOLUME_REMOVABLE,
            families::DISK_VOLUME_READ_ONLY,
        ]
        .iter()
        .map(|id| MetricCapability {
            metric_id: (*id).to_string(),
            state: Availability::Available,
            source: COLLECTOR_ID.to_string(),
            explanation: Some(
                "Logical filesystem capacity; allocated size and reclaimable bytes differ (contract §4)."
                    .to_string(),
            ),
            reason_code: None,
            remediation: None,
            human_message_key: Some("capabilities.diskSemantics".to_string()),
            precision: None,
            freshness_budget_ms: Some(120_000),
            permissions_required: Vec::new(),
            optional_helper_required: false,
        })
        .collect()
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        let disks = Disks::new_with_refreshed_list();
        let mut readings = Vec::new();
        for disk in disks.list() {
            let entity = Self::volume_entity(disk.mount_point());
            let vol = |metric_id: &str, value: MetricValue, unit: MetricUnit| {
                reading(
                    metric_id,
                    &entity,
                    Some(value),
                    unit,
                    Availability::Available,
                    COLLECTOR_ID,
                    clock,
                    None,
                    None,
                )
            };
            readings.push(vol(
                families::DISK_VOLUME_CAPACITY,
                MetricValue::Number(disk.total_space() as f64),
                MetricUnit::Bytes,
            ));
            readings.push(vol(
                families::DISK_VOLUME_AVAILABLE,
                MetricValue::Number(disk.available_space() as f64),
                MetricUnit::Bytes,
            ));
            readings.push(vol(
                families::DISK_VOLUME_FILESYSTEM,
                MetricValue::Text(disk.file_system().to_string_lossy().to_string()),
                MetricUnit::Text,
            ));
            readings.push(vol(
                families::DISK_VOLUME_REMOVABLE,
                MetricValue::Flag(disk.is_removable()),
                MetricUnit::Boolean,
            ));
            readings.push(vol(
                families::DISK_VOLUME_READ_ONLY,
                MetricValue::Flag(disk.is_read_only()),
                MetricUnit::Boolean,
            ));
        }
        Ok(readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::SystemClock;

    #[test]
    fn enumerates_at_least_one_volume_with_positive_capacity() {
        let mut collector = DisksCollector::new();
        let readings = collector.sample(&SystemClock).expect("disk sample");
        let count_matching =
            |metric_id: &str| readings.iter().filter(|r| r.metric_id == metric_id).count();
        let capacity_count = count_matching(families::DISK_VOLUME_CAPACITY);
        assert!(
            capacity_count > 0,
            "a booted machine has at least one mounted volume"
        );
        for capacity in readings
            .iter()
            .filter(|r| r.metric_id == families::DISK_VOLUME_CAPACITY)
        {
            match capacity.value.clone() {
                Some(MetricValue::Number(bytes)) => assert!(bytes > 0.0),
                other => panic!("capacity wrong shape: {other:?}"),
            }
            assert!(capacity.entity_id.starts_with("vol:"));
        }
        // Every volume carries its filesystem label and flags.
        assert_eq!(
            count_matching(families::DISK_VOLUME_FILESYSTEM),
            capacity_count
        );
        assert_eq!(
            count_matching(families::DISK_VOLUME_REMOVABLE),
            capacity_count
        );
        assert_eq!(
            count_matching(families::DISK_VOLUME_READ_ONLY),
            capacity_count
        );
    }
}
