//! Stable metric identifiers (contract §3). These strings are API: they may
//! gain families, but existing ids must never change meaning.

pub const CPU_TOTAL_USAGE: &str = "cpu.total.usage";
pub const CPU_CORE_USAGE: &str = "cpu.core.usage";
pub const CPU_PROCESS_USAGE_CORES: &str = "cpu.process.usage_cores";
pub const CPU_PROCESS_USAGE_HOST: &str = "cpu.process.usage_host";
pub const MEMORY_PHYSICAL_TOTAL: &str = "memory.physical.total";
pub const MEMORY_AVAILABLE: &str = "memory.available";
pub const MEMORY_SWAP_USED: &str = "memory.swap.used";
pub const DISK_VOLUME_CAPACITY: &str = "disk.volume.capacity";
pub const DISK_VOLUME_AVAILABLE: &str = "disk.volume.available";
pub const DISK_DEVICE_READ_BYTES_TOTAL: &str = "disk.device.read_bytes_total";
pub const DISK_DEVICE_READ_RATE: &str = "disk.device.read_rate";
pub const NET_INTERFACE_RX_TOTAL: &str = "net.interface.rx_total";
pub const NET_INTERFACE_RX_RATE: &str = "net.interface.rx_rate";
pub const GPU_ENGINE_UTILIZATION: &str = "gpu.engine.utilization";
pub const GPU_MEMORY_DEDICATED_USED: &str = "gpu.memory.dedicated_used";
pub const SENSOR_TEMPERATURE: &str = "sensor.temperature";
pub const SENSOR_FAN_SPEED: &str = "sensor.fan.speed";
pub const BATTERY_CHARGE: &str = "battery.charge";
pub const BATTERY_CYCLES: &str = "battery.cycles";
pub const BLUETOOTH_DEVICE_CHARGE: &str = "bluetooth.device.charge";

/// Every metric family the product roadmap covers. Platform collectors
/// report capabilities against this list so "we can't measure X" is always
/// an explicit row, never an absence.
pub const ALL_FAMILIES: &[&str] = &[
    CPU_TOTAL_USAGE,
    CPU_CORE_USAGE,
    CPU_PROCESS_USAGE_CORES,
    CPU_PROCESS_USAGE_HOST,
    MEMORY_PHYSICAL_TOTAL,
    MEMORY_AVAILABLE,
    MEMORY_SWAP_USED,
    DISK_VOLUME_CAPACITY,
    DISK_VOLUME_AVAILABLE,
    DISK_DEVICE_READ_BYTES_TOTAL,
    DISK_DEVICE_READ_RATE,
    NET_INTERFACE_RX_TOTAL,
    NET_INTERFACE_RX_RATE,
    GPU_ENGINE_UTILIZATION,
    GPU_MEMORY_DEDICATED_USED,
    SENSOR_TEMPERATURE,
    SENSOR_FAN_SPEED,
    BATTERY_CHARGE,
    BATTERY_CYCLES,
    BLUETOOTH_DEVICE_CHARGE,
];
