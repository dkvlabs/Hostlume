# Copy/paste into Codex — run one implementation phase

Replace `<NN>` and `<EXACT_PHASE_FILE>` with the next phase from `docs/phases/README.md`.

```text
IMPLEMENT THE NEXT PHASE OF THE CROSS-PLATFORM SYSTEM MONITOR.

Selected phase: <NN> — <EXACT_PHASE_FILE>

Before changes:
- Read AGENTS.md and README.md.
- Inspect actual repository state, current tests, existing progress reports,
  and docs/feature-matrix.md. Preserve existing work.
- Read docs/architecture.md, docs/metrics-data-contract.md,
  docs/design.md, docs/platform-capabilities.md,
  docs/security-privacy.md and docs/testing-qa.md.
- Read docs/implementation-phases.md and
  docs/phases/<EXACT_PHASE_FILE> completely.

Execution:
1. State expected outputs, owning FX feature IDs and acceptance criteria.
2. Identify platform requirements and API/library assumptions to verify.
3. Implement production code with real native metrics and explicit
   availability/error states. Mocks only in tests and labeled development demo.
4. Implement unit, integration, UI and security tests applicable to this phase.
5. Run formatting, lint, typecheck, cargo tests and any available native builds.
6. Record exact test commands/results; do not imply unavailable OSes were tested.
7. If hardware prevents a metric, implement a capability probe with an accurate
   reason and document platform_limited instead of fabricating measurements.
8. Update docs/feature-matrix.md ONLY with evidence, write
   docs/progress/PHASE-<NN>.md and update ADRs if architecture changes.
9. Stop at phase exit gate: if blocked, report root cause, workaround,
   untested targets and a narrowly scoped remaining checklist.

Non-negotiable:
- No remote write/control routes; LAN access auth+TLS+opt-in.
- No unapproved scripts, elevated helpers, permanent deletion or fan control.
- Do not use the reference application's code, trademarks or UI assets.
- Production build must not silently use mock data.

Return this report:
PHASE / IMPLEMENTED FX IDS / MODIFIED FILES / TESTS RUN / NATIVE OS TESTS /
UNTESTED OS-HARDWARE / SECURITY CHECK / BLOCKERS / NEXT PHASE READINESS.
```

## Phase selection example

For phase 02, use:

```text
Selected phase: 02 — 02-core-telemetry-dashboard.md
```

Send the entire template above with that substitution, or use an existing Codex task workflow to apply the same execution checklist. Only ask an agent to parallelize native collectors after Phase 01 contracts are frozen.
