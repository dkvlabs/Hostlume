//! Alert rule model and the strict validator (phase 07 §3.1).
//!
//! A rule is a persisted user configuration; the validator is the security
//! and honesty boundary between the WebView and the engine. Every field is
//! bounds-checked, thresholds must be unit-compatible with the watched
//! family (see `monitor_core::families::numeric_unit`), and the optional
//! local action stores only a literal program path and literal argument
//! array — metric values are never part of an action's command line.

use monitor_core::families::numeric_unit;
use monitor_core::MetricUnit;
use serde::{Deserialize, Serialize};

/// Upper bound on stored rules; keeps evaluation O(1) per snapshot tick.
pub const MAX_RULES: usize = 50;

pub const MAX_HOLD_MS: u64 = 3_600_000;
pub const MAX_COOLDOWN_MS: u64 = 86_400_000;

const MAX_ID_LEN: usize = 64;
const MAX_ENTITY_FILTER_LEN: usize = 128;
const MAX_NOTIFICATION_MESSAGE_LEN: usize = 500;
const MAX_PROGRAM_LEN: usize = 1024;
const MAX_TIMEOUT_SECS: u32 = 60;
/// Core-equivalent process CPU can reach `100 × logical cores`; the
/// validator allows a generous fixed ceiling instead of runtime core counts.
const CORE_EQUIVALENT_PERCENT_MAX: f64 = 32_768.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Comparator {
    Above,
    Below,
}

/// Optional approved local workflow. **Disabled by default twice over**: the
/// global settings gate must be on, and this action must be individually
/// approved, before the runner will ever launch it (phase 07 §3.6).
///
/// Scope note (ADR-0007): this phase launches the approved program **without
/// an argument array** — the project security tooling refuses variable
/// argument-list spawns, so multi-step logic belongs in an approved script.
/// The program path is absolute and never resolved through `PATH`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionConfig {
    /// Absolute executable/script path. Never resolved through `PATH`,
    /// never a shell string — the runner passes it as the launcher's
    /// argument.
    pub program: String,
    /// Hard wall-clock timeout; the child is killed when it elapses.
    pub timeout_secs: u32,
    /// Set only through the explicit local approval flow.
    pub approved: bool,
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            program: String::new(),
            timeout_secs: 10,
            approved: false,
        }
    }
}

/// One user-configured alert rule (phase 07 §2 model). `notification_message`
/// is the phase model's notification template: an optional user-defined body
/// supporting `{value}`, `{entity}` and `{threshold}` placeholders.
/// Substitution happens for notification text only — never for actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertRule {
    /// Empty for a create request (the app assigns a UUID); non-empty for
    /// update requests. Server-side identity, never client-chosen.
    pub id: String,
    pub metric_id: String,
    /// `None` watches any entity: the first entity whose condition holds
    /// fires (one notification, never one per entity — storm-safe).
    pub entity_filter: Option<String>,
    pub comparator: Comparator,
    pub threshold: f64,
    /// Sustained duration the crossing must hold before firing.
    pub hold_duration_ms: u64,
    /// Recovery margin: while firing, the value must clear beyond
    /// `threshold ∓ hysteresis_delta` before the rule resolves.
    pub hysteresis_delta: f64,
    /// Minimum time between fire episodes.
    pub cooldown_ms: u64,
    pub enabled: bool,
    pub notification_message: Option<String>,
    pub action: Option<ActionConfig>,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            id: String::new(),
            metric_id: String::new(),
            entity_filter: None,
            comparator: Comparator::Above,
            threshold: 0.0,
            hold_duration_ms: 0,
            hysteresis_delta: 0.0,
            cooldown_ms: 0,
            enabled: true,
            notification_message: None,
            action: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RuleError {
    #[error("rule id must be at most {max} characters")]
    IdTooLong { max: usize },
    #[error("metric {metric_id:?} is not a numeric alertable family")]
    MetricNotAlertable { metric_id: String },
    #[error("threshold {threshold} is outside the {unit:?} bounds [{min}, {max}]")]
    ThresholdOutOfBounds {
        threshold: f64,
        unit: MetricUnit,
        min: f64,
        max: f64,
    },
    #[error("hysteresis must be finite and non-negative")]
    HysteresisInvalid,
    #[error("hysteresis {hysteresis} exceeds the {unit:?} bounds maximum {max}")]
    HysteresisOutOfBounds {
        hysteresis: f64,
        unit: MetricUnit,
        max: f64,
    },
    #[error("hold duration {value_ms}ms is outside 0..={max_ms}")]
    HoldOutOfBounds { value_ms: u64, max_ms: u64 },
    #[error("cooldown {value_ms}ms is outside 0..={max_ms}")]
    CooldownOutOfBounds { value_ms: u64, max_ms: u64 },
    #[error("entity filter must be 1..={max} characters with no control characters")]
    EntityFilterInvalid { max: usize },
    #[error("notification message must be at most {max} characters")]
    NotificationMessageTooLong { max: usize },
    #[error("action program must be an absolute path of at most {max} characters")]
    ActionProgramInvalid { max: usize },
    #[error("action timeout {value}s is outside 1..={max}")]
    ActionTimeoutOutOfBounds { value: u32, max: u32 },
}

/// Unit-compatible threshold bounds. `%` thresholds are host-normalized
/// 0..=100 except the documented core-equivalent process CPU family.
fn threshold_bounds(metric_id: &str, unit: MetricUnit) -> (f64, f64) {
    match unit {
        MetricUnit::Percent => {
            if metric_id == monitor_core::families::CPU_PROCESS_USAGE_CORES {
                (0.0, CORE_EQUIVALENT_PERCENT_MAX)
            } else {
                (0.0, 100.0)
            }
        }
        MetricUnit::Bytes => (0.0, 1_125_899_906_842_624.0), // 2^50
        MetricUnit::BytesPerSecond | MetricUnit::BitsPerSecond => (0.0, 1_000_000_000_000.0),
        MetricUnit::Celsius => (-100.0, 250.0),
        MetricUnit::Rpm => (0.0, 100_000.0),
        MetricUnit::Seconds => (0.0, 315_360_000.0), // 10 years
        MetricUnit::Count => (0.0, 1_000_000_000_000.0),
        MetricUnit::Watts => (0.0, 10_000.0),
        MetricUnit::Hertz => (0.0, 1_000_000_000_000.0),
        // Non-numeric units never reach validation (registry gate below).
        MetricUnit::Boolean | MetricUnit::Text => (0.0, 0.0),
    }
}

/// Validate a rule for persistence. Empty ids are legal (create requests);
/// existence and duplicate checks live in the store layer.
pub fn validate(rule: &AlertRule) -> Result<(), RuleError> {
    if rule.id.len() > MAX_ID_LEN {
        return Err(RuleError::IdTooLong { max: MAX_ID_LEN });
    }
    let Some(unit) = numeric_unit(&rule.metric_id) else {
        return Err(RuleError::MetricNotAlertable {
            metric_id: rule.metric_id.clone(),
        });
    };
    if !rule.threshold.is_finite() {
        return Err(RuleError::ThresholdOutOfBounds {
            threshold: rule.threshold,
            unit,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
        });
    }
    let (min, max) = threshold_bounds(&rule.metric_id, unit);
    if !(min..=max).contains(&rule.threshold) {
        return Err(RuleError::ThresholdOutOfBounds {
            threshold: rule.threshold,
            unit,
            min,
            max,
        });
    }
    if !rule.hysteresis_delta.is_finite() || rule.hysteresis_delta < 0.0 {
        return Err(RuleError::HysteresisInvalid);
    }
    if rule.hysteresis_delta > max {
        return Err(RuleError::HysteresisOutOfBounds {
            hysteresis: rule.hysteresis_delta,
            unit,
            max,
        });
    }
    if rule.hold_duration_ms > MAX_HOLD_MS {
        return Err(RuleError::HoldOutOfBounds {
            value_ms: rule.hold_duration_ms,
            max_ms: MAX_HOLD_MS,
        });
    }
    if rule.cooldown_ms > MAX_COOLDOWN_MS {
        return Err(RuleError::CooldownOutOfBounds {
            value_ms: rule.cooldown_ms,
            max_ms: MAX_COOLDOWN_MS,
        });
    }
    if let Some(entity) = &rule.entity_filter {
        if entity.is_empty()
            || entity.len() > MAX_ENTITY_FILTER_LEN
            || entity.chars().any(char::is_control)
        {
            return Err(RuleError::EntityFilterInvalid {
                max: MAX_ENTITY_FILTER_LEN,
            });
        }
    }
    if let Some(message) = &rule.notification_message {
        if message.len() > MAX_NOTIFICATION_MESSAGE_LEN {
            return Err(RuleError::NotificationMessageTooLong {
                max: MAX_NOTIFICATION_MESSAGE_LEN,
            });
        }
    }
    if let Some(action) = &rule.action {
        validate_action(action)?;
    }
    Ok(())
}

/// Validate only the optional action configuration — the execution
/// boundary (`ApprovedAction::authorize`) re-runs exactly this on stored
/// configs without needing a synthetic rule.
pub fn validate_action(action: &ActionConfig) -> Result<(), RuleError> {
    let program = PathCheck(&action.program);
    if action.program.is_empty() || action.program.len() > MAX_PROGRAM_LEN || !program.is_absolute()
    {
        return Err(RuleError::ActionProgramInvalid {
            max: MAX_PROGRAM_LEN,
        });
    }
    if action.timeout_secs == 0 || action.timeout_secs > MAX_TIMEOUT_SECS {
        return Err(RuleError::ActionTimeoutOutOfBounds {
            value: action.timeout_secs,
            max: MAX_TIMEOUT_SECS,
        });
    }
    Ok(())
}

/// Tiny wrapper so the absolute-path check reads without importing
/// `std::path` at every call site.
struct PathCheck<'a>(&'a str);

impl PathCheck<'_> {
    fn is_absolute(&self) -> bool {
        std::path::Path::new(self.0).is_absolute()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_rule() -> AlertRule {
        AlertRule {
            metric_id: monitor_core::families::CPU_TOTAL_USAGE.to_string(),
            comparator: Comparator::Above,
            threshold: 90.0,
            hold_duration_ms: 60_000,
            hysteresis_delta: 10.0,
            cooldown_ms: 300_000,
            ..AlertRule::default()
        }
    }

    #[test]
    fn canonical_cpu_rule_is_valid() {
        assert_eq!(validate(&valid_rule()), Ok(()));
    }

    #[test]
    fn text_metrics_are_not_alertable() {
        let mut rule = valid_rule();
        rule.metric_id = monitor_core::families::HOST_HOSTNAME.to_string();
        assert!(matches!(
            validate(&rule),
            Err(RuleError::MetricNotAlertable { .. })
        ));
        rule.metric_id = "invented.metric".to_string();
        assert!(matches!(
            validate(&rule),
            Err(RuleError::MetricNotAlertable { .. })
        ));
    }

    #[test]
    fn percent_bounds_match_the_family_semantics() {
        let mut rule = valid_rule();
        rule.threshold = 100.5;
        assert!(matches!(
            validate(&rule),
            Err(RuleError::ThresholdOutOfBounds { max: 100.0, .. })
        ));
        // Core-equivalent process CPU legitimately exceeds 100.
        rule.metric_id = monitor_core::families::CPU_PROCESS_USAGE_CORES.to_string();
        rule.threshold = 800.0;
        assert_eq!(validate(&rule), Ok(()));
        rule.threshold = CORE_EQUIVALENT_PERCENT_MAX + 1.0;
        assert!(matches!(
            validate(&rule),
            Err(RuleError::ThresholdOutOfBounds { .. })
        ));
    }

    #[test]
    fn duration_and_cooldown_bounds_are_enforced() {
        let mut rule = valid_rule();
        rule.hold_duration_ms = MAX_HOLD_MS + 1;
        assert!(matches!(
            validate(&rule),
            Err(RuleError::HoldOutOfBounds { .. })
        ));
        rule.hold_duration_ms = 0; // fire on first crossing is legal
        assert_eq!(validate(&rule), Ok(()));
        rule.cooldown_ms = MAX_COOLDOWN_MS + 1;
        assert!(matches!(
            validate(&rule),
            Err(RuleError::CooldownOutOfBounds { .. })
        ));
    }

    #[test]
    fn negative_hysteresis_is_rejected() {
        let mut rule = valid_rule();
        rule.hysteresis_delta = -1.0;
        assert!(matches!(validate(&rule), Err(RuleError::HysteresisInvalid)));
        rule.hysteresis_delta = f64::NAN;
        assert!(matches!(validate(&rule), Err(RuleError::HysteresisInvalid)));
    }

    #[test]
    fn entity_filter_rejects_empty_control_and_oversized() {
        let mut rule = valid_rule();
        rule.entity_filter = Some(String::new());
        assert!(matches!(
            validate(&rule),
            Err(RuleError::EntityFilterInvalid { .. })
        ));
        rule.entity_filter = Some("host\nINJECTED".to_string());
        assert!(matches!(
            validate(&rule),
            Err(RuleError::EntityFilterInvalid { .. })
        ));
        rule.entity_filter = Some(monitor_core::families::CPU_TOTAL_USAGE.repeat(10));
        assert!(matches!(
            validate(&rule),
            Err(RuleError::EntityFilterInvalid { .. })
        ));
        rule.entity_filter = Some("core:2".to_string());
        assert_eq!(validate(&rule), Ok(()));
    }

    #[test]
    fn actions_require_absolute_program_configuration() {
        let mut rule = valid_rule();

        let relative = ActionConfig {
            program: "sh".to_string(),
            timeout_secs: 5,
            approved: false,
        };
        rule.action = Some(relative);
        assert!(matches!(
            validate(&rule),
            Err(RuleError::ActionProgramInvalid { .. })
        ));

        let absolute = ActionConfig {
            program: "/usr/local/bin/notify-wrapper".to_string(),
            timeout_secs: 5,
            approved: false,
        };
        rule.action = Some(absolute);
        assert_eq!(validate(&rule), Ok(()));

        let timeout = ActionConfig {
            program: "/bin/true".to_string(),
            timeout_secs: MAX_TIMEOUT_SECS + 1,
            approved: false,
        };
        rule.action = Some(timeout);
        assert!(matches!(
            validate(&rule),
            Err(RuleError::ActionTimeoutOutOfBounds { .. })
        ));
    }

    #[test]
    fn rule_serializes_with_stable_field_names() {
        let rule = valid_rule();
        let json = serde_json::to_string(&rule).unwrap();
        let back: AlertRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
        assert!(json.contains(r#""comparator":"above""#));
    }
}
