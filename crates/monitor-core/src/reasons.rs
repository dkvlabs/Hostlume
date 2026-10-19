//! Canonical reason codes carried on non-available readings
//! (phase 02 §2A.2). Stable strings; the UI resolves them to localized
//! messages via `human_message_key`.

pub const WARMUP: &str = "WARMUP";
pub const SLEEP_GAP: &str = "SLEEP_GAP";
pub const COUNTER_RESET: &str = "COUNTER_RESET";
pub const PERMISSION_DENIED: &str = "PERMISSION_DENIED";
pub const DRIVER_UNSUPPORTED: &str = "DRIVER_UNSUPPORTED";
pub const COLLECTOR_ERROR: &str = "COLLECTOR_ERROR";
pub const COLLECTOR_NOT_IMPLEMENTED: &str = "COLLECTOR_NOT_IMPLEMENTED";
pub const WRONG_PLATFORM: &str = "WRONG_PLATFORM";
pub const NO_PUBLIC_READABLE_SENSOR: &str = "NO_PUBLIC_READABLE_SENSOR";

// Phase 05: hardware-conditional reasons. Each names why a reading is not
// on this host — never a guess standing in for a value.
pub const NO_BATTERY: &str = "NO_BATTERY";
pub const ON_EXTERNAL_POWER: &str = "ON_EXTERNAL_POWER";
pub const TIME_ESTIMATE_UNKNOWN: &str = "TIME_ESTIMATE_UNKNOWN";
pub const NO_PUBLIC_READABLE_GPU: &str = "NO_PUBLIC_READABLE_GPU";
pub const UNIFIED_MEMORY: &str = "UNIFIED_MEMORY";
pub const NOT_VALIDATED_ON_HARDWARE: &str = "NOT_VALIDATED_ON_HARDWARE";
pub const NOT_EXPOSED_BY_OS_API: &str = "NOT_EXPOSED_BY_OS_API";
pub const PROBE_TOOL_NOT_INSTALLED: &str = "PROBE_TOOL_NOT_INSTALLED";
pub const NO_BLUETOOTH_CONTROLLER: &str = "NO_BLUETOOTH_CONTROLLER";
pub const NO_SENSORS_DETECTED: &str = "NO_SENSORS_DETECTED";
pub const NO_SAMPLE: &str = "NO_SAMPLE";
