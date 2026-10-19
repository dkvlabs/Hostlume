# Phase 00 — Reference research, feasibility and decisions

**Outcome:** Turn the specification into a validated build plan. **No production app functionality is expected.** All FX features remain `planned`.

## 0. Preconditions

- Read root `AGENTS.md`, reference audit, requirements, feature matrix, architecture, platform capabilities, security and design.
- Record current OS, architecture and available build machines (do not imply hardware is present if it isn't).
- Confirm documentation date and verify any changed API/toolchain capabilities using official sources.

## 1. Tasks in order

1. **Create research log:** `docs/progress/PHASE-00.md`, `docs/decisions/`, `docs/evidence/`. Include verified source URLs and inspection date.
2. **Platform targets:** decide minimum tested macOS/Windows/Linux versions based on *current* Tauri/Rust/webview support; write exact `SUPPORTED_TARGETS.md`. Do not promise Intel Mac, ARM Windows or Linux ARM release without hardware/build path.
3. **Dependency/license scan:** evaluate `sysinfo`, Tauri 2 plugins, SQLite client, GPU vendors, optional sensors readers, tray/WidgetKit, and candidate updater. Record SPDX license or source link and whether redistribution is acceptable; defer unreviewed libraries.
4. **Metric semantics:** adopt `docs/metrics-data-contract.md`; resolve per-process CPU normalization, UTC timestamps, missingness and OS memory categories.
5. **Native spikes:** on available hardware, prove a real CPU/memory snapshot. Explore physical disk counters, per-core CPU and battery; log attempts and permissions.
6. **High-risk feasibility:** research Apple Silicon temperatures/fans and GPU %, Windows sensor coverage, Linux tray and macOS WidgetKit/Windows widgets. Record what can be genuinely tested and what remains unproven.
7. **Security decisions:** verify remote API default off, TLS+token required for LAN, action gating and no privileged default.
8. **Create ADRs:** for SQLite layer, typed IPC strategy, native sensor integration and CI platform matrix, with tradeoffs.
9. **UI concept review:** accept original design tokens/screens; clearly tag any reference behavioral unknowns requiring manual inspection.
10. **Bootstrap readiness:** produce an ordered dependency list and agreed phase 01 target.

## 2. Required documents

- `docs/progress/PHASE-00.md` with evidence/unknowns.
- `docs/SUPPORTED_TARGETS.md` listing *validated vs intended* targets.
- `docs/decisions/ADR-0001-telemetry-contract.md` and other accepted decisions.
- `docs/evidence/FEASIBILITY_SPIKES.md` including tested hardware and original sources.
- Update existing architecture/design documents only for resolved decisions.

## 3. Verification and risks

- Verify source docs exist and all links to local build docs are valid.
- Confirm target toolchain commands (`rustc --version`, `cargo --version`, `node --version`, `pnpm --version`, installed WebView/Tauri prerequisites) **if tools are available**; record actual versions.
- If a platform is not currently available, record `NOT TESTED — no hardware/runner`; do not claim tested.
- If a sensor source is unlicensed or unstable, defer it to `platform_limited` research without blocking P0 metrics.

## 4. Exit checklist

- [ ] Realistic target OS list and unsupported assumptions recorded.
- [ ] Feasibility risks prioritized with named mitigation or experiment.
- [ ] Legal/licensing and native helper concerns documented.
- [ ] Telemetry data contract accepted.
- [ ] Native source/probe strategy per OS specified; no invented APIs.
- [ ] Phase 01 initialization approach and CI plan accepted.

**Agent handoff:** “Phase 00 completed with research evidence. Start Phase 01; do not implement sensors or UI copies yet.”
