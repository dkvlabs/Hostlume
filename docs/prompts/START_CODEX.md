# Copy/paste into Codex — initialize this project

> Open the unzipped **repository root** in Codex before sending. Use Codex's repository editing capabilities. This is a task prompt, not a verified command.

```text
You are the principal engineer responsible for implementing a production-grade, original cross-platform system monitoring application for macOS, Windows and Linux.

The project currently contains a documentation build kit, not production code.

1. Read the root AGENTS.md and README.md in full.
2. Read docs/reference-audit.md, requirements.md, feature-matrix.md,
   architecture.md, metrics-data-contract.md, design.md,
   platform-capabilities.md, security-privacy.md, testing-qa.md,
   release-distribution.md and implementation-phases.md.
3. Read docs/phases/00-discovery.md and execute PHASE 00 ONLY.
4. Inspect the repository before changing anything and preserve existing files.
5. Verify dependency and OS API assumptions with current official documentation;
   never invent functions, versions, licenses or hardware support.
6. Create the phase 00 research/evidence files and concise implementation plan.
7. Only after that phase is completed and accepted should you prepare Phase 01.

Build stack:
- Tauri 2 + Rust
- React, TypeScript, Vite
- Tailwind CSS + shadcn/ui
- ECharts + Zustand
- SQLite with migrations and retention
- Optional Axum local read-only remote server

Core principles:
- Real native readings only in production. Never invent sensor readings.
- Capability states: available, unsupported, permission_required,
  temporarily_unavailable, stale, error.
- Cross-platform Rust adapters feed one shared metric/snapshot bus.
- Protect against PID reuse before process termination.
- Storage cleanup goes to native Trash/Recycle Bin only, with consent.
- Remote server disabled by default; opt-in LAN requires token auth and TLS;
  no remote process/shell/filesystem controls.
- Original UI and branding; do not copy the reference app's assets.

Work in small verifiable steps and run available tests. Do not report a native
OS as tested unless the build or tests actually ran on that OS. Update
feature-matrix.md and docs/progress/PHASE-00.md with evidence and blockers.

Final response must include: implemented documents/files, validated sources,
tests actually run, untested hardware/OS, security concerns and next phase readiness.
```

When phase 00 passes, execute Phase 01 using `PHASE_RUNNER.md`. Repeat sequentially. Do not paste all phase prompts at once and accept unverified claims of completion.
