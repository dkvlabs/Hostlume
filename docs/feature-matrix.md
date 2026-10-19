# Feature matrix — single source of truth for scope/status

**Status convention:** `planned` → `in_progress` → `implemented` → `verified`; or `platform_limited` with platform/hardware explanation. **Initial status: all planned.** Never silently mark as verified. `Parity` means publicly described reference functionality, not copied internals/UI.

| FX ID | Feature | Reference? | Priority | Owning phase | Initial status | Key acceptance evidence |
|---|---|---|---|---|---|---|
| FX-001 | Device identity, uptime and hardware summary | Yes | P0 | 02 | in_progress | Native values + unavailable states |
| FX-002 | CPU overall and per-core history | Yes | P0 | 02 | in_progress | Two samples, rates and per-core tests |
| FX-003 | Memory/pressure/swap | Yes | P0 | 02 | in_progress | OS-specific categories and chart |
| FX-004 | Disks/mounts/capacity/external drives | Yes | P0 | 02 | in_progress | Add/remove devices; eject guarded |
| FX-005 | Configurable overview dashboard | Yes | P0 | 02 | in_progress | Real snapshot subscription; responsive |
| FX-006 | Network interfaces, addresses and speeds | Yes | P0 | 02 | in_progress | Per-adapter bytes/sec, resets/gaps |
| FX-007 | Applications/process history and quit | Yes | P0 | 03 | in_progress | PID+start-time validation, permission handling |
| FX-008 | Disk visualizer, large files, Move to Trash | Yes | P0 | 04 | in_progress | Cancel, safe symlinks, native Trash |
| FX-009 | Disk I/O read/write counters and rates | Yes | P1 | 04 | in_progress | Real adapter-based source + test |
| FX-010 | GPU utilization, memory, displays | Yes | P1 | 05 | in_progress | macOS adapter/display metadata validated live; GPU% platform_limited (no public non-root source); Windows DXGI compile-gated; Linux DRM sysfs fixture-tested |
| FX-011 | Advanced network history/map/IP info | Yes | P1 | 04 | in_progress | Interface switching; external lookup opt-in |
| FX-012 | Temperatures and fan sensors | Yes | P1 | 05 | in_progress | 21 live sensors on M3 Max; fan RPM platform_limited with reasoned unsupported; Linux hwmon fixture-tested; Windows documented unsupported |
| FX-013 | Battery cycles/health/charge/time | Yes | P1 | 05 | in_progress | Live macOS validation (cycles 180, health 85.08% vs ioreg ground truth); Linux sysfs fixture-tested; Windows GetSystemPowerStatus compile-gated, cycles/health unsupported |
| FX-014 | Bluetooth accessory battery | Yes | P1 | 05 | in_progress | macOS system_profiler probe (literal args, hashed ids); parser fixture-tested; live charge needs an accessory; Windows not implemented yet |
| FX-015 | Menu bar / system tray monitors | Yes | P1 | 06 | in_progress | macOS tray live-validated (icon + live CPU title + close-to-tray + pause/resume); pause/resume scheduler-tested; Windows/Linux compile-gated |
| FX-016 | Widgets / native mini-dashboard | Yes (macOS) | P1 | 06 | platform_limited | WidgetKit explicitly gated (no signing identity — no fake widget); compact floating monitor fallback implemented on the shared snapshot bus |
| FX-017 | Notifications and local opt-in actions | Yes | P1 | 07 | implemented | Deterministic hold/hysteresis/cooldown/restart engine (139 Rust tests); live macOS smoke: real-metric fire (CPU 79.9%) + audit episode + restart no-double-fire + UNUserNotificationCenter prompt; actions = no-args approved launch, double-gated default-off, audited (ADR-0007); Windows/Linux compile-gated |
| FX-018 | Comparison, recording, CSV export | Yes | P1 | 08 | implemented | Compare screen: ≤4 aligned ECharts series, unit axes, table view, range selection (unit-tested alignment machinery via bounded history queries); recording sessions with crash recovery (interrupted status) + CSV export (documented header, RFC quoting, formula escape, paged streaming, row ceiling); CSV import intentionally not implemented |
| FX-019 | Mobile web monitor / read-only REST API | Yes | P1 | 08 | implemented (LAN platform_limited) | Loopback server live-validated: OFF→no listener (lsof), token auth 401 negatives, POST→405, kill/trash→404, private projection (no ipv4/hostname), bounded history, SSE cap, session cookie, clean exit frees port; LAN fails closed until TLS (ADR-0008) — second-device test deferred with it |
| FX-020 | Preferences, themes, retention, i18n | Yes (subset) | P0 | 01/09 | in_progress | Persistent schema migrations, UI states |
| FX-021 | User-facing capability diagnostics | Enhancement | P1 | 05 | in_progress | Merged per-family report (exactly-once test), reasoned unsupported rows, Sensors→Capabilities tab with re-probe |
| FX-022 | Multi-computer monitoring | Enhancement | P3 | 10 (optional) | planned | Multi-agent secure pairing specification |

## Cross-cutting work (not standalone feature rows)

- **Phase 00:** reference audit; OS support research; license and entitlement checks; risk register.
- **Phase 01:** app shell, Rust workspace, platform traits, telemetry envelope, settings + IPC, tests.
- **Phase 03:** SQLite history, aggregation and retention; process identities.
- **Phase 08:** endpoint schemas and remote threat model.
- **Phase 09:** regression, accessibility, distro/machine validation, installers, crash handling, documentation.

## Evidence format

For each status change append an evidence record in `docs/progress/PHASE-XX.md`:

```md
| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
| FX-002 | macOS / Apple Silicon | <sha> | real collector + unit | PASS | none | <path> |
```

Use a real commit and real log reference only when available. A verification performed on Windows does **not** verify Linux.

## Phase 05 evidence records (19.10.2026)

| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
|---|---|---|---|---|---|---|
| FX-012 | macOS / Apple M3 Max | <phase-05 sha> | live app smoke (80 s) | PASS — 21 sensors, 60.9–64.2 °C | fan RPM unsupported (no public non-root source) | docs/progress/PHASE-05.md, `collectors/sensors.rs` tests |
| FX-013 | macOS / Apple M3 Max | <phase-05 sha> | live app smoke vs ioreg | PASS — cycles 180, health 85.08% (7298/8579 mAh) | time estimate unknown/unlimited preserved (on AC) | docs/progress/PHASE-05.md, `collectors/battery.rs` tests |
| FX-010 | macOS / Apple M3 Max | <phase-05 sha> | live probe test | PASS — adapter+display metadata, unified-memory honest | GPU% platform_limited | `monitor-platform-macos` lib test |
| FX-010 | Windows | — | compile verification pending CI | NOT RUN (no hardware) | DXGI runtime unvalidated | `monitor-platform-windows/src/gpu_adapters.rs` |
| FX-013 | Windows | — | compile verification pending CI | NOT RUN (no hardware) | runtime unvalidated | `collectors/battery.rs` windows backend |
| FX-012/013/014 | Linux | — | fixture tests only | PASS (parsers) | live hardware pending CI/desktop validation | cfg(linux) modules in `collectors/` |
| FX-021 | macOS / Apple M3 Max | <phase-05 sha> | unit + live | PASS — every family exactly once, reasoned rows | — | `apps/desktop/src-tauri/src/lib.rs` test |

## Phase 06 evidence records (19.10.2026)

| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
|---|---|---|---|---|---|---|
| FX-015 | macOS / Apple M3 Max | <phase-06 sha> | live smoke (System Events + screenshots) | PASS — status item at (1324,7) 61×24; live title 59%→58%; close-to-tray hides window, process alive | tray menu item click-through needs human (muda popup not AX-reachable) | docs/progress/PHASE-06.md |
| FX-015 | macOS / Apple M3 Max | <phase-06 sha> | scheduler unit test | PASS — pause stops publishing; resume resets baselines exactly once | — | `scheduler::tests::pause_stops_publishing_and_resume_resets_baselines` |
| FX-016 | macOS / Apple M3 Max | <phase-06 sha> | gate review | GATED — WidgetKit needs signed extension; compact floating monitor implemented as fallback | no native widget | docs/decisions/, docs/progress/PHASE-06.md |
| FX-015 | Windows | — | compile verification pending CI | NOT RUN (no hardware) | Explorer restart/DPI/overflow untested | `tray.rs` (cross-platform Tauri tray) |
| FX-015 | Linux | — | compile verification pending CI | NOT RUN (no desktop session) | GNOME-no-tray fallback untested | `tray.rs` + `toggle_compact_window` |

## Phase 07 evidence records (19.10.2026)

| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
|---|---|---|---|---|---|---|
| FX-017 | macOS / Apple M3 Max | <phase-07 sha> | live app smoke (seeded rules) | PASS — real-metric fire (CPU 79.9%), `alert_events` episode persisted without action config; restart did NOT re-fire (cooldown restored); clean SIGTERM exit | dev-mode notifications attribute to Terminal (bundled builds use app id) | docs/progress/PHASE-07.md |
| FX-017 | macOS / Apple M3 Max | <phase-07 sha> | deterministic engine tests (virtual time) | PASS — hold/hysteresis/cooldown/sleep-gap/missing-sample/restart scenarios | — | `crates/monitor-alerts/src/engine.rs` tests |
| FX-017 | macOS / Apple M3 Max | <phase-07 sha> | action security tests | PASS — both gates required, stored config revalidated, no-args literal-launcher spawn, hard timeout kills runaway child | argument arrays deferred (security tooling; ADR-0007) | `apps/desktop/src-tauri/src/actions.rs` tests |
| FX-017 | macOS / Apple M3 Max | <phase-07 sha> | UI tests | PASS — 97 frontend tests (rule editor gates, fixture probe, honest notification state) | — | `src/features/alerts/*.test.tsx` |
| FX-017 | Windows / Linux | — | compile verification pending CI | NOT RUN (cross targets not installed) | notifications + `unsupported_on_platform` action refusal unvalidated at runtime | `alerts.rs` (platform branches) |

## Phase 08 evidence records (19.10.2026)

| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
|---|---|---|---|---|---|---|
| FX-019 | macOS / Apple M3 Max | <phase-08 sha> | live smoke over loopback HTTP | PASS — OFF: no listener (lsof); seeded opt-in: 127.0.0.1:45454 LISTEN; no/bad token → 401; POST snapshot → 405; kill/trash routes → 404; snapshot payload contains no ipv4/hostname; history bounded (oversized range 400); SSE event captured; dashboard served; session cookie HttpOnly+SameSite=Strict; SIGTERM freed port | loopback only — LAN refused by design | docs/progress/PHASE-08.md |
| FX-019 | monitor-remote crate | <phase-08 sha> | integration tests (real HTTP) | PASS — 8 tests: OFF/LAN fail-closed, token negatives, read-only methods, history bounds, session exchange, rotation invalidates token+sessions, SSE 503 at cap, flood → 429 | — | `crates/monitor-remote/tests/integration.rs` |
| FX-018 | monitor-history crate | <phase-08 sha> | unit tests | PASS — session lifecycle/interrupt recovery/bounded lists; CSV paging cursors, RFC quoting + formula escape | UI-path export not manually clicked (commands unit-covered) | `crates/monitor-history/src/{sessions,export}.rs` tests |
| FX-018 | frontend | <phase-08 sha> | vitest | PASS — 97 tests (guards for new payloads, settings fixtures) | Compare chart interaction manual pass pending | `src/features/compare/ComparePage.tsx` |
| FX-018/019 | Windows / Linux | — | compile verification pending CI | NOT RUN (cross targets not installed) | runtime unvalidated | `crates/monitor-remote` (pure library, no platform gates) |
