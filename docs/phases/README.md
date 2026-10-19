# Detailed phase execution guides

Use `docs/prompts/PHASE_RUNNER.md` with **exactly one phase file**. Every phase includes objective, sequential work, files, tests, platform gates and an acceptance checklist.

| Order | File |
|---|---|
| 00 | [`00-discovery.md`](00-discovery.md) |
| 01 | [`01-foundation.md`](01-foundation.md) |
| 02 | [`02-core-telemetry-dashboard.md`](02-core-telemetry-dashboard.md) |
| 03 | [`03-processes-history.md`](03-processes-history.md) |
| 04 | [`04-storage-network.md`](04-storage-network.md) |
| 05 | [`05-gpu-sensors-power.md`](05-gpu-sensors-power.md) |
| 06 | [`06-tray-widgets.md`](06-tray-widgets.md) |
| 07 | [`07-alerts-actions.md`](07-alerts-actions.md) |
| 08 | [`08-comparison-remote.md`](08-comparison-remote.md) |
| 09 | [`09-qa-release.md`](09-qa-release.md) |
| 10 (optional) | [`10-multi-host-extension.md`](10-multi-host-extension.md) |

### Agent cadence

For each phase:

1. Read current code and docs.
2. Check accepted architecture/capability decisions.
3. List tasks and tests with feature IDs.
4. Implement the smallest complete vertical slices.
5. Verify with commands that actually ran.
6. Record evidence in `docs/progress/PHASE-XX.md`.
7. Update `docs/feature-matrix.md` statuses only with evidence.
8. Stop at the phase gate and report blockers.
