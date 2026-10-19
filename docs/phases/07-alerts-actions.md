# Phase 07 — Threshold alerts, notifications and approved local workflows

**Owns:** FX-017.  
**Outcome:** User-configurable safe monitoring rules with reliable edge/hold behavior and no alert storms.

## 1. Preconditions

- Real metrics with clear availability, SQLite persistence and per-metric history.
- Platform OS notification permission flow and action security review.

## 2. Rule model and lifecycle

```text
Rule: id, metric_id, entity_filter, comparator, threshold_in_canonical_unit,
      hold_duration_ms, hysteresis_delta, cooldown_ms, enabled,
      notification_template_id, optional_approved_local_action_id

Rule state: inactive -> pending -> firing -> recovering -> cooldown -> inactive
```

Rules evaluate monotonic sample time. Missing samples **pause/reset** sustained-duration timers according to a documented policy; do not assume missing measurements are below threshold. Persist last trigger event/audit; restore rule state safely on restart without double firing.

## 3. Implementation sequence

1. Define strict validator for rule metric ID, unit-compatible threshold, comparator, hold period, hysteresis/cooldown bounds, duplicate rule IDs and entity existence.
2. Implement deterministic rule state machine in Rust; distinguish threshold crossing, sustained duration and clear threshold for recovery.
3. Persist rules/events. Sample-driven evaluation subscribes to snapshot bus, never polls independently.
4. Integrate platform notification system; request notification permissions with visible explanation and graceful denied state.
5. Implement rule editor and alert event log in Settings → Alerts; include preview, enable/disable, duplicate, test with fixture data (no live destructive action), and notification troubleshooting.
6. Optional local action workflow **disabled by default**: store only user-defined executable/argument config, require local approval, limit concurrency/timeouts, never shell-interpolate live metric strings. Review process execution command selection for each OS before implementation.
7. Actions are not reachable from remote API. Display a special “Action enabled” state with last-run result/audit.
8. Add throttling and quiet hours as optional settings only after core logic has tests.

## 4. Deterministic test scenarios

- CPU 90% threshold held 60 s: spikes shorter than 60 s do not alert; exactly 60 s alerts once.
- Fluctuating values around threshold: hysteresis avoids rapid resolve/retrigger.
- Cooldown 5 min: persistent high CPU triggers once, not every sample.
- Sleep and a missed sample: no accidental 4-hour “sustained” condition on resume.
- Metric unsupported/permission denied: no false trigger.
- System restart: no duplicate immediate notification unless rule condition is freshly proven.
- Malicious process name/metric text: cannot reach shell code; timeout and concurrency enforce limits.
- Notification permission denied: event is recorded but error state shown.

## 5. Acceptance gate

- [ ] All hold/hysteresis/cooldown tests pass with virtual monotonic time.
- [ ] Rules persist and respect disabled state/restart behavior.
- [ ] OS notifications work on native test targets or are explicitly permission-limited.
- [ ] Optional actions cannot execute without user opt-in and approval.
- [ ] No path from any remote route to local workflow or process controls.
- [ ] Security negatives and source code locations in `docs/progress/PHASE-07.md`.
