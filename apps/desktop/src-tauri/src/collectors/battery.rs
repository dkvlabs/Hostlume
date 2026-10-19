//! Battery / power source collector (FX-013, phase 05 §2.5).
//!
//! Per-OS sources — all public, unprivileged, read-only:
//! * **macOS:** the documented IOPowerSources API (presence, charge,
//!   charging state, AC/battery source, time-remaining estimate with
//!   explicit unknown/unlimited sentinels) plus the `AppleSmartBattery`
//!   IOKit registry entry for cycle count and design/full-charge capacity.
//!   Raw registry `TimeRemaining = 65535` sentinels are never surfaced —
//!   the IOPS estimate API encodes the same facts safely.
//! * **Linux:** `/sys/class/power_supply/BAT*/` sysfs files.
//! * **Windows:** `GetSystemPowerStatus` (charge percent, AC state,
//!   lifetime estimate). Cycle count and design/full-charge capacity are
//!   not exposed by public Windows power APIs — explicit `unsupported`,
//!   never inferred from charge percentages.
//!
//! Health is computed only where the OS exposes both full-charge and design
//! capacity (`full/design × 100`). There is no "guessed health".

use monitor_core::{
    families, reasons, Availability, Clock, Collector, CollectorError, MetricCapability,
    MetricReading, MetricUnit, MetricValue, PollGroup,
};

use super::reading;

pub const COLLECTOR_ID: &str = "collector-battery";

/// Entity for the first internal battery. Multi-battery hosts are not
/// enumerated in v1; an explicit capability note covers that limitation.
pub(crate) const BATTERY_ENTITY: &str = "battery:0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerStatus {
    Charging,
    Discharging,
    Full,
    NotCharging,
    Unknown,
}

impl PowerStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerStatus::Charging => "charging",
            PowerStatus::Discharging => "discharging",
            PowerStatus::Full => "full",
            PowerStatus::NotCharging => "not_charging",
            PowerStatus::Unknown => "unknown",
        }
    }

    /// Map an OS status word (Linux sysfs convention, also accepted from
    /// other backends after normalization).
    pub fn parse_os_status(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "charging" => PowerStatus::Charging,
            "discharging" => PowerStatus::Discharging,
            "full" => PowerStatus::Full,
            "not charging" | "not_charging" => PowerStatus::NotCharging,
            _ => PowerStatus::Unknown,
        }
    }
}

/// Time-remaining estimate with the OS's own three-valued semantics kept
/// intact — collapsing "unknown" into 0 s or "on AC" into a countdown is
/// exactly the fabrication the contract forbids.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeEstimate {
    Seconds(f64),
    Unknown,
    Unlimited,
}

/// One backend-normalized battery state. `None` fields mean "this OS
/// exposes nothing for it" and become explicit unavailable states, never
/// zeros.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatterySnapshot {
    pub charge_percent: Option<f64>,
    pub status: Option<PowerStatus>,
    pub cycles: Option<u64>,
    pub health_percent: Option<f64>,
    pub design_capacity_mah: Option<f64>,
    pub full_charge_capacity_mah: Option<f64>,
    pub time_remaining: Option<TimeEstimate>,
    pub power_draw_watts: Option<f64>,
}

impl BatterySnapshot {
    /// Health from reported capacities. Both values must be positive;
    /// anything else means "not exposed", not 100%.
    pub fn health_from_capacities(full_charge: f64, design: f64) -> Option<f64> {
        if full_charge.is_finite() && design.is_finite() && full_charge > 0.0 && design > 0.0 {
            let health = full_charge / design * 100.0;
            // Sanity band: batteries report 0–100+%; wild ratios indicate a
            // misread pair of keys, not a real condition.
            (0.0..=150.0).contains(&health).then_some(health)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct BatteryCollector {
    backend: Backend,
}

#[derive(Debug)]
enum Backend {
    #[cfg(target_os = "macos")]
    MacOs(macos::MacOsBattery),
    #[cfg(target_os = "linux")]
    Linux(linux::LinuxBattery),
    #[cfg(target_os = "windows")]
    Windows(windows_backend::WindowsBattery),
    /// Cross-compiled (capability report only); never sampled.
    WrongPlatform,
}

impl BatteryCollector {
    pub fn new() -> Self {
        let backend = if cfg!(target_os = "macos") {
            #[cfg(target_os = "macos")]
            {
                Backend::MacOs(macos::MacOsBattery::new())
            }
            #[cfg(not(target_os = "macos"))]
            {
                Backend::WrongPlatform
            }
        } else if cfg!(target_os = "linux") {
            #[cfg(target_os = "linux")]
            {
                Backend::Linux(linux::LinuxBattery::new())
            }
            #[cfg(not(target_os = "linux"))]
            {
                Backend::WrongPlatform
            }
        } else if cfg!(target_os = "windows") {
            #[cfg(target_os = "windows")]
            {
                Backend::Windows(windows_backend::WindowsBattery::new())
            }
            #[cfg(not(target_os = "windows"))]
            {
                Backend::WrongPlatform
            }
        } else {
            Backend::WrongPlatform
        };
        Self { backend }
    }

    fn battery_present(&self) -> bool {
        match &self.backend {
            #[cfg(target_os = "macos")]
            Backend::MacOs(b) => b.battery_present(),
            #[cfg(target_os = "linux")]
            Backend::Linux(b) => b.battery_present(),
            #[cfg(target_os = "windows")]
            Backend::Windows(b) => b.battery_present(),
            Backend::WrongPlatform => false,
        }
    }

    fn sample_backend(&mut self) -> Result<Option<BatterySnapshot>, CollectorError> {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            Backend::MacOs(b) => b.sample(),
            #[cfg(target_os = "linux")]
            Backend::Linux(b) => b.sample(),
            #[cfg(target_os = "windows")]
            Backend::Windows(b) => b.sample(),
            Backend::WrongPlatform => Err(CollectorError::Unsupported {
                collector: COLLECTOR_ID,
                reason: "compiled for a different target OS".to_string(),
            }),
        }
    }

    fn expose(&self, family: &str) -> Expose {
        match &self.backend {
            #[cfg(target_os = "macos")]
            Backend::MacOs(b) => b.expose(family),
            #[cfg(target_os = "linux")]
            Backend::Linux(b) => b.expose(family),
            #[cfg(target_os = "windows")]
            Backend::Windows(b) => b.expose(family),
            Backend::WrongPlatform => Expose::Unsupported {
                reason: reasons::WRONG_PLATFORM,
                explanation: "This build of the battery collector is running on a different \
                              operating system.",
            },
        }
    }
}

impl Default for BatteryCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// What a backend has to say about one battery family on this host.
pub enum Expose {
    /// Real source wired and probed.
    Available,
    /// Host genuinely has no battery.
    NoBattery,
    /// The OS/driver does not expose this datum publicly.
    Unsupported {
        reason: &'static str,
        explanation: &'static str,
    },
}

impl Collector for BatteryCollector {
    fn id(&self) -> &'static str {
        COLLECTOR_ID
    }

    fn poll_group(&self) -> PollGroup {
        // Battery state moves slowly; design.md §9 and phase 05 §2.7 ask
        // for slower schedules to avoid hammering the power subsystem.
        PollGroup::Slow
    }

    fn capabilities(&self) -> Vec<MetricCapability> {
        let present = self.battery_present();
        let families_and_explanations: [(&str, &str); 6] = [
            (
                families::BATTERY_CHARGE,
                "State of charge and charging state come from the operating system's power \
                 service.",
            ),
            (
                families::BATTERY_STATUS,
                "Charging/discharging/full/not-charging state from the OS power service.",
            ),
            (families::BATTERY_CYCLES, "Reported cycle count."),
            (
                families::BATTERY_HEALTH,
                "Health = full-charge capacity / design capacity, only when the OS reports \
                 both.",
            ),
            (
                families::BATTERY_TIME_REMAINING,
                "Runtime estimate, valid only while discharging; the OS 'on AC' and 'unknown' \
                 states are preserved, never converted to 0.",
            ),
            (
                families::BATTERY_POWER_DRAW,
                "Instantaneous battery power (signed; negative while discharging).",
            ),
        ];
        families_and_explanations
            .into_iter()
            .map(|(family, base_explanation)| {
                let expose = self.expose(family);
                let (state, reason, explanation): (
                    Availability,
                    Option<&'static str>,
                    Option<String>,
                ) = match expose {
                    Expose::Available if present => (Availability::Available, None, None),
                    Expose::Available => (
                        Availability::Unsupported,
                        Some(reasons::NO_BATTERY),
                        Some(
                            "No internal battery was detected on this machine (desktop or \
                             battery removed)."
                                .to_string(),
                        ),
                    ),
                    Expose::NoBattery => (
                        Availability::Unsupported,
                        Some(reasons::NO_BATTERY),
                        Some(
                            "No internal battery was detected on this machine (desktop or \
                             battery removed)."
                                .to_string(),
                        ),
                    ),
                    Expose::Unsupported {
                        reason,
                        explanation,
                    } => (
                        Availability::Unsupported,
                        Some(reason),
                        Some(explanation.to_string()),
                    ),
                };
                MetricCapability {
                    metric_id: family.to_string(),
                    state,
                    source: COLLECTOR_ID.to_string(),
                    explanation: explanation
                        .or_else(|| present.then(|| base_explanation.to_string())),
                    reason_code: reason.map(str::to_string),
                    remediation: None,
                    human_message_key: None,
                    precision: Some(1),
                    freshness_budget_ms: Some(70_000),
                    permissions_required: Vec::new(),
                    optional_helper_required: false,
                }
            })
            .collect()
    }

    fn sample(&mut self, clock: &dyn Clock) -> Result<Vec<MetricReading>, CollectorError> {
        let Some(snapshot) = self.sample_backend()? else {
            // No battery on this host: absence is the honest output.
            return Ok(Vec::new());
        };
        let mut readings = Vec::new();
        if let Some(charge) = snapshot.charge_percent {
            readings.push(number_reading(
                families::BATTERY_CHARGE,
                charge.clamp(0.0, 100.0),
                MetricUnit::Percent,
                clock,
            ));
        }
        if let Some(status) = snapshot.status {
            readings.push(reading(
                families::BATTERY_STATUS,
                BATTERY_ENTITY,
                Some(MetricValue::Text(status.as_str().to_string())),
                MetricUnit::Text,
                Availability::Available,
                COLLECTOR_ID,
                clock,
                None,
                None,
            ));
        }
        if let Some(cycles) = snapshot.cycles {
            readings.push(number_reading(
                families::BATTERY_CYCLES,
                cycles as f64,
                MetricUnit::Count,
                clock,
            ));
        }
        if let Some(health) = snapshot.health_percent {
            readings.push(number_reading(
                families::BATTERY_HEALTH,
                health,
                MetricUnit::Percent,
                clock,
            ));
        }
        match snapshot.time_remaining {
            Some(TimeEstimate::Seconds(secs)) if secs.is_finite() && secs > 0.0 => {
                readings.push(number_reading(
                    families::BATTERY_TIME_REMAINING,
                    secs,
                    MetricUnit::Seconds,
                    clock,
                ));
            }
            Some(TimeEstimate::Unlimited) => {
                // On external power a runtime countdown does not exist.
                readings.push(reading(
                    families::BATTERY_TIME_REMAINING,
                    BATTERY_ENTITY,
                    None,
                    MetricUnit::Seconds,
                    Availability::Unsupported,
                    COLLECTOR_ID,
                    clock,
                    Some(reasons::ON_EXTERNAL_POWER),
                    None,
                ));
            }
            Some(TimeEstimate::Unknown) | Some(TimeEstimate::Seconds(_)) | None => {
                readings.push(reading(
                    families::BATTERY_TIME_REMAINING,
                    BATTERY_ENTITY,
                    None,
                    MetricUnit::Seconds,
                    Availability::TemporarilyUnavailable,
                    COLLECTOR_ID,
                    clock,
                    Some(reasons::TIME_ESTIMATE_UNKNOWN),
                    None,
                ));
            }
        }
        if let Some(watts) = snapshot.power_draw_watts {
            readings.push(number_reading(
                families::BATTERY_POWER_DRAW,
                watts,
                MetricUnit::Watts,
                clock,
            ));
        }
        Ok(readings)
    }
}

fn number_reading(
    metric_id: &str,
    value: f64,
    unit: MetricUnit,
    clock: &dyn Clock,
) -> MetricReading {
    reading(
        metric_id,
        BATTERY_ENTITY,
        Some(MetricValue::Number(value)),
        unit,
        Availability::Available,
        COLLECTOR_ID,
        clock,
        None,
        None,
    )
}

// ---------------------------------------------------------------------------
// macOS backend: IOPowerSources API + AppleSmartBattery registry entry.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    use super::{BatterySnapshot, PowerStatus, TimeEstimate};
    use monitor_core::{reasons, CollectorError};

    use objc2_core_foundation::{
        CFBoolean, CFDictionary, CFMutableDictionary, CFNumber, CFRetained, CFString, CFType,
    };
    use objc2_io_kit::{
        kIOMainPortDefault, IOObjectRelease, IORegistryEntryCreateCFProperty, IOServiceMatching,
    };

    // The objc2-io-kit bindings do not export the singular lookup, so this
    // module declares the documented IOKitLib.h prototype directly (same
    // framework the bindings already link). The call consumes the matching
    // dictionary per the C API contract.
    //
    // SAFETY contract (module-wide): these are read-only registry queries;
    // every returned handle is released, every borrowed CF value is only
    // read through checked `downcast_ref` before use.
    extern "C-unwind" {
        fn IOServiceGetMatchingService(
            main_port: libc::mach_port_t,
            matching: *mut CFMutableDictionary,
        ) -> libc::mach_port_t;

        // Borrowed lookup into an untyped CF dictionary (the bindings' typed
        // `get` is unavailable for the Opaque variant). The returned value is
        // owned by the dictionary and is never released here.
        fn CFDictionaryGetValue(
            the_dict: *const std::ffi::c_void,
            key: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
    }

    use objc2_io_kit::io_registry_entry_t;

    #[derive(Debug)]
    pub struct MacOsBattery {
        /// The `AppleSmartBattery` registry entry; its existence is the
        /// battery-presence signal. Re-probed when missing (hot-plug edge).
        entry: Option<io_registry_entry_t>,
    }

    impl MacOsBattery {
        pub fn new() -> Self {
            Self {
                entry: probe_registry_entry(),
            }
        }

        pub fn battery_present(&self) -> bool {
            self.entry.is_some()
        }

        pub fn expose(&self, family: &str) -> super::Expose {
            use monitor_core::families;
            if !self.battery_present() {
                return super::Expose::NoBattery;
            }
            match family {
                families::BATTERY_CHARGE
                | families::BATTERY_STATUS
                | families::BATTERY_CYCLES
                | families::BATTERY_HEALTH
                | families::BATTERY_TIME_REMAINING
                | families::BATTERY_POWER_DRAW => super::Expose::Available,
                _ => super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    explanation: "This battery datum is not exposed by macOS power APIs.",
                },
            }
        }

        pub fn sample(&mut self) -> Result<Option<BatterySnapshot>, CollectorError> {
            if self.entry.is_none() {
                self.entry = probe_registry_entry();
            }
            let Some(entry) = self.entry else {
                return Ok(None);
            };
            let mut snapshot = BatterySnapshot {
                // Top-level CurrentCapacity is the reported percentage.
                charge_percent: registry_number(entry, "CurrentCapacity"),
                cycles: registry_number(entry, "CycleCount").map(|v| v.max(0.0) as u64),
                ..BatterySnapshot::default()
            };
            // Capacities live in the nested BatteryData dictionary (mAh).
            if let Some(battery_data) = registry_dictionary(entry, "BatteryData") {
                let design = dict_number(&battery_data, "DesignCapacity");
                let full = dict_number(&battery_data, "FullChargeCapacity");
                snapshot.design_capacity_mah = design;
                snapshot.full_charge_capacity_mah = full;
                if let (Some(full), Some(design)) = (full, design) {
                    snapshot.health_percent =
                        super::BatterySnapshot::health_from_capacities(full, design);
                }
                if snapshot.charge_percent.is_none() {
                    snapshot.charge_percent = dict_number(&battery_data, "CurrentCapacity");
                }
            }
            // Charging flags are top-level CFBoolean properties.
            let is_charging = registry_bool(entry, "IsCharging");
            let external = registry_bool(entry, "ExternalConnected");
            let fully_charged = registry_bool(entry, "FullyCharged");
            snapshot.status = Some(match (fully_charged, is_charging, external) {
                (Some(true), _, _) => PowerStatus::Full,
                (_, Some(true), _) => PowerStatus::Charging,
                (_, _, Some(true)) => PowerStatus::NotCharging,
                (None, None, None) => PowerStatus::Unknown,
                _ => PowerStatus::Discharging,
            });
            // Power draw: Voltage (mV) × Amperage (mA) → W; amperage is
            // negative while discharging, so the sign carries the meaning.
            if let (Some(millivolts), Some(milliamps)) = (
                registry_number(entry, "Voltage"),
                registry_number(entry, "Amperage"),
            ) {
                snapshot.power_draw_watts = Some(millivolts * milliamps / 1.0e9);
            }
            // The estimate API returns -1 (unknown) / -2 (unlimited) / seconds
            // — the documented replacement for raw 65535 sentinels.
            let estimate = objc2_io_kit::IOPSGetTimeRemainingEstimate();
            snapshot.time_remaining =
                Some(if estimate == objc2_io_kit::kIOPSTimeRemainingUnlimited {
                    TimeEstimate::Unlimited
                } else if estimate == objc2_io_kit::kIOPSTimeRemainingUnknown {
                    TimeEstimate::Unknown
                } else if estimate > 0.0 {
                    TimeEstimate::Seconds(estimate)
                } else {
                    TimeEstimate::Unknown
                });
            Ok(Some(snapshot))
        }
    }

    impl Default for MacOsBattery {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for MacOsBattery {
        fn drop(&mut self) {
            if let Some(entry) = self.entry {
                let _ = IOObjectRelease(entry);
            }
        }
    }

    fn probe_registry_entry() -> Option<io_registry_entry_t> {
        unsafe {
            let dict = IOServiceMatching(c"AppleSmartBattery".as_ptr())?;
            let raw = CFRetained::into_raw(dict);
            let entry = IOServiceGetMatchingService(kIOMainPortDefault, raw.as_ptr());
            (entry != 0).then_some(entry)
        }
    }

    fn registry_number(entry: io_registry_entry_t, key: &str) -> Option<f64> {
        unsafe {
            let value: CFRetained<CFType> =
                IORegistryEntryCreateCFProperty(entry, Some(&CFString::from_str(key)), None, 0)?;
            value
                .downcast_ref::<CFNumber>()?
                .as_f64()
                .filter(|v| v.is_finite())
        }
    }

    fn registry_bool(entry: io_registry_entry_t, key: &str) -> Option<bool> {
        unsafe {
            let value: CFRetained<CFType> =
                IORegistryEntryCreateCFProperty(entry, Some(&CFString::from_str(key)), None, 0)?;
            Some(value.downcast_ref::<CFBoolean>()?.value())
        }
    }

    /// The nested BatteryData dictionary, borrowed for the duration of the
    /// returned guard's lifetime (the retained CFType keeps it alive).
    fn registry_dictionary(entry: io_registry_entry_t, key: &str) -> Option<CFRetained<CFType>> {
        unsafe { IORegistryEntryCreateCFProperty(entry, Some(&CFString::from_str(key)), None, 0) }
    }

    /// Read one number out of a borrowed, untyped CF dictionary. The value
    /// pointer is dictionary-owned; it is interpreted only after a checked
    /// CFNumber downcast and never released.
    fn dict_number(dict: &CFRetained<CFType>, key: &str) -> Option<f64> {
        let dict = dict.downcast_ref::<CFDictionary>()?;
        let key_cf = CFString::from_str(key);
        let raw = unsafe {
            CFDictionaryGetValue(
                dict as *const CFDictionary as *const std::ffi::c_void,
                &*key_cf as *const CFString as *const std::ffi::c_void,
            )
        };
        if raw.is_null() {
            return None;
        }
        unsafe { (&*(raw as *const CFType)).downcast_ref::<CFNumber>() }?
            .as_f64()
            .filter(|v| v.is_finite())
    }
}

// ---------------------------------------------------------------------------
// Linux backend: /sys/class/power_supply sysfs.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux {
    use super::{BatterySnapshot, PowerStatus, TimeEstimate};
    use monitor_core::CollectorError;
    use std::path::{Path, PathBuf};

    #[derive(Debug)]
    pub struct LinuxBattery {
        root: PathBuf,
        battery_dir: Option<PathBuf>,
    }

    impl LinuxBattery {
        pub fn new() -> Self {
            let root = PathBuf::from("/sys/class/power_supply");
            let battery_dir = find_battery_dir(&root);
            Self { root, battery_dir }
        }

        pub fn battery_present(&self) -> bool {
            self.battery_dir.is_some()
        }

        pub fn expose(&self, family: &str) -> super::Expose {
            use monitor_core::families;
            let Some(dir) = &self.battery_dir else {
                return super::Expose::NoBattery;
            };
            match family {
                families::BATTERY_CHARGE | families::BATTERY_STATUS => super::Expose::Available,
                families::BATTERY_CYCLES => {
                    if dir.join("cycle_count").exists() {
                        super::Expose::Available
                    } else {
                        super::Expose::Unsupported {
                            reason: reasons::NOT_EXPOSED_BY_OS_API,
                            explanation: "This battery's kernel driver does not publish a \
                                          cycle_count file.",
                        }
                    }
                }
                families::BATTERY_HEALTH => {
                    let full = dir.join("charge_full").exists() || dir.join("energy_full").exists();
                    let design = dir.join("charge_full_design").exists()
                        || dir.join("energy_full_design").exists();
                    if full && design {
                        super::Expose::Available
                    } else {
                        super::Expose::Unsupported {
                            reason: reasons::NOT_EXPOSED_BY_OS_API,
                            explanation: "This battery's driver does not publish both \
                                          full-charge and design capacity files, so health \
                                          cannot be computed without guessing.",
                        }
                    }
                }
                families::BATTERY_TIME_REMAINING => {
                    if dir.join("power_now").exists() || dir.join("current_now").exists() {
                        super::Expose::Available
                    } else {
                        super::Expose::Unsupported {
                            reason: reasons::NOT_EXPOSED_BY_OS_API,
                            explanation: "This battery's driver publishes no current/power \
                                          file, so no honest runtime estimate exists.",
                        }
                    }
                }
                families::BATTERY_POWER_DRAW => {
                    if dir.join("power_now").exists()
                        || (dir.join("voltage_now").exists() && dir.join("current_now").exists())
                    {
                        super::Expose::Available
                    } else {
                        super::Expose::Unsupported {
                            reason: reasons::NOT_EXPOSED_BY_OS_API,
                            explanation: "No voltage/current or power file exposed by this \
                                          driver.",
                        }
                    }
                }
                _ => super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    explanation: "This battery datum is not exposed by the kernel.",
                },
            }
        }

        pub fn sample(&mut self) -> Result<Option<BatterySnapshot>, CollectorError> {
            // Re-probe: batteries can appear on hotplug of expanded docks.
            self.battery_dir = find_battery_dir(&self.root);
            let Some(dir) = self.battery_dir.clone() else {
                return Ok(None);
            };
            let mut snapshot = BatterySnapshot {
                charge_percent: read_u64(&dir, "capacity").map(|v| v as f64),
                status: read_text(&dir, "status").map(|text| PowerStatus::parse_os_status(&text)),
                cycles: read_u64(&dir, "cycle_count"),
                ..BatterySnapshot::default()
            };
            let full = read_u64(&dir, "charge_full").or_else(|| read_u64(&dir, "energy_full"));
            let design = read_u64(&dir, "charge_full_design")
                .or_else(|| read_u64(&dir, "energy_full_design"));
            snapshot.full_charge_capacity_mah = full.map(|v| v as f64);
            snapshot.design_capacity_mah = design.map(|v| v as f64);
            if let (Some(full), Some(design)) = (full, design) {
                snapshot.health_percent =
                    super::BatterySnapshot::health_from_capacities(full as f64, design as f64);
            }
            let now = read_u64(&dir, "charge_now").or_else(|| read_u64(&dir, "energy_now"));
            let rate = read_u64(&dir, "power_now").or_else(|| read_u64(&dir, "current_now"));
            let voltage = read_u64(&dir, "voltage_now");
            // µV × µA / 1e12 → W; µW / 1e6 → W.
            snapshot.power_draw_watts = match (voltage, read_u64(&dir, "current_now"), rate) {
                (Some(v), Some(i), _) if rate.is_none() => Some(v as f64 * i as f64 / 1.0e12),
                (_, _, Some(p)) => Some(p as f64 / 1.0e6),
                _ => None,
            };
            // Runtime estimate only while discharging; charging/full/AC are
            // not runtime states.
            snapshot.time_remaining = match snapshot.status {
                Some(PowerStatus::Discharging) => match (now, rate) {
                    (Some(now), Some(rate)) if rate > 0 => {
                        Some(TimeEstimate::Seconds(now as f64 / rate as f64 * 3600.0))
                    }
                    _ => Some(TimeEstimate::Unknown),
                },
                Some(PowerStatus::Charging)
                | Some(PowerStatus::Full)
                | Some(PowerStatus::NotCharging) => Some(TimeEstimate::Unlimited),
                _ => Some(TimeEstimate::Unknown),
            };
            Ok(Some(snapshot))
        }
    }

    impl Default for LinuxBattery {
        fn default() -> Self {
            Self::new()
        }
    }

    use monitor_core::{families, reasons};

    fn find_battery_dir(root: &Path) -> Option<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            // Kernel spec: `type` file carries "Battery" for batteries; AC
            // adapters and USB supplies must not masquerade as one.
            if read_text(&entry, "type").as_deref() == Some("Battery") {
                return Some(entry);
            }
        }
        None
    }

    fn read_u64(dir: &Path, file: &str) -> Option<u64> {
        read_text(dir, file)?.trim().parse::<u64>().ok()
    }

    fn read_text(dir: &Path, file: &str) -> Option<String> {
        std::fs::read_to_string(dir.join(file)).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn fixture(files: &[(&str, &str)]) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("hostlume-battery-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            for (name, content) in files {
                std::fs::write(dir.join(name), content).unwrap();
            }
            dir
        }

        #[test]
        fn sysfs_battery_fixture_parses_like_real_hardware() {
            // Values modeled on a common ThinkPad battery (µAh / µV units).
            let dir = fixture(&[
                ("type", "Battery\n"),
                ("status", "Discharging\n"),
                ("capacity", "62\n"),
                ("cycle_count", "131\n"),
                ("charge_full", "3201000\n"),
                ("charge_full_design", "4558000\n"),
                ("charge_now", "1994000\n"),
                ("power_now", "9413000\n"),
            ]);
            let mut backend = LinuxBattery {
                root: dir.clone(),
                battery_dir: None,
            };
            let snapshot = backend.sample().unwrap().expect("battery found");
            assert_eq!(snapshot.charge_percent, Some(62.0));
            assert_eq!(snapshot.status, Some(PowerStatus::Discharging));
            assert_eq!(snapshot.cycles, Some(131));
            let health = snapshot.health_percent.expect("health computable");
            assert!((health - (3_201_000.0 / 4_558_000.0 * 100.0)).abs() < 1e-9);
            let time = snapshot.time_remaining;
            assert_eq!(
                time,
                Some(TimeEstimate::Seconds(1_994_000.0 / 9_413_000.0 * 3600.0))
            );
            let draw = snapshot.power_draw_watts.expect("draw");
            assert!((draw - 9.413).abs() < 1e-9);
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn ac_adapter_directory_is_not_a_battery() {
            let dir = fixture(&[("type", "Mains\n"), ("online", "1\n")]);
            let backend = LinuxBattery {
                root: dir.clone(),
                battery_dir: None,
            };
            assert!(!backend.battery_present());
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn missing_cycle_count_is_unsupported_not_zero() {
            let dir = fixture(&[
                ("type", "Battery\n"),
                ("status", "Full\n"),
                ("capacity", "100\n"),
            ]);
            let backend = LinuxBattery {
                root: dir.clone(),
                battery_dir: None,
            };
            assert!(matches!(
                backend.expose(monitor_core::families::BATTERY_CYCLES),
                super::super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    ..
                }
            ));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// Windows backend: GetSystemPowerStatus.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows_backend {
    use super::{BatterySnapshot, PowerStatus, TimeEstimate};
    use monitor_core::CollectorError;
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    #[derive(Debug, Default)]
    pub struct WindowsBattery;

    const BATTERY_LIFETIME_UNKNOWN: u32 = 0xFFFF_FFFF;

    impl WindowsBattery {
        pub fn new() -> Self {
            Self
        }

        fn query(&self) -> Option<SYSTEM_POWER_STATUS> {
            let mut status = SYSTEM_POWER_STATUS::default();
            // SAFETY: output-pointer call per Win32 contract.
            unsafe { GetSystemPowerStatus(&mut status) }.ok().ok()?;
            Some(status)
        }

        pub fn battery_present(&self) -> bool {
            let Some(status) = self.query() else {
                return false;
            };
            // BatteryFlag 128 = no system battery; 255 = unknown.
            status.BatteryFlag & 128 == 0 && status.BatteryFlag != 255
        }

        pub fn expose(&self, family: &str) -> super::Expose {
            use monitor_core::{families, reasons};
            if !self.battery_present() {
                return super::Expose::NoBattery;
            }
            match family {
                families::BATTERY_CHARGE | families::BATTERY_STATUS => super::Expose::Available,
                families::BATTERY_TIME_REMAINING => super::Expose::Available,
                families::BATTERY_CYCLES => super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    explanation: "Windows power APIs do not publicly expose battery cycle \
                                  counts; reporting a guess is not allowed.",
                },
                families::BATTERY_HEALTH => super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    explanation: "Windows does not publicly expose full-charge and design \
                                  capacity through its standard power API, so health cannot \
                                  be computed without guessing.",
                },
                families::BATTERY_POWER_DRAW => super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    explanation: "Windows exposes no instantaneous battery drain rate through \
                                  its standard power API.",
                },
                _ => super::Expose::Unsupported {
                    reason: reasons::NOT_EXPOSED_BY_OS_API,
                    explanation: "This battery datum is not exposed by the Windows power API.",
                },
            }
        }

        pub fn sample(&mut self) -> Result<Option<BatterySnapshot>, CollectorError> {
            let Some(status) = self.query() else {
                return Err(CollectorError::Io {
                    collector: COLLECTOR_ID,
                    message: "GetSystemPowerStatus failed".to_string(),
                });
            };
            if status.BatteryFlag & 128 != 0 || status.BatteryFlag == 255 {
                return Ok(None);
            }
            let on_ac = status.ACLineStatus == 1;
            let charge = if status.BatteryLifePercent == 255 {
                None
            } else {
                Some(status.BatteryLifePercent as f64)
            };
            let is_charging = status.BatteryFlag & 8 != 0;
            let power_status = if status.BatteryLifePercent == 100 && on_ac {
                PowerStatus::Full
            } else if is_charging {
                PowerStatus::Charging
            } else if on_ac {
                PowerStatus::NotCharging
            } else {
                PowerStatus::Discharging
            };
            let time_remaining = if on_ac {
                Some(TimeEstimate::Unlimited)
            } else if status.BatteryLifeTime == BATTERY_LIFETIME_UNKNOWN {
                Some(TimeEstimate::Unknown)
            } else {
                Some(TimeEstimate::Seconds(status.BatteryLifeTime as f64))
            };
            Ok(Some(BatterySnapshot {
                charge_percent: charge,
                status: Some(power_status),
                time_remaining,
                ..BatterySnapshot::default()
            }))
        }
    }

    impl Default for WindowsBattery {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::FakeClock;

    #[test]
    fn health_math_and_guards() {
        // Phase 00/05 measured values from this MacBook: 7298/8579 mAh.
        let health = BatterySnapshot::health_from_capacities(7298.0, 8579.0).expect("health");
        assert!((health - 85.07).abs() < 0.05, "health {health}");
        assert_eq!(BatterySnapshot::health_from_capacities(0.0, 8579.0), None);
        assert_eq!(BatterySnapshot::health_from_capacities(-1.0, 8579.0), None);
        assert_eq!(BatterySnapshot::health_from_capacities(100.0, 0.0), None);
        assert_eq!(
            BatterySnapshot::health_from_capacities(f64::NAN, 100.0),
            None
        );
    }

    #[test]
    fn os_status_words_map_including_unknowns() {
        assert_eq!(
            PowerStatus::parse_os_status("Charging"),
            PowerStatus::Charging
        );
        assert_eq!(
            PowerStatus::parse_os_status("Not charging"),
            PowerStatus::NotCharging
        );
        assert_eq!(PowerStatus::parse_os_status("bogus"), PowerStatus::Unknown);
    }

    #[test]
    fn capabilities_cover_all_battery_families() {
        let collector = BatteryCollector::new();
        let caps = collector.capabilities();
        let expected = [
            families::BATTERY_CHARGE,
            families::BATTERY_STATUS,
            families::BATTERY_CYCLES,
            families::BATTERY_HEALTH,
            families::BATTERY_TIME_REMAINING,
            families::BATTERY_POWER_DRAW,
        ];
        assert_eq!(caps.len(), expected.len());
        for (cap, family) in caps.iter().zip(expected) {
            assert_eq!(cap.metric_id, family);
        }
        if cfg!(target_os = "macos") {
            // This MacBook: battery present with real sources for every family.
            assert!(caps.iter().all(|c| c.state == Availability::Available));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn live_macos_battery_reports_real_registry_values() {
        let mut collector = BatteryCollector::new();
        let readings = collector.sample(&FakeClock::default()).expect("sample ok");
        assert!(!readings.is_empty(), "this MacBook has a battery");
        let charge = readings
            .iter()
            .find(|r| r.metric_id == families::BATTERY_CHARGE)
            .expect("charge reading");
        let Some(MetricValue::Number(charge)) = charge.value else {
            panic!("charge must be numeric");
        };
        assert!((0.0..=100.0).contains(&charge), "charge {charge}");
        // Phase 00 measured CycleCount=180 on this machine; assert only the
        // plausible band so the test survives aging hardware.
        let cycles = readings
            .iter()
            .find(|r| r.metric_id == families::BATTERY_CYCLES)
            .expect("cycles reading");
        let Some(MetricValue::Number(cycles)) = cycles.value else {
            panic!("cycles must be numeric");
        };
        assert!((0.0..=5000.0).contains(&cycles), "cycles {cycles}");
        let health = readings
            .iter()
            .find(|r| r.metric_id == families::BATTERY_HEALTH)
            .expect("health reading");
        let Some(MetricValue::Number(health)) = health.value else {
            panic!("health must be numeric");
        };
        assert!((0.0..=150.0).contains(&health), "health {health}");
        // Time remaining must never be a 65535 sentinel leak.
        let time = readings
            .iter()
            .find(|r| r.metric_id == families::BATTERY_TIME_REMAINING)
            .expect("time reading");
        if time.availability == Availability::Available {
            let Some(MetricValue::Number(secs)) = time.value else {
                panic!("time must be numeric when available");
            };
            assert!(secs > 0.0 && secs < 24.0 * 3600.0 * 4.0, "secs {secs}");
        } else {
            assert_eq!(time.value, None, "unavailable time carries null");
        }
    }
}
