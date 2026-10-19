//! Settings v2 schema, validation bounds and the user-settable patch.
//!
//! Settings live in SQLite via the versioned migration runner
//! (`settings_store.rs`); there is no second source of truth (ADR-0002).
//! v2 adds the Overview card layout (FX-005); v1 rows migrate at load with
//! defaults appended (`settings_store::load_settings`).

use serde::{Deserialize, Serialize};

pub const SETTINGS_SCHEMA_VERSION: u32 = 7;

/// Safe polling bounds (seconds). The contract's base tick is 3 s; anything
/// below 1 s would fight the documented PDH/collectors cost profile, and
/// anything above a minute defeats "live monitoring".
pub const POLLING_MIN_SECS: u32 = 1;
pub const POLLING_MAX_SECS: u32 = 60;
pub const POLLING_DEFAULT_SECS: u32 = 3;

/// Locale key registry. UI ships English; Romanian catalogs are prepared
/// (requirements.md: translation files from day one).
pub const SUPPORTED_LOCALES: &[&str] = &["en", "ro"];
pub const DEFAULT_LOCALE: &str = "en";

/// Overview card ids (FX-005). Order in `overview_cards` is display order.
/// Phase 05 adds the hardware-conditional cards; the UI auto-hides them
/// behind capability notices when their data is unsupported (design.md §5).
pub const OVERVIEW_CARD_IDS: &[&str] = &[
    "cpu", "memory", "storage", "network", "host", "gpu", "sensors", "battery",
];

/// Temperature display unit (design.md §9: °C/°F setting).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

/// Tray configuration (phase 06 §2.3: persisted visible indicators). The
/// tray reads the shared snapshot bus — its "update rate" is the base
/// polling cadence, never a second timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayConfig {
    pub enabled: bool,
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_network: bool,
    /// macOS menu-bar icon text (compact CPU percentage).
    pub show_title: bool,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_cpu: true,
            show_memory: true,
            show_network: true,
            show_title: true,
        }
    }
}

/// Alert settings (phase 07 §3.4/§3.8). Notification delivery defaults on;
/// **local action workflows default off** — the master gate plus the
/// per-rule approval are both required before the runner will ever spawn a
/// configured program (security-privacy.md §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertsConfig {
    pub notifications_enabled: bool,
    pub actions_enabled: bool,
    pub quiet_hours_enabled: bool,
    /// Minute of local day (0..=1439); `start > end` wraps midnight.
    pub quiet_start_minute: u16,
    pub quiet_end_minute: u16,
}

pub const QUIET_MINUTE_MAX: u16 = 23 * 60 + 59;

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            actions_enabled: false,
            quiet_hours_enabled: false,
            quiet_start_minute: 22 * 60,
            quiet_end_minute: 8 * 60,
        }
    }
}

/// History retention bounds. 0 means "keep this tier forever"; the defaults
/// mirror the contract proposal (raw 24h, 1m 30d, 1h 365d).
pub const RAW_HOURS_MAX: u32 = 720;
pub const ROLLUP_1M_DAYS_MAX: u32 = 730;
pub const ROLLUP_1H_DAYS_MAX: u32 = 3650;

/// Display preferences (phase 04 §3.11): rates as bytes/s or bits/s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateUnit {
    #[default]
    Bytes,
    Bits,
}

/// External IP lookup: **off by default**; when enabled the app names the
/// provider and performs an explicit HTTPS lookup (security-privacy.md §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalLookup {
    pub enabled: bool,
    /// Fixed, documented provider; no arbitrary user URLs.
    pub provider: String,
    pub timeout_secs: u32,
}

impl Default for ExternalLookup {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "ipify".to_string(),
            timeout_secs: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryRetention {
    pub raw_hours: u32,
    pub rollup_1m_days: u32,
    pub rollup_1h_days: u32,
}

impl Default for HistoryRetention {
    fn default() -> Self {
        Self {
            raw_hours: monitor_history::DEFAULT_RAW_HOURS,
            rollup_1m_days: monitor_history::DEFAULT_ROLLUP_1M_DAYS,
            rollup_1h_days: monitor_history::DEFAULT_ROLLUP_1H_DAYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverviewCard {
    pub id: String,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    /// Stored now; the close-to-tray behavior becomes active when the tray
    /// integration lands (Phase 06). Until then the app always exits on
    /// window close, regardless of this flag.
    pub close_to_tray: bool,
    pub locale: String,
    pub overview_cards: Vec<OverviewCard>,
    pub history: HistoryRetention,
    pub rate_unit: RateUnit,
    pub external_lookup: ExternalLookup,
    pub temperature_unit: TemperatureUnit,
    pub tray: TrayConfig,
    /// Launch at login (tauri-plugin-autostart; OS login item applied when
    /// the setting changes — never silently at startup).
    pub launch_at_login: bool,
    /// Alert notifications, quiet hours and the local-action master gate.
    pub alerts: AlertsConfig,
}

pub fn default_overview_cards() -> Vec<OverviewCard> {
    OVERVIEW_CARD_IDS
        .iter()
        .map(|id| OverviewCard {
            id: (*id).to_string(),
            visible: true,
        })
        .collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
            overview_cards: default_overview_cards(),
            history: HistoryRetention::default(),
            rate_unit: RateUnit::default(),
            external_lookup: ExternalLookup::default(),
            temperature_unit: TemperatureUnit::default(),
            tray: TrayConfig::default(),
            launch_at_login: false,
            alerts: AlertsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("polling interval {value}s is outside the safe bounds [{min}, {max}]s")]
    PollingOutOfBounds { value: u32, min: u32, max: u32 },
    #[error("unsupported locale {locale:?}; supported: {supported:?}")]
    UnsupportedLocale {
        locale: String,
        supported: Vec<String>,
    },
    #[error("unsupported settings schema version {found} (this build supports {supported})")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("overview card {id:?} is unknown")]
    UnknownOverviewCard { id: String },
    #[error("overview card {id:?} appears more than once")]
    DuplicateOverviewCard { id: String },
    #[error("overview card list must not be empty")]
    EmptyOverviewCards,
    #[error("history retention {field}={value} is outside the supported range")]
    HistoryRetentionOutOfBounds { field: &'static str, value: u32 },
    #[error("external lookup timeout {value}s is outside 1..=30")]
    ExternalLookupTimeoutOutOfBounds { value: u32 },
    #[error("quiet-hours minute {value} is outside 0..={max}")]
    QuietMinuteOutOfBounds { value: u16, max: u16 },
}

impl Settings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(SettingsError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: SETTINGS_SCHEMA_VERSION,
            });
        }
        if !(POLLING_MIN_SECS..=POLLING_MAX_SECS).contains(&self.polling_base_seconds) {
            return Err(SettingsError::PollingOutOfBounds {
                value: self.polling_base_seconds,
                min: POLLING_MIN_SECS,
                max: POLLING_MAX_SECS,
            });
        }
        if !SUPPORTED_LOCALES.contains(&self.locale.as_str()) {
            return Err(SettingsError::UnsupportedLocale {
                locale: self.locale.clone(),
                supported: SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect(),
            });
        }
        if self.overview_cards.is_empty() {
            return Err(SettingsError::EmptyOverviewCards);
        }
        let mut seen = Vec::new();
        for card in &self.overview_cards {
            if !OVERVIEW_CARD_IDS.contains(&card.id.as_str()) {
                return Err(SettingsError::UnknownOverviewCard {
                    id: card.id.clone(),
                });
            }
            if seen.contains(&card.id) {
                return Err(SettingsError::DuplicateOverviewCard {
                    id: card.id.clone(),
                });
            }
            seen.push(card.id.clone());
        }
        self.history.validate()?;
        if self.external_lookup.timeout_secs == 0 || self.external_lookup.timeout_secs > 30 {
            return Err(SettingsError::ExternalLookupTimeoutOutOfBounds {
                value: self.external_lookup.timeout_secs,
            });
        }
        if self.external_lookup.enabled && self.external_lookup.provider != "ipify" {
            return Err(SettingsError::UnsupportedLocale {
                locale: format!("provider:{}", self.external_lookup.provider),
                supported: vec!["ipify".to_string()],
            });
        }
        if self.alerts.quiet_start_minute > QUIET_MINUTE_MAX
            || self.alerts.quiet_end_minute > QUIET_MINUTE_MAX
        {
            return Err(SettingsError::QuietMinuteOutOfBounds {
                value: self
                    .alerts
                    .quiet_start_minute
                    .max(self.alerts.quiet_end_minute),
                max: QUIET_MINUTE_MAX,
            });
        }
        Ok(())
    }

    /// Merge a user patch onto this instance. Callers must `validate()`
    /// afterwards and persist only on success (all-or-nothing update).
    pub fn apply_patch(&mut self, patch: SettingsPatch) {
        if let Some(appearance) = patch.appearance {
            self.appearance = appearance;
        }
        if let Some(compact) = patch.compact_mode {
            self.compact_mode = compact;
        }
        if let Some(polling) = patch.polling_base_seconds {
            self.polling_base_seconds = polling;
        }
        if let Some(close_to_tray) = patch.close_to_tray {
            self.close_to_tray = close_to_tray;
        }
        if let Some(locale) = patch.locale {
            self.locale = locale;
        }
        if let Some(overview_cards) = patch.overview_cards {
            self.overview_cards = overview_cards;
        }
        if let Some(history) = patch.history {
            self.history = history;
        }
        if let Some(rate_unit) = patch.rate_unit {
            self.rate_unit = rate_unit;
        }
        if let Some(temperature_unit) = patch.temperature_unit {
            self.temperature_unit = temperature_unit;
        }
        if let Some(external_lookup) = patch.external_lookup {
            self.external_lookup = external_lookup;
        }
        if let Some(alerts) = patch.alerts {
            self.alerts = alerts;
        }
    }
}

impl HistoryRetention {
    pub fn validate(&self) -> Result<(), SettingsError> {
        let checks: [(&'static str, u32, u32); 3] = [
            ("raw_hours", self.raw_hours, RAW_HOURS_MAX),
            ("rollup_1m_days", self.rollup_1m_days, ROLLUP_1M_DAYS_MAX),
            ("rollup_1h_days", self.rollup_1h_days, ROLLUP_1H_DAYS_MAX),
        ];
        for (field, value, max) in checks {
            if value > max {
                return Err(SettingsError::HistoryRetentionOutOfBounds { field, value });
            }
        }
        Ok(())
    }
}

/// User-settable subset of [`Settings`]. `schema_version` is intentionally
/// not patchable; unknown keys are rejected so client bugs surface loudly.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsPatch {
    pub appearance: Option<Appearance>,
    pub compact_mode: Option<bool>,
    pub polling_base_seconds: Option<u32>,
    pub close_to_tray: Option<bool>,
    pub locale: Option<String>,
    pub overview_cards: Option<Vec<OverviewCard>>,
    pub history: Option<HistoryRetention>,
    pub rate_unit: Option<RateUnit>,
    pub external_lookup: Option<ExternalLookup>,
    pub temperature_unit: Option<TemperatureUnit>,
    pub tray: Option<TrayConfig>,
    pub launch_at_login: Option<bool>,
    pub alerts: Option<AlertsConfig>,
}

/// The v5 row shape (adds temperature unit + Phase 05 overview cards;
/// predates tray configuration and launch-at-login).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsV5 {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    pub close_to_tray: bool,
    pub locale: String,
    pub overview_cards: Vec<OverviewCard>,
    pub history: HistoryRetention,
    pub rate_unit: RateUnit,
    pub external_lookup: ExternalLookup,
    pub temperature_unit: TemperatureUnit,
}

impl Default for SettingsV5 {
    fn default() -> Self {
        Self {
            schema_version: 5,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
            overview_cards: default_overview_cards(),
            history: HistoryRetention::default(),
            rate_unit: RateUnit::default(),
            external_lookup: ExternalLookup::default(),
            temperature_unit: TemperatureUnit::default(),
        }
    }
}

impl SettingsV5 {
    pub fn into_v6(self) -> SettingsV6 {
        SettingsV6 {
            schema_version: 6,
            appearance: self.appearance,
            compact_mode: self.compact_mode,
            polling_base_seconds: self.polling_base_seconds,
            close_to_tray: self.close_to_tray,
            locale: self.locale,
            overview_cards: self.overview_cards,
            history: self.history,
            rate_unit: self.rate_unit,
            external_lookup: self.external_lookup,
            temperature_unit: self.temperature_unit,
            tray: TrayConfig::default(),
            launch_at_login: false,
        }
    }
}

/// The v6 row shape (adds tray configuration and launch-at-login; predates
/// the alert settings of phase 07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsV6 {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    pub close_to_tray: bool,
    pub locale: String,
    pub overview_cards: Vec<OverviewCard>,
    pub history: HistoryRetention,
    pub rate_unit: RateUnit,
    pub external_lookup: ExternalLookup,
    pub temperature_unit: TemperatureUnit,
    pub tray: TrayConfig,
    pub launch_at_login: bool,
}

impl Default for SettingsV6 {
    fn default() -> Self {
        Self {
            schema_version: 6,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
            overview_cards: default_overview_cards(),
            history: HistoryRetention::default(),
            rate_unit: RateUnit::default(),
            external_lookup: ExternalLookup::default(),
            temperature_unit: TemperatureUnit::default(),
            tray: TrayConfig::default(),
            launch_at_login: false,
        }
    }
}

impl SettingsV6 {
    pub fn into_v7(self) -> Settings {
        Settings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            appearance: self.appearance,
            compact_mode: self.compact_mode,
            polling_base_seconds: self.polling_base_seconds,
            close_to_tray: self.close_to_tray,
            locale: self.locale,
            overview_cards: self.overview_cards,
            history: self.history,
            rate_unit: self.rate_unit,
            external_lookup: self.external_lookup,
            temperature_unit: self.temperature_unit,
            tray: self.tray,
            launch_at_login: self.launch_at_login,
            alerts: AlertsConfig::default(),
        }
    }
}

/// The v4 row shape (adds display preferences; predates the temperature
/// unit and the Phase 05 overview cards).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsV4 {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    pub close_to_tray: bool,
    pub locale: String,
    pub overview_cards: Vec<OverviewCard>,
    pub history: HistoryRetention,
    pub rate_unit: RateUnit,
    pub external_lookup: ExternalLookup,
}

impl Default for SettingsV4 {
    fn default() -> Self {
        Self {
            schema_version: 4,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
            overview_cards: default_overview_cards_v4(),
            history: HistoryRetention::default(),
            rate_unit: RateUnit::default(),
            external_lookup: ExternalLookup::default(),
        }
    }
}

/// The v4-era card list (before the gpu/sensors/battery cards existed).
fn default_overview_cards_v4() -> Vec<OverviewCard> {
    ["cpu", "memory", "storage", "network", "host"]
        .iter()
        .map(|id| OverviewCard {
            id: (*id).to_string(),
            visible: true,
        })
        .collect()
}

impl SettingsV4 {
    /// Carry every v4 field forward; the Phase 05 additions take defaults
    /// and missing overview cards are appended in canonical order so user
    /// layout choices survive the migration.
    pub fn into_v5(self) -> SettingsV5 {
        let mut overview_cards = self.overview_cards;
        for id in OVERVIEW_CARD_IDS {
            if !overview_cards.iter().any(|card| &card.id == id) {
                overview_cards.push(OverviewCard {
                    id: (*id).to_string(),
                    visible: true,
                });
            }
        }
        SettingsV5 {
            schema_version: 5,
            appearance: self.appearance,
            compact_mode: self.compact_mode,
            polling_base_seconds: self.polling_base_seconds,
            close_to_tray: self.close_to_tray,
            locale: self.locale,
            overview_cards,
            history: self.history,
            rate_unit: self.rate_unit,
            external_lookup: self.external_lookup,
            temperature_unit: TemperatureUnit::default(),
        }
    }
}

/// The v3 row shape (adds history retention; predates display prefs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsV3 {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    pub close_to_tray: bool,
    pub locale: String,
    pub overview_cards: Vec<OverviewCard>,
    pub history: HistoryRetention,
}

impl Default for SettingsV3 {
    fn default() -> Self {
        Self {
            schema_version: 3,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
            overview_cards: default_overview_cards(),
            history: HistoryRetention::default(),
        }
    }
}

impl SettingsV3 {
    pub fn into_v4(self) -> SettingsV4 {
        SettingsV4 {
            schema_version: 4,
            appearance: self.appearance,
            compact_mode: self.compact_mode,
            polling_base_seconds: self.polling_base_seconds,
            close_to_tray: self.close_to_tray,
            locale: self.locale,
            overview_cards: self.overview_cards,
            history: self.history,
            rate_unit: RateUnit::default(),
            external_lookup: ExternalLookup::default(),
        }
    }
}

/// The v2 row shape (adds `overview_cards`, predates `history`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsV2 {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    pub close_to_tray: bool,
    pub locale: String,
    pub overview_cards: Vec<OverviewCard>,
}

impl Default for SettingsV2 {
    fn default() -> Self {
        Self {
            schema_version: 2,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
            overview_cards: default_overview_cards(),
        }
    }
}

impl SettingsV2 {
    pub fn into_v3(self) -> SettingsV3 {
        SettingsV3 {
            schema_version: 3,
            appearance: self.appearance,
            compact_mode: self.compact_mode,
            polling_base_seconds: self.polling_base_seconds,
            close_to_tray: self.close_to_tray,
            locale: self.locale,
            overview_cards: self.overview_cards,
            history: HistoryRetention::default(),
        }
    }
}

/// The v1 row shape, kept verbatim for the load-time migration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsV1 {
    pub schema_version: u32,
    pub appearance: Appearance,
    pub compact_mode: bool,
    pub polling_base_seconds: u32,
    pub close_to_tray: bool,
    pub locale: String,
}

impl Default for SettingsV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            appearance: Appearance::System,
            compact_mode: false,
            polling_base_seconds: POLLING_DEFAULT_SECS,
            close_to_tray: false,
            locale: DEFAULT_LOCALE.to_string(),
        }
    }
}

impl SettingsV1 {
    /// Carry every v1 field forward; later additions take defaults.
    pub fn into_latest(self) -> Settings {
        SettingsV2 {
            schema_version: 2,
            appearance: self.appearance,
            compact_mode: self.compact_mode,
            polling_base_seconds: self.polling_base_seconds,
            close_to_tray: self.close_to_tray,
            locale: self.locale,
            overview_cards: default_overview_cards(),
        }
        .into_v3()
        .into_v4()
        .into_v5()
        .into_v6()
        .into_v7()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let s = Settings::default();
        s.validate().expect("defaults must satisfy schema v2");
        assert_eq!(s.appearance, Appearance::System);
        assert_eq!(s.polling_base_seconds, POLLING_DEFAULT_SECS);
        assert_eq!(s.locale, "en");
        assert_eq!(s.overview_cards.len(), OVERVIEW_CARD_IDS.len());
    }

    #[test]
    fn polling_bounds_are_enforced() {
        let mut s = Settings {
            polling_base_seconds: 0,
            ..Settings::default()
        };
        assert!(matches!(
            s.validate(),
            Err(SettingsError::PollingOutOfBounds { value: 0, .. })
        ));
        s.polling_base_seconds = POLLING_MIN_SECS;
        assert!(s.validate().is_ok());
        s.polling_base_seconds = POLLING_MAX_SECS;
        assert!(s.validate().is_ok());
        s.polling_base_seconds = POLLING_MAX_SECS + 1;
        assert!(s.validate().is_err());
    }

    #[test]
    fn locale_registry_is_enforced() {
        let mut s = Settings {
            locale: "de".into(),
            ..Settings::default()
        };
        assert!(matches!(
            s.validate(),
            Err(SettingsError::UnsupportedLocale { .. })
        ));
        s.locale = "ro".into();
        assert!(s.validate().is_ok());
    }

    #[test]
    fn overview_card_rules_are_enforced() {
        let mut s = Settings {
            overview_cards: Vec::new(),
            ..Settings::default()
        };
        assert!(matches!(
            s.validate(),
            Err(SettingsError::EmptyOverviewCards)
        ));
        s.overview_cards = vec![OverviewCard {
            id: "cpu".into(),
            visible: true,
        }];
        assert!(s.validate().is_ok());
        s.overview_cards = vec![
            OverviewCard {
                id: "cpu".into(),
                visible: true,
            },
            OverviewCard {
                id: "definitely-not-a-card".into(),
                visible: true,
            },
        ];
        assert!(matches!(
            s.validate(),
            Err(SettingsError::UnknownOverviewCard { id } ) if id == "definitely-not-a-card"
        ));
        s.overview_cards = vec![
            OverviewCard {
                id: "cpu".into(),
                visible: true,
            },
            OverviewCard {
                id: "cpu".into(),
                visible: false,
            },
        ];
        assert!(matches!(
            s.validate(),
            Err(SettingsError::DuplicateOverviewCard { id }) if id == "cpu"
        ));
    }

    #[test]
    fn patch_applies_and_old_version_is_rejected() {
        let mut s = Settings::default();
        s.apply_patch(SettingsPatch {
            appearance: Some(Appearance::Dark),
            polling_base_seconds: Some(5),
            overview_cards: Some(vec![
                OverviewCard {
                    id: "network".into(),
                    visible: true,
                },
                OverviewCard {
                    id: "cpu".into(),
                    visible: false,
                },
            ]),
            ..Default::default()
        });
        s.validate().expect("patched settings valid");
        assert_eq!(s.appearance, Appearance::Dark);
        assert_eq!(s.polling_base_seconds, 5);
        assert_eq!(s.overview_cards[0].id, "network", "order is preserved");
        assert!(!s.overview_cards[1].visible);
        assert!(!s.compact_mode, "unpatched fields must stay untouched");

        s.schema_version = SETTINGS_SCHEMA_VERSION + 1;
        assert!(matches!(
            s.validate(),
            Err(SettingsError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn patch_rejects_unknown_fields() {
        let raw = r#"{"bogus": true}"#;
        let parsed: Result<SettingsPatch, _> = serde_json::from_str(raw);
        assert!(parsed.is_err(), "unknown patch keys must be refused");
    }

    #[test]
    fn v1_settings_migrate_to_latest_with_defaults() {
        let v1 = SettingsV1 {
            appearance: Appearance::Dark,
            polling_base_seconds: 7,
            locale: "ro".into(),
            ..SettingsV1::default()
        };
        let migrated = v1.clone().into_latest();
        migrated.validate().expect("migrated settings valid");
        assert_eq!(migrated.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated.appearance, Appearance::Dark);
        assert_eq!(migrated.polling_base_seconds, 7);
        assert_eq!(migrated.locale, "ro");
        assert_eq!(migrated.overview_cards, default_overview_cards());
        assert_eq!(migrated.history, HistoryRetention::default());
        assert_eq!(migrated.alerts, AlertsConfig::default());
        // Old rows deserialize as v1 exactly (round-trip safety of fixture).
        let json = serde_json::to_string(&v1).unwrap();
        let back: SettingsV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v1);
    }

    #[test]
    fn v2_settings_migrate_to_v3_with_history_defaults() {
        let v2 = SettingsV2 {
            appearance: Appearance::Light,
            overview_cards: vec![OverviewCard {
                id: "cpu".into(),
                visible: false,
            }],
            ..SettingsV2::default()
        };
        let migrated = v2.into_v3().into_v4().into_v5().into_v6().into_v7();
        migrated.validate().expect("migrated settings valid");
        assert_eq!(migrated.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated.appearance, Appearance::Light);
        assert!(!migrated.overview_cards[0].visible);
        assert_eq!(
            migrated.history.raw_hours,
            monitor_history::DEFAULT_RAW_HOURS
        );
    }

    #[test]
    fn v5_settings_migrate_to_v6_with_tray_defaults() {
        let v5 = SettingsV5 {
            appearance: Appearance::Dark,
            polling_base_seconds: 7,
            ..SettingsV5::default()
        };
        let migrated = v5.into_v6().into_v7();
        migrated.validate().expect("migrated settings valid");
        assert_eq!(migrated.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated.polling_base_seconds, 7);
        assert!(migrated.tray.enabled, "tray defaults on");
        assert!(!migrated.launch_at_login, "opt-in stays off");
    }

    #[test]
    fn v6_settings_migrate_to_v7_with_alert_defaults() {
        let v6 = SettingsV6 {
            appearance: Appearance::Dark,
            polling_base_seconds: 9,
            ..SettingsV6::default()
        };
        let migrated = v6.into_v7();
        migrated.validate().expect("migrated settings valid");
        assert_eq!(migrated.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated.polling_base_seconds, 9);
        assert!(
            migrated.alerts.notifications_enabled,
            "notifications default on"
        );
        assert!(!migrated.alerts.actions_enabled, "local actions stay OFF");
        assert!(!migrated.alerts.quiet_hours_enabled);
    }

    #[test]
    fn quiet_hours_minutes_are_bounded() {
        let mut s = Settings::default();
        s.alerts.quiet_start_minute = QUIET_MINUTE_MAX + 1;
        assert!(matches!(
            s.validate(),
            Err(SettingsError::QuietMinuteOutOfBounds { .. })
        ));
        s.alerts.quiet_start_minute = 22 * 60;
        s.alerts.quiet_end_minute = 8 * 60;
        assert!(s.validate().is_ok(), "start > end wraps midnight");
    }

    #[test]
    fn v4_settings_migrate_to_v5_appending_new_cards() {
        let v4 = SettingsV4 {
            appearance: Appearance::Dark,
            overview_cards: vec![OverviewCard {
                id: "cpu".into(),
                visible: false,
            }],
            ..SettingsV4::default()
        };
        let migrated = v4.into_v5().into_v6().into_v7();
        migrated.validate().expect("migrated settings valid");
        assert_eq!(migrated.schema_version, SETTINGS_SCHEMA_VERSION);
        assert!(!migrated.overview_cards[0].visible);
        // User order preserved; Phase 05 cards appended in canonical order.
        let ids: Vec<&str> = migrated
            .overview_cards
            .iter()
            .map(|card| card.id.as_str())
            .collect();
        for required in ["gpu", "sensors", "battery"] {
            assert!(ids.contains(&required), "missing card {required}");
        }
        assert_eq!(
            migrated.temperature_unit,
            TemperatureUnit::default(),
            "new display preference takes its default"
        );
    }

    #[test]
    fn temperature_unit_patch_applies() {
        let mut s = Settings::default();
        s.apply_patch(SettingsPatch {
            temperature_unit: Some(TemperatureUnit::Fahrenheit),
            ..Default::default()
        });
        s.validate().expect("patched settings valid");
        assert_eq!(s.temperature_unit, TemperatureUnit::Fahrenheit);
    }

    #[test]
    fn history_retention_bounds_are_enforced() {
        let mut s = Settings::default();
        s.history.raw_hours = RAW_HOURS_MAX + 1;
        assert!(matches!(
            s.validate(),
            Err(SettingsError::HistoryRetentionOutOfBounds {
                field: "raw_hours",
                ..
            })
        ));
        s.history.raw_hours = 0; // keep-forever is legal
        assert!(s.validate().is_ok());
    }
}
