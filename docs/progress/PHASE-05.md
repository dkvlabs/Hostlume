# Phase 05 — Hardware-dependent GPU, temperature, fans, battery, Bluetooth + capability report

Date: 19.10.2026 · Machine: MacBook Pro (Mac15,11), Apple M3 Max, 14 logical cores, 36 GiB RAM · OS: macOS 27.0.1 arm64 · Toolchain: rustc/cargo 1.99.1 (Homebrew), node 24.22.0, pnpm 10.33.2

## PHASE

PHASE: 05 — GPU, sensors & power (FX-010, FX-012, FX-013, FX-014, FX-021)

## IMPLEMENTED FX IDS

| FX ID | Feature | Status on macOS (this machine) | Status elsewhere |
|---|---|---|---|
| FX-010 | GPU utilization, memory, displays | Adapter/display metadata **implemented + validated** (`get_gpu_info`, Apple M3 Max, unified memory, no fabricated VRAM); live GPU% **platform_limited**: no public non-root API (powermetrics is root-gated — rejected by the no-root rule) | Windows: DXGI adapter/display provider implemented, compile-gated on CI, runtime untested; live counters `unsupported` (`NOT_VALIDATED_ON_HARDWARE`). Linux: DRM sysfs live counters implemented (`gpu_busy_percent`, `mem_info_vram_used`), runtime validation deferred to CI/hardware |
| FX-012 | Temperatures and fan sensors | Temperatures **implemented + validated** (21 reporting sensors live, IOKit HID via sysinfo). Fan RPM **platform_limited**: IOKit HID path exposes none and root tooling rejected — explicit `unsupported`/`NO_PUBLIC_READABLE_SENSOR`, never 0 RPM | Linux: sysinfo hwmon temps + direct `fan*_input` RPM reads implemented (fixture-tested; CI/hardware pending). Windows: both families `unsupported` with documented reason (standard accounts cannot read sensors without third-party kernel drivers) |
| FX-013 | Battery cycles/health/charge/time | **Implemented + validated live**: charge %, charging/full state, cycle count, health = full-charge/design (85.08% — matches `ioreg` ground truth 7298/8579), signed power draw, time estimate via `IOPSGetTimeRemainingEstimate` (unknown/unlimited preserved, 65535 sentinel never surfaced) | Linux: `/sys/class/power_supply` implemented (fixture-tested, CI pending). Windows: `GetSystemPowerStatus` charge/status/time implemented (compile-gated); cycles/health/draw `unsupported` (`NOT_EXPOSED_BY_OS_API`) |
| FX-014 | Bluetooth accessory battery | Enumeration **implemented** via `system_profiler SPBluetoothDataType -json` (public, rootless, ~0.2 s); charge parsed per component (`device_batteryLevel*`); no accessory with a battery was connected during the smoke run, so live charge values are **untested on this machine** — parser fixture-tested | Linux: `upower --dump` implemented (fixture-tested; CI pending). Windows: not implemented this build (`COLLECTOR_NOT_IMPLEMENTED`) |
| FX-021 | User-facing capability diagnostics | **Implemented + validated**: merged per-family report (every `ALL_FAMILIES` id exactly once, tested), real probe states with source + reason + explanation, Sensors → Capabilities tab with re-probe (`reprobe_capabilities`, bumps revision) | Same report shape on all OSes; rows reflect each host's probed truth |

## MODIFIED / ADDED FILES

- `crates/monitor-core/src/`: `families.rs` (+battery.status/health/time_remaining/power_draw, bluetooth.device.name), `reasons.rs` (+10 hardware-conditional codes), `gpu.rs` (new: GpuAdapter/GpuDisplay/GpuInfoReport), `lib.rs`
- `crates/monitor-platform-macos/src/`: `lib.rs` (Phase 05 role), `gpu_adapters.rs` (new, live-probed)
- `crates/monitor-platform-windows/src/`: `lib.rs`, `gpu_adapters.rs` (DXGI; pure normalization cfg-free), `Cargo.toml` (windows 0.62.2, target-gated)
- `crates/monitor-platform-linux/src/`: `lib.rs`, `gpu_adapters.rs` (DRM sysfs)
- `apps/desktop/src-tauri/src/collectors/`: `sensors.rs`, `battery.rs`, `bluetooth.rs`, `gpu.rs` (all new), `mod.rs`
- `apps/desktop/src-tauri/src/`: `lib.rs` (merged capability report + report-invariant test + registration), `scheduler.rs` (4 new collectors), `commands.rs` (`reprobe_capabilities`, `get_gpu_info`), `state.rs` (`Mutex<CapabilityReport>` + revision), `settings.rs` (v5: `TemperatureUnit`, 3 new overview cards, `SettingsV4` + `into_v5`), `settings_store.rs` (full 1..=4 → 5 routing; **fixes a Phase 04 latent bug**: stored v3 rows previously hit the `Err` arm)
- `apps/desktop/src-tauri/Cargo.toml` (target-gated objc2-io-kit/objc2-core-foundation/libc; windows)
- Frontend: `lib/ipc/{types,guards,client}.ts`, `lib/metrics.ts`, `features/sensors/SensorsPage.tsx` + test (new), `features/overview/{cards.tsx,OverviewPage.tsx}` (+Gpu/Sensors/Battery cards), `features/settings/SettingsPage.tsx` (temperature unit), `app/App.tsx` (sensors route), `app/App.test.tsx`, `lib/i18n/dictionaries.ts` (en+ro), store/guard test fixtures

## TESTS RUN (exact)

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `cargo test --workspace` | PASS — 110 passed, 0 failed, 1 ignored (CPU cross-check) |
| `pnpm --dir apps/desktop lint` | PASS |
| `pnpm --dir apps/desktop typecheck` | PASS |
| `pnpm --dir apps/desktop test` | PASS — 78 passed, 0 failed (13 files) |
| `pnpm --dir apps/desktop build` | PASS (dist built) |

New test coverage includes: sensor-id slugification/dedup stability, live macOS sensor enumeration + physical-range temperature assertions, battery health math and guards, live macOS battery registry values (plausibility-banded), Linux power_supply sysfs fixture (ThinkPad-style µAh values), AC-adapter-is-not-a-battery, cycle-count-missing → unsupported-not-zero, Bluetooth profiler fixture (per-component charge + no-characteristic → no reading), upower dump fixture (BlueZ path on `Device:` and `native-path:` lines), MAC-address gate, hashed entity ids, DXGI UTF-16/adapter normalization fixtures, DRM mode-line parsing, GPU busy-percent clamping, merged capability report covering every family exactly once with explained unsupported rows, settings v4→v5 card append + unit default, SensorsPage UI (6 tests).

## NATIVE OS TESTS (macOS 27.0.1, Apple M3 Max — real hardware)

- Live app smoke (dev binary, 22 s and 80 s runs + SIGTERM, clean exit both times):
  - `sensor.temperature`: 21 distinct sensors persisted, 60.9–64.2 °C (machine under compile load — plausible; values match Phase 00 spike source).
  - `battery.charge` = 100.0, `battery.cycles` = 180, `battery.health` = 85.08 — cycles/health byte-consistent with the Phase 00 `ioreg -rn AppleSmartBattery` ground truth (CycleCount=180; 7298/8579 mAh). `battery.power_draw` = 0 W on AC (InstantAmperage=0 — correct, not fabricated).
  - `gpu.*` / `bluetooth.*` rows in history: **0** (unsupported families emit nothing — verified absence).
- Settings v5 live migration: seeded a real v4 row (polling 9 s, locale `ro`, cpu card hidden), ran the app — row migrated to v5 preserving user prefs, appending gpu/sensors/battery cards visible, `temperature_unit=celsius`.
- `cargo test -p monitor-platform-macos` includes a live GPU metadata probe (returns the M3 Max adapter + display, unified-memory honest).

## UNTESTED OS / HARDWARE

- **Windows (all features):** no hardware/runner here. Windows-specific code (`GetSystemPowerStatus` battery backend, DXGI gpu_adapters) is compile-gated to the `windows` CI job; API signatures verified against the fetched windows 0.62.2 source (`CreateDXGIFactory1<T>`, `EnumAdapters1`, `GetDesc1`, `EnumOutputs`, `GetDesc`, `DXGI_ADAPTER_DESC1`, `DXGI_OUTPUT_DESC` incl. its `Win32_Graphics_Gdi`/`_Common` feature requirement). First green CI push is the compile gate; runtime validation needs real Windows hardware.
- **Linux (all features):** fan/temperature/battery/GPU/Bluetooth collectors are fixture-tested on this machine where parsing is pure; the cfg(linux) I/O paths and live hardware behavior run only on the Ubuntu CI job (no desktop session / sensors on runners) and remain unvalidated on real desktop hardware.
- **Bluetooth charge on this Mac:** no battery-reporting accessory connected during the smoke run — parser fixture-tested only.
- **Intel Mac battery/temperatures**, multi-battery hosts, NVIDIA/AMD discrete GPUs, Windows laptop/desktop battery pairs: untested (recorded for Phase 09 hardware matrix).

## SECURITY CHECK

- No new network code; everything works offline. No remote routes touched (FX-019 unchanged).
- Process spawns: three fixed-literal probes (`system_profiler` ×2, `upower`), argument lists are compile-time constants — no device-derived string ever reaches a process (enforced by the repo's security hook and asserted by test). No shell invocation anywhere.
- Bluetooth privacy: device MAC addresses are hashed into entity ids (`bt:<sha256[..16]>`); raw addresses never enter snapshots, history, or logs. Device names are user-visible local UI content only.
- No privileged helper, no root path, no fan-control or write endpoint (fans explicitly read-only in UI copy).
- Trash/scan/process-action guards from earlier phases untouched.
- Post-commit deep security scan (Mimosa): re-run after commit — see commit note below.

## BLOCKERS

None blocking the phase. Standing platform limits (documented, not blockers): macOS GPU% and fan RPM have no public non-root source (powermetrics root-gating, IOKit HID exposes temperatures only); Windows sensors need third-party kernel drivers (rejected by policy); Windows Bluetooth battery needs a WinRT surface not yet wired.

## NEXT PHASE READINESS

**Next phase: 06 — menu bar/tray and widgets (FX-015, FX-016), ready pending owner acceptance of Phase 05.**
