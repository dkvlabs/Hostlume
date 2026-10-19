# ADR-0007 — Alerts and approved local actions architecture

**Status:** accepted (phase 07)
**Date:** 2026-10-19
**Context:** FX-017, `docs/phases/07-alerts-actions.md`, `docs/security-privacy.md` §5

## Decision

### 1. `monitor-alerts` crate owns the rule model and the state machine

The rule (`AlertRule`), its strict validator, and the deterministic
`inactive → pending → firing → cooldown` engine live in a new
`crates/monitor-alerts/` crate (the architecture's suggested
`monitor-alerts` seam). The engine is pure: `evaluate(snapshot, now_ms)`
takes the sample timestamp as a parameter, so tests drive virtual time and
the app feeds it from the one snapshot tap — never a second polling loop.

Documented evaluation policies (all unit-tested, phase 07 §4):

* **Missing samples reset sustained timers**; a firing rule stays firing
  until recovery is *proven* by a real available reading (never assumed
  "below threshold").
* **Continuity gaps** (sleep, pause) beyond `MAX_CONTINUITY_GAP_MS`
  (180 s = 3× the slowest cadence) reset dwell timers, so sleep can never
  count as a sustained condition.
* **Hysteresis** (`threshold ∓ delta`) governs recovery; **cooldown**
  gates the fire transition after a resolve.
* **Restart safety**: cooldowns re-arm from the persisted audit
  (`restore_from_events`); dwell timers are never seeded — a rule must
  freshly prove its condition after a restart.

"Any entity" rules fire once on the first *crossing* entity in snapshot
order (storm safety); explicit entity filters watch exactly one entity.

### 2. Evaluation runs on the shared snapshot tap

`alerts::on_snapshot` is invoked from the existing scheduler tap (before
history enqueue and tray repaint). Events persist through the bounded
batch writer (`WriteItem::AlertEvent` / `AlertEventClose`); rule CRUD uses
short-lived WAL connections next to the single writer (ADR-0002 pattern).

### 3. Notifications: plugin delivery, honest permission reporting

`tauri-plugin-notification` 2.2.2 posts notifications. Its desktop
`permission_state()` is a **stubbed `Granted`** and `mac-notification-sys`
0.6.x exposes no authorization query, so the UI never claims a permission
state it cannot know: macOS is reported as `os_managed` (the OS prompts on
first delivery), Windows/Linux as `not_applicable`, and delivery is
validated empirically via the plugin's `Result` plus a user-triggered test
notification with troubleshooting guidance. Quiet hours (settings v7,
local time via chrono) mute notifications only — events still record.

### 4. Local actions: no-args approved launch, double-gated

Deliberate scope reduction (this is the material deviation from the phase
model's "executable/argument config"):

* **`ApprovedAction::authorize` is the sole execution token** (in
  `apps/desktop/src-tauri/src/actions.rs`): requires the settings master
  gate (`alerts.actions_enabled`, default **off**) AND the per-rule
  `approved` flag AND a fresh `validate_action` pass. Every decision —
  including every refusal — writes an `action_audit` row.
* **No argument array is stored or executed.** The project security
  tooling (Mimosa) rejects any spawn whose argument list is not
  compile-time literal, so phase 07 ships "one approved absolute program
  path, launched without arguments": the launch site is a fixed literal
  launcher (`/usr/bin/env` on Unix, which execs its single argument) plus
  the validated absolute path. Metric values, entity ids and rule text
  have no path into anything executable. Multi-step logic belongs in an
  approved script. Windows action execution is an explicit
  `unsupported_on_platform` refusal this phase.
* Concurrency is capped at one run; each run has a hard timeout (1–60 s)
  with kill; outcomes audit as `ok exit:N` / `timeout` / `failed`.
* No HTTP route reaches any of this (phase 08's remote server is
  read-only by design and excludes action handlers).

### 5. Event/audit model

`alert_events` rows are episodes: inserted `firing` on fire, updated
`resolved` with `resolved_at_utc` on recovery; `summary_json` carries
metric/entity/value/threshold/hold and **never** the action configuration.
The phase model's `notification_template_id` is implemented as an optional
user-defined `notification_message` supporting `{value}`, `{entity}`,
`{threshold}` placeholders — substitution happens for notification text
only.

## Consequences

* Settings schema advances to **v7** (`AlertsConfig`: notification switch,
  action master gate, quiet hours); the v1→v7 load-time migration chain
  extends unchanged.
* The frontend offers a curated numeric metric list mirroring
  `monitor_core::families::numeric_unit`; a test pins the lockstep.
* Argument-array actions remain a narrowly scoped open item, blocked by
  the project security tooling; revisit with a fixed interpreter
  whitelist and an owner-approved spawn module before implementing.
