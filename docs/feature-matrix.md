# Feature matrix — single source of truth for scope/status

**Status convention:** `planned` → `in_progress` → `implemented` → `verified`; or `platform_limited` with platform/hardware explanation. **Initial status: all planned.** Never silently mark as verified. `Parity` means publicly described reference functionality, not copied internals/UI.

| FX ID | Feature | Reference? | Priority | Owning phase | Status (phase 09 audit, 19.10.2026) | Key acceptance evidence |
|---|---|---|---|---|---|---|
| FX-001 | Device identity, uptime and hardware summary | Yes | P0 | 02 | verified (macOS arm64); Windows/Linux compile-gated | 8 host readings live in the release bundle; collector tests; dump_live_readings |
| FX-002 | CPU overall and per-core history | Yes | P0 | 02 | verified (macOS arm64); Windows/Linux compile-gated | Release-bundle Overview live: total + 14 per-core bars + sparkline refreshing every tick (after the phase 09 sequence fix); rate/warm-up tests |
| FX-003 | Memory/pressure/swap | Yes | P0 | 02 | verified (macOS arm64); Windows/Linux compile-gated | Live card 22.0 GiB/36.0 GiB + swap 7.3/8.0 GiB with sparkline; range tests |
| FX-004 | Disks/mounts/capacity/external drives | Yes | P0 | 02 | verified (macOS arm64); Windows/Linux compile-gated | 15 disk readings live; removable flag honest; eject not implemented (guarded by design) |
| FX-005 | Configurable overview dashboard | Yes | P0 | 02 | verified (macOS arm64) | Card order/visibility persisted (settings v8); release bundle renders real snapshot; phase 09 fixed the settings-guard defect that hid all cards |
| FX-006 | Network interfaces, addresses and speeds | Yes | P0 | 02 | verified (macOS arm64); Windows/Linux compile-gated | 128 interface readings live; reset/wrap/sleep-gap tests |
| FX-007 | Applications/process history and quit | Yes | P0 | 03 | verified (macOS arm64, guard level); live force-quit of a protected process needs a dedicated test machine | PID+start-time identity recheck, protected-target refusal, PID-reuse tests; real process table in release bundle; hostile-name XSS inertness test (phase 09) |
| FX-008 | Disk visualizer, large files, Move to Trash | Yes | P0 | 04 | verified (macOS arm64); Windows/Linux compile-gated | Live OS-trash move (phase 04), scope/symlink-loop/cancel/permission tests |
| FX-009 | Disk I/O read/write counters and rates | Yes | P1 | 04 | platform_limited (macOS: `SOURCE_NOT_AVAILABLE_IN_BUILD`, honest unsupported row); Linux `/proc/diskstats` fixture-tested; Windows not run | collectors/disk_io.rs |
| FX-010 | GPU utilization, memory, displays | Yes | P1 | 05 | implemented (macOS adapter/display metadata live); GPU % platform_limited (no public non-root source); Windows DXGI compile-gated; Linux DRM fixture-tested | phase 05 live probe |
| FX-011 | Advanced network history/map/IP info | Yes | P1 | 04 | implemented with limitation: external IP lookup has **no transport** (typed `EXTERNAL_LOOKUP_TRANSPORT_UNAVAILABLE`, opt-in switch only) | NetworkPage + commands.rs |
| FX-012 | Temperatures and fan sensors | Yes | P1 | 05 | verified (macOS M3 Max: 21 sensors live); fan RPM platform_limited; Linux hwmon fixture-tested; Windows unsupported | phase 05 + phase 09 live dump |
| FX-013 | Battery cycles/health/charge/time | Yes | P1 | 05 | verified (macOS: 5 battery readings live vs ioreg in phase 05); Linux fixture-tested; Windows compile-gated | collectors/battery.rs |
| FX-014 | Bluetooth accessory battery | Yes | P1 | 05 | platform_limited (probe implemented, parser fixture-tested; a live charge value needs a connected accessory exposing it; Windows not implemented) | collectors/bluetooth.rs |
| FX-015 | Menu bar / system tray monitors | Yes | P1 | 06 | verified (macOS arm64: icon, live CPU title, close-to-tray, pause/resume); Windows/Linux compile-gated | phase 06 live smoke |
| FX-016 | Widgets / native mini-dashboard | Yes (macOS) | P1 | 06 | platform_limited (WidgetKit gated on a signing identity; compact floating monitor shipped) | phase 06 |
| FX-017 | Notifications and local opt-in actions | Yes | P1 | 07 | verified (macOS arm64: real-metric fire, restart no-double-fire, notification prompt); actions no-args by design (ADR-0007); Windows/Linux compile-gated | phase 07 live smoke; engine + action security tests |
| FX-018 | Comparison, recording, CSV export | Yes | P1 | 08 | implemented (commands and alignment machinery unit-covered; Compare chart interaction and UI-click export not manually exercised); CSV import intentionally absent | monitor-history sessions/export tests |
| FX-019 | Mobile web monitor / read-only REST API | Yes | P1 | 08 | verified (loopback, macOS): OFF→no listener re-confirmed on the release bundle; auth/405/404/privacy/bounds/SSE/cookie/CORS tests; **LAN platform_limited** until TLS (ADR-0008) | monitor-remote 9 integration + 11 unit tests; phase 08/09 smokes |
| FX-020 | Preferences, themes, retention, i18n | Yes (subset) | P0 | 01/09 | verified (macOS arm64): settings v1→v8 migrations, en/ro, themes, retention controls, **Reset settings** (phase 09), settings load fixed in the release bundle | settings tests + contract fixture + SettingsPage tests |
| FX-021 | User-facing capability diagnostics | Enhancement | P1 | 05 | verified (macOS arm64): every family exactly once with reasons; Sensors→Capabilities tab | lib.rs test + phase 05 live |
| FX-022 | Multi-computer monitoring | Enhancement | P3 | 10 (optional) | implemented (macOS arm64): paired-agent console + LAN-TLS agent verified on loopback against a demo harness and by 11 real-TLS integration tests; **a real second machine over physical LAN is not validated** (no second device in this session) | `crates/monitor-remote` (client/pairing/tls/protocol), `apps/desktop/src-tauri/src/hosts.rs`, `src/features/hosts/HostsPage.tsx`; ADR-0009 |

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

## Phase 09 evidence records (19.10.2026) — release candidate audit

| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
|---|---|---|---|---|---|---|
| FX-001–006, FX-020 | macOS 27.0.1 / Apple M3 Max | phase-09 tree (release bundle 0.1.0 arm64) | **release-bundle launch + live dashboard** (window-scoped screenshots) | PASS — Overview Live, refreshing every 3 s after the sequence fix; CPU/memory/per-core/sparklines real; settings load after the guard fix | the first launch exposed both defects (frozen dashboard, settings "Connecting…") — fixed and re-verified in the rebuilt bundle | docs/progress/PHASE-09.md §3 |
| all P0/P1 | macOS / M3 Max | phase-09 tree | **30 min + 10 min soaks** of the release bundle | PASS — CPU ≈0.5 % avg; main RSS ≈61 MB (frozen UI) / ≈99 MB (live UI); WebKit ≈41 / ≈86 MB; no panics; exit 0 on OS quit | DB growth ≈73 MB/h (bounded by retention; plan documented); total RSS above proposed 150 MB with live UI | docs/progress/PERF-2026-10-19.md |
| FX-019 | macOS / M3 Max | phase-09 tree | release bundle `lsof` on three launches | PASS — no listener on 45454 with remote OFF (default) | — | docs/progress/PHASE-09.md §7 |
| FX-019 | monitor-remote crate | phase-09 tree | integration test (new) | PASS — no `Access-Control-Allow-Origin` ever; foreign `Origin` still 401 without token | — | `tests/integration.rs::cross_origin_requests_get_no_cors_grant_and_still_need_auth` |
| FX-007 | frontend | phase-09 tree | vitest (new) | PASS — markup-shaped process name renders as inert text | — | `src/features/applications/ApplicationsPage.test.tsx` |
| FX-020 | frontend + Rust | phase-09 tree | cross-language contract fixture (new) | PASS — `Settings::default()` serialization accepted by `isSettings`; drift fails the Rust test | — | `settings.rs::default_settings_fixture_matches_frontend_contract`, `guards.test.ts` |
| FX-020 | frontend | phase-09 tree | vitest (new) | PASS — factory reset double-confirmed, called once, failure keeps settings | — | `src/features/settings/SettingsPage.test.tsx`, `settingsStore.test.ts` |
| FX-002–006 | frontend | phase-09 tree | vitest regression (new) | PASS — event sequence == fetched sequence applies; coalesced refetch | — | `src/app/useTelemetry.test.tsx` |
| all | dependencies | phase-09 tree | `cargo deny check` + `pnpm audit` + SBOM | PASS — advisories/bans/licenses/sources ok; npm clean after ECharts 6.1.0 | 4 transitive `unmaintained` notices accepted with reasons (Linux stack) | docs/evidence/DEPENDENCY_LICENSE_SCAN.md §Phase 09, docs/release/sbom/ |
| all | macOS Intel / Windows x64 / Linux GNOME+KDE | — | native build + install + smoke | **NOT RUN** (no hardware or runners) | excluded from release claims per phase 09 §2/§5 | ADR-0005 |
| all | macOS arm64 | — | code signing / notarization | **BLOCKED** — ad-hoc linker signature only; Gatekeeper refuses on other Macs | needs owner's Apple Developer identity | docs/progress/PHASE-09.md §12 |

## Phase 10 evidence records (20.10.2026) — optional multi-host (FX-022)

| FX | Target | Build | Method | Result | Limitations | Sources |
|---|---|---|---|---|---|---|
| FX-022 | monitor-remote crate | phase-10 tree | 11 TLS integration tests over real loopback HTTPS (pinned client ↔ rcgen agent) | PASS — pair end-to-end (hash persisted, not token); wrong code burns the window; MITM fingerprint mismatch rejected and burns; version/shape probes do NOT burn; revoked device 401 on next request; impostor agent receives **zero** HTTP requests (handshake fails first); reconnect after restart with pinned cert; clock skew never affects liveness; duplicate device replaces credential; rename keeps host id; partition → offline, no crash; pairing refused on plain loopback | agent bound to loopback via the test seam (production LAN binds 0.0.0.0) | `crates/monitor-remote/tests/multi_host.rs` |
| FX-022 | monitor-remote crate | phase-10 tree | unit tests (new: pairing, tls, protocol, client) | PASS — RFC 4231 HMAC vectors; code burn/expiry; key file 0600; self-healing TLS identity regeneration; verifier pin/mismatch; liveness derivation from local clock; backoff caps | — | `src/{pairing,tls,protocol,client}.rs` tests |
| FX-022 | monitor-history crate | phase-10 tree | unit test (new) | PASS — `delete_host_samples` removes all three tiers for one host and leaves every other host intact; idempotent | — | `retention.rs::host_delete_tests` |
| FX-022 | app crate (macOS arm64) | phase-10 tree | unit tests (new: hosts runtime, credential store, settings v9) | PASS — credential file 0600 and never in the settings shape; liveness ages from the local clock (online→stale→offline; terminal states sticky); headline ring bounded; missing credential starts no worker; v8 row migrates to v9 keeping remote config; **phase-06 defect fixed+pinned: tray/launch_at_login patches were accepted but never applied** | — | `hosts.rs` tests, `settings.rs`/`settings_store.rs` tests |
| FX-022 | frontend | phase-10 tree | vitest (new: HostsPage 7, guards 2) | PASS — stale host never rendered as live (values stay, badge/warn); removal double-confirmed with explicit purge; pairing failure surfaces the typed reason; success pins the fingerprint; guards reject the pre-phase-10 status shape and malformed hosts | — | `src/features/hosts/HostsPage.test.tsx`, `guards.test.ts` |
| FX-022 | macOS 27.0.1 / M3 Max | **release bundle 0.1.0 arm64** | **live console ↔ demo agent over pinned TLS on loopback**: release app (Hosts page) polling `demo-agent` every 2 s, history recording on | PASS — card Live with real polled values (CPU 38.9 %, memory 12.6 GiB, sparklines), last response "1 s ago", agent clock + skew −2.0 s displayed; **TLS connection app→agent ESTABLISHED (lsof)**; **2,600+ readings ingested into local history under the agent's host_id** (5 series); host removal path restores settings and deletes rows | values are the harness's synthetic demo series (labelled DEMO), not a second real machine | `docs/evidence/phase-10-hosts-live.png`, docs/progress/PHASE-10.md §6 |
| FX-022 | macOS / M3 Max | phase-10 tree | console resource measurement while polling (2 s interval) | PASS — main process ps-instantaneous **0.0–1.7 %** CPU (0.0 % after the agent stops), RSS 94–101 MB; one thread per host asleep between polls; bounded rings | measured against the loopback harness, not a LAN hop | docs/progress/PHASE-10.md §7 |
| FX-022 | macOS / M3 Max | release bundle | privacy review of what leaves the machine / enters it | PASS — console stores token only in `remote-credentials.json` (0600), never in settings/WebView/logs; agent persists hashes only; manifest carries user-chosen display name (no hostname/serials/MACs); snapshot projection remains numeric-only | — | ADR-0009, docs/progress/PHASE-10.md §8 |
| FX-022 | two physical devices over LAN | — | real second-machine validation | **NOT RUN** (no second device) | LAN bind (0.0.0.0), discovery-free addressing on a real network, mDNS absence, and Wi-Fi interruption behaviour are unvalidated on hardware | docs/progress/PHASE-10.md §10 |
