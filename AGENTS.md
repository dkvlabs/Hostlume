# AGENTS.md — instructions for Codex and coding agents

You are building an **independent** cross-platform system monitor described by the `docs/` directory. This repository initially contains **specifications only**. Do not present features described in docs as implemented.

## 1. Read order and sources of truth

Before writing code, read:

1. `README.md`
2. `docs/reference-audit.md`
3. `docs/requirements.md` + `docs/feature-matrix.md`
4. `docs/architecture.md` + `docs/metrics-data-contract.md`
5. `docs/design.md` + `docs/platform-capabilities.md`
6. `docs/security-privacy.md` + `docs/testing-qa.md`
7. `docs/implementation-phases.md` and **the current** `docs/phases/NN-*.md`

**Precedence:** User's latest request > safety and security constraints in this file > currently accepted architecture decisions > phase document > optional suggestions. Escalate real contradictions in a short design note rather than inventing a workaround. All substantive architectural deviations require a decision note in `docs/decisions/` (create when needed).

## 2. Working protocol (one phase at a time)

- Inspect repository state and existing code/tests; do not overwrite user changes.
- Identify current phase and `FX-*` feature IDs before modifying code.
- Write a concise implementation plan; define acceptance tests **before** implementation.
- Build vertical slices: real native collector -> normalized telemetry -> Rust tests -> IPC -> React -> UI test.
- Use existing libraries when appropriate **after checking maintained status, APIs, platform support and licenses**. Lock versions; do not invent crate APIs.
- Run formatting, linting, type checks and available tests. Never say a test/build passed when it was not run.
- After work, update `docs/feature-matrix.md` statuses (only if actual evidence supports it), a phase report under `docs/progress/PHASE-XX.md` and any affected docs. Track blocked capabilities by OS/hardware.
- Do not proceed to a new phase with outstanding exit criteria unless the user explicitly accepts and the blocker is documented.

## 3. Scope boundaries

**Ship parity first.** No auth accounts, subscription billing, cloud analytics, AI-generated recommendations, remote control, fan control, multi-host fleet features, or telemetry uploads in v1. Phase 10 is optional.

Use `APP_NAME` until the user supplies final naming/branding. Never use iStatistica's name, logo, screenshots, text or icons as product assets. Functional descriptions are reference material; implement original code and original visuals.

## 4. Technology and folder conventions

- Tauri **2**, Rust core, React + TypeScript + Vite, Tailwind CSS + shadcn/ui, ECharts, Zustand.
- Rust portable collectors use `sysinfo` where its semantics are appropriate; isolate `macOS`, `Windows` and `Linux` collectors behind capability-driven interfaces.
- SQLite local history (WAL), migrations and retention/downsampling. Optional Axum web server remains disabled by default.
- Suggested code: `apps/desktop/` and `crates/{monitor-core,monitor-platform-macos,monitor-platform-windows,monitor-platform-linux,monitor-history,monitor-storage,monitor-alerts,monitor-remote}/`.
- Keep UI business logic independent from Tauri; use typed IPC adapters and shared API types.
- Avoid oversized files and circular dependencies. Centralize unit conversions and metric definitions.

## 5. Data integrity — NEVER break

- Do **not** manufacture hardware sensor measurements, CPU values, rates or history.
- Every reading must carry timestamp, source/collector, normalized units, availability, and an optional reason.
- System CPU percentage is **0–100 total capacity**, but per-process CPU may be **0–100 x logical-CPU-count**, with a documented normalized alternative. Define which one is displayed.
- Throughput = change in monotonic byte counter / monotonic elapsed seconds; handle counter reset, reboot, interface recreation, sleep/wake and wraparound.
- Do not sum memory category fields that have different semantics across OSes.
- Distinguish unified GPU memory from dedicated VRAM, disk logical size from physical usage, and live values from cached readings.
- Unsupported, stale, permission-restricted and errored metrics must have visible UI states, not zeros.
- `mock` providers exist for tests and explicitly marked demo builds only. Production builds must never silently fallback to simulated telemetry.

## 6. Security defaults

- Everything works offline. No external IP/country lookups or network requests without user opt-in.
- The optional remote server is **off by default** and bound to loopback until user configures LAN use. Protect LAN access with strong auth and TLS; API remains **read-only**.
- A desktop WebView is untrusted input. All Tauri commands require least-privilege capabilities, validation and explicit paths/actions.
- Storage cleanup uses OS Trash/Recycle Bin and explicit user confirmation; do not implement recursive permanent deletion.
- Process termination requires explicit confirmation, process identity recheck (`pid` + creation/start-time), and OS permission handling. Never force-kill a process without an informed user action.
- Command/workflow actions are disabled by default, require local approval, must not be exposed through remote API and cannot receive untrusted metric interpolations as executable code.
- Never default to administrator/root execution. Signed native helpers are separate, consented and optional.
- Redact tokens, usernames, sensitive paths and addresses in logs/diagnostic exports.

## 7. Verification commands (examples, not claims of execution)

Run available scripts after bootstrapping (fix commands to match actual repo scripts):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
```

Each supported target also requires **native** build, smoke test, architecture, packaging and permission checks. Cross-compiling is not hardware validation.

## 8. Required response/report format

End each phase with:

```text
PHASE: NN — title
Implemented: [paths, feature IDs]
Tests run: [commands and exact outcomes]
Manual validation: [OS, hardware, results]
Not tested: [explicit list]
Known limitations: [with status + fallback]
Risks/security: [changes and mitigations]
Feature matrix updated: yes/no
Next phase: NN+1 [ready/blocked]
```

If blocked, document concrete evidence and a bounded alternative. Do not mark a page complete when only the React screen works. **Production is a behavior-and-test milestone, not a screenshot milestone.**
