# Copy/paste into Codex — independent phase review

```text
Perform a critical, evidence-based review of the current system-monitor phase.

Read AGENTS.md, docs/feature-matrix.md, docs/testing-qa.md,
docs/security-privacy.md, docs/implementation-phases.md and the selected
phase document. Inspect the actual changed code and tests.

Check:
- Feature parity claims versus actual runtime behavior.
- Real hardware reading paths, capability states, units and data provenance.
- Sleep/resume, zero/unsupported semantics and counter reset handling.
- IPC validation, privilege boundaries, Tauri permissions and remote safety.
- Process PID identity or storage Trash safeguards if this phase touches them.
- Memory/CPU overhead, bounded queues and SQLite growth.
- Platform differences and actual native test evidence.
- No default mock values in production.

Run checks available in this environment. Never present an unrun check as passed.

Return a severity-ranked findings list with file and line references,
reproduction steps, concrete fixes, tests added/needed, and a final pass/block
recommendation for the phase gate. Prioritize functional correctness and
security over formatting nitpicks.
```
