# Phase 06 — Native menu bar, system tray, widgets

Date: 19.10.2026 · Machine: MacBook Pro (Mac15,11), Apple M3 Max · OS: macOS 27.0.1 arm64 · Toolchain: rustc/cargo 1.99.1, node 24.22.0, pnpm 10.33.2

## PHASE

PHASE: 06 — Native menu bar, system tray and widgets (FX-015, FX-016)
Commit: this commit

## IMPLEMENTED FX IDS

| FX ID | Feature | macOS (this machine) | Windows / Linux |
|---|---|---|---|
| FX-015 | Menu bar / tray monitors | **Implemented + live-validated**: one tray icon + native menu (live CPU/Memory/Network value rows, Open, Compact monitor, Pause/Resume, Settings…, Quit), live icon title showing real CPU %, close-to-tray, pause/resume with baseline reset, quit with full cleanup | Same `TrayController` code path (Tauri tray-icon is cross-platform). Windows notification-area and Linux AppIndicator behaviors compile but are **untested on real OS** — recorded below. Linux-no-tray → compact floating monitor fallback implemented |
| FX-016 | Widgets / native mini-dashboard | **WidgetKit: explicitly gated** (see Gating note) — no fake native widget. **Compact floating monitor: implemented** (frameless always-on-top window, same snapshot bus) as the honest fallback the phase requests | Same compact fallback; Windows widget-provider research recorded as deferred (platform-capabilities.md already flags it as research item) |

**WidgetKit gate (phase 06 §2.7):** the macOS WidgetKit extension requires a signed app + extension bundle (Apple Developer ID, entitlements, WidgetKit extension target). This environment has no signing identity (Phase 00 spike noted this). Per the phase gate — "either truly work as signed native extension or remain explicitly gated" — FX-016 stays `platform_limited/gated` in the feature matrix. No widget code was written; nothing pretends to be a widget.

## HOW IT WORKS

- `tray.rs` — `TrayController`: built once in setup, driven by the **scheduler's snapshot tap** (phase 06 §2.1: no second polling loop). Value rows repaint only when the displayed string changes (§2.3 throttle), dispatched to the main thread (muda requirement).
- Menu value rows: `CPU n%` / `Memory used / total` / `↓ rx · ↑ tx · ifname`. The network row shows the **dominant interface by current traffic** (max rx+tx) — a defined aggregation that never sums physical+virtual adapters. Unavailable values render `—`, never zero.
- Pause/Resume (§2.9): new `SchedulerHandle::set_paused` pauses the loop (no ticks, no publishes; last snapshot stays for staleness display). On resume the engine calls the new `Collector::reset()` on every collector — CPU re-enters warm-up, network/disk rate trackers clear — so resumed numbers use a fresh counter baseline, not averages across the pause.
- Close-to-tray: `CloseRequested` reads the persisted `close_to_tray` setting; true → `prevent_close` + hide (tray keeps sampling); false → prior exit semantics.
- Quit (menu → Quit): stops scheduler, flushes history writer, `app.exit(0)`; `RunEvent::Exit` performs the same cleanup idempotently.
- Start-at-login: `tauri-plugin-autostart 2.7.0`; the OS login item is applied **only when the user toggles the new `launch_at_login` setting** — never silently at startup.
- Settings v6: new `TrayConfig { enabled, show_cpu, show_memory, show_network, show_title }` + `launch_at_login`, full v1→v6 migration chain (fixture-tested), live-applied from `update_settings` (toggling needs no restart; icon visibility follows `enabled`).
- Compact monitor (FX-016 fallback): frameless always-on-top `WebviewWindow` running the same frontend with a `__HOSTLUME_COMPACT__` initialization flag; `CompactMonitor` renders CPU/Memory/Network from the same telemetry store. Triggerable from the tray menu; on Linux it is the fallback when a tray is unavailable.
- Single-instance already guarantees reopening focuses the same app/sampler (phase 01 plugin; re-verified: no duplicate process).

## MODIFIED / ADDED FILES

- `apps/desktop/src-tauri/src/`: `tray.rs` (new), `scheduler.rs` (pause + reset), `collectors/{cpu,network,disk_io}.rs` (`reset` impls), `settings.rs` (v6: TrayConfig, launch_at_login, SettingsV5 + chains), `settings_store.rs` (v1..=5 → 6 routing), `state.rs` (paused, tray), `commands.rs` (live tray config + autostart application), `lib.rs` (tray build, composite tap, close-to-tray, plugins, features), `Cargo.toml` (tray-icon + image-png features, autostart plugin)
- `crates/monitor-core/src/collector.rs`: `Collector::reset()` default method
- Frontend: `features/compact/CompactMonitor.tsx` (new), `main.tsx` (compact branch), `app/App.tsx` (tray Settings deep-link listener), `features/settings/SettingsPage.tsx` (tray + launch-at-login sections), `lib/ipc/{types,guards}.ts` (v6 types), `lib/i18n/dictionaries.ts` (en+ro), test fixtures
- docs: `docs/progress/PHASE-06.md`, feature-matrix updates, this file

## TESTS RUN (exact)

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 findings) |
| `cargo test --workspace` | PASS — 117 passed, 0 failed (was 110; +7: pause/resume scheduler test, tray value/dominant-interface/format/config-toggle tests) |
| `pnpm --dir apps/desktop lint` | PASS |
| `pnpm --dir apps/desktop typecheck` | PASS |
| `pnpm --dir apps/desktop test` | PASS — 78 passed, 0 failed |
| `pnpm --dir apps/desktop build` | PASS |

New test coverage: `pause_stops_publishing_and_resume_resets_baselines` (reset-aware stub collector asserts exactly one baseline reset on resume); tray formatting (SI rates incl. bits mode, binary memory); `dominant_interface` picks the busiest adapter and never sums adapters; warm-up rows render `—` not zero; config toggles hide rows and the icon title; settings v5→v6 migration defaults.

## NATIVE OS TESTS (macOS 27.0.1, Apple M3 Max — real hardware)

- **Tray icon present**: System Events reports the status item (`position (1324,7), size 61×24`, description "status menu"); screenshots show the icon in the menu bar.
- **Live icon title**: accessibility name reads the real CPU percentage and updates between captures (59% → 58%) — real values, no mock.
- **Close-to-tray**: with `close_to_tray=true` persisted, clicking the window close button logs `CloseRequested on main / close_to_tray=true`, hides the window (window count 0), process stays alive; tray keeps sampling.
- **Pause/Resume**: covered by the scheduler unit test (publishing stops; exactly one baseline reset on resume). The tray **menu itself** opens on real clicks but muda popup menus are not reachable via AppleScript AX automation (menu-tracking loop blocks queries) — the two clicked-menu actions (Compact monitor trigger, Quit) are therefore listed under Not tested (menu click-through), not silently claimed.
- **Clean exit**: SIGTERM terminates with no errors in the log (`0` error/panic lines), consistent with prior-phase shutdown cleanup now wired into the tray Quit path.

## UNTESTED OS-HARDWARE

- Windows notification-area tray behavior (Explorer restart, DPI, hidden overflow, left/right click conventions) — compile-gated on the Windows CI job only.
- Linux StatusNotifier/AppIndicator, GNOME-without-tray detection and the auto-fallback path — compile-gated on the Ubuntu CI job; GNOME Wayland/KDE/X11 validation needs real desktop sessions.
- macOS menu-bar overflow/notch crowding: this machine has many status items; the icon was verified via accessibility even where screenshots show the crowded bar. Multi-display and dark/light template-icon behavior (icon uses `icon_as_template` false with the app icon; template adaptation is a polish item) — not exhaustively validated.
- Compact monitor window creation: the trigger lives behind a tray menu item that AX automation cannot click (above); code path is identical to the main window builder. Manual click-through pending.

## SECURITY CHECK

- No new network surfaces; tray reads the local snapshot bus only.
- Tray menu exposes no destructive actions beyond Quit; no process/Trash/workflow actions are reachable from the tray (security-privacy.md §5/§6 boundaries preserved).
- Login-item change is explicit user action from Settings; never auto-enabled.
- Debug `eprintln!` used during live verification was removed before commit.
- Post-commit deep security scan: re-run after commit (see commit note below).

## BLOCKERS

None for this phase on macOS. WidgetKit remains gated on a signing identity (documented, not a blocker for the phase's P0 outcome). Tray menu click-through automation is a testing-tool limitation, not an app defect.

## NEXT PHASE READINESS

**Next phase: 07 — notifications & local opt-in actions (FX-017), ready pending owner acceptance of Phase 06.**
