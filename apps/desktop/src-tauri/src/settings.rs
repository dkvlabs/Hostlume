//! Settings v2 schema, validation bounds and the user-settable patch.
//!
//! Settings live in SQLite via the versioned migration runner
//! (`settings_store.rs`); there is no second source of truth (ADR-0002).
//! v2 adds the Overview card layout (FX-005); v1 rows migrate at load with
//! defaults appended (`settings_store::load_settings`).

use serde::{Deserialize, Serialize};

pub const SETTINGS_SCHEMA_VERSION: u32 = 3;

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
pub const OVERVIEW_CARD_IDS: &[&str] = &["cpu", "memory", "storage", "network", "host"];

/// History retention bounds. 0 means "keep this tier forever"; the defaults
/// mirror the contract proposal (raw 24h, 1m 30d, 1h 365d).
pub const RAW_HOURS_MAX: u32 = 720;
pub const ROLLUP_1M_DAYS_MAX: u32 = 730;
pub const ROLLUP_1H_DAYS_MAX: u32 = 3650;

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
    pub fn into_v3(self) -> Settings {
        Settings {
            schema_version: SETTINGS_SCHEMA_VERSION,
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
                id: "gpu".into(),
                visible: true,
            },
        ];
        assert!(matches!(
            s.validate(),
            Err(SettingsError::UnknownOverviewCard { id } ) if id == "gpu"
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
        let migrated = v2.into_v3();
        migrated.validate().expect("migrated settings valid");
        assert_eq!(migrated.schema_version, 3);
        assert_eq!(migrated.appearance, Appearance::Light);
        assert!(!migrated.overview_cards[0].visible);
        assert_eq!(
            migrated.history.raw_hours,
            monitor_history::DEFAULT_RAW_HOURS
        );
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
