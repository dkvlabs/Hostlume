# Phase 00 — Reference research, feasibility and decisions
Date: 19.10.2026
Commit: a72577e (HEAD at time of writing; Phase 00 artifacts are in the working tree, not yet committed)
Feature IDs: none implemented — per docs/phases/00-discovery.md, **all FX features remain `planned`**
Status: complete (awaiting owner acceptance)

## Changes implemented

Research/evidence artifacts only — no production code, no app functionality (phase mandate):

- `docs/progress/PHASE-00.md` — this research log, verification results, bootstrap plan.
- `docs/SUPPORTED_TARGETS.md` — validated vs intended OS/arch targets.
- `docs/decisions/ADR-0001-telemetry-contract.md` — contract adopted; per-process CPU normalization, warm-up, missingness, timestamps, memory, retention resolved.
- `docs/decisions/ADR-0002-sqlite-history-layer.md` — rusqlite 0.40.2 (bundled) + custom migration runner + single-writer queue.
- `docs/decisions/ADR-0003-ipc-transport.md` — Tauri commands + one `telemetry://snapshot` broadcast with sequence/generation resubscribe protocol.
- `docs/decisions/ADR-0004-native-metric-sources.md` — per-OS source strategy; no privileged helper in v1; `battery` crate rejected (unmaintained).
- `docs/decisions/ADR-0005-ci-platform-matrix.md` — GitHub Actions matrix; CI-vs-hardware validation ladder.
- `docs/evidence/FEASIBILITY_SPIKES.md` — live spike logs (macOS arm64, unprivileged).
- `docs/evidence/DEPENDENCY_LICENSE_SCAN.md` — verified versions/licenses/redistribution decisions.
- `docs/evidence/RISK_REGISTER.md` — 14 risks with status, mitigation and owning gate.
- Throwaway spike code kept **outside** the repo at `/tmp/hostlume-spike` (not a deliverable).

Existing files were preserved; no prior document was modified. `docs/feature-matrix.md` intentionally **unchanged** (see "Feature matrix edits").

## Preconditions (phase doc §0)

- Required docs read in full: AGENTS.md, README.md, reference-audit.md, requirements.md, feature-matrix.md, architecture.md, metrics-data-contract.md, design.md, platform-capabilities.md, security-privacy.md, testing-qa.md, release-distribution.md, implementation-phases.md, phases/00-discovery.md, prompts/PHASE_RUNNER.md, plus both templates.
- Docs drafted 18.10.2026 (reference-audit date); re-verification of toolchain/API facts performed **19.10.2026** against official sources (log below).
- Machine record: MacBook Pro Mac15,11, Apple M3 Max, arm64, macOS 27.0.1 (26A434). **No other build machines available in this environment** — Windows/Linux: NOT TESTED, no hardware/runner.

## Verification

| Environment | Command / scenario | Result | Log file |
|---|---|---|---|
| macOS arm64 | `rustc --version` / `cargo --version` → 1.99.1; `node --version` → v24.22.0; `pnpm --version` → 10.33.2; `xcode-select -p` → full Xcode | PASS (versions recorded) | this file, SUPPORTED_TARGETS.md |
| macOS arm64 | sysinfo 0.39.6 Rust spike: CPU warm-up + valid per-core/global usage, memory/swap, disks (incl. external exFAT), 29 network interfaces with interval counters, 836 processes with pid+start_time | PASS | docs/evidence/FEASIBILITY_SPIKES.md §spike 1 |
| macOS arm64 | sysinfo `Components` temperature spike | PASS — 22 real sensors, unprivileged | FEASIBILITY_SPIKES.md §spike 2 |
| macOS arm64 | `ioreg -rn AppleSmartBattery` battery keys (cycles 180, design/full capacity, charging state) | PASS unprivileged | FEASIBILITY_SPIKES.md §cross-check 1 |
| macOS arm64 | `iostat -d` per-device rates; `vm_stat`; `sysctl kern.boottime` vs sysinfo `boot_time` (identical: 1792171723) | PASS | FEASIBILITY_SPIKES.md §cross-checks 3–5 |
| macOS arm64 | `powermetrics` without root | EXPECTED FAIL — `must be invoked as the superuser`; `smc` sampler unrecognized → root-gated thermal tooling unusable by design | FEASIBILITY_SPIKES.md §cross-check 6 |
| macOS arm64 | Re-verification pass 19.10.2026 (phase-runner invocation): toolchain versions re-recorded (unchanged); both spike binaries re-run from the retained `/tmp/hostlume-spike` workspace | PASS — all findings reproduced; 22 sensors again; `als-temp` live-returned no reading (first-hand evidence for per-sensor null semantics) | FEASIBILITY_SPIKES.md §spikes 1–2 |
| Windows | any build/test | NOT_RUN — no hardware/runner | RISK_REGISTER R10 |
| Linux | any build/test | NOT_RUN — no hardware/runner | RISK_REGISTER R10 |

No `cargo test`/lint suites exist yet (no workspace) — the only executable verification possible in Phase 00 was the spike, which ran and is logged with exact outputs.

Link validation (phase doc §3, re-checked 19.10.2026): every local `docs/…` path referenced by the Phase 00 artifacts resolves to an existing file. The only two absent referenced paths are documented intentional absences: `docs/evidence/REFERENCE_WALKTHROUGH.md` (deliberately not created — no reference-app inspection performed, RISK_REGISTER R14) and `docs/progress/CI-BASELINE.md` (ADR-0005 names it as a Phase 01 deliverable, created at first green CI run).

## Verified sources (fetched 19.10.2026)

| Source | URL | Verified fact |
|---|---|---|
| Tauri 2 prerequisites | https://v2.tauri.app/start/prerequisites/ | macOS 10.15+; Windows WebView2 preinstalled on Win10 1803+; MSVC toolchain; Linux WebKitGTK **4.1** |
| crates.io API: tauri | https://crates.io/api/v1/crates/tauri | stable 2.12.4 (3.0.0-alpha.5 pre-release), Apache-2.0 OR MIT, updated 2026-10-14 |
| crates.io API: sysinfo | https://crates.io/api/v1/crates/sysinfo | 0.39.6, MIT, updated 2026-07-09 |
| crates.io API: rusqlite | https://crates.io/api/v1/crates/rusqlite | 0.40.2, MIT, updated 2026-08-08 |
| crates.io API: axum | https://crates.io/api/v1/crates/axum | 0.8.9, MIT, updated 2026-04-14 |
| crates.io API: trash | https://crates.io/api/v1/crates/trash | 5.2.9, MIT, updated 2026-09-13 |
| crates.io API: battery | https://crates.io/api/v1/crates/battery | 0.7.8, **updated 2020-11-01 → rejected (unmaintained)** |
| npm: echarts | https://registry.npmjs.org/echarts/latest | 6.1.0, Apache-2.0 |
| npm: zustand | https://registry.npmjs.org/zustand/latest | 5.0.15, MIT |
| npm: vite | https://registry.npmjs.org/vite/latest | 8.3.4, MIT |
| npm: react | https://registry.npmjs.org/react/latest | 19.3.1, MIT |
| npm: tailwindcss | https://registry.npmjs.org/tailwindcss/latest | 4.3.3, MIT |
| npm: shadcn | https://registry.npmjs.org/shadcn/latest | 4.22.1, MIT |
| npm: typescript | https://registry.npmjs.org/typescript/latest | 7.0.2, Apache-2.0 (Vite/tooling compatibility to confirm in Phase 01) |
| Microsoft PDH docs | https://learn.microsoft.com/en-us/windows/win32/perfctrs/about-performance-counters | Performance counters "not designed to be collected more than once per second"; recommends GetSystemTimes/GlobalMemoryStatusEx/psapi for lower overhead |
| Linux hwmon sysfs ABI | https://docs.kernel.org/hwmon/sysfs-interface.html | `<type><number>_<item>` naming; temperature in millidegrees Celsius; standard attributes world-readable; fan unit statement not surfaced by fetch → re-confirm at implementation |
| Apple WidgetKit overview | https://developer.apple.com/documentation/widgetkit (via tutorials JSON) | Timeline-based updates, "energy-efficient" refresh; Live Activities/Controls use different mechanisms |
| Apple "Creating a Widget Extension" | https://developer.apple.com/documentation/widgetkit/creating-a-widget-extension (via tutorials JSON) | Widget = **Application Extension target**; "You build widgets using SwiftUI"; App Groups **not** mentioned on this page → sharing mechanism to verify at Phase 06 |
| Apple IOPSlib power docs | https://developer.apple.com/documentation/iokit/iopslib_h | **404 on fetch — URL unverified.** Battery facts instead proven empirically via `ioreg AppleSmartBattery` (unprivileged). Locate correct IOKit power-source doc page before Phase 05 implementation |

Reference-app walkthrough: **not performed.** `docs/evidence/REFERENCE_WALKTHROUGH.md` was deliberately not created; per reference-audit.md, precise UX parity claims stay out of scope until/unless the reference product is actually inspected (RISK_REGISTER R14).

## Platform capability findings

| Metric | OS + hardware | Source | Availability | Notes |
|---|---|---|---|---|
| CPU global + per-core | macOS 27.0.1 / M3 Max | sysinfo 0.39.6 | available (proven) | first-sample warm-up policy still required; per-core id = slice index |
| Memory/swap breakdown | macOS / M3 Max | sysinfo + vm_stat | available (proven) | 16 KiB pages; OS-specific breakdown struct required |
| Disks/mounts incl. external | macOS / M3 Max | sysinfo Disks | available (proven) | exFAT external volume enumerated unprivileged |
| Network cumulative+interval bytes | macOS / M3 Max | sysinfo Networks | available (proven) | 29 interfaces; rate math in-process |
| Process list w/ pid+start_time | macOS / M3 Max | sysinfo | available (proven) | per-process CPU needs two samples (first refresh → 0.0%) |
| Temperatures | macOS / M3 Max (Apple Silicon) | sysinfo Components (IOKit HID Apple-vendor sensors) | available (proven) | 22 sensors; `als-temp` returned no reading → real availability-state material; no critical thresholds exposed |
| Fan RPM | macOS / M3 Max | — | **unsupported (no source found)** | powermetrics root-gated; research item R1 |
| GPU utilization/VRAM | macOS / M3 Max | — | **unproven** | Phase 05 spike required (R1) |
| Battery charge/cycles/health | macOS / M3 Max | IOKit `AppleSmartBattery` via ioreg | available (proven unprivileged; Rust adapter pending) | cycles=180, design=8579 mAh, full=7300 mAh → health derivable; sysinfo has no battery API |
| Disk device I/O counters | macOS | iostat = rate-only; IOKit IOBlockStorageDriver = unproven | partially proven (R12) | provenance must be marked per contract §4 |
| Everything above on Windows | — | — | NOT TESTED — no hardware/runner | doc-level facts only (PDH guidance) |
| Everything above on Linux | — | — | NOT TESTED — no hardware/runner | doc-level facts only (hwmon ABI) |

## Security and data integrity

- All spikes ran **unprivileged**; no sudo used; `powermetrics` refusal documented rather than worked around — consistent with "never default to administrator/root" (AGENTS.md §6).
- No network requests were made by spikes; all verification fetches were read-only documentation/registry lookups.
- No secrets, tokens or personal data written into artifacts. The spike evidence quotes field names and generic system values; the machine's hostname and battery adapter serial appear only in raw local run output and are intentionally not reproduced in `docs/evidence/FEASIBILITY_SPIKES.md`. Diagnostic exports will apply the redaction rules of security-privacy.md §7 (tokens, usernames, sensitive paths, addresses).
- SQL policy pre-committed (ADR-0002): parameter binding only; no formatted SQL strings.
- Data-integrity contract adopted with the null-not-zero invariant and WARMUP semantics (ADR-0001); mock-only-in-tests policy restated in ADR-0004.
- Remote-server defaults (off; loopback; token+TLS for LAN; read-only routes) re-affirmed from security-privacy.md §6 — no implementation exists yet, so nothing to audit; Phase 08 gate owns it.

## Blockers / untested

- **BLOCKER for cross-platform claims:** no Windows or Linux machine/runner in this environment. Mitigation: ADR-0005 CI matrix becomes the Phase 01 verification vehicle; until green, SUPPORTED_TARGETS.md keeps those rows at "NOT tested".
- **Untested hardware/OS (explicit):** Windows 10/11 (any arch); all Linux distros; Intel macOS; macOS <27 on real app builds; any battery-absent desktop; any machine with fans/sensors beyond this M3 Max; GPU telemetry on all OSes; Bluetooth battery on all OSes.
- No Tauri app has been built yet — spikes used plain Rust binaries. Tauri 2.12.4 build viability on this machine is a Phase 01 task, not a Phase 00 claim.
- Apple IOKit power-source documentation URL 404'd (source log) — battery implementation must locate the correct doc page or rely on the empirically proven `AppleSmartBattery` keys with an in-repo test.

## Feature matrix edits

**None.** docs/phases/00-discovery.md mandates all FX features remain `planned` in Phase 00; no FX acceptance evidence exists yet. The matrix's Phase 00 cross-cutting scope (reference audit, OS support research, license checks, risk register) is fulfilled by: reference-audit.md (pre-existing, re-read), SUPPORTED_TARGETS.md, DEPENDENCY_LICENSE_SCAN.md, RISK_REGISTER.md.

## UI concept review (phase task 9)

- **Accepted as Phase 01 baseline:** the original design system in design.md — semantic color tokens (§2 dark/light proposals pending contrast tests), 8 px rhythm, component inventory with mandatory per-state rendering (§4), state treatment table (§15), and the accessibility/UI checklist (§16). No reference-app screenshot, icon, layout or asset is copied anywhere; screens are original compositions serving the same information need.
- **Reference behavioral unknowns, tagged for manual inspection only** (from reference-audit.md qualifications; no claim of parity is made until then): exact chart sampling/aggregation behavior, event/notification semantics, minimum-OS behavior, and per-model GPU/fan/temperature coverage. These stay in RISK_REGISTER R14; `docs/evidence/REFERENCE_WALKTHROUGH.md` is deliberately **not created** because the reference product was not inspected in this session.
- **Constraints carried into Phase 01:** `APP_NAME` branding placeholder until the owner supplies final naming; i18n message keys from day one (English default, Romanian files prepared per requirements.md); every UI component must implement the full availability-state set rather than rendering zeros.

## Gate review

- [x] Realistic target OS list and unsupported assumptions recorded (SUPPORTED_TARGETS.md)
- [x] Feasibility risks prioritized with named mitigation/experiment (RISK_REGISTER.md + spike priority list)
- [x] Legal/licensing and native-helper concerns documented (DEPENDENCY_LICENSE_SCAN.md, ADR-0004 helper policy)
- [x] Telemetry data contract accepted (ADR-0001)
- [x] Native source/probe strategy per OS specified; no invented APIs (ADR-0004; spike API corrections logged)
- [x] Phase 01 initialization approach and CI plan accepted (ADR-0005 + bootstrap plan below)

## Bootstrap readiness — ordered dependency list (task 10)

1. `rustup`-managed stable Rust (CI) / Homebrew 1.99.1 (local); targets: `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`.
2. pnpm 10.x, Node ≥ LTS (24 present locally).
3. Cargo workspace root + `crates/monitor-core` (contract types first — ADR-0001) with `serde`.
4. `apps/desktop`: Vite 8 + React 19 + TypeScript (pin exact version after confirming Vite compatibility) + Tailwind 4 + shadcn CLI + zustand 5 + echarts 6.
5. `tauri` 2.12.x + `tauri-build`; capability files; strict CSP.
6. `sysinfo` 0.39 (pin `=0.39.6` initially), `rusqlite` 0.40 (`bundled`), `trash` 5.2 (Phase 04), `axum` 0.8 (Phase 08 only).
7. Test tooling: `cargo test` + vitest (evaluate at Phase 01) + eslint; E2E approach decided at Phase 01 (testing-qa.md §1 caution about WebDriver on macOS).
8. CI: GitHub Actions workflows per ADR-0005 (verify runner labels first).

**Phase 01 agreed target:** Tauri 2 + Rust workspace skeleton with `monitor-core` contract types, settings persistence (ADR-0002), typed IPC shell (ADR-0003), React app shell with theme tokens scaffold, and green fmt/clippy/test/build on macOS arm64 locally + Windows x64 & Linux x86_64 on CI. Exit gate: "build/test on all available native targets" with honest SUPPORTED_TARGETS.md status flips.

## Concise implementation plan (Phase 00 → 01 handoff)

- Freeze contract types in `monitor-core` (Availability, MetricUnit, MetricReading, SystemSnapshot, MetricCapability) + fixture catalog scaffolding (CPU-01, MEM-01, NET-01… as test data, not code promises).
- Platform crates compile on all three OSes from day one (cfg-gated), so CI catches divergence immediately; macOS adapter reuses the spike-proven sysinfo paths.
- Settings: single versioned schema in SQLite (app_settings), no secrets stored; IPC commands validate every argument in Rust.
- UI: layout skeleton + semantic tokens from design.md §2; no metric cards wired to fake data — cards render `temporarily_unavailable` until Phase 02 collectors exist.
- Do not implement sensors, tray, widgets, remote server or any reference-app visual imitation in Phase 01 (phase doc handoff line).

**Agent handoff:** "Phase 00 completed with research evidence. Start Phase 01; do not implement sensors or UI copies yet."
