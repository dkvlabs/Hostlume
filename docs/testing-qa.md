# Verification strategy, hardware QA and performance benchmarks

## Testing philosophy

**Differentiate three levels:** (1) pure logic tests with fixtures, (2) OS collector integration tests, (3) end-to-end desktop and packaging tests on real hardware. Mock screenshots do **not** establish monitoring behavior. A feature may be `implemented` on one OS and `platform_limited` on another.

## 1. Test categories and tools

| Layer | Tests | Required |
|---|---|---|
| Rust core | CPU/rate math, units, availability, scheduling, capabilities | All platforms CI |
| Platform adapters | Real metric probes, privilege paths, device change, process identity | Native OS CI + manual hardware |
| History | migrations, retention, rollup, DST, CSV, corrupt DB recovery | CI |
| Alerts | hold interval, threshold crossing, hysteresis, cooldown, restart | CI |
| Storage | symlink/reparse, hardlinks, mount boundaries, cancellation, Trash safeguards | CI + manual |
| HTTP server | default off, auth, TLS, data minimization, overload, CORS | CI + LAN security test |
| UI component | keyboard/a11y, missing values, chart gaps, virtual tables | Unit + browser DOM tests |
| Desktop E2E | boot, permissions, tray, close-to-tray, sleep/resume, installer | Native OS test machines |
| Performance | app memory/CPU overhead, DB growth, 10k processes, scan throughput | Named reference hardware |

Use `cargo test`, `cargo clippy`, TypeScript typecheck, ESLint, Vitest, Playwright or a verified desktop E2E approach compatible with current Tauri 2 targets. Select maintained tooling at phase 00/01; do not assume a specific WebDriver integration exists on macOS without testing.

## 2. Deterministic test fixture catalog

- **CPU-01:** baseline sample missing then valid per-core load; CPU count changes; process 160% core-equivalent on 4 cores.
- **NET-01:** byte counters grow with known monotonic seconds; 32-bit wrap; reset to 0; interface rename; VPN overlap.
- **DISK-01:** hot-unplug; file system reports 0 free; sparse file, hardlink and symlink loop; inaccessible directory; cancel scan.
- **SLEEP-01:** UTC wall clock jumps 1 hour while monotonic interval resets; no spurious negative rate.
- **SENSOR-01:** exposed `55_000` millidegree Celsius Linux hwmon -> `55°C`; absent fan does not mean 0 RPM.
- **MEM-01:** Apple categories vs Linux `MemAvailable`; no double-counted RAM.
- **BAT-01:** no battery; present with no cycle count; 100%-charged; charging estimate unavailable.
- **BT-01:** connected headset without battery characteristic; presence does not imply charge reading.
- **PROCESS-01:** same PID reused with different start time after user confirmation; action rejected.
- **ALERT-01:** 90% CPU threshold for 60 s, 10% hysteresis, 5m cooldown, process restarts and interrupted sampling.
- **HIST-01:** DST crossing, unavailable samples, 24h raw rollup and CSV round-trip.
- **REM-01:** remote server initially off; bad token; LAN no TLS; huge history query rejected.

Use synthetic fixtures only in tests and clearly labeled developer preview; production measurement paths use real collectors exclusively.

## 3. Hardware and OS matrix

| Target | Minimum validation | Extended validation |
|---|---|---|
| macOS Apple Silicon | one recent Mac laptop or desktop, Tauri native build, process + network + tray | second generation, sensor helper and WidgetKit extension |
| macOS Intel | one Intel Mac if supported by release | laptop battery + thermal sensors |
| Windows x64 | Windows 11, integrated GPU, protected processes, MSI/EXE | Windows 10 supported target, NVIDIA/AMD dGPU, multiple displays |
| Linux GNOME | Ubuntu LTS with Wayland, no tray extension and with extension if relevant | X11 and hardware sensors |
| Linux KDE | current supported KDE distro with Wayland, RPM/DEB/AppImage as applicable | NVIDIA/AMD/Intel GPU drivers |
| Laptop vs desktop | at least one battery-present and one battery-absent host | different Bluetooth peripherals |

Document CPU model, OS version, driver, display server, test date and each discovered capability. **Native hardware** validation of sensors cannot be replaced with a VM.

## 4. Real-metric cross-check procedure

1. Select a stable workload and sampling period (3 s default); log actual intervals.
2. Compare total CPU and per-core usage with the OS monitor within a **documented tolerance** using synchronized windows, not instantaneous visual guesses.
3. Verify memory against platform-specific counters, checking the agreed semantics.
4. Generate controlled network traffic and check receive/send deltas on the named interface. Test Wi-Fi → Ethernet and VPN attach/detach.
5. Generate disk writes on a temporary volume and verify same-device I/O readings without conflating filesystem cache.
6. For available GPU/sensors/battery, corroborate against trusted native/system/vendor tool for that specific device; if source unavailable mark it as limitation.
7. Log metric freshness, drop counts and collector CPU time; ensure no growth over a 4h soak test.

## 5. Performance test profile

**Proposed targets** (measured, not guaranteed): average process CPU <2% and resident memory <150 MB in steady monitoring on stated reference hardware at 3 s sampling; dashboard rendering remains fluid with a typical graph set. Create tests for:

- Idle/background mode for 30m and 4h, with and without open UI.
- Starting/stopping recordings and reconnecting remote web client.
- ~100–1000 running processes where available; 10k-row **synthetic table performance**, not a claim 10k real OS processes were run.
- Deep directory scan (millions of files if dedicated test dataset), cancel at mid-scan, avoid UI freezes.
- 1M historical points in fixture database, bounded history queries, retention and WAL checkpoint.
- Sleep/resume and screen lock with both tray and dashboard hidden.

Measure CPU, RSS, GPU if relevant, disk writes, event queue backlog, dropped events, DB size and time-to-first-real-sample. Save summary under `docs/progress/PERF-<date>.md`.

## 6. UI/accessibility tests

- Dark/light/system changes without relaunch; correct values and color contrast.
- 200% zoom, keyboard navigation, accessible dialog focus, table virtualization.
- Accessibility of `unsupported`/`permission_required`/`stale` with screen reader.
- Tooltips do not rely solely on hover; charts also accessible in data-table form.
- Localization: long translations, Romanian number formatting, pluralization and bytes units.
- Minimized/hidden and foreground app lifecycle.
- Device unplug mid-detail-view does not crash.
- Popover opens predictably from tray and remembers layout settings.

## 7. Release block conditions

Block a release for any of:

- Fake readings in production or platform feature secretly unavailable.
- Unsafe Trash/force-quit workflow or missed PID identity guard.
- Remote API available without user opt-in/auth/TLS controls.
- Unbounded memory/CPU/disk growth in normal monitoring.
- Crash/data corruption during sleep/resume, shutdown or device removal.
- Target-platform installer not actually built and manually launched.
- License violation or unreviewed privileged hardware helper.

## 8. Required evidence template

```md
# Validation: <target device> — <date>
Build SHA: <sha>
OS and version: <OS>
Hardware and driver: <device>
Architecture: <arm64/x64>
Tests run: <commands and outputs>
Manual scenarios: <case/result>
Sensors available: <names and data origins>
Metrics unavailable: <reason codes>
CPU/RSS benchmark: <tool/method/results>
Crashes or open defects: <links or descriptions>
Sign-off: <tester / status>
```
