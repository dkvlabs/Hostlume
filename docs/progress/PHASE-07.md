# Phase 07 report — threshold alerts, notifications and approved local workflows

**Date:** 2026-10-19 · **Phase owner:** FX-017 · **Stack:** `monitor-alerts` (new) + app runtime + React settings UI

## 1. What shipped

### Rule engine (`crates/monitor-alerts/` — new crate)

* `AlertRule` model + strict validator: metric must be a *numeric alertable family* (`monitor_core::families::numeric_unit`, new registry in monitor-core), threshold unit-compatible and bounded per unit (percent 0–100, with the documented 0–32768 core-equivalent exception for `cpu.process.usage_cores`), hold 0–3 600 000 ms, cooldown 0–86 400 000 ms, finite non-negative hysteresis, bounded entity filter / message, action = absolute program + bounded timeout.
* Deterministic state machine `inactive → pending → firing → cooldown` over injected sample times (phase 07 §2). Policies (each unit-tested): missing samples **reset** dwell timers and never imply "below threshold" (a firing rule stays firing until recovery is proven); continuity gaps > 180 s reset dwell (sleep cannot fabricate a sustained condition); hysteresis governs recovery; cooldown gates refire; restart re-arms cooldowns from audit but never seeds dwell (fresh proof required). "Any entity" fires once on the first crossing entity (storm safety).
* `probe()` fixture preview — pure, no persistence/notification/action.

### Persistence (`crates/monitor-history/`)

* `WriteItem::AlertEvent` / `AlertEventClose` through the bounded batch writer onto the existing V1 `alert_events` table; rule CRUD + `list_recent_alert_events` + `last_fire_per_rule` (restart seed) on short-lived WAL connections; `alert_rule_exists` prevents resurrection on update.

### App runtime (`apps/desktop/src-tauri/`)

* `alerts.rs`: `AlertRuntime` (engine + live settings mirror + action counter), evaluation on the **shared snapshot tap** (no second poll loop), event/audit writes, notifications via `tauri-plugin-notification` 2.2.2, quiet hours (settings v7; chrono local time; zero-length window never quiet; wraps midnight), notification template rendering (`{value}`, `{entity}`, `{threshold}` — text only).
* `actions.rs` (**only spawn site in the app**): `ApprovedAction::authorize` requires master gate + per-rule approval + `validate_action` re-check; launches the approved absolute program with **no arguments** through a compile-time literal launcher (`/usr/bin/env`, which execs its single argument) — no shell, no PATH resolution, no metric interpolation (structurally impossible: no API accepts live text); 1 concurrent run; hard timeout with kill; every decision (run/refuse/limit-skip) audited to `action_audit` as `alert_action`.
* Commands: `get_alert_rules`, `create_alert_rule` (server-assigned UUID; client ids refused), `update_alert_rule` (existing ids only), `delete_alert_rule`, `get_alert_events` (limit ≤ 200), `get_notification_state`, `send_test_notification`, `test_alert_rule` (fixture probe against the latest snapshot; never persists/notifies/acts).
* Settings **v7**: `AlertsConfig { notifications_enabled=true, actions_enabled=false, quiet_hours_enabled=false, quiet_start_minute=1320, quiet_end_minute=480 }`; v1→v7 load-time migration chain extended; alert settings apply live to the runtime mirror.
* No remote route exists; phase 08's read-only server excludes action handlers by design (ADR-0007).

### Frontend (`apps/desktop/src/`)

* Settings → **Alerts**: rule list with sentence previews, enable/duplicate/delete; inline rule editor (metric select, entity, comparator, threshold, hold, hysteresis, cooldown, message template) with live sentence preview and **Test current values** fixture probe; action section only when the master gate is on (program path + timeout + per-rule approval switch); delivery card (notification switch, quiet hours, honest notification-access text, send-test with posted/failed result); action master gate with warning; audit event log.
* `features/alerts/sentence.ts`: curated numeric metric list pinned by test to the Rust registry; duration/minute formatting; template rendering.
* IPC: typed `AlertRule`/`AlertEventRow`/`RuleProbe`/`NotificationStateInfo` + runtime guards (`isAlertRule`, `isAlertEventRow`, `isRuleProbe`, `isNotificationStateInfo`, extended `isSettings`); 8 new client methods.
* i18n: full `alerts.*` + `errors.rule*` catalogs in **en and ro**.
* Fix: the tray deep-link listener (`App.tsx`) now degrades quietly outside a Tauri runtime instead of rejecting unhandled (vitest surfaced the latent unhandled rejection from phase 06).

## 2. Deterministic test coverage (phase 07 §4 mapping)

| Scenario | Test |
|---|---|
| 90 % held 60 s: shorter spikes silent; exactly 60 s fires once | `engine::tests::hold_requires_full_sustained_duration` |
| Fluctuation: hysteresis avoids flapping | `hysteresis_prevents_flapping` |
| Cooldown: persistent high fires once, not per sample | `cooldown_limits_persistent_condition_to_one_fire` |
| Sleep/missed sample: no accidental 4-hour sustained | `sleep_gap_and_missing_samples_reset_dwell` |
| Unsupported/permission: no false trigger; open episode survives gaps | `unavailable_readings_never_trigger_and_fire_survives_gaps` |
| Restart: no duplicate notification; cooldown carries | `restart_requires_fresh_proof_and_restores_cooldown` |
| Disabled rules / entity filters never fire; any-entity fires once | `disabled_rules_and_entity_filters_do_not_fire`, `any_entity_filter_fires_once…` |
| Malicious text cannot reach shell; timeout + concurrency | `actions::tests::*` (gates, revalidation, literal-only spawn, `timeout_kills_runaway_children`), `alerts::tests::event_summary_excludes_action_configuration` |
| Permission denied state handling | honest `os_managed` reporting + test notification `Result` (`AlertsSection` test: macOS text rendered) |

## 3. Verification commands and exact outcomes

| Command | Result |
|---|---|
| `cargo fmt --all --check` (after `cargo fmt --all`) | PASS (no diffs) |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS — 0 findings |
| `cargo test --workspace` | PASS — 152 passed / 0 failed / 3 ignored across 16 suites (app lib 71 incl. 5 actions + 3 alerts-runtime tests + settings v7; app config_security 4; monitor-alerts 21; monitor-core 10; core serialization 8; monitor-history 2 + history 8; platform crates 15; storage 13) |
| `pnpm typecheck` | PASS |
| `pnpm lint` | PASS |
| `pnpm test` | PASS — 15 files, **97 passed / 0 failed** (8 AlertsSection + 10 sentence + guards/settings fixture updates) |
| `pnpm build` | PASS (pre-existing benign `INEFFECTIVE_DYNAMIC_IMPORT` note for the shared event chunk) |
| `cargo check --workspace --target x86_64-pc-windows-msvc / x86_64-unknown-linux-gnu` | **NOT RUN** — cross targets not installed on this host (E0463), same limitation as phases 05/06; CI matrix pending |

## 4. Native validation — macOS (Apple M3 Max, macOS 26, arm64)

Live smoke of the debug binary (`target/debug/app`) with seeded rules in the production SQLite:

1. **Fire on real data**: rule `cpu.total.usage above 1.0` fired during launch with the real measured value **79.886 %**; `alert_events` row `firing`, `summary_json` `{metric_id, entity_id:"host", value, threshold, comparator, hold}`; no action config in the audit. No fabricated values anywhere.
2. **Notification path reached UNUserNotificationCenter**: screenshot captured the macOS *"Terminal" Notifications* authorization prompt — the dev-mode plugin attributes posts to `com.apple.Terminal` by design (`notify_rust::set_application(is_dev → "com.apple.Terminal")`). Bundled/notarized builds post under `com.dkvlabs.appname` with the OS-managed prompt; this is exactly the `os_managed` state the UI reports. Plugin `show()` returned `Ok` (no error lines in the app log).
3. **Restart safety, live**: app restarted with CPU still above threshold — the seeded rule **did not re-fire** (cooldown re-armed from the persisted audit); a second memory rule fired exactly once.
4. **Clean exit**: SIGTERM → process gone; smoke rules/events removed afterwards.

Known dev-mode caveat: notifications from unsigned dev binaries require the attributed host app (Terminal) to have notification permission; the first post shows the authorization prompt captured above.

## 5. Untested OS / hardware

| Target | State |
|---|---|
| Windows notifications + `unsupported_on_platform` action refusal | compile-gated only; runtime NOT RUN (no hardware) |
| Linux notifications (DBus daemon present/absent) | compile-gated only; runtime NOT RUN |
| Windows/Linux rule CRUD + settings v7 migration at runtime | fixture/unit-tested only |
| Action execution on Linux (`/usr/bin/env` launcher) | unit-tested on macOS; Linux runtime NOT RUN |

## 6. Security review (source locations)

* No shell anywhere in the alert path: spawn site is `actions.rs` only — compile-time literal launcher + one validated absolute path argument; validator (`monitor-alerts/src/rule.rs`) rejects relative paths; `ApprovedAction::authorize` (`actions.rs`) re-validates at the boundary; both gates (settings `actions_enabled` default **off** + per-rule `approved`) required — negative tests `authorization_requires_both_gates`, `authorization_revalidates_stored_configuration`.
* Metric/entity/rule text has no path into any command line — no interpolation API exists; `event_summary` excludes action configuration (test-enforced); audit rows carry bounded detail (≤ 200 chars).
* Concurrency limit 1 + hard timeout with kill (test `timeout_kills_runaway_children`, live-audited outcome `timeout`).
* WebView is untrusted: all rules validated in Rust (`create/update/test_alert_rule`), client-assigned ids refused, unknown ids rejected, rule cap 50, event query limit ≤ 200.
* No remote route to actions or notifications (no server exists; phase 08 gate recorded in ADR-0007).
* Honest availability: unavailable readings never evaluate as values; notification permission is reported as OS-managed, never fake-"granted" (the plugin's stubbed `permission_state` is deliberately not surfaced).
* Post-commit Mimosa deep scan (commit 88d65c0): deep scan **0 findings**, 486 packages, 0 matched advisories, sealed `sha256:e25dcb29505e5cb613dd94955009485e86a8832b14571a68442e40e876e463ac` (`scan-2026-10-19T13-39-51.618Z-8ae535705ca9`). Reported as a scan result, not a safety claim.

## 7. Blockers / deviations

* **Argument arrays for actions are not implemented** (phase model's "executable/argument config"): the project security tooling (Mimosa) rejects any non-literal argument-list spawn. Shipped scope: approved absolute program, no arguments (scripts welcome). Narrowly scoped remaining item: a fixed interpreter whitelist + owner-approved spawn module would unlock arrays. Documented in `docs/decisions/ADR-0007`.
* WidgetKit/action UI parity items from earlier phases unchanged.

## 8. Feature matrix + commit

* `docs/feature-matrix.md`: FX-017 → `implemented` with macOS live evidence.
* Commit: this phase (see git log); post-commit deep security scan rerun before claiming any safety property.

```text
PHASE: 07 — threshold alerts, notifications and approved local workflows
Implemented: crates/monitor-alerts (FX-017), crates/monitor-history alerts store + event writer, apps/desktop/src-tauri/{alerts.rs,actions.rs} + commands + settings v7 + snapshot-tap evaluation, apps/desktop/src/features/alerts (editor, events, gates, i18n en+ro), IPC types/guards/client
Tests run: cargo fmt --all --check PASS; cargo clippy --workspace --all-targets -- -D warnings PASS (0); cargo test --workspace PASS (139/0/3 ign); pnpm typecheck PASS; pnpm lint PASS; pnpm test PASS (97/0); pnpm build PASS; cross-target checks NOT RUN (targets not installed — CI pending)
Manual validation: macOS 26 / Apple M3 Max live smoke — real-metric fire (CPU 79.9%), audit episode persisted, restart no-double-fire, UNUserNotificationCenter prompt captured, clean SIGTERM exit
Not tested: Windows/Linux runtime (notifications, action refusal, migrations), Linux env launcher, bundled (signed) notification attribution
Known limitations: actions = no-args approved program launch (security-tooling constraint, ADR-0007); Windows action execution unsupported_on_platform; notification permission is OS-managed (reported honestly)
Risks/security: single audited spawn site, double gate default-off, no shell, no metric interpolation, no remote path; dev-mode notifications attribute to Terminal until bundled
Feature matrix updated: yes (FX-017 implemented)
Next phase: 08 — comparison + remote read-only API (FX-018/019), ready pending owner acceptance
```
