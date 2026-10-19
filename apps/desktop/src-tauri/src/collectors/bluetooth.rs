//! Bluetooth accessory collector (FX-014, phase 05 §2.6).
//!
//! Read-only enumeration of connected accessories and, where the OS
//! exposes it, per-component battery charge:
//! * **macOS:** `system_profiler SPBluetoothDataType -json` — Apple's own
//!   reporting tool, public and rootless (~0.2 s measured). Charge appears
//!   only for devices that report it (`device_batteryLevel*` keys);
//!   devices without a battery characteristic get **no** charge reading —
//!   never a fabricated 0%.
//! * **Linux:** `upower --dump` (UPower aggregates BlueZ device batteries),
//!   parsed from fixed-argument output.
//! * **Windows:** not implemented in this build (GATT battery service
//!   enumeration needs a WinRT surface that is not wired yet) — explicit
//!   `unsupported`, tracked for a later phase.
//!
//! Security and privacy notes:
//! * Each probe spawns a compile-time-literal program with literal
//!   arguments at the spawn site — no variable and no device-derived
//!   string ever reaches a process invocation.
//! * Device addresses never enter the metric stream or history (metrics
//!   contract §8). Entities are `bt:<sha256(address)[..16]>`; the readable
//!   device name rides along as a text reading.

use monitor_core::{
    families, reasons, Availability, Clock, Collector, CollectorError, MetricCapability,
    MetricReading, MetricUnit, MetricValue, PollGroup,
};

use super::reading;

pub const COLLECTOR_ID: &str = "collector-bluetooth";

const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);
const MAX_DEVICES: usize = 16;

/// One connected accessory with only the fields the metric stream needs.
#[derive(Debug, Clone, PartialEq)]
pub struct BtDevice {
    pub name: String,
    /// OS-reported device address (used only for the privacy hash).
    pub address: String,
    /// `(component, percent)` pairs; empty when the device exposes no
    /// battery characteristic (headsets without battery reporting, dongles…).
    pub charge: Vec<(String, f64)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BluetoothProbe {
    pub controller_present: bool,
    pub devices: Vec<BtDevice>,
}

/// Strict MAC-48 (`AA:BB:CC:DD:EE:FF`) form check — a sanity gate on parsed
/// data before it is hashed into an entity id.
pub fn is_mac_address(text: &str) -> bool {
    let groups: Vec<&str> = text.split(':').collect();
    groups.len() == 6
        && groups
            .iter()
            .all(|group| group.len() == 2 && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Stable, privacy-preserving entity id from a device address.
pub fn bt_entity(address: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(address.as_bytes());
    let hex: String = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("");
    format!("bt:{}", &hex[..16])
}

/// Parse `system_profiler SPBluetoothDataType -json` output. Pure function
/// over the text so fixtures can be tested on every OS.
pub fn parse_profiler_json(text: &str) -> Result<BluetoothProbe, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("invalid profiler JSON: {err}"))?;
    let mut probe = BluetoothProbe::default();
    let Some(entries) = value.get("SPBluetoothDataType").and_then(|v| v.as_array()) else {
        return Ok(probe);
    };
    for entry in entries {
        if entry.get("controller_properties").is_some() {
            probe.controller_present = true;
        }
        // Only connected devices get live readings; paired-but-absent ones
        // would produce stale battery values.
        if let Some(connected) = entry.get("device_connected").and_then(|v| v.as_array()) {
            for device_entry in connected {
                if let Some(device) = parse_profiler_device(device_entry) {
                    probe.devices.push(device);
                }
            }
        }
    }
    probe.devices.truncate(MAX_DEVICES);
    Ok(probe)
}

/// Profiler devices are single-key objects: `{ "<name>": {props…} }`.
fn parse_profiler_device(entry: &serde_json::Value) -> Option<BtDevice> {
    let object = entry.as_object()?;
    let (name, props) = object.into_iter().next()?;
    let props = props.as_object()?;
    let address = props
        .get("device_address")
        .and_then(|v| v.as_str())?
        .to_string();
    let mut charge = Vec::new();
    for (key, value) in props {
        let Some(component) = key.strip_prefix("device_batteryLevel") else {
            continue;
        };
        let Some(raw) = value.as_str() else {
            continue;
        };
        if let Ok(percent) = raw.trim().trim_end_matches('%').parse::<f64>() {
            let component = if component.is_empty() {
                "device".to_string()
            } else {
                component.to_ascii_lowercase()
            };
            charge.push((component, percent.clamp(0.0, 100.0)));
        }
    }
    charge.sort_by(|a, b| a.0.cmp(&b.0));
    Some(BtDevice {
        name: name.clone(),
        address,
        charge,
    })
}

/// Parse `upower --dump` output for BlueZ-backed device batteries. Blocks
/// start with `Device:`; BlueZ paths contain `/org/bluez/` and carry a
/// `dev_XX_YY…` address plus a `percentage:` line.
pub fn parse_upower_dump(text: &str) -> Vec<(String, f64)> {
    // (address, percent) for devices exposing a battery. The BlueZ object
    // path can appear on the `Device:` line or on a `native-path:` line
    // inside the block — both are scanned.
    let mut devices: Vec<(String, f64)> = Vec::new();
    let mut current_address: Option<String> = None;
    let mut current_percent: Option<f64> = None;
    let mut in_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Device:") {
            if let (Some(address), Some(percent)) = (current_address, current_percent) {
                devices.push((address, percent));
            }
            current_address = None;
            current_percent = None;
            in_block = true;
        }
        if !in_block {
            continue;
        }
        if current_address.is_none() {
            if let Some(path_start) = trimmed.find("/org/bluez/") {
                if let Some(dev_pos) = trimmed[path_start..].find("dev_") {
                    let raw = &trimmed[path_start + dev_pos + "dev_".len()..];
                    let raw = raw.split_whitespace().next().unwrap_or("");
                    let candidate = raw.replace('_', ":");
                    if is_mac_address(&candidate) {
                        current_address = Some(candidate);
                    }
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix("percentage:") {
            if current_percent.is_none() {
                current_percent = rest
                    .trim()
                    .trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| v.clamp(0.0, 100.0));
            }
        }
    }
    if let (Some(address), Some(percent)) = (current_address, current_percent) {
        devices.push((address, percent));
    }
    devices.truncate(MAX_DEVICES);
    devices
}

/// Read a spawned probe's stdout with a bounded wait, killing it on
/// timeout. Blocking by design: the Slow poll group runs on the scheduler
/// loop and these probes are measured fast (≤0.2 s macOS, ~0.5 s Linux).
fn read_probe_stdout(mut child: std::process::Child) -> Result<String, ProbeError> {
    use std::io::Read;
    use std::sync::mpsc;
    let Some(mut stdout) = child.stdout.take() else {
        return Err(ProbeError::Spawn(std::io::Error::other(
            "probe has no stdout",
        )));
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = stdout.read_to_string(&mut buffer);
        let _ = tx.send(buffer);
    });
    match rx.recv_timeout(PROBE_TIMEOUT) {
        Ok(text) => {
            let _ = child.wait();
            Ok(text)
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(ProbeError::Timeout)
        }
    }
}

/// macOS probe: Apple's own Bluetooth reporting tool. The program name and
/// arguments are literals at the spawn site by policy (module security
/// note) — keep them that way.
#[cfg(target_os = "macos")]
fn probe_bluetooth_raw() -> Result<String, ProbeError> {
    use std::process::{Command, Stdio};
    let program = "system_profiler";
    let child = Command::new(program)
        .args(["SPBluetoothDataType", "-json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => ProbeError::ToolMissing,
            _ => ProbeError::Spawn(err),
        })?;
    read_probe_stdout(child)
}

/// Linux probe: UPower aggregates BlueZ device batteries. Literals at the
/// spawn site by policy.
#[cfg(target_os = "linux")]
fn probe_bluetooth_raw() -> Result<String, ProbeError> {
    use std::process::{Command, Stdio};
    let program = "upower";
    let child = Command::new(program)
        .args(["--dump"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => ProbeError::ToolMissing,
            _ => ProbeError::Spawn(err),
        })?;
    read_probe_stdout(child)
}

#[cfg(target_os = "windows")]
fn probe_bluetooth_raw() -> Result<String, ProbeError> {
    Err(ProbeError::ToolMissing)
}

enum ProbeError {
    ToolMissing,
    Timeout,
    Spawn(std::io::Error),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::ToolMissing => write!(formatter, "probe tool not installed"),
            ProbeError::Timeout => write!(formatter, "probe timed out"),
            ProbeError::Spawn(err) => write!(formatter, "probe spawn failed: {err}"),
        }
    }
}

#[derive(Debug, Default)]
pub struct BluetoothCollector {
    /// Capabilities are probed once at construction; re-probe happens via
    /// the explicit capability re-probe command, not every tick.
    probe_seen_controller: bool,
}

impl BluetoothCollector {
    pub fn new() -> Self {
        let probe_seen_controller =
            matches!(Self::probe(), Some(probe) if probe.controller_present);
        Self {
            probe_seen_controller,
        }
    }

    fn probe() -> Option<BluetoothProbe> {
        let text = probe_bluetooth_raw().ok()?;
        if cfg!(target_os = "macos") {
            parse_profiler_json(&text).ok()
        } else if cfg!(target_os = "linux") {
            let mut probe = BluetoothProbe {
                controller_present: !text.trim().is_empty(),
                devices: Vec::new(),
            };
            for (address, percent) in parse_upower_dump(&text) {
                probe.devices.push(BtDevice {
                    name: address.clone(),
                    address,
                    charge: vec![("device".to_string(), percent)],
                });
            }
            Some(probe)
        } else {
            None
        }
    }

    fn platform_state(&self) -> (Availability, Option<&'static str>, String) {
        if cfg!(target_os = "windows") {
            (
                Availability::Unsupported,
                Some(reasons::COLLECTOR_NOT_IMPLEMENTED),
                "Windows Bluetooth battery enumeration (GATT battery service) is not \
                 implemented in this build yet; no fabricated charge values are shown."
                    .to_string(),
            )
        } else if self.probe_seen_controller {
            (
                Availability::Available,
                None,
                "Connected accessories and their reported charge levels are enumerated \
                 through the OS Bluetooth stack."
                    .to_string(),
            )
        } else if self.has_native_probe() {
            (
                Availability::Unsupported,
                Some(reasons::NO_BLUETOOTH_CONTROLLER),
                "No Bluetooth controller or connected accessory was found on this machine."
                    .to_string(),
            )
        } else {
            (
                Availability::TemporarilyUnavailable,
                Some(reasons::PROBE_TOOL_NOT_INSTALLED),
                "The OS Bluetooth battery probe tool is not installed on this machine.".to_string(),
            )
        }
    }

    fn has_native_probe(&self) -> bool {
        cfg!(target_os = "macos") || cfg!(target_os = "linux")
    }
}

impl Collector for BluetoothCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        // Accessory charge changes on minutes-scale and the probe spawns a
        // process; 60 s keeps devices unbothered (phase doc §2.7).
        PollGroup::Slow
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        let (state, reason, explanation) = self.platform_state();
        vec![
            MetricCapability {
                metric_id: families::BLUETOOTH_DEVICE_CHARGE.to_string(),
                state,
                source: COLLECTOR_ID.to_string(),
                explanation: Some(explanation.clone()),
                reason_code: reason.map(str::to_string),
                remediation: None,
                human_message_key: None,
                precision: Some(0),
                freshness_budget_ms: Some(70_000),
                permissions_required: Vec::new(),
                optional_helper_required: false,
            },
            MetricCapability {
                metric_id: families::BLUETOOTH_DEVICE_NAME.to_string(),
                state,
                source: COLLECTOR_ID.to_string(),
                explanation: Some(explanation),
                reason_code: reason.map(str::to_string),
                remediation: None,
                human_message_key: None,
                precision: None,
                freshness_budget_ms: Some(70_000),
                permissions_required: Vec::new(),
                optional_helper_required: false,
            },
        ]
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        if cfg!(target_os = "windows") {
            return Ok(Vec::new());
        }
        let probe = Self::probe();
        let Some(probe) = probe else {
            let probe_err = probe_bluetooth_raw().err().map_or_else(
                || "unknown probe failure".to_string(),
                |err| err.to_string(),
            );
            // Tool missing is a *stable* condition, not a transient error:
            // surface it as typed unsupported so diagnostics don't fill up
            // with per-minute failures.
            return Err(CollectorError::Unsupported {
                collector: COLLECTOR_ID,
                reason: format!("Bluetooth probe unavailable on this platform: {probe_err}"),
            });
        };
        let mut readings = Vec::new();
        for device in &probe.devices {
            let entity = bt_entity(&device.address);
            readings.push(reading(
                families::BLUETOOTH_DEVICE_NAME,
                &entity,
                Some(MetricValue::Text(device.name.clone())),
                MetricUnit::Text,
                Availability::Available,
                COLLECTOR_ID,
                clock,
                None,
                None,
            ));
            for (component, percent) in &device.charge {
                readings.push(reading(
                    families::BLUETOOTH_DEVICE_CHARGE,
                    &format!("{entity}:{component}"),
                    Some(MetricValue::Number(*percent)),
                    MetricUnit::Percent,
                    Availability::Available,
                    COLLECTOR_ID,
                    clock,
                    None,
                    None,
                ));
            }
            // Devices without a battery characteristic emit no charge
            // reading at all — absence, not 0%.
        }
        Ok(readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "SPBluetoothDataType": [
            {
                "controller_properties": {
                    "controller_address": "60:3E:5F:69:33:71",
                    "controller_state": "attrib_on"
                },
                "device_connected": [
                    {
                        "Magic Trackpad": {
                            "device_address": "11:22:33:44:55:66",
                            "device_batteryLevel": "82%",
                            "device_minorType": "Trackpad"
                        }
                    },
                    {
                        "AirPods Pro": {
                            "device_address": "AA:BB:CC:DD:EE:FF",
                            "device_batteryLevelCase": "33%",
                            "device_batteryLevelLeft": "100%",
                            "device_batteryLevelRight": "100%"
                        }
                    },
                    {
                        "Plain Headset": {
                            "device_address": "01:02:03:04:05:06",
                            "device_minorType": "Headphones"
                        }
                    }
                ],
                "device_not_connected": [
                    { "Away Keyboard": { "device_address": "99:88:77:66:55:44" } }
                ]
            }
        ]
    }"#;

    #[test]
    fn profiler_fixture_parses_charge_and_absence() {
        let probe = parse_profiler_json(FIXTURE).expect("fixture parses");
        assert!(probe.controller_present);
        assert_eq!(probe.devices.len(), 3, "connected devices only");
        let trackpad = probe
            .devices
            .iter()
            .find(|d| d.name == "Magic Trackpad")
            .expect("trackpad");
        assert_eq!(trackpad.charge, vec![("device".to_string(), 82.0)]);
        let airpods = probe
            .devices
            .iter()
            .find(|d| d.name == "AirPods Pro")
            .expect("airpods");
        assert_eq!(
            airpods.charge,
            vec![
                ("case".to_string(), 33.0),
                ("left".to_string(), 100.0),
                ("right".to_string(), 100.0),
            ]
        );
        // No battery characteristic → no charge entry, never 0%.
        let headset = probe
            .devices
            .iter()
            .find(|d| d.name == "Plain Headset")
            .expect("headset");
        assert!(headset.charge.is_empty());
    }

    #[test]
    fn empty_or_invalid_probes_do_not_panic() {
        let probe = parse_profiler_json(r#"{"SPBluetoothDataType": []}"#).expect("parses");
        assert!(!probe.controller_present);
        assert!(probe.devices.is_empty());
        assert!(parse_profiler_json("not json").is_err());
    }

    #[test]
    fn upower_dump_yields_bluez_device_batteries() {
        let dump = "Device: /org/freedesktop/UPower/devices/keyboard_battery\n\
                    \x20 native-path: /org/bluez/hci0/dev_1A_2B_3C_4D_5E_6F\n\
                    \x20 type: battery\n\
                    \x20 percentage: 80%\n\
                    Device: /org/freedesktop/UPower/devices/DisplayDevice\n\
                    \x20 percentage: 55%\n\
                    Device: /org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF\n\
                    \x20 type: battery\n\
                    \x20 percentage: 45%\n";
        let devices = parse_upower_dump(dump);
        assert_eq!(devices.len(), 2, "bluez paths only");
        assert_eq!(devices[0].0, "1A:2B:3C:4D:5E:6F");
        assert_eq!(devices[0].1, 80.0);
        assert_eq!(devices[1].0, "AA:BB:CC:DD:EE:FF");
        assert_eq!(devices[1].1, 45.0);
    }

    #[test]
    fn mac_gate_blocks_non_addresses() {
        assert!(is_mac_address("AA:BB:CC:DD:EE:FF"));
        assert!(!is_mac_address("-a"));
        assert!(!is_mac_address("AA:BB:CC:DD:EE"));
        assert!(!is_mac_address(""));
    }

    #[test]
    fn addresses_never_appear_in_entity_ids() {
        let entity = bt_entity("AA:BB:CC:DD:EE:FF");
        assert!(entity.starts_with("bt:"));
        assert!(!entity.to_uppercase().contains("AA:BB"));
        assert_eq!(entity, bt_entity("AA:BB:CC:DD:EE:FF"), "stable");
        assert_ne!(entity, bt_entity("11:11:11:11:11:11"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn live_capabilities_are_honest_for_this_host() {
        let collector = BluetoothCollector::new();
        let caps = collector.capabilities();
        assert_eq!(caps.len(), 2);
        // On this dev Mac a controller exists; on a controller-less CI box
        // the row must still be a *reasoned* state, never blank.
        assert!(caps
            .iter()
            .all(|c| c.explanation.as_deref().unwrap_or("").len() > 10));
    }
}
