//! Stable metric identifiers (contract §3 plus the host/static families
//! added in Phase 02). These strings are API: they may gain families, but
//! existing ids must never change meaning.

// Host identity / static system facts (FX-001).
pub const HOST_HOSTNAME: &str = "host.hostname";
pub const HOST_OS_VERSION: &str = "host.os.version";
pub const HOST_OS_KERNEL: &str = "host.os.kernel";
pub const HOST_ARCH: &str = "host.arch";
pub const HOST_UPTIME: &str = "host.uptime";
pub const HOST_CPU_BRAND: &str = "host.cpu.brand";
pub const HOST_CPU_CORES_LOGICAL: &str = "host.cpu.cores.logical";
pub const HOST_CPU_CORES_PHYSICAL: &str = "host.cpu.cores.physical";

// CPU (FX-002).
pub const CPU_TOTAL_USAGE: &str = "cpu.total.usage";
pub const CPU_CORE_USAGE: &str = "cpu.core.usage";
pub const CPU_CORE_FREQUENCY: &str = "cpu.core.frequency";
pub const CPU_PROCESS_USAGE_CORES: &str = "cpu.process.usage_cores";
pub const CPU_PROCESS_USAGE_HOST: &str = "cpu.process.usage_host";

// Memory (FX-003).
pub const MEMORY_PHYSICAL_TOTAL: &str = "memory.physical.total";
pub const MEMORY_PHYSICAL_USED: &str = "memory.physical.used";
pub const MEMORY_AVAILABLE: &str = "memory.available";
pub const MEMORY_SWAP_USED: &str = "memory.swap.used";
pub const MEMORY_SWAP_TOTAL: &str = "memory.swap.total";

// Volumes (FX-004).
pub const DISK_VOLUME_CAPACITY: &str = "disk.volume.capacity";
pub const DISK_VOLUME_AVAILABLE: &str = "disk.volume.available";
pub const DISK_VOLUME_FILESYSTEM: &str = "disk.volume.filesystem";
pub const DISK_VOLUME_REMOVABLE: &str = "disk.volume.removable";
pub const DISK_VOLUME_READ_ONLY: &str = "disk.volume.read_only";
pub const DISK_DEVICE_READ_BYTES_TOTAL: &str = "disk.device.read_bytes_total";
pub const DISK_DEVICE_READ_RATE: &str = "disk.device.read_rate";
pub const DISK_DEVICE_WRITE_BYTES_TOTAL: &str = "disk.device.write_bytes_total";
pub const DISK_DEVICE_WRITE_RATE: &str = "disk.device.write_rate";

// Network (FX-006).
pub const NET_INTERFACE_RX_TOTAL: &str = "net.interface.rx_total";
pub const NET_INTERFACE_TX_TOTAL: &str = "net.interface.tx_total";
pub const NET_INTERFACE_RX_RATE: &str = "net.interface.rx_rate";
pub const NET_INTERFACE_TX_RATE: &str = "net.interface.tx_rate";
pub const NET_INTERFACE_IPV4: &str = "net.interface.ipv4";
pub const NET_INTERFACE_IPV6: &str = "net.interface.ipv6";

// Conditional hardware families (Phases 04/05).
pub const GPU_ENGINE_UTILIZATION: &str = "gpu.engine.utilization";
pub const GPU_MEMORY_DEDICATED_USED: &str = "gpu.memory.dedicated_used";
pub const SENSOR_TEMPERATURE: &str = "sensor.temperature";
pub const SENSOR_FAN_SPEED: &str = "sensor.fan.speed";
pub const BATTERY_CHARGE: &str = "battery.charge";
pub const BATTERY_CYCLES: &str = "battery.cycles";
pub const BLUETOOTH_DEVICE_CHARGE: &str = "bluetooth.device.charge";

// Phase 05 additions: battery detail and Bluetooth device identity. These
// ids may gain families, but existing ids must never change meaning.
pub const BATTERY_STATUS: &str = "battery.status";
pub const BATTERY_HEALTH: &str = "battery.health";
pub const BATTERY_TIME_REMAINING: &str = "battery.time_remaining";
pub const BATTERY_POWER_DRAW: &str = "battery.power_draw";
pub const BLUETOOTH_DEVICE_NAME: &str = "bluetooth.device.name";

/// Every metric family the product roadmap covers. Platform collectors
/// report capabilities against this list so "we can't measure X" is always
/// an explicit row, never an absence.
pub const ALL_FAMILIES: &[&str] = &[
    HOST_HOSTNAME,
    HOST_OS_VERSION,
    HOST_OS_KERNEL,
    HOST_ARCH,
    HOST_UPTIME,
    HOST_CPU_BRAND,
    HOST_CPU_CORES_LOGICAL,
    HOST_CPU_CORES_PHYSICAL,
    CPU_TOTAL_USAGE,
    CPU_CORE_USAGE,
    CPU_CORE_FREQUENCY,
    CPU_PROCESS_USAGE_CORES,
    CPU_PROCESS_USAGE_HOST,
    MEMORY_PHYSICAL_TOTAL,
    MEMORY_PHYSICAL_USED,
    MEMORY_AVAILABLE,
    MEMORY_SWAP_USED,
    MEMORY_SWAP_TOTAL,
    DISK_VOLUME_CAPACITY,
    DISK_VOLUME_AVAILABLE,
    DISK_VOLUME_FILESYSTEM,
    DISK_VOLUME_REMOVABLE,
    DISK_VOLUME_READ_ONLY,
    DISK_DEVICE_READ_BYTES_TOTAL,
    DISK_DEVICE_READ_RATE,
    DISK_DEVICE_WRITE_BYTES_TOTAL,
    DISK_DEVICE_WRITE_RATE,
    NET_INTERFACE_RX_TOTAL,
    NET_INTERFACE_TX_TOTAL,
    NET_INTERFACE_RX_RATE,
    NET_INTERFACE_TX_RATE,
    NET_INTERFACE_IPV4,
    NET_INTERFACE_IPV6,
    GPU_ENGINE_UTILIZATION,
    GPU_MEMORY_DEDICATED_USED,
    SENSOR_TEMPERATURE,
    SENSOR_FAN_SPEED,
    BATTERY_CHARGE,
    BATTERY_CYCLES,
    BATTERY_STATUS,
    BATTERY_HEALTH,
    BATTERY_TIME_REMAINING,
    BATTERY_POWER_DRAW,
    BLUETOOTH_DEVICE_CHARGE,
    BLUETOOTH_DEVICE_NAME,
];
