//! Deterministic rule evaluation (phase 07 §2/§3.2).
//!
//! The engine is a pure state machine over (`snapshot`, `now_utc_ms`) pairs
//! supplied by the caller — production feeds it from the shared snapshot
//! tap (one bus, no second polling loop), tests feed it virtual monotonic
//! time. Documented policies:
//!
//! * **Missing sample** (`temporarily_unavailable`, `error`, permission
//!   gaps, collector restart): sustained-duration timers *reset*. A missing
//!   measurement is never assumed to be below the threshold, so a firing
//!   rule stays firing — only *proven* recovery resolves it.
//! * **Continuity gap** (sleep, scheduler pause, app suspension): if the
//!   gap between accepted samples exceeds [`MAX_CONTINUITY_GAP_MS`], the
//!   dwell timer resets so a 4-hour sleep can never count as a 60-second
//!   sustained condition (phase 07 §4).
//! * **Hysteresis**: while firing, recovery requires clearing
//!   `threshold ∓ hysteresis_delta`, not a momentary dip (phase 07 §4).
//! * **Cooldown**: after a resolve, no new fire before `cooldown_ms`
//!   elapses — a persistently high metric triggers once, not per sample.
//! * **Restart safety**: `restore_from_events` re-arms cooldowns from the
//!   persisted audit but never seeds the dwell timer; a rule must freshly
//!   prove its condition after a restart (phase 07 §4).

use std::collections::HashMap;

use monitor_core::{Availability, MetricValue, SystemSnapshot};
use serde::{Deserialize, Serialize};

use crate::rule::{AlertRule, Comparator};

/// Largest accepted gap between consecutive samples before dwell timers
/// reset. Three times the slowest poll cadence (60 s) tolerates one missed
/// slow tick without discarding legitimately continuous evidence.
pub const MAX_CONTINUITY_GAP_MS: i64 = 180_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Inactive,
    Pending,
    Firing,
    Cooldown,
}

#[derive(Debug)]
struct RuleRuntime {
    phase: Phase,
    pending_since_ms: Option<i64>,
    cooldown_until_ms: Option<i64>,
    /// Open audit episode while firing (`<rule_id>-<fired_at_ms>`).
    open_event_id: Option<String>,
    /// Epoch ms of the last accepted (present + available) sample.
    last_accepted_ms: Option<i64>,
}

impl Default for RuleRuntime {
    fn default() -> Self {
        Self {
            phase: Phase::Inactive,
            pending_since_ms: None,
            cooldown_until_ms: None,
            open_event_id: None,
            last_accepted_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionKind {
    Fired,
    Resolved,
}

/// One deterministic state transition the app layer must act on: persist an
/// audit row, notify (fired), and optionally run the approved local action.
#[derive(Debug, Clone, Serialize)]
pub struct Transition {
    pub rule_id: String,
    pub event_id: String,
    pub kind: TransitionKind,
    pub at_utc_ms: i64,
    pub metric_id: String,
    pub entity_id: String,
    pub value: f64,
    pub threshold: f64,
    pub comparator: Comparator,
    pub hold_duration_ms: u64,
    /// Present only on `Fired`; the app layer still applies its own
    /// approval gates before ever spawning this.
    pub action: Option<crate::rule::ActionConfig>,
}

/// What a rule *would* do against the current snapshot — the fixture-driven
/// preview. Never mutates state, never notifies, never runs actions.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RuleProbe {
    pub matched_entity: Option<String>,
    pub availability: Option<Availability>,
    pub value: Option<f64>,
    pub crossing_now: bool,
    /// Human-readable, English-only explanation (diagnostic surface).
    pub explanation: String,
}

pub struct AlertEngine {
    rules: Vec<AlertRule>,
    runtimes: HashMap<String, RuleRuntime>,
}

impl AlertEngine {
    pub fn new(rules: Vec<AlertRule>) -> Self {
        let runtimes = rules
            .iter()
            .map(|rule| (rule.id.clone(), RuleRuntime::default()))
            .collect();
        Self { rules, runtimes }
    }

    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Replace the whole rule set (after any CRUD change). Runtime state of
    /// rules that still exists is kept so an edit does not re-arm cooldowns.
    pub fn replace_rules(&mut self, rules: Vec<AlertRule>) {
        for rule in &rules {
            self.runtimes.entry(rule.id.clone()).or_default();
        }
        let known: Vec<String> = rules.iter().map(|r| r.id.clone()).collect();
        self.runtimes.retain(|id, _| known.contains(id));
        self.rules = rules;
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) else {
            return false;
        };
        rule.enabled = enabled;
        if !enabled {
            if let Some(rt) = self.runtimes.get_mut(id) {
                *rt = RuleRuntime::default();
            }
        }
        true
    }

    /// Re-arm cooldowns from persisted audit after an app restart. A rule
    /// whose last fire is still inside its cooldown resumes cooling; every
    /// other rule starts `Inactive` (fresh proof required before firing).
    pub fn restore_from_events(&mut self, last_fires: &HashMap<String, i64>, now_utc_ms: i64) {
        for rule in &self.rules {
            let Some(fire_ms) = last_fires.get(&rule.id).copied() else {
                continue;
            };
            let Some(rt) = self.runtimes.get_mut(&rule.id) else {
                continue;
            };
            if rule.enabled && now_utc_ms < fire_ms + rule.cooldown_ms as i64 {
                rt.phase = Phase::Cooldown;
                rt.cooldown_until_ms = Some(fire_ms + rule.cooldown_ms as i64);
            }
        }
    }

    /// Evaluate every enabled rule against one snapshot. `now_utc_ms` is
    /// the caller's sample timestamp (epoch ms) — injected, so tests drive
    /// virtual time and production stays on the tap's monotonic read.
    pub fn evaluate(&mut self, snapshot: &SystemSnapshot, now_utc_ms: i64) -> Vec<Transition> {
        let mut transitions = Vec::new();
        let Self { rules, runtimes } = self;
        for rule in rules.iter().filter(|r| r.enabled) {
            let Some(rt) = runtimes.get_mut(&rule.id) else {
                continue;
            };
            // Continuity: a large gap voids accumulated dwell.
            if let Some(last) = rt.last_accepted_ms {
                if now_utc_ms - last > MAX_CONTINUITY_GAP_MS && rt.phase == Phase::Pending {
                    rt.phase = Phase::Inactive;
                    rt.pending_since_ms = None;
                }
            }
            match sample_of(snapshot, rule) {
                SampleOf::Missing => {
                    // Missing sample: reset sustained timers, never assume
                    // "below threshold". A firing rule keeps firing until
                    // recovery is *proven* by a real reading.
                    if rt.phase == Phase::Pending {
                        rt.phase = Phase::Inactive;
                        rt.pending_since_ms = None;
                    }
                    rt.last_accepted_ms = None;
                }
                SampleOf::Present { entity_id, value } => {
                    rt.last_accepted_ms = Some(now_utc_ms);
                    step(rule, rt, entity_id, value, now_utc_ms, &mut transitions);
                }
            }
        }
        transitions
    }

    /// Fixture preview: what would this rule do against `snapshot` right
    /// now? Pure — no state mutation, no notifications, no actions.
    pub fn probe(snapshot: &SystemSnapshot, rule: &AlertRule) -> RuleProbe {
        match sample_of(snapshot, rule) {
            SampleOf::Missing => RuleProbe {
                matched_entity: None,
                availability: snapshot
                    .metrics
                    .iter()
                    .find(|r| r.metric_id == rule.metric_id)
                    .map(|r| r.availability),
                value: None,
                crossing_now: false,
                explanation: if rule.entity_filter.is_some() {
                    "No available reading for this metric and entity filter; the rule will not accumulate hold time."
                } else {
                    "No available reading for this metric; the rule will not accumulate hold time."
                }
                .to_string(),
            },
            SampleOf::Present { entity_id, value } => {
                let crossing = crosses(rule, value);
                RuleProbe {
                    matched_entity: Some(entity_id),
                    availability: Some(Availability::Available),
                    value: Some(value),
                    crossing_now: crossing,
                    explanation: if crossing {
                        format!(
                            "Value {value} crosses the threshold now; the rule fires once it holds for {} ms.",
                            rule.hold_duration_ms
                        )
                    } else {
                        "Value does not cross the threshold; the rule stays inactive.".to_string()
                    },
                }
            }
        }
    }
}

enum SampleOf {
    Missing,
    Present { entity_id: String, value: f64 },
}

/// Contract honesty at the alert boundary: only `available` numeric values
/// participate. Unavailable readings are *missing*, never zero.
///
/// Entity selection: an explicit `entity_filter` watches exactly that
/// entity. "Any entity" watches the first *crossing* entity in snapshot
/// order (falling back to the first available entity while nothing crosses,
/// so dwell resets and recovery checks still have a witness) — one
/// notification per episode, never one per entity.
fn sample_of(snapshot: &SystemSnapshot, rule: &AlertRule) -> SampleOf {
    let matches: Vec<(&str, f64)> = snapshot
        .metrics
        .iter()
        .filter(|r| r.metric_id == rule.metric_id && r.availability == Availability::Available)
        .filter(|r| {
            rule.entity_filter
                .as_ref()
                .is_none_or(|f| &r.entity_id == f)
        })
        .filter_map(|r| match r.value {
            Some(MetricValue::Number(value)) => Some((r.entity_id.as_str(), value)),
            _ => None,
        })
        .collect();
    if matches.is_empty() {
        return SampleOf::Missing;
    }
    let chosen = matches
        .iter()
        .find(|(_, value)| crosses(rule, *value))
        .or_else(|| matches.first())
        .expect("non-empty");
    let (entity_id, value) = *chosen;
    SampleOf::Present {
        entity_id: entity_id.to_string(),
        value,
    }
}

fn crosses(rule: &AlertRule, value: f64) -> bool {
    match rule.comparator {
        Comparator::Above => value > rule.threshold,
        Comparator::Below => value < rule.threshold,
    }
}

fn cleared(rule: &AlertRule, value: f64) -> bool {
    match rule.comparator {
        Comparator::Above => value < rule.threshold - rule.hysteresis_delta,
        Comparator::Below => value > rule.threshold + rule.hysteresis_delta,
    }
}

fn step(
    rule: &AlertRule,
    rt: &mut RuleRuntime,
    entity_id: String,
    value: f64,
    now_utc_ms: i64,
    transitions: &mut Vec<Transition>,
) {
    let crossing = crosses(rule, value);
    // Crossing transitions (Inactive → Pending, Cooldown → Pending) fall
    // through into the pending evaluation immediately, so a zero hold fires
    // on the exact crossing tick and a cooldown that expires on the same
    // tick as a re-cross does not add a spurious one-tick delay. The loop
    // therefore runs at most twice.
    let mut current = rt.phase;
    loop {
        match current {
            Phase::Inactive => {
                if crossing {
                    rt.phase = Phase::Pending;
                    rt.pending_since_ms = Some(now_utc_ms);
                    current = Phase::Pending;
                    continue;
                }
            }
            Phase::Pending => {
                if !crossing {
                    rt.phase = Phase::Inactive;
                    rt.pending_since_ms = None;
                    break;
                }
                let since = rt.pending_since_ms.unwrap_or(now_utc_ms);
                let held = now_utc_ms - since >= rule.hold_duration_ms as i64;
                let cooled_down = rt.cooldown_until_ms.is_none_or(|until| now_utc_ms >= until);
                if held && cooled_down {
                    let event_id = format!("{}-{now_utc_ms}", rule.id);
                    rt.phase = Phase::Firing;
                    rt.open_event_id = Some(event_id.clone());
                    rt.pending_since_ms = None;
                    transitions.push(Transition {
                        rule_id: rule.id.clone(),
                        event_id,
                        kind: TransitionKind::Fired,
                        at_utc_ms: now_utc_ms,
                        metric_id: rule.metric_id.clone(),
                        entity_id,
                        value,
                        threshold: rule.threshold,
                        comparator: rule.comparator,
                        hold_duration_ms: rule.hold_duration_ms,
                        action: rule.action.clone(),
                    });
                }
            }
            Phase::Firing => {
                if cleared(rule, value) {
                    rt.phase = Phase::Cooldown;
                    rt.cooldown_until_ms = Some(now_utc_ms + rule.cooldown_ms as i64);
                    let event_id = rt.open_event_id.take().unwrap_or_else(|| {
                        // Defensive: a firing phase always carries its
                        // episode id; this arm only exists for hand-built
                        // runtimes.
                        format!("{}-{now_utc_ms}", rule.id)
                    });
                    transitions.push(Transition {
                        rule_id: rule.id.clone(),
                        event_id,
                        kind: TransitionKind::Resolved,
                        at_utc_ms: now_utc_ms,
                        metric_id: rule.metric_id.clone(),
                        entity_id,
                        value,
                        threshold: rule.threshold,
                        comparator: rule.comparator,
                        hold_duration_ms: rule.hold_duration_ms,
                        action: None,
                    });
                }
            }
            Phase::Cooldown => {
                let until = rt.cooldown_until_ms.unwrap_or(now_utc_ms);
                if crossing {
                    rt.phase = Phase::Pending;
                    rt.pending_since_ms = Some(now_utc_ms);
                    current = Phase::Pending;
                    continue;
                }
                if now_utc_ms >= until {
                    rt.phase = Phase::Inactive;
                    rt.cooldown_until_ms = None;
                }
            }
        }
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::ActionConfig;
    use monitor_core::families;

    fn engine_with(rule: AlertRule) -> AlertEngine {
        AlertEngine::new(vec![rule])
    }

    fn cpu_rule(threshold: f64, hold_ms: u64, hysteresis: f64, cooldown_ms: u64) -> AlertRule {
        AlertRule {
            id: "rule-1".to_string(),
            metric_id: families::CPU_TOTAL_USAGE.to_string(),
            comparator: Comparator::Above,
            threshold,
            hold_duration_ms: hold_ms,
            hysteresis_delta: hysteresis,
            cooldown_ms,
            ..AlertRule::default()
        }
    }

    fn snapshot_with(metric_id: &str, entity: &str, value: f64) -> SystemSnapshot {
        let mut snapshot = SystemSnapshot::empty("h", "g", 1, "2026-10-19T00:00:00.000Z");
        snapshot.metrics.push(monitor_core::MetricReading {
            metric_id: metric_id.to_string(),
            entity_id: entity.to_string(),
            value: Some(MetricValue::Number(value)),
            unit: monitor_core::MetricUnit::Percent,
            availability: Availability::Available,
            source: "test".to_string(),
            collected_at_utc: "2026-10-19T00:00:00.000Z".to_string(),
            monotonic_sequence: 1,
            reason_code: None,
            human_message_key: None,
            precision: None,
        });
        snapshot
    }

    fn snapshot_unavailable(metric_id: &str, entity: &str) -> SystemSnapshot {
        let mut snapshot = SystemSnapshot::empty("h", "g", 1, "2026-10-19T00:00:00.000Z");
        snapshot.metrics.push(monitor_core::MetricReading {
            metric_id: metric_id.to_string(),
            entity_id: entity.to_string(),
            value: None,
            unit: monitor_core::MetricUnit::Percent,
            availability: Availability::TemporarilyUnavailable,
            source: "test".to_string(),
            collected_at_utc: "2026-10-19T00:00:00.000Z".to_string(),
            monotonic_sequence: 1,
            reason_code: Some("WARMUP".to_string()),
            human_message_key: None,
            precision: None,
        });
        snapshot
    }

    /// Phase 07 §4: "CPU 90% threshold held 60 s: spikes shorter than 60 s
    /// do not alert; exactly 60 s alerts once."
    #[test]
    fn hold_requires_full_sustained_duration() {
        let mut engine = engine_with(cpu_rule(90.0, 60_000, 10.0, 0));
        let at_95 = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        // 59 s of sustained 95%: no alert.
        for t in [0_i64, 10_000, 30_000, 59_000] {
            assert!(
                engine.evaluate(&at_95, t).is_empty(),
                "spike shorter than hold must not alert at t={t}"
            );
        }
        // Exactly 60 s: fires once.
        let fired = engine.evaluate(&at_95, 60_000);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, TransitionKind::Fired);
        assert_eq!(fired[0].entity_id, "host");
        assert_eq!(fired[0].value, 95.0);
        // Still high afterwards: no repeat firing.
        assert!(engine.evaluate(&at_95, 90_000).is_empty());
    }

    #[test]
    fn hold_zero_fires_on_first_crossing() {
        let mut engine = engine_with(cpu_rule(90.0, 0, 0.0, 0));
        let at_95 = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let fired = engine.evaluate(&at_95, 1_000);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, TransitionKind::Fired);
    }

    /// Phase 07 §4: "Fluctuating values around threshold: hysteresis avoids
    /// rapid resolve/retrigger."
    #[test]
    fn hysteresis_prevents_flapping() {
        let mut engine = engine_with(cpu_rule(90.0, 0, 10.0, 0));
        let above = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let in_band = snapshot_with(families::CPU_TOTAL_USAGE, "host", 85.0); // > 90-10
        let below = snapshot_with(families::CPU_TOTAL_USAGE, "host", 79.0); // < 80

        assert_eq!(engine.evaluate(&above, 0)[0].kind, TransitionKind::Fired);
        // Dips inside the hysteresis band keep the rule firing.
        assert!(engine.evaluate(&in_band, 1_000).is_empty());
        assert!(engine.evaluate(&above, 2_000).is_empty());
        // Only clearing beyond threshold - hysteresis resolves…
        let resolved = engine.evaluate(&below, 3_000);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].kind, TransitionKind::Resolved);
        assert_eq!(resolved[0].event_id, fired_event_id(0));
        // …and re-crossing starts a fresh episode (hold = 0).
        let again = engine.evaluate(&above, 4_000);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].kind, TransitionKind::Fired);
    }

    fn fired_event_id(at_ms: i64) -> String {
        format!("rule-1-{at_ms}")
    }

    /// Phase 07 §4: "Cooldown 5 min: persistent high CPU triggers once,
    /// not every sample."
    #[test]
    fn cooldown_limits_persistent_condition_to_one_fire() {
        let mut engine = engine_with(cpu_rule(90.0, 0, 0.0, 300_000));
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let low = snapshot_with(families::CPU_TOTAL_USAGE, "host", 10.0);

        assert_eq!(engine.evaluate(&high, 0).len(), 1, "first fire");
        // Resolve, then the condition returns and stays high: the rule
        // must not fire again inside the cooldown window.
        assert_eq!(engine.evaluate(&low, 1_000).len(), 1, "resolved");
        assert!(engine.evaluate(&high, 2_000).is_empty());
        assert!(engine.evaluate(&high, 100_000).is_empty());
        assert!(engine.evaluate(&high, 299_000).is_empty());
        assert!(engine.evaluate(&high, 300_000).is_empty());
        // Hold satisfied, cooldown elapsed (resolve was at t=1_000): fires
        // again exactly once.
        let fired = engine.evaluate(&high, 301_000);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, TransitionKind::Fired);
    }

    /// Phase 07 §4: "Sleep and a missed sample: no accidental 4-hour
    /// sustained condition on resume."
    #[test]
    fn sleep_gap_and_missing_samples_reset_dwell() {
        let mut engine = engine_with(cpu_rule(90.0, 60_000, 0.0, 0));
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let missing = snapshot_unavailable(families::CPU_TOTAL_USAGE, "host");

        // 30 s accumulated…
        engine.evaluate(&high, 0);
        engine.evaluate(&high, 30_000);
        // …then the machine sleeps for 4 hours: the next accepted sample
        // must restart dwell from zero, not inherit the old timer.
        assert!(engine.evaluate(&high, 30_000 + 4 * 3_600_000).is_empty());
        assert!(engine
            .evaluate(&high, 30_000 + 4 * 3_600_000 + 59_000)
            .is_empty());
        assert_eq!(
            engine
                .evaluate(&high, 30_000 + 4 * 3_600_000 + 60_000)
                .len(),
            1,
            "fires only after the condition is freshly proven"
        );

        // Missing samples also reset the dwell timer.
        let mut engine = engine_with(cpu_rule(90.0, 60_000, 0.0, 0));
        engine.evaluate(&high, 0);
        engine.evaluate(&high, 30_000);
        engine.evaluate(&missing, 33_000);
        engine.evaluate(&high, 36_000);
        assert!(engine.evaluate(&high, 89_000).is_empty());
        assert_eq!(engine.evaluate(&high, 96_000).len(), 1);
    }

    /// Phase 07 §4: "Metric unsupported/permission denied: no false
    /// trigger." Unavailable readings are missing, never zero.
    #[test]
    fn unavailable_readings_never_trigger_and_fire_survives_gaps() {
        let mut engine = engine_with(cpu_rule(90.0, 0, 0.0, 0));
        let missing = snapshot_unavailable(families::CPU_TOTAL_USAGE, "host");
        assert!(engine.evaluate(&missing, 0).is_empty());
        // A permission-denied phase while firing keeps the episode open —
        // no proof of recovery, no false resolve.
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        assert_eq!(engine.evaluate(&high, 1_000).len(), 1);
        assert!(engine.evaluate(&missing, 2_000).is_empty());
        let still_open = snapshot_unavailable(families::CPU_TOTAL_USAGE, "host");
        assert!(engine.evaluate(&still_open, 10_000).is_empty());
        // Real recovery closes it with the original episode id.
        let low = snapshot_with(families::CPU_TOTAL_USAGE, "host", 10.0);
        let resolved = engine.evaluate(&low, 11_000);
        assert_eq!(resolved[0].event_id, fired_event_id(1_000));
    }

    /// Phase 07 §4: "System restart: no duplicate immediate notification
    /// unless rule condition is freshly proven."
    #[test]
    fn restart_requires_fresh_proof_and_restores_cooldown() {
        // Fresh proof: a rule that fired long ago starts inactive and must
        // accumulate hold time again after restart.
        let mut engine = engine_with(cpu_rule(90.0, 60_000, 0.0, 0));
        engine.restore_from_events(&HashMap::new(), 1_000_000);
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        assert!(engine.evaluate(&high, 1_000_000).is_empty());
        assert!(engine.evaluate(&high, 1_059_000).is_empty());
        assert_eq!(engine.evaluate(&high, 1_060_000).len(), 1);

        // Cooldown carry-over: a fire 60 s ago with a 5-minute cooldown
        // stays cooling across the restart and cannot re-fire immediately.
        let mut engine = engine_with(cpu_rule(90.0, 0, 0.0, 300_000));
        engine.restore_from_events(
            &HashMap::from([("rule-1".to_string(), 940_000_i64)]),
            1_000_000,
        );
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        assert!(engine.evaluate(&high, 1_000_000).is_empty());
        assert!(engine.evaluate(&high, 1_239_000).is_empty());
        assert_eq!(engine.evaluate(&high, 1_240_000).len(), 1);
    }

    #[test]
    fn disabled_rules_and_entity_filters_do_not_fire() {
        let mut rule = cpu_rule(90.0, 0, 0.0, 0);
        rule.enabled = false;
        let mut engine = engine_with(rule);
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        assert!(engine.evaluate(&high, 0).is_empty());

        let mut filtered = cpu_rule(90.0, 0, 0.0, 0);
        filtered.entity_filter = Some("core:1".to_string());
        let mut engine = engine_with(filtered);
        // "host" exists but does not match the filter: nothing fires.
        assert!(engine.evaluate(&high, 0).is_empty());
        // Once the filtered entity appears, it fires with that identity.
        let core = snapshot_with(families::CPU_TOTAL_USAGE, "core:1", 95.0);
        let fired = engine.evaluate(&core, 1_000);
        assert_eq!(fired[0].entity_id, "core:1");
    }

    /// "Any entity" fires once on the first matching entity — never one
    /// notification per entity (storm safety).
    #[test]
    fn any_entity_filter_fires_once_for_the_first_matching_entity() {
        let mut engine = engine_with(cpu_rule(90.0, 0, 0.0, 0));
        let mut snapshot = SystemSnapshot::empty("h", "g", 1, "2026-10-19T00:00:00.000Z");
        for (entity, value) in [("core:0", 50.0), ("core:1", 95.0), ("core:2", 99.0)] {
            snapshot.metrics.push(monitor_core::MetricReading {
                metric_id: families::CPU_TOTAL_USAGE.to_string(),
                entity_id: entity.to_string(),
                value: Some(MetricValue::Number(value)),
                unit: monitor_core::MetricUnit::Percent,
                availability: Availability::Available,
                source: "test".to_string(),
                collected_at_utc: "2026-10-19T00:00:00.000Z".to_string(),
                monotonic_sequence: 1,
                reason_code: None,
                human_message_key: None,
                precision: None,
            });
        }
        let fired = engine.evaluate(&snapshot, 0);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].entity_id, "core:1", "first matching entity");
    }

    #[test]
    fn fired_transition_carries_the_rule_action() {
        let mut rule = cpu_rule(90.0, 0, 0.0, 0);
        rule.action = Some(ActionConfig {
            program: "/usr/bin/true".to_string(),
            timeout_secs: 5,
            approved: true,
        });
        let mut engine = engine_with(rule);
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let fired = engine.evaluate(&high, 0);
        let action = fired[0].action.as_ref().expect("action carried on fire");
        assert_eq!(action.program, "/usr/bin/true");
        // Resolved transitions never carry actions.
        let low = snapshot_with(families::CPU_TOTAL_USAGE, "host", 10.0);
        let resolved = engine.evaluate(&low, 1_000);
        assert!(resolved[0].action.is_none());
    }

    #[test]
    fn below_comparator_mirrors_the_semantics() {
        let rule = AlertRule {
            id: "rule-1".to_string(),
            metric_id: families::BATTERY_CHARGE.to_string(),
            comparator: Comparator::Below,
            threshold: 20.0,
            hold_duration_ms: 0,
            hysteresis_delta: 5.0,
            cooldown_ms: 0,
            ..AlertRule::default()
        };
        let mut engine = engine_with(rule);
        let low = snapshot_with(families::BATTERY_CHARGE, "battery:0", 15.0);
        assert_eq!(engine.evaluate(&low, 0)[0].kind, TransitionKind::Fired);
        let in_band = snapshot_with(families::BATTERY_CHARGE, "battery:0", 23.0); // < 20+5
        assert!(engine.evaluate(&in_band, 1_000).is_empty());
        let recovered = snapshot_with(families::BATTERY_CHARGE, "battery:0", 30.0);
        assert_eq!(
            engine.evaluate(&recovered, 2_000)[0].kind,
            TransitionKind::Resolved
        );
    }

    #[test]
    fn probe_is_pure_and_reports_the_truth() {
        let rule = cpu_rule(90.0, 60_000, 0.0, 0);
        let mut engine = engine_with(rule.clone());
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let probe = AlertEngine::probe(&high, &rule);
        assert!(probe.crossing_now);
        assert_eq!(probe.matched_entity.as_deref(), Some("host"));
        assert_eq!(probe.value, Some(95.0));
        // Probing never mutates the engine.
        assert!(engine.evaluate(&high, 0).is_empty());

        let missing = snapshot_unavailable(families::CPU_TOTAL_USAGE, "host");
        let probe = AlertEngine::probe(&missing, &rule);
        assert_eq!(probe.value, None);
        assert!(!probe.crossing_now);
        assert_eq!(
            probe.availability,
            Some(Availability::TemporarilyUnavailable)
        );
    }

    #[test]
    fn replace_rules_keeps_surviving_runtime_state() {
        let mut engine = engine_with(cpu_rule(90.0, 0, 0.0, 300_000));
        let high = snapshot_with(families::CPU_TOTAL_USAGE, "host", 95.0);
        let low = snapshot_with(families::CPU_TOTAL_USAGE, "host", 10.0);
        engine.evaluate(&high, 0);
        engine.evaluate(&low, 1_000); // resolved → cooling until 301_000
        let mut edited = cpu_rule(85.0, 0, 0.0, 300_000);
        edited.id = "rule-1".to_string();
        engine.replace_rules(vec![edited]);
        // Edited rule keeps its cooldown: no immediate re-fire on edit.
        assert!(engine.evaluate(&high, 2_000).is_empty());
        assert_eq!(engine.evaluate(&high, 301_000).len(), 1);
    }
}
