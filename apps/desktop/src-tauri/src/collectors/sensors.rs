//! Temperature and fan sensor collector (FX-012, phase 05 §2.4).
//!
//! Sources, chosen for being public and rootless:
//! * **macOS:** sysinfo `Components` — IOKit HID event system (proven in the
//!   Phase 00 spike: 22 sensors on Apple Silicon, unprivileged). Fan RPM is
//!   *not* exposed by this path and Apple's sampler tooling is root-gated,
//!   so fans are an explicit `unsupported` with a named reason — never 0 RPM.
//! * **Linux:** the same sysinfo hwmon path for temperatures (kernel
//!   millidegree values normalized by sysinfo) plus direct
//!   `/sys/class/hwmon/hwmon*/fan*_input` reads for fan RPM.
//! * **Windows:** standard accounts cannot read motherboard sensors without
//!   third-party kernel drivers, which the security constraints reject;
//!   both families are explicit `unsupported` with that reason.
//!
//! Sensor identity is stable: `sensor:<slug>` derived from the OS label
//! (`PMU tdie6` → `sensor:pmu_tdie6`), so history continuity survives
//! enumeration-order changes.

use monitor_core::{
    families, reasons, Availability, Clock, Collector, CollectorError, MetricCapability,
    MetricReading, PollGroup,
};

#[cfg(not(target_os = "windows"))]
use monitor_core::{MetricUnit, MetricValue};

#[cfg(not(target_os = "windows"))]
use sysinfo::Components;

#[cfg(not(target_os = "windows"))]
use super::reading;

pub const COLLECTOR_ID: &str = "collector-sensors";

/// Millidegree-to-degree scaling for Linux hwmon temperature files is done
/// by sysinfo itself; fan inputs are already RPM integers.
#[derive(Debug)]
pub struct SensorsCollector {
    #[cfg(not(target_os = "windows"))]
    components: Components,
    #[cfg(not(target_os = "windows"))]
    sensor_ids: Vec<String>,
    #[cfg(target_os = "linux")]
    fans: Vec<FanSource>,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct FanSource {
    /// `fan:<hwmon-name>:<index>` — stable while the hwmon chip keeps its
    /// kernel-assigned name and fan index.
    entity: String,
    path: std::path::PathBuf,
}

impl Default for SensorsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorsCollector {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self {}
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut collector = Self {
                components: Components::new_with_refreshed_list(),
                sensor_ids: Vec::new(),
                #[cfg(target_os = "linux")]
                fans: Vec::new(),
            };
            collector.reindex_sensors();
            #[cfg(target_os = "linux")]
            {
                collector.fans = probe_fans();
            }
            collector
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn reindex_sensors(&mut self) {
        self.components = Components::new_with_refreshed_list();
        self.sensor_ids = stable_sensor_ids(
            self.components
                .list()
                .iter()
                .map(|component| component.label()),
        );
    }
}

/// `PMU tdie6` → `pmu_tdie6`; non-alphanumeric runs collapse to `_`.
/// Repeated labels after normalization keep their first-seen order and get
/// a deterministic `-2`, `-3`, … suffix, so ids stay stable across polls.
fn stable_sensor_ids<'a, I>(labels: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut ids = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for label in labels {
        let mut slug: String = label
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        while slug.contains("__") {
            slug = slug.replace("__", "_");
        }
        let slug = slug.trim_matches('_').to_ascii_lowercase();
        let base = if slug.is_empty() {
            "sensor".to_string()
        } else {
            slug
        };
        let count = seen.entry(base.clone()).or_insert(0);
        *count += 1;
        ids.push(if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        });
    }
    ids
}

impl Collector for SensorsCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        // Temperatures change on seconds-scale; 10 s captures them without
        // hammering the HID/hwmon interfaces (design.md §9).
        PollGroup::Medium
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        #[cfg(not(target_os = "windows"))]
        let temp_supported = !self.sensor_ids.is_empty();
        #[cfg(target_os = "windows")]
        let temp_supported = false;
        #[cfg(target_os = "linux")]
        let fan_supported = !self.fans.is_empty();
        #[cfg(not(target_os = "linux"))]
        let fan_supported = false;

        vec![
            MetricCapability {
                metric_id: families::SENSOR_TEMPERATURE.to_string(),
                state: if temp_supported {
                    Availability::Available
                } else {
                    Availability::Unsupported
                },
                source: COLLECTOR_ID.to_string(),
                explanation: if temp_supported {
                    Some(format!(
                        "{} temperature sensor{} reported by the operating system.",
                        self.sensor_ids_len(),
                        if self.sensor_ids_len() == 1 { "" } else { "s" }
                    ))
                } else if cfg!(target_os = "windows") {
                    Some(
                        "Windows does not expose motherboard/CPU sensors to standard user \
                         accounts; reading them requires third-party kernel drivers, which \
                         this app does not install."
                            .to_string(),
                    )
                } else {
                    Some("No temperature sensors were enumerated on this machine.".to_string())
                },
                reason_code: if temp_supported {
                    None
                } else if cfg!(target_os = "windows") {
                    Some(reasons::NO_PUBLIC_READABLE_SENSOR.to_string())
                } else {
                    Some(reasons::NO_SENSORS_DETECTED.to_string())
                },
                remediation: None,
                human_message_key: None,
                precision: Some(1),
                freshness_budget_ms: Some(15_000),
                permissions_required: Vec::new(),
                optional_helper_required: false,
            },
            MetricCapability {
                metric_id: families::SENSOR_FAN_SPEED.to_string(),
                state: if fan_supported {
                    Availability::Available
                } else {
                    Availability::Unsupported
                },
                source: COLLECTOR_ID.to_string(),
                explanation: if fan_supported {
                    Some(format!("{} fan speed sensor(s) found.", self.fans_len()))
                } else if cfg!(target_os = "macos") {
                    Some(
                        "macOS does not expose fan RPM through public non-root interfaces \
                         (the HID sensor path returns temperatures only, and Apple's \
                         sampler tooling requires root). Fans would show 0 RPM here — \
                         which would be fabricated, so they are marked unsupported \
                         instead."
                            .to_string(),
                    )
                } else if cfg!(target_os = "windows") {
                    Some(
                        "Windows does not expose fan speeds to standard user accounts \
                         without third-party kernel drivers, which this app does not \
                         install."
                            .to_string(),
                    )
                } else {
                    Some(
                        "No fan input files were found under /sys/class/hwmon; this \
                         machine may have no controllable-reporting fans (passive \
                         cooling) or its drivers do not export them."
                            .to_string(),
                    )
                },
                reason_code: if fan_supported {
                    None
                } else if cfg!(target_os = "linux") {
                    Some(reasons::NO_SENSORS_DETECTED.to_string())
                } else {
                    Some(reasons::NO_PUBLIC_READABLE_SENSOR.to_string())
                },
                remediation: None,
                human_message_key: None,
                precision: Some(0),
                freshness_budget_ms: Some(15_000),
                permissions_required: Vec::new(),
                optional_helper_required: false,
            },
        ]
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        #[cfg(target_os = "windows")]
        {
            let _ = clock;
            // Explicit unsupported capability; emitting nothing is the honest
            // output (never placeholder 0 values).
            return Ok(Vec::new());
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.components.refresh(true);
            self.reindex_if_labels_changed();
            let mut readings = Vec::new();
            for (index, component) in self.components.list().iter().enumerate() {
                let Some(entity) = self.sensor_ids.get(index) else {
                    continue;
                };
                let entity = format!("sensor:{entity}");
                match component.temperature() {
                    Some(celsius) if celsius.is_finite() => readings.push(reading(
                        families::SENSOR_TEMPERATURE,
                        &entity,
                        Some(MetricValue::Number(celsius as f64)),
                        MetricUnit::Celsius,
                        Availability::Available,
                        COLLECTOR_ID,
                        clock,
                        None,
                        None,
                    )),
                    // Sensor enumerated but silent right now: visible state,
                    // null value (metrics contract §2 invariant).
                    _ => readings.push(reading(
                        families::SENSOR_TEMPERATURE,
                        &entity,
                        None,
                        MetricUnit::Celsius,
                        Availability::TemporarilyUnavailable,
                        COLLECTOR_ID,
                        clock,
                        Some(reasons::NO_SAMPLE),
                        None,
                    )),
                }
            }
            #[cfg(target_os = "linux")]
            for fan in &self.fans {
                match std::fs::read_to_string(&fan.path) {
                    Ok(text) => match text.trim().parse::<f64>() {
                        Ok(rpm) if rpm.is_finite() => readings.push(reading(
                            families::SENSOR_FAN_SPEED,
                            &fan.entity,
                            Some(MetricValue::Number(rpm)),
                            MetricUnit::Rpm,
                            Availability::Available,
                            COLLECTOR_ID,
                            clock,
                            None,
                            None,
                        )),
                        _ => readings.push(reading(
                            families::SENSOR_FAN_SPEED,
                            &fan.entity,
                            None,
                            MetricUnit::Rpm,
                            Availability::TemporarilyUnavailable,
                            COLLECTOR_ID,
                            clock,
                            Some(reasons::NO_SAMPLE),
                            None,
                        )),
                    },
                    Err(_) => readings.push(reading(
                        families::SENSOR_FAN_SPEED,
                        &fan.entity,
                        None,
                        MetricUnit::Rpm,
                        Availability::TemporarilyUnavailable,
                        COLLECTOR_ID,
                        clock,
                        Some(reasons::NO_SAMPLE),
                        None,
                    )),
                }
            }
            Ok(readings)
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl SensorsCollector {
    fn reindex_if_labels_changed(&mut self) {
        let current: Vec<String> = stable_sensor_ids(
            self.components
                .list()
                .iter()
                .map(|component| component.label()),
        );
        if current != self.sensor_ids {
            self.sensor_ids = current;
        }
    }

    fn sensor_ids_len(&self) -> usize {
        self.sensor_ids.len()
    }

    #[cfg(target_os = "linux")]
    fn fans_len(&self) -> usize {
        self.fans.len()
    }

    #[cfg(not(target_os = "linux"))]
    fn fans_len(&self) -> usize {
        0
    }
}

#[cfg(target_os = "linux")]
impl SensorsCollector {
    fn fans_len(&self) -> usize {
        self.fans.len()
    }
}

#[cfg(target_os = "windows")]
impl SensorsCollector {
    fn sensor_ids_len(&self) -> usize {
        0
    }

    fn fans_len(&self) -> usize {
        0
    }
}

/// Enumerate `/sys/class/hwmon/hwmon*/fan*_input` once at probe time. Fan
/// indexes are kernel-stable per chip; a hotplug re-probe happens when the
/// collector is reconstructed.
#[cfg(target_os = "linux")]
fn probe_fans() -> Vec<FanSource> {
    let mut fans = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") else {
        return fans;
    };
    let mut hwmons: Vec<std::path::PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    hwmons.sort();
    for hwmon in hwmons {
        let Ok(files) = std::fs::read_dir(&hwmon) else {
            continue;
        };
        let chip_name = std::fs::read_to_string(hwmon.join("name"))
            .map(|text| text.trim().to_string())
            .unwrap_or_else(|_| "hwmon".to_string());
        let mut inputs: Vec<String> = files
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.starts_with("fan") && name.ends_with("_input"))
            .collect();
        inputs.sort();
        for input in inputs {
            let index = input
                .trim_start_matches("fan")
                .trim_end_matches("_input")
                .to_string();
            fans.push(FanSource {
                entity: format!("fan:{chip_name}:{index}"),
                path: hwmon.join(input),
            });
        }
    }
    fans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::FakeClock;

    #[test]
    fn sensor_ids_are_slugified_and_deduplicated() {
        let ids = stable_sensor_ids(["PMU tdie6", "NAND CH0 temp", "PMU tdie6", ""]);
        assert_eq!(
            ids,
            vec!["pmu_tdie6", "nand_ch0_temp", "pmu_tdie6-2", "sensor"]
        );
        // Stability: same labels again produce the same ids in same order.
        let again = stable_sensor_ids(["PMU tdie6", "NAND CH0 temp", "PMU tdie6", ""]);
        assert_eq!(ids, again);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn live_macos_linux_probe_enumerates_sensors() {
        let collector = SensorsCollector::new();
        let caps = collector.capabilities();
        assert_eq!(caps.len(), 2);
        if cfg!(target_os = "macos") {
            // Phase 00 spike: this machine exposes 22 sensors. The probe
            // must find them without privileges.
            assert_eq!(
                caps[0].state,
                Availability::Available,
                "temperature sensors must enumerate on this Mac"
            );
            assert!(caps[0]
                .explanation
                .as_deref()
                .unwrap_or("")
                .contains("sensor"));
            // Fan RPM: honest unsupported on macOS, never fabricated.
            assert_eq!(caps[1].state, Availability::Unsupported);
            assert_eq!(
                caps[1].reason_code.as_deref(),
                Some(reasons::NO_PUBLIC_READABLE_SENSOR)
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn live_sample_emits_real_temperatures_with_units() {
        let mut collector = SensorsCollector::new();
        let readings = collector.sample(&FakeClock::default()).expect("sample ok");
        if cfg!(target_os = "macos") {
            assert!(!readings.is_empty(), "real sensors must yield readings");
            assert!(readings
                .iter()
                .all(|r| r.metric_id == families::SENSOR_TEMPERATURE));
            let available: Vec<_> = readings
                .iter()
                .filter(|r| r.availability == Availability::Available)
                .collect();
            assert!(!available.is_empty(), "at least one live temperature");
            for r in &available {
                let Some(MetricValue::Number(value)) = r.value else {
                    panic!("available reading must carry a number");
                };
                // Plausibility band for a machine idling/compiling — guards
                // against fabricated constants or unit slips.
                assert!(
                    (0.0..=150.0).contains(&value),
                    "temperature {value} out of physical range"
                );
                assert_eq!(r.unit, MetricUnit::Celsius);
                assert!(r.entity_id.starts_with("sensor:"));
            }
        }
    }
}
