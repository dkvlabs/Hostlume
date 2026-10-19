//! Portable sysinfo-based collectors (Phases 02–04) and the Phase 05
//! hardware-conditional collectors (sensors, battery, Bluetooth, GPU).
//!
//! Layering per ADR-0004/ADR-0006: sysinfo is the portable baseline and
//! lives in the app runtime; OS-specific probing (IOKit battery access,
//! hwmon fans, DRM GPU counters, profiler/UPower subprocess probes) is
//! cfg-split inside these collectors with the pure parsing separated from
//! platform I/O. Every reading carries its `source` (= collector id), an
//! availability state and, when absent, a null value — collectors never
//! fabricate numbers.

pub mod battery;
pub mod bluetooth;
pub mod cpu;
pub mod disk_io;
pub mod disks;
pub mod gpu;
pub mod host;
pub mod memory;
pub mod network;
pub mod sensors;

use monitor_core::{Availability, Clock, MetricReading, MetricUnit, MetricValue};

pub use battery::BatteryCollector;
pub use bluetooth::BluetoothCollector;
pub use cpu::CpuCollector;
pub use disk_io::DiskIoCollector;
pub use disks::DisksCollector;
pub use gpu::GpuCollector;
pub use host::HostCollector;
pub use memory::MemoryCollector;
pub use network::NetworkCollector;
pub use sensors::SensorsCollector;

/// Shared reading builder so every emitted reading has identical
/// provenance fields; the scheduler stamps the enclosing sequence.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reading(
    metric_id: &str,
    entity_id: &str,
    value: Option<MetricValue>,
    unit: MetricUnit,
    availability: Availability,
    source: &'static str,
    clock: &dyn Clock,
    reason_code: Option<&str>,
    human_message_key: Option<&str>,
) -> MetricReading {
    MetricReading {
        metric_id: metric_id.to_string(),
        entity_id: entity_id.to_string(),
        value,
        unit,
        availability,
        source: source.to_string(),
        collected_at_utc: clock.now_utc_iso8601(),
        monotonic_sequence: 0, // stamped by the scheduler on assembly
        reason_code: reason_code.map(str::to_string),
        human_message_key: human_message_key.map(str::to_string),
        precision: None,
    }
}

/// Standard entity key for host-scoped metrics.
pub(crate) const HOST_ENTITY: &str = "host";

#[cfg(test)]
pub(crate) mod test_support {
    //! Controllable clock for deterministic collector tests. Test-only.

    use monitor_core::Clock;
    use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

    #[derive(Debug, Default)]
    pub struct FakeClock {
        pub epoch_ms: AtomicI64,
        pub mono_ms: AtomicU64,
    }

    impl FakeClock {
        pub fn advance(&self, wall_ms: i64, mono_ms: u64) {
            self.epoch_ms.fetch_add(wall_ms, Ordering::Relaxed);
            self.mono_ms.fetch_add(mono_ms, Ordering::Relaxed);
        }
    }

    impl Clock for FakeClock {
        fn now_epoch_ms(&self) -> i64 {
            self.epoch_ms.load(Ordering::Relaxed)
        }
        fn monotonic_ns(&self) -> u64 {
            self.mono_ms.load(Ordering::Relaxed) * 1_000_000
        }
    }
}
