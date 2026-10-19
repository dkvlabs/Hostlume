# Implementation roadmap — ordered, independently verifiable phases

> **Read:** `AGENTS.md` first. Each phase has its own document in [`phases/`](phases/README.md). This roadmap is not permission to skip tests or call placeholders completed.

## Why this sequencing

Build a **real multi-OS monitoring spine first**, then add history and complex workflows, then hardware-dependent capabilities, then secure remote distribution. This avoids shipping three impressive-looking UI shells with simulated counters. The work can be distributed across Codex agents only after types/contracts are stabilized.

## Phase plan

| Phase | Name | Main output | Related features | Exit gate |
|---|---|---|---|---|
| **00** | Reference and platform discovery | validated assumptions, spike plan, feature semantics, licenses | all | architecture and risk inventory approved |
| **01** | App shell and contracts | Tauri 2 + Rust workspace, React shell, typed IPC, settings, CI | FX-020 | build/test on all available native targets |
| **02** | Portable collectors & dashboard | actual CPU/memory/disks/network, capabilities, responsive overview | FX-001–006 | live metrics and deterministic math tests |
| **03** | History + applications | SQLite raw/rollups, process manager, safe quit/force quit | FX-007 | PID identity guard, history tests, UI workflow |
| **04** | Storage analyzer + network detail | treemap/cancel/Trash, disk I/O, interface diagnostics | FX-008,009,011 | large scan safety, device changes, native Trash |
| **05** | GPU, sensors & power | native capabilities/probes, GPU, temperatures/fans, battery, Bluetooth | FX-010,012–014,021 | hardware matrix with explicit unsupported paths |
| **06** | Tray and widgets | menu-bar/tray indicators, popup, native widgets/fallback | FX-015–016 | close-to-tray and OS-specific smoke tests |
| **07** | Alerts & approved actions | rule engine, notifications, optional local workflows | FX-017 | duration/hysteresis/cooldown, negative security tests |
| **08** | Comparison + remote API | synchronized charts, recording/CSV, Axum read-only LAN UI | FX-018–019 | tests for auth/TLS/loopback and CSV round trip |
| **09** | Hardening and release | native installers, performance/security/a11y QA, documentation | all | release gates pass on each supported OS |
| **10** | Optional post-v1 multi-host | design for paired collector agents and centralized viewer | FX-022 | new threat model; explicitly not v1 |

### Milestones

**M1 — Usable cross-platform baseline:** 00–02.  
**M2 — Investigation toolkit:** 03–04.  
**M3 — Hardware-specific and desktop integrations:** 05–07.  
**M4 — Remote capability and production candidate:** 08–09.

## Phase dependency rules

```text
00 -> 01 -> 02 -> 03 -> 04 -> 05 -> 06 -> 07 -> 08 -> 09
                                          └── 10 (after v1, optional)
```

- 02 depends on telemetry contracts established in 01.
- 03 can start only after the collector event/snapshot format is stable.
- 04 depends on safe action abstraction and persisted history; filesystem scanning can be separately prototyped.
- 05 is allowed to mark some **metric/host** combinations `platform_limited` only after documented probe evidence and reasonable alternatives.
- 06 consumes shared metrics; must not create duplicate collectors.
- 07 uses 03 history/timing and 05 capability model.
- 08 uses history and capability filter; remote API never reaches action handlers.
- 09 is a release validation phase, not a place to postpone essential correctness.

## Time and staffing assumptions

Do not treat these as estimates or promises. Native hardware integrations, real OS packaging and vendor SDK licensing can dominate elapsed time. Use **work units** and acceptance criteria rather than guessing calendar completion. For parallel coding, split independent adapters after 01 stabilizes the telemetry trait. Parallelize UI, source collectors and deterministic test fixtures without conflicting on core types.

## Every phase must produce

1. Implemented feature IDs mapped to actual code paths.
2. Automated tests and native manual-validation evidence, with run commands and outcomes.
3. UX states for unsupported/permission/stale/error cases.
4. Updated feature matrix status and `docs/progress/PHASE-XX.md` (create on first phase).
5. Design/architecture ADR for any material deviation.
6. Exact outstanding blockers, cross-OS differences and next phase readiness.

## Definition of done by layer

| Layer | Definition of done |
|---|---|
| Rust collector | true OS readings, typed failures, bounded cost, native unit test |
| IPC | typed response, authorization, revalidation of mutable operations |
| UI | real subscribed data, accessible states, loading/error/empty handled |
| History | persisted with UTC, retention/rollup test and gap correctness |
| OS capability | hardware-probed; actual source and limitations documented |
| Phase | tests run, artifact exists, evidence recorded, dependencies satisfied |

## Prioritization under resource constraints

If hardware access or OS-specific integrations block progress, ship a truthful **limited-capability** release rather than fake values. Keep P0 CPU/RAM/disks/network/processes/storage functionality strong; conditional GPU/fan/temperature belongs behind capability flags. Remote server must pass security gates or remain disabled/unshipped; never expose an insecure substitute just to claim parity.
