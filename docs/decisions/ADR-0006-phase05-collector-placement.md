# ADR-0006: Phase 05 collector placement, capability-report ownership and probe rules

Status: accepted
Date: 19.10.2026
Owner: principal engineer (Phase 05 agent)
Related features: FX-010, FX-012, FX-013, FX-014, FX-021
Related: ADR-0004 (native sources), ADR-0001 (telemetry contract)

## Context

Phase 05 adds four hardware-conditional collectors (sensors, battery, Bluetooth, GPU) whose implementations differ per OS, plus a user-facing capability report (FX-021) that must name, for every metric family, what this host can deliver and why. ADR-0004 anticipated the platform crates would host the OS-specific collectors; Phase 04 already deviated from that for the Linux diskstats collector (app-runtime cfg-split, unit-testable parsers).

Phase 05 also needs two process-probe surfaces (`system_profiler` on macOS, `upower` on Linux) and, on macOS, two IOKit C functions that the `objc2-io-kit` 0.3.2 bindings do not export (`IOServiceGetMatchingService` and `CFDictionaryGetValue`). The security pipeline rejects any process spawn whose program or arguments are not compile-time literals.

## Decision

1. **Reading collectors live in the app runtime crate** (`apps/desktop/src-tauri/src/collectors/`), cfg-split per OS with pure parsing separated from platform I/O — the Phase 04 pattern, now the rule (amends ADR-0004's placement expectation). Rationale: the parsers are fixture-testable on every OS and CI runner, the scheduler registration is one place, and platform crates keep a sharply defined role. This is recorded as an amendment note in this ADR rather than silently diverging again.
2. **Platform crates own (a) cross-compile guards and (b) on-demand metadata providers** — not polled metrics. `monitor-platform-macos::gpu_adapters`, `monitor-platform-windows::gpu_adapters` (DXGI), `monitor-platform-linux::gpu_adapters` (DRM sysfs) serve static adapter/display identity to the `get_gpu_info` command. Natively they emit **zero** capability rows; every metric family is owned by exactly one app-runtime collector whose capability row reflects the *probed* host state. Cross-compiled, they emit explicit `WRONG_PLATFORM` rows that the report builder filters — CI catches divergence without ever letting a static voice contradict a probed one.
3. **Capability report:** `build_capability_report` merges all collectors' `capabilities()`, drops `WRONG_PLATFORM`, and deduplicates by `metric_id` (first wins). A workspace test asserts every family in `ALL_FAMILIES` appears exactly once and every `unsupported` row carries an explanation. `reprobe_capabilities` rebuilds it on demand and bumps the capabilities revision the scheduler stamps on snapshots.
4. **Process probes are literal-only.** Each probe names a fixed program and fixed arguments at the spawn site (`system_profiler SPBluetoothDataType -json`, `upower --dump`, `system_profiler SPDisplaysDataType -json`); device-derived strings are parsed from output only, never passed to a process. Bluetooth entities are `bt:<sha256(address)[..16]>` so MAC addresses never enter history or any remote surface.
5. **macOS battery access** uses the documented public IOPowerSources API (`IOPSGetTimeRemainingEstimate` — whose −1/−2 sentinels replace raw `TimeRemaining = 65535` registry values) plus the `AppleSmartBattery` registry entry read through the crate's bindings; the two missing C functions are declared locally with their IOKitLib.h/CoreFoundation prototypes and SAFETY comments. No private API, no helper, no root.

## Consequences

- Windows DXGI battery/GPU code paths compile only on the Windows CI runner; runtime hardware validation remains gated on the first green CI push plus real hardware (tracked in `docs/progress/PHASE-05.md` and `CI-BASELINE.md`).
- `parse_upower_dump` tolerates BlueZ paths on either the `Device:` line or `native-path:` line; devices without a battery percentage emit no charge reading.
- Future collectors must follow the same shape: pure parser + thin cfg I/O + truthful capability rows; probes stay literal-only.
