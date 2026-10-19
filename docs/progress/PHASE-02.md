# Phase 02 — Real portable telemetry and Overview dashboard

Date: 19.10.2026
Commit: this commit
Feature IDs: FX-001, FX-002, FX-003, FX-004, FX-005, FX-006 (→ `in_progress`: implemented and validated on macOS arm64; Windows/Linux validation pending CI/hardware)
Status: complete (awaiting owner acceptance)

## Changes implemented (code map)

### 2A. Sampling core

- `crates/monitor-core/src/sampling.rs` — `RateTracker`/`RateOutcome`: deterministic interval math on monotonic inputs. Returns explicit `Warmup` / `NoElapsedTime` / `CounterReset` / `Rate(f64)`; invalid durations keep the baseline instead of dividing by zero. Unit tests include the phase-doc example (1000 B / 0.5 s = 2000 B/s), counter reset, and 32-bit wrap.
- `crates/monitor-core/src/reasons.rs` — canonical reason codes: `WARMUP`, `SLEEP_GAP`, `COUNTER_RESET`, `PERMISSION_DENIED`, `DRIVER_UNSUPPORTED`, `COLLECTOR_ERROR`, …
- `crates/monitor-core/src/collector.rs` — `PollGroup` (Fast/Medium/Slow) added to the `Collector` trait; the scheduler owns exactly one timer per group.
- `apps/desktop/src-tauri/src/scheduler.rs` — full engine rewrite: group scheduling (fast = settings-driven 1–60 s, medium 10 s, slow 60 s), keyed latest-value assembly into `SystemSnapshot` with strictly increasing sequence stamped onto every reading, **error isolation** (a failed collector's previously owned keys flip to explicit `error`/`COLLECTOR_ERROR`/null readings; healthy collectors unaffected; a never-succeeded collector contributes no rows at all), graceful cancellation, and per-collector `CollectorDiagnostic` (ticks, error count, last success UTC, work duration, bounded error text — no host identifiers) exposed via the new `get_diagnostics` command.
- Diagnostics avoid full host identifiers by design (security-privacy.md §2).

### 2B. Real collectors (`apps/desktop/src-tauri/src/collectors/`)

Portable sysinfo 0.39.6 baseline (ADR-0004 layering; lives in the app runtime until a second consumer exists — no speculative crate). Every reading carries `source` = collector id. API surface verified against the vendored crate source before use (`is_removable`, `ip_networks`, `MINIMUM_CPU_UPDATE_INTERVAL`, `physical_core_count`).

- `host.rs` (FX-001, slow): hostname, OS long version, kernel, arch, uptime, CPU brand, logical cores, physical cores (explicit `unsupported` where the OS gives none).
- `cpu.rs` (FX-002, fast): total + per-core usage (0–100 clamp), per-core frequency (Hz); **first tick emits `WARMUP` with null values** for all usage readings — never a fake 0.
- `memory.rs` (FX-003, medium): total/available/used (documented derivation `used = total − available`, sysinfo semantics) and swap total/used. Swap zeros are real measurements, not sentinels; OS pressure classifications deliberately deferred to native adapters (documented below).
- `disks.rs` (FX-004, slow): full re-enumeration per tick (mount/unmount handled by disappearance), per-volume entity `vol:<mount>`: capacity, available, filesystem, removable, read-only (logical-capacity semantics documented in capabilities).
- `network.rs` (FX-006, fast): stable entity keys `if:<name>:<generation>` surviving renames, cumulative rx/tx counters, **rates computed by `RateTracker` on our monotonic clock**, IPv4/IPv6 address lists (external IP stays OFF), disappeared interfaces pruned (no invented zeros), and **sleep detection**: wall-vs-monotonic skew > 5 s resets all trackers and the cycle reports `SLEEP_GAP`, not a bogus spike. A deterministic `FakeClock` test drives a 1 h wall jump with 0 monotonic movement.

### 2C. Overview UI (FX-005)

- `apps/desktop/src/lib/metrics.ts` — snapshot selectors + `MetricHistory` bounded ring buffers (400 samples ≈ 20 min) keyed `<metric>|<entity>`; non-available/missing readings push `null` gap markers.
- `stores/telemetryStore.ts` — history integration on every snapshot; stale/lastKnown semantics from Phase 01 unchanged.
- `components/metric/Sparkline.tsx` — gap-aware SVG sparkline: line breaks at null markers and at time distances > `maxGapMs`; charts never connect through gaps. (Full ECharts time-series component is deferred to Phase 03 where persistent history exists — documented decision.)
- `features/overview/cards.tsx` — CPU (total + sparkline + per-core bars), Memory (used/total + bar + swap), Storage (per-volume capacity bars with fs/removable/read-only badges), Network (interface selector with auto-fallback on unplug, rx/tx rates + sparklines, totals, addresses), Host (identity/uptime/cores).
- `OverviewPage.tsx` — composition driven by persisted settings (visibility + order), time-range control (1m/5m/15m bounded by ring-buffer span), honest waiting/error states.
- Settings v2 (FX-020 continuation): `overview_cards` layout (visibility + order, validated: known ids, no duplicates, non-empty) with **load-time JSON migration v1→v2** in the store (defaults appended, persisted once) — covered by store tests including a real v1 fixture row.

## Verification

| Environment | Command / scenario | Result | Evidence |
|---|---|---|---|
| macOS arm64 | `cargo fmt --all --check` | PASS | this run |
| macOS arm64 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS | this run |
| macOS arm64 | `cargo test --workspace` | PASS — **50 tests, 0 failed** (core sampling math incl. exact 2000 B/s, reset, wrap; serialization; settings v2 validation + v1→v2 migration; network sleep-gap; CPU warm-up→valid on real cores; memory/disks/network range checks on real machine; scheduler groups/merge/isolation/diagnostics/graceful stop; **live-engine test**: real collectors → snapshot with available CPU + memory within 1.6 s) | this run |
| macOS arm64 | `cargo test -p app --test cpu_crosscheck -- --ignored` | **PASS — controlled-load cross-check on native hardware**: warm-up null confirmed; under 2 spinning threads total usage rises above the quieter of two idle baselines (direction+range check, not a calibration) | this run; first run failed because the machine was still busy from the build itself → test now takes min of two idle samples |
| macOS arm64 | `pnpm lint` / `pnpm typecheck` | PASS | this run |
| macOS arm64 | `pnpm test` | PASS — **63 tests, 0 failed** (ring buffers bounded + gap markers, sparkline segment splitting incl. time-gap break, Overview real values/warm-up dash/hidden cards/order, network selector switch without data leakage, settings v2 guards) | this run |
| macOS arm64 | `pnpm build` + `cargo build -p app` (fresh dist embedded) | PASS | this run |
| macOS arm64 | Benchmark: app binary, 3 s polling, untouched window, ps samples at 15/60/120 s | **CPU ≈ 0% (ps cumulative average), RSS 133.9 → 109.4 → 110.0 MB** — within the <150 MB / <2% targets; measured on debug build (release expected lower) | figures above; logged here per phase doc §5 |
| Windows x64 | any build/test | NOT_RUN — no hardware/runner; portable code is cfg-free and compiles in CI matrix (authored, not yet executed) | R10 |
| Linux x64 | any build/test | NOT_RUN — no hardware/runner | R10 |

Native API/library verification (19.10.2026, vendored sysinfo 0.39.6 source): `System::{host_name,long_os_version,kernel_version,cpu_arch,uptime,physical_core_count,global_cpu_usage,refresh_cpu_all}`, `Cpu::{cpu_usage,brand,frequency}`, `Disk::{mount_point,total_space,available_space,file_system,is_removable,is_read_only}`, `NetworkData::{total_received,total_transmitted,ip_networks}`, `MINIMUM_CPU_UPDATE_INTERVAL` (200 ms on Apple). Rates deliberately use our own monotonic clock + `RateTracker`, not sysinfo's internal deltas, to keep reset/sleep semantics contract-controlled.

## Platform capability findings

| Metric | macOS 27.0.1 / M3 Max | Notes |
|---|---|---|
| Host identity/uptime | available (proven) | physical core count may be `unsupported` on other hosts — explicit state implemented |
| CPU total/per-core/frequency | available (proven) | first sample WARMUP enforced; cross-check passed under controlled load |
| Memory total/available/used/swap | available (proven) | OS pressure classification **deferred** to native adapters (Phase 05) — not invented |
| Volumes incl. external/removable | available (proven) | logical capacity semantics labeled; I/O counters stay Phase 04 (R12) |
| Network counters/rates/addresses | available (proven) | MAC addresses intentionally not exposed; external IP stays OFF |

## Security and data integrity

- Rates only from monotonic deltas with explicit reset/gap states; no fabricated values anywhere (enforced by invariant tests + guards).
- Network entity keys are name+generation; MAC addresses and external lookups are not collected; addresses render locally only (no remote surface exists).
- Diagnostics contain no host identifiers and bound error text; `get_diagnostics` is read-only.
- Settings v2 migration is checksummed at the runner level and covered by a fixture test; validation rejects unknown card ids, duplicates, and empty layouts.
- No mock providers in release paths; UI tests mock only the IPC module.

## Blockers / untested

- **Windows / Linux:** portable collectors compile platform-neutrally and their logic is OS-independent, but **no reading was taken on those OSes** — the CI matrix must run green first, and per-OS quirks (e.g. Linux `MemAvailable` semantics, Windows PDH choices per ADR-0004) are validated then. SUPPORTED_TARGETS.md stays honest.
- **Memory pressure / OS-specific classifications:** deferred to platform adapters (Phase 05) rather than inventing cross-OS equivalence; not a blocker for FX-003's P0 scope (used/available/swap).
- **ECharts:** not yet adopted — sparklines are contract-correct SVG with gap handling; the full time-series component arrives with persistent history (Phase 03) where zoom/brush requirements exist. No ADR required (component-level choice, documented here).
- **Benchmark caveats:** debug binary, 2-minute window, window visible but untouched. The 4-hour soak and closed-window variant belong to Phase 09's performance gate; figures are recorded, not guaranteed.
- Live snapshot content was verified headlessly via the live-engine test; pixel-level dashboard review is the owner's manual step.

## Feature matrix edits

FX-001…FX-006: `planned` → **`in_progress`** — macOS-arm64-validated collectors with unit, integration, and cross-check evidence (above); Windows/Linux rows stay open until CI/native runs. FX-020 remains `in_progress` (settings v2 + layout persistence added this phase).

## Gate review (phase doc §5)

- [x] Live readings verified on each available native OS (macOS arm64); untested OS clearly listed (Windows/Linux NOT_RUN).
- [x] First CPU sample warm-up state; second sample sensible percent (unit test + cross-check).
- [x] Rate math on monotonic elapsed; reset/unplug/sleep handled (RateTracker tests; interface prune; SLEEP_GAP deterministic test).
- [x] Charts never connect through gaps (sparkline segment tests); adapter failure cannot crash (error-isolation test, loop keeps running).
- [x] Dashboard reconfigures and retains settings (card layout persisted via settings v2 migration); no per-card polling (single scheduler, one timer per group).
- [x] Benchmark and test outcomes captured (this file).

**Handoff:** "Phase 02 completed with real, tested telemetry on macOS arm64. Phase 03 (processes + history) may start after owner acceptance; CI baseline for Windows/Linux still lands with the first push."
