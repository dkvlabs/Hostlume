//! Alert runtime (phase 07): engine wiring on the shared snapshot tap,
//! OS notifications, quiet hours and the **disabled-by-default** approved
//! local action dispatch.
//!
//! Security model (security-privacy.md §5, phase 07 §3.6/§3.7):
//! * Notifications are the default action; local workflows need BOTH the
//!   settings master gate (`alerts.actions_enabled`) and the per-rule
//!   `approved` flag (see `actions::ApprovedAction::authorize`).
//! * The runner never uses a shell and metric values are never
//!   interpolated into anything executable (actions.rs is the only spawn
//!   site; it takes one absolute program path).
//! * Concurrency is capped at one running action; each run has a hard
//!   timeout and leaves an audit row. No HTTP route exists that can reach
//!   any of this (the remote server is a phase 08 read-only design that
//!   excludes action handlers by construction).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use monitor_alerts::{ActionConfig, AlertEngine, AlertRule, RuleProbe, Transition, TransitionKind};
use monitor_core::{Clock, SystemClock};
use monitor_history::{AlertEventRow, WriteItem};
use serde::Serialize;
use tauri::AppHandle;

use crate::actions::{self, ActionRefusal};
use crate::commands::CommandError;
use crate::settings::AlertsConfig;
use crate::state::AppState;

/// Shared alert state: the rule engine plus a live mirror of the alert
/// settings so the tap never touches SQLite on the hot path.
pub struct AlertRuntime {
    pub engine: Mutex<AlertEngine>,
    pub config: Mutex<AlertsConfig>,
    /// 0/1 live-action counter, shared into the worker thread.
    pub action_active: Arc<AtomicU32>,
}

impl AlertRuntime {
    /// Load persisted rules and apply restart safety: re-arm still-valid
    /// cooldowns from the audit, never seed dwell timers.
    pub fn open(
        conn: &rusqlite::Connection,
        config: AlertsConfig,
    ) -> Result<Self, rusqlite::Error> {
        let rules = load_rules(conn)?;
        let mut engine = AlertEngine::new(rules);
        let last_fires = monitor_history::last_fire_per_rule(conn)?;
        engine.restore_from_events(&last_fires, SystemClock.now_epoch_ms());
        Ok(Self {
            engine: Mutex::new(engine),
            config: Mutex::new(config),
            action_active: Arc::new(AtomicU32::new(0)),
        })
    }
}

/// Deserialize stored rules; rows that no longer validate (older schema,
/// hand-edited database) are skipped loudly instead of resurrected.
fn load_rules(conn: &rusqlite::Connection) -> Result<Vec<AlertRule>, rusqlite::Error> {
    let mut rules = Vec::new();
    for row in monitor_history::list_alert_rules(conn)? {
        match serde_json::from_str::<AlertRule>(&row.condition_json) {
            Ok(mut rule) => {
                rule.enabled = row.enabled;
                if monitor_alerts::validate(&rule).is_ok() {
                    rules.push(rule);
                } else {
                    eprintln!("alert rule {} failed validation; skipping", row.id);
                }
            }
            Err(error) => eprintln!("alert rule {} unreadable; skipping: {error}", row.id),
        }
    }
    Ok(rules)
}

/// Is `now_utc_ms` inside the configured quiet window (local wall clock)?
/// `start == end` is a zero-length window and never quiet; `start > end`
/// wraps midnight. Quiet hours suppress *notifications* only — events are
/// still recorded and approved actions still run (explicit automation).
pub fn in_quiet_hours(config: &AlertsConfig, now_utc_ms: i64) -> bool {
    if !config.quiet_hours_enabled {
        return false;
    }
    let minute_of_day = local_minute_of_day(now_utc_ms);
    let (start, end) = (config.quiet_start_minute, config.quiet_end_minute);
    if start == end {
        return false;
    }
    if start < end {
        (start..end).contains(&minute_of_day)
    } else {
        minute_of_day >= start || minute_of_day < end
    }
}

/// Local minute-of-day via chrono (IANA zone database). Falls back to UTC
/// minutes if the zone cannot be determined — documented degradation, not
/// a fabricated "always quiet".
fn local_minute_of_day(now_utc_ms: i64) -> u16 {
    use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
    match Utc.timestamp_millis_opt(now_utc_ms) {
        chrono::LocalResult::Single(utc) => {
            let local: DateTime<Local> = utc.into();
            (local.hour() * 60 + local.minute()) as u16
        }
        _ => {
            let utc = DateTime::<Utc>::from_timestamp_millis(now_utc_ms).unwrap_or_default();
            (utc.hour() * 60 + utc.minute()) as u16
        }
    }
}

/// Substitute the notification template placeholders. Notification text
/// only — this never touches anything executable.
fn render_notification_message(rule: &AlertRule, entity: &str, value: f64) -> String {
    match rule.notification_message.as_deref() {
        Some(template) if !template.is_empty() => template
            .replace("{value}", &format_value(value))
            .replace("{entity}", entity)
            .replace("{threshold}", &format_value(rule.threshold)),
        _ => {
            let comparator = match rule.comparator {
                monitor_alerts::Comparator::Above => "above",
                monitor_alerts::Comparator::Below => "below",
            };
            format!(
                "{} on {} is {} ({} {} for {} s)",
                rule.metric_id,
                entity,
                format_value(value),
                comparator,
                format_value(rule.threshold),
                rule.hold_duration_ms / 1_000
            )
        }
    }
}

fn format_value(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value:.1}")
    }
}

/// The audit summary persisted with an event. Deliberately excludes the
/// action configuration (program paths stay out of event rows).
fn event_summary(transition: &Transition) -> String {
    serde_json::json!({
        "metric_id": transition.metric_id,
        "entity_id": transition.entity_id,
        "value": transition.value,
        "threshold": transition.threshold,
        "comparator": transition.comparator,
        "hold_duration_ms": transition.hold_duration_ms,
    })
    .to_string()
}

/// Handle one evaluator transition: persist the audit episode, notify
/// (unless quiet / disabled), and gate-run the optional local action.
fn handle_transition(
    app: &AppHandle,
    state: &AppState,
    config: &AlertsConfig,
    transition: &Transition,
) {
    match transition.kind {
        TransitionKind::Fired => {
            enqueue(
                state,
                WriteItem::AlertEvent {
                    id: transition.event_id.clone(),
                    rule_id: transition.rule_id.clone(),
                    triggered_at_utc_ms: transition.at_utc_ms,
                    summary_json: Some(event_summary(transition)),
                },
            );
            if config.notifications_enabled && !in_quiet_hours(config, transition.at_utc_ms) {
                if let Err(error) = send_alert_notification(app, transition) {
                    eprintln!("notification failed: {error}");
                }
            }
            if let Some(action) = &transition.action {
                dispatch_action(state, config, &transition.rule_id, action);
            }
        }
        TransitionKind::Resolved => {
            enqueue(
                state,
                WriteItem::AlertEventClose {
                    id: transition.event_id.clone(),
                    resolved_at_utc_ms: transition.at_utc_ms,
                },
            );
        }
    }
}

fn enqueue(state: &AppState, item: WriteItem) {
    let guard = state
        .history_writer
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some(writer) = guard.as_ref() {
        writer.enqueue(item);
    }
}

/// Post the OS notification through the Tauri notification plugin. The
/// desktop plugin has no real permission query (its `permission_state` is a
/// stubbed `Granted`), so delivery is validated empirically by the `Result`
/// and the user-facing troubleshooting explains the OS-level switches.
fn send_alert_notification(
    app: &AppHandle,
    transition: &Transition,
) -> Result<(), tauri_plugin_notification::Error> {
    use tauri_plugin_notification::NotificationExt;
    let title = app
        .config()
        .product_name
        .clone()
        .unwrap_or_else(|| "APP_NAME".to_string());
    let body = render_notification_message(
        &rule_for_notification(transition),
        &transition.entity_id,
        transition.value,
    );
    app.notification().builder().title(title).body(body).show()
}

/// Reconstruct the template-bearing fields for message rendering without
/// carrying the action config around.
fn rule_for_notification(transition: &Transition) -> AlertRule {
    AlertRule {
        id: transition.rule_id.clone(),
        metric_id: transition.metric_id.clone(),
        comparator: transition.comparator,
        threshold: transition.threshold,
        hold_duration_ms: transition.hold_duration_ms,
        ..AlertRule::default()
    }
}

/// Decide the action gate, then run (or explicitly refuse) on a worker
/// thread. Every decision — including every refusal — leaves an audit row.
fn dispatch_action(state: &AppState, config: &AlertsConfig, rule_id: &str, action: &ActionConfig) {
    let audit = |result: String, detail: Option<String>| {
        state.record_audit(0, 0, "alert_action", &result, detail);
    };
    let approved = match actions::ApprovedAction::authorize(rule_id, action, config.actions_enabled)
    {
        Ok(approved) => approved,
        Err(ActionRefusal::Validation(text)) => {
            audit(
                ActionRefusal::InvalidConfig.to_string(),
                Some(actions::truncate(format!("rule:{rule_id} {text}"))),
            );
            return;
        }
        Err(refusal) => {
            audit(refusal.to_string(), Some(format!("rule:{rule_id}")));
            return;
        }
    };
    if state.alerts.action_active.load(Ordering::SeqCst) >= actions::MAX_CONCURRENT_ACTIONS {
        audit(
            "skipped_concurrency_limit".to_string(),
            Some(format!("rule:{rule_id}")),
        );
        return;
    }
    state.alerts.action_active.fetch_add(1, Ordering::SeqCst);
    let writer_slot = state
        .history_writer
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let Some(writer) = writer_slot else {
        state.alerts.action_active.fetch_sub(1, Ordering::SeqCst);
        return;
    };
    let active_counter = Arc::clone(&state.alerts.action_active);
    let rule_id_owned = approved.rule_id().to_string();
    std::thread::Builder::new()
        .name("alert-action".into())
        .spawn(move || {
            let outcome = actions::execute(&approved);
            writer.enqueue(WriteItem::Audit {
                ts_utc_ms: SystemClock.now_epoch_ms(),
                target_pid: outcome.pid as i32,
                target_start_time: 0,
                mode: "alert_action".to_string(),
                result: outcome.result.clone(),
                detail: Some(actions::truncate(format!(
                    "rule:{rule_id_owned} {} {}",
                    outcome.result, outcome.detail
                ))),
            });
            active_counter.fetch_sub(1, Ordering::SeqCst);
        })
        .map(|_| ())
        .unwrap_or_else(|error| {
            audit(
                "failed_spawn_worker".to_string(),
                Some(actions::truncate(error.to_string())),
            );
            state.alerts.action_active.fetch_sub(1, Ordering::SeqCst);
        });
}

// ---- Tauri commands (Settings → Alerts) ------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct NotificationStateInfo {
    pub platform: &'static str,
    /// `os_managed`: the OS owns the permission (macOS prompts on first
    /// delivery); `not_applicable`: no per-app gate on this platform.
    pub state: &'static str,
    /// i18n key for the UI explanation.
    pub detail_key: &'static str,
}

#[tauri::command]
pub fn get_notification_state() -> NotificationStateInfo {
    if cfg!(target_os = "macos") {
        NotificationStateInfo {
            platform: "macos",
            state: "os_managed",
            detail_key: "alerts.notificationStateMacos",
        }
    } else if cfg!(target_os = "windows") {
        NotificationStateInfo {
            platform: "windows",
            state: "not_applicable",
            detail_key: "alerts.notificationStateWindows",
        }
    } else {
        NotificationStateInfo {
            platform: "linux",
            state: "not_applicable",
            detail_key: "alerts.notificationStateLinux",
        }
    }
}

/// Send one test notification and report the honest delivery attempt.
#[tauri::command]
pub fn send_test_notification(app: AppHandle) -> Result<(), CommandError> {
    use tauri_plugin_notification::NotificationExt;
    let title = app
        .config()
        .product_name
        .clone()
        .unwrap_or_else(|| "APP_NAME".to_string());
    app.notification()
        .builder()
        .title(title)
        .body("Test notification — alerts are wired up.")
        .show()
        .map_err(|error| CommandError::store(format!("notification: {error}")))
}

#[tauri::command]
pub fn get_alert_rules(state: tauri::State<'_, AppState>) -> Vec<AlertRule> {
    state
        .alerts
        .engine
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .rules()
        .to_vec()
}

fn store_rule(
    state: &AppState,
    rule: &AlertRule,
    created_at_utc_ms: i64,
    updated_at_utc_ms: i64,
) -> Result<(), CommandError> {
    let condition_json =
        serde_json::to_string(rule).map_err(|error| CommandError::store(error.to_string()))?;
    let row = monitor_history::AlertRuleRow {
        id: rule.id.clone(),
        condition_json,
        enabled: rule.enabled,
        created_at_utc_ms,
        updated_at_utc_ms,
    };
    let store = crate::commands::open_history_query_connection(state)?;
    monitor_history::upsert_alert_rule(store.connection(), &row)
        .map_err(|error| CommandError::store(error.to_string()))
}

/// Replace the engine's rule set from the store after a CRUD write.
fn reload_engine(state: &AppState) -> Result<(), CommandError> {
    let store = crate::commands::open_history_query_connection(state)?;
    let rules =
        load_rules(store.connection()).map_err(|error| CommandError::store(error.to_string()))?;
    state
        .alerts
        .engine
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .replace_rules(rules);
    Ok(())
}

/// Create a rule. The client sends `id: ""` — server-side identity only.
#[tauri::command]
pub fn create_alert_rule(
    state: tauri::State<'_, AppState>,
    mut rule: AlertRule,
) -> Result<AlertRule, CommandError> {
    if !rule.id.is_empty() {
        return Err(CommandError {
            code: "RULE_ID_CLIENT_ASSIGNED",
            message_key: "errors.ruleInvalid",
            reason: Some("create requests must not carry an id".to_string()),
        });
    }
    monitor_alerts::validate(&rule).map_err(rule_invalid)?;
    {
        let engine = state
            .alerts
            .engine
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if engine.rules().len() >= monitor_alerts::MAX_RULES {
            return Err(CommandError {
                code: "RULE_LIMIT_REACHED",
                message_key: "errors.ruleLimit",
                reason: Some(format!("max {}", monitor_alerts::MAX_RULES)),
            });
        }
    }
    rule.id = uuid::Uuid::new_v4().to_string();
    let now = SystemClock.now_epoch_ms();
    store_rule(&state, &rule, now, now)?;
    reload_engine(&state)?;
    Ok(rule)
}

/// Update an existing rule; unknown ids are rejected (no resurrection).
#[tauri::command]
pub fn update_alert_rule(
    state: tauri::State<'_, AppState>,
    rule: AlertRule,
) -> Result<AlertRule, CommandError> {
    monitor_alerts::validate(&rule).map_err(rule_invalid)?;
    let created_at_utc_ms = {
        let store = crate::commands::open_history_query_connection(&state)?;
        let rows = monitor_history::list_alert_rules(store.connection())
            .map_err(|error| CommandError::store(error.to_string()))?;
        let Some(row) = rows.iter().find(|row| row.id == rule.id) else {
            return Err(CommandError {
                code: "RULE_NOT_FOUND",
                message_key: "errors.ruleNotFound",
                reason: None,
            });
        };
        row.created_at_utc_ms
    };
    store_rule(&state, &rule, created_at_utc_ms, SystemClock.now_epoch_ms())?;
    reload_engine(&state)?;
    Ok(rule)
}

#[tauri::command]
pub fn delete_alert_rule(
    state: tauri::State<'_, AppState>,
    rule_id: String,
) -> Result<bool, CommandError> {
    let store = crate::commands::open_history_query_connection(&state)?;
    let deleted = monitor_history::delete_alert_rule(store.connection(), &rule_id)
        .map_err(|error| CommandError::store(error.to_string()))?;
    if deleted {
        reload_engine(&state)?;
    }
    Ok(deleted)
}

#[tauri::command]
pub fn get_alert_events(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<AlertEventRow>, CommandError> {
    let limit = limit.unwrap_or(50).clamp(1, 200) as i64;
    let store = crate::commands::open_history_query_connection(&state)?;
    monitor_history::list_recent_alert_events(store.connection(), limit)
        .map_err(|error| CommandError::store(error.to_string()))
}

/// Fixture preview (phase 07 §3.5): what would this rule do against the
/// current snapshot? No persistence, no notification, no action.
#[tauri::command]
pub fn test_alert_rule(
    state: tauri::State<'_, AppState>,
    rule: AlertRule,
) -> Result<RuleProbe, CommandError> {
    monitor_alerts::validate(&rule).map_err(rule_invalid)?;
    let snapshot = state
        .latest
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .ok_or_else(CommandError::no_snapshot_yet)?;
    Ok(AlertEngine::probe(&snapshot, &rule))
}

fn rule_invalid(error: monitor_alerts::RuleError) -> CommandError {
    CommandError {
        code: "RULE_INVALID",
        message_key: "errors.ruleInvalid",
        reason: Some(error.to_string()),
    }
}

/// Called from the snapshot tap: evaluate + dispatch. Kept as a free
/// function so `lib.rs` stays declarative.
pub fn on_snapshot(app: &AppHandle, state: &AppState, snapshot: &monitor_core::SystemSnapshot) {
    let now = SystemClock.now_epoch_ms();
    let transitions = {
        let mut engine = state
            .alerts
            .engine
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        engine.evaluate(snapshot, now)
    };
    if transitions.is_empty() {
        return;
    }
    let config = *state
        .alerts
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    for transition in &transitions {
        handle_transition(app, state, &config, transition);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AlertsConfig {
        AlertsConfig {
            quiet_hours_enabled: true,
            quiet_start_minute: 22 * 60,
            quiet_end_minute: 8 * 60,
            ..AlertsConfig::default()
        }
    }

    fn utc_ms(hour: u32, minute: u32) -> i64 {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(2026, 10, 19, hour, minute, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn quiet_hours_follow_the_documented_window_policy() {
        let cfg = config();
        // The policy test pins membership against the same conversion the
        // production path uses, so it is zone-independent.
        for utc_ms in [utc_ms(0, 0), utc_ms(6, 30), utc_ms(12, 0), utc_ms(23, 45)] {
            let minute = local_minute_of_day(utc_ms);
            let expected = if cfg.quiet_start_minute < cfg.quiet_end_minute {
                (cfg.quiet_start_minute..cfg.quiet_end_minute).contains(&minute)
            } else {
                minute >= cfg.quiet_start_minute || minute < cfg.quiet_end_minute
            };
            assert_eq!(in_quiet_hours(&cfg, utc_ms), expected);
        }
        // Switch off = never quiet.
        let mut off = config();
        off.quiet_hours_enabled = false;
        assert!(!in_quiet_hours(&off, utc_ms(23, 45)));
        // Zero-length window is never quiet.
        let mut zero = config();
        zero.quiet_start_minute = 600;
        zero.quiet_end_minute = 600;
        assert!(!in_quiet_hours(&zero, utc_ms(10, 0)));
    }

    #[test]
    fn template_renders_placeholders_but_only_for_notifications() {
        let mut rule = AlertRule {
            notification_message: Some("{value} on {entity} crossed {threshold}".to_string()),
            ..AlertRule::default()
        };
        let rendered = render_notification_message(&rule, "host", 94.5);
        assert_eq!(rendered, "94.5 on host crossed 0");

        rule.notification_message = None;
        let default_text = render_notification_message(&rule, "host", 94.26);
        assert!(
            default_text.contains("94.3"),
            "one-decimal default: {default_text}"
        );
    }

    #[test]
    fn event_summary_excludes_action_configuration() {
        let transition = Transition {
            rule_id: "r".into(),
            event_id: "r-1".into(),
            kind: TransitionKind::Fired,
            at_utc_ms: 1,
            metric_id: "cpu.total.usage".into(),
            entity_id: "host".into(),
            value: 95.0,
            threshold: 90.0,
            comparator: monitor_alerts::Comparator::Above,
            hold_duration_ms: 60_000,
            action: Some(ActionConfig {
                program: "/usr/bin/secret-script".to_string(),
                timeout_secs: 5,
                approved: true,
            }),
        };
        let summary = event_summary(&transition);
        assert!(!summary.contains("secret-script"), "no program in audit");
        assert!(summary.contains("cpu.total.usage"));
    }
}
