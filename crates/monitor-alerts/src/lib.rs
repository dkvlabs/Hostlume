//! monitor-alerts — deterministic threshold-alert rule engine (phase 07).
//!
//! Responsibilities stay deliberately narrow: the rule model and validator,
//! and the pure state machine over injected sample times. Persistence,
//! OS notifications, quiet hours and the approved-action runner live in the
//! app runtime (`apps/desktop/src-tauri`), which feeds this engine from the
//! shared snapshot tap — never from a second polling loop.

pub mod engine;
pub mod rule;

pub use engine::{AlertEngine, RuleProbe, Transition, TransitionKind, MAX_CONTINUITY_GAP_MS};
pub use rule::{
    validate, validate_action, ActionConfig, AlertRule, Comparator, RuleError, MAX_RULES,
};
