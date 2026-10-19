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
