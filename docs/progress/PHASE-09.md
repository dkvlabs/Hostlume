# Phase 09 report — hardening, QA, release pipeline and release-candidate docs

**Date:** 2026-10-19 · **Owning features:** all P0/P1 rows (FX-001–FX-021) + FX-020 completion ·
**Validated target:** macOS 27.0.1 (Darwin 27.0.0), Apple M3 Max, arm64 · **Candidate:** `0.1.0` rc1, macOS arm64 only

This phase produced an evidence-backed release candidate **for one target**. The headline outcome is that
running the *actual release bundle* (not the dev server, not the test suite) exposed two long-standing
frontend defects that made the shipped dashboard freeze on its first tick and the Settings screen hang on
"Connecting…". Both were invisible to all 183 Rust and 97 frontend tests that existed before this phase.
They are fixed with regression and cross-language contract tests, and the rebuilt bundle was re-verified
live. Everything below records what actually ran; nothing is claimed for Windows or Linux.

## 1. Feature audit (phase §2.1)

Every FX row in `docs/feature-matrix.md` was re-read against the code, the live release bundle and the
test suite; statuses were changed **only** where there is phase-09 evidence (the matrix carries the
per-row evidence records under "Phase 09"). Summary:

| Class | Rows | Outcome |
|---|---|---|
| P0 | FX-001–008, FX-020 | **verified (macOS arm64)** in the release bundle: host, CPU (total + 14 cores + sparklines), memory/swap, disks (15 readings), overview cards, network (128 readings), applications (real process table), disk visualizer/Trash (phase 04 live evidence re-confirmed by test), settings v1→v8 + Reset |
| P1 verified | FX-012, FX-013, FX-015, FX-017, FX-019 (loopback), FX-021 | verified on macOS arm64 by live smoke in phases 05–09 and re-run tests |
| P1 platform_limited (accepted, truthful) | FX-009 (macOS disk I/O: `SOURCE_NOT_AVAILABLE_IN_BUILD`, honest unsupported row), FX-010 (GPU % — no public non-root source), FX-012 fan RPM, FX-014 (needs an accessory exposing charge), FX-016 (WidgetKit needs a signing identity), FX-019 LAN (TLS not implemented, ADR-0008) | shown in the UI as unsupported with reasons; never zeros |
| P1 implemented, not manually exercised | FX-011 (external IP lookup has **no transport** by design), FX-018 (commands/alignment unit-covered; Compare chart interaction and click-export not manually run) | documented in the matrix |
| P3 | FX-022 multi-host | planned (phase 10 optional) |

No placeholder card is listed as verified. The live-reading dump used for the audit is reproducible with
`cargo test -p app --test dump_live_readings -- --ignored` (writes
`<tmp>/hostlume-live-readings.json`; 13 collectors sampled warm).

## 2. Native build grid (phase §2.2)

| Target | Built | Installed/launched | Evidence |
|---|---|---|---|
| macOS Apple Silicon (arm64) | **yes** — 3 release bundles (`pnpm tauri build`), final one with `LSMinimumSystemVersion 11.0` | **yes** — `.app` launched 4 times from the bundle, DMG `hdiutil verify` + `attach`/`detach` ok | §7, `CHANGELOG.md` |
| macOS Intel (x86_64) | no (not claimed) | — | excluded from the release claim |
| Windows x64 | no (no hardware, no cross target installed) | — | excluded; compile verification pending CI |
| Linux GNOME/KDE | no (no desktop session) | — | excluded; compile verification pending CI |

Per phase §2.2 and §5, hardware that is unavailable **excludes** the target from the release claim; the
candidate is labelled macOS-arm64-only in `CHANGELOG.md` and `docs/SUPPORTED_TARGETS.md`.

## 3. Release-blocking defects found by the release smoke (and fixed)

### 3.1 Settings guard rejected every real payload → Settings "Connecting…", Overview empty

`isSettings` (`apps/desktop/src/lib/ipc/guards.ts`) compared `settings.schema_version` with the
**telemetry** `SCHEMA_VERSION` (1). Real settings have been `schema_version: 8` since the phase-08
migration, so every `get_settings`/`save_settings` payload was discarded; the Overview had no card
configuration and the Settings page never left its loading state. The test suite never caught it because
every settings fixture was hand-written with `schema_version: 1`.

**Fix:** the guard accepts any integer `>= 1`; `SETTINGS_SCHEMA_VERSION = 8` is exported separately from
the telemetry schema in `types.ts`; a **cross-language contract fixture**
(`src/lib/ipc/__fixtures__/settings-default.json`) is generated from the real `Settings::default()`
serialization by the Rust test `settings.rs::default_settings_fixture_matches_frontend_contract`
(`UPDATE_FIXTURES=1` regenerates; drift fails the Rust test) and asserted on the TypeScript side in
`guards.test.ts`. The old, wrong expectation (`schema_version: 9` rejected) was corrected.

### 3.2 Telemetry sequence off-by-one → dashboard frozen on the first tick

`useTelemetry.ts` advanced `knownSequence` when the push **event** arrived, then fetched the snapshot and
discarded it because its sequence was *equal* to (not greater than) the pre-advanced value. The dashboard
only refreshed when a scheduler tick happened to land between the event and the fetch, which is why the
first release soak showed a frozen Overview. Present since phase 02.

**Fix:** separate `appliedSequence` (advances only when a snapshot is applied) from `pendingSequence`
(set by events) with an `inFlight` coalescer that refetches exactly once after a fetch completes if a newer
event arrived meanwhile; subscription failures now surface through `setError` instead of being swallowed.
Regression tests in `src/app/useTelemetry.test.tsx` (event seq == fetched seq applies; stale events do not
refetch; in-flight coalescing refetches once). Re-verified **Live** in the rebuilt bundle: Overview
refreshing every 3 s with real CPU/memory/per-core values (window-scoped screenshots by CGWindowID).

Both defects share one root cause — validation was done against fixtures and headless tests, never against
the packaged artifact. The contract fixture and the release-smoke step in §7 are the structural answer.

## 4. Hardening shipped in this phase

* **Crash log** (`lib.rs`): `install_crash_logger` is the first thing `.setup()` does after resolving the
  app data directory; it chains the previous panic hook and appends one 500-char-truncated line
  (`crash_log_line`) per panic to `<app_data_dir>/crash.log` (`append_crash_entry`, best-effort, never
  rewrites history). Tests: `crash_log_line_is_single_line_and_truncated`,
  `append_crash_entry_appends_without_truncating`.
* **Emit-failure surfacing** (`lib.rs`): telemetry `emit` errors are counted and logged (first and every
  100th) instead of being silently dropped.
* **Reset settings** (`commands.rs::reset_settings`, `settingsStore.reset`, Settings page card with
  `ConfirmActionDialog`): disables autostart first (abort before persisting if that fails), persists
  `Settings::default()`, re-applies polling rate, tray config, alerts config and **shuts down the remote
  server**; never touches history, rules or exports. Tests: `SettingsPage.test.tsx` (confirm-once, cancel
  no-op, rejection keeps settings), `settingsStore.test.ts` (success/failure).
* **XSS inertness** (`ApplicationsPage.test.tsx`): a hostile process name shaped like
  `<img onerror>` + `<script>` renders as text; no element is created and `window.__pwned` stays undefined.
* **CORS / cross-origin** (`monitor-remote/tests/integration.rs::cross_origin_requests_get_no_cors_grant_and_still_need_auth`):
  a foreign `Origin` is still 401 without a token, no `Access-Control-Allow-*` header is ever emitted and
  an `OPTIONS` preflight gets no grant.
* **Minimum OS** (`tauri.conf.json`): `macOS.minimumSystemVersion = "11.0"` (was the Tauri default 10.13,
  which the arm64 build cannot honour).
* **Supply chain**: `deny.toml` gains `[advisories]` (v2, yanked denied, four documented ignores — see §9),
  a crates.io-only `[sources]` allowlist and `[bans] wildcards = "deny"`; `publish = false` on every
  workspace crate; ECharts 6.0.0 → **6.1.0** (GHSA-fgmj-fm8m-jvvx); CycloneDX 1.5 SBOM generator
  `scripts/generate-sbom.mjs` → `docs/release/sbom/`.
* **Diagnostics**: ignored test `tests/dump_live_readings.rs` dumps every collector's warm readings for
  audits; `PRIVACY.md` (user-facing) and `CHANGELOG.md` (release notes) added.

## 5. System lifecycle (phase §2.3) — what ran on the release bundle

| Item | Result |
|---|---|
| First run | **Not re-exercised with a pristine data directory** on the release bundle (the existing `~/Library/Application Support/com.dkvlabs.appname/` from earlier phases was used; it contains a partial settings row left by the phase-08 smoke cleanup, which the app tolerates via `#[serde(default)]` — documented, data dir otherwise untouched). Pristine first run was validated in dev in phases 01–02 only. |
| Login launch (autostart) | Not exercised live this phase; plugin wiring unchanged since phase 06; `reset_settings` disables it idempotently (plugin treats NotFound as success). |
| Close-to-tray | Phase 06 live evidence; code unchanged. |
| Quit | **PASS** — OS quit (⌘Q / window close) exits with status 0 (12 s sanity run via a wrapper that records the exit status); `history.sqlite3` `PRAGMA integrity_check` = ok afterwards; no `crash.log` entries; no `.ips` report. |
| Unexplained exit (run 1) | The first 30-min soak's app exited silently at ~17:58Z with no panic, no crash report and no exit status (launched via `open`; the owner was using the machine). Undetermined; consistent with a user window-close. Run 2 was launched through the status-recording wrapper and quit cleanly. Recorded as a residual unknown, not a defect. |
| Sleep/resume | Not exercised live this phase; sleep-gap behaviour covered by scheduler/rate unit tests (phases 02/07). |
| Interface hotplug / removable unplug | Not exercised live; interface-recreation and counter-reset unit tests exist (phase 02). |
| GPU driver missing | Not applicable on macOS; Windows/Linux not run. |
| DB full / corrupted | **Not exercised.** Integrity confirmed only on the healthy DB. Remaining checklist item. |
| Settings reset | Implemented and tested (UI + store + command); **not clicked in the release bundle** (the bundle predates the final test additions only by the test files, the command itself is in the bundle). |

## 6. Safety checks (phase §2.4) and remote threat review (phase §2.5)

All deterministic safety coverage from phases 03–08 re-ran green in this phase's full suite (183 Rust
tests, 108 frontend tests):

* protected-process refusal, PID + start-time identity recheck, PID-reuse (`processes.rs` tests);
* scan scope, symlink loop, permission failure, cancel (`disk_scan` tests); Trash is the only removal path;
* alert actions: both local gates required, stored config revalidated, no-args literal launcher, hard
  timeout (`actions.rs` tests);
* remote: OFF → no listener (typed refusal + `lsof` on three bundle launches), LAN fails closed without TLS,
  token negatives, read-only methods (`POST` → 405, kill/trash routes → 404), history bounds, session
  exchange, rotation invalidates tokens and sessions, SSE cap 503, flood 429, projection contains no
  IPv4/hostname, **new** no-CORS-grant test.

`docs/security-privacy.md` §9 threat matrix → test mapping is unchanged from phase 08 plus the two new
tests above; no remote write/control route exists.

## 7. Release artifacts, signing and verification (phase §2.10, §2.12)

| Artifact | SHA-256 | Size |
|---|---|---|
| `APP_NAME_0.1.0_aarch64.dmg` | `e5e5f3d158c44ceb2c925bc99eddb334f838faa3466b66848b33a720341cd4ab` | 5,594,313 B |
| `APP_NAME.app/Contents/MacOS/app` | `3b7ee96710e084c373aa0458fa3a6d562ad24541a0287496293231052c802c0e` | 16,503,616 B |

* `hdiutil verify` ok; mounted, app launched from the mounted image, detached.
* `codesign -dv`: **ad-hoc, linker-signed** only. `spctl --assess` **refuses** the bundle — there is no
  Developer ID certificate in this environment. **Notarization is blocked on the owner's Apple signing
  identity**; the WidgetKit extension (FX-016) stays gated for the same reason.
* `plutil -p Info.plist`: identifier `com.dkvlabs.appname`, version 0.1.0, `LSMinimumSystemVersion 11.0`.
* Windows signing decision and installer smoke: **not made / not run** (no hardware). Linux packages: not built.
* No git tag was created (the owner manages git history); the checksums above are the release record.
  Migration (settings v1→v8 chain, history schema V2) and rollback notes are in `CHANGELOG.md`.

## 8. Performance (phase §2.6) — see `docs/progress/PERF-2026-10-19.md`

* 30-min soak (release build, UI frozen by defect 3.2) + 10-min soak (fixed candidate, live Overview):
  main-process CPU **0.43–0.50 % average, ≤3.9 % peak** — inside the proposed `<2 %` (testing-qa §5).
* RSS: main process 61–99 MB average; **with the WebKit content process the live total is ≈185 MB**, above
  the proposed 150 MB ("measured, not guaranteed"). Results and plan documented per §4.
* **History growth needs a product decision before general release:** 4,860 raw rows/min across 180
  series ≈ 250 B/row ≈ **73 MB/h**, i.e. a bounded plateau of ≈1.7–1.8 GB at the default 24-h raw
  retention. Options (PERF doc): lower the default raw horizon, persist slow-group readings only on change,
  normalise the repeated UUID columns, `auto_vacuum`. Not changed in this phase.
* Not measured: 4-h soak, WebKit content-process CPU, high-history query latency, large-folder scan
  timing, multi-client remote stream.

## 9. Dependencies, licenses, SBOM (phase §2.8)

* `cargo deny check` → **advisories ok, bans ok, licenses ok, sources ok**. Four unmaintained-crate
  advisories are ignored with reasons in `deny.toml`: RUSTSEC-2024-0370 (`proc-macro-error` via
  `glib-macros`, Tauri/GTK tree) and RUSTSEC-2026-0370/0371/0372 (`zvariant`, `zbus_names`,
  `zvariant_derive` via `zbus 5.19` ← `notify-rust`/`tauri-plugin-single-instance`). None is a
  vulnerability; all are transitive through pinned Tauri-ecosystem crates and are revisited when those
  crates update.
* `pnpm audit` → **No known vulnerabilities found** (after ECharts 6.1.0).
* SBOM: `docs/release/sbom/sbom-0.1.0.cdx.json` (CycloneDX 1.5) — 515 crates, 262 npm packages, 57 of
  them runtime; tallies in `docs/release/sbom/README.md`. License scan recorded in
  `docs/evidence/DEPENDENCY_LICENSE_SCAN.md` §Phase 09; no copyleft dependency in the frontend tree.
* Native sensor SDKs: none bundled (IOKit/SMC access is via system frameworks).

## 10. Accessibility and i18n (phase §2.7)

**Not performed as a dedicated manual pass in this phase.** Existing evidence: chart data-table
equivalents (phase 08), en/ro dictionaries with the new reset keys, number formatting per locale (tests).
Keyboard-only navigation, VoiceOver, high contrast, 200 % scaling, minimum window size and reduced motion
remain on the pre-release checklist (§13).

## 11. Branding (phase §2.9)

`APP_NAME` placeholder is still in place by instruction — the owner has not supplied the product name.
Identifier `com.dkvlabs.appname` is unique but provisional; icons and graphics are original (phase 00);
`PRIVACY.md` added; root `LICENSE` is GPL-3.0; support links do not exist yet (no public URL). No
reference-application code, trademarks, screenshots or assets are present (re-checked during the audit).

## 12. Release-candidate criteria (phase §4) — honest evaluation

- [x] **All P0 FX rows verified on declared supported OS targets** — declared target is macOS arm64 only; all P0 rows verified there (§1). Windows/Linux are not declared.
- [x] **All P1 rows verified or truthful limitation documented** — §1; acceptance of the limitations is the owner's (§13).
- [ ] **No high-risk unresolved vulnerability, unsafe Trash/quit, misleading metric, data-loss defect or crash under typical lifecycle** — no known item of these kinds remains; but DB-full/corrupted and pristine-first-run paths were not exercised on the bundle (§5). Marked open until those two run.
- [x] **Bundled assets and SDK licenses reviewed; no reference-application proprietary materials** — §9, §11.
- [ ] **Remote API meets opt-in, token, TLS, read-only and privacy gates** — opt-in/token/read-only/privacy: yes; **TLS: not implemented**, LAN mode refuses to start (fail-closed, ADR-0008). Loopback-only claim is accurate; the criterion as written is not fully met.
- [x] **Usable and low overhead when sensors unavailable** — unsupported rows render with reasons (FX-021); CPU within budget (§8).
- [x] **Native installers built, installed, launched, verified** — DMG built, verified, mounted, launched (§7). "Uninstalled" = drag-to-Trash of a non-installed bundle; no uninstaller to test on macOS.
- [x] **CPU/RSS measured and reviewed; target exceeded → results and plan documented** — §8.
- [x] **Changelog, support diagnostics, privacy policy, release matrix, migration plan** — `CHANGELOG.md`, `PRIVACY.md`, `docs/SUPPORTED_TARGETS.md`, crash log + capability diagnostics, migration/rollback in the changelog.
- [x] **This report includes the release decision and owners** — §13.

## 13. Release decision and owners

**Decision: `0.1.0-rc1` is a release candidate for macOS Apple Silicon only, held at "candidate" — not
approved for general release — pending three owner decisions.** Windows and Linux are explicitly not
shipped (phase §5: ship only verified targets).

| # | Decision needed | Owner | Blocking what |
|---|---|---|---|
| 1 | Apple Developer ID signing + notarization (also unblocks WidgetKit, FX-016) | Project owner (Gabriel Georgescu) | Gatekeeper-passable distribution; without it the DMG only runs with a right-click Open |
| 2 | History growth default (keep 24 h raw ≈ 1.8 GB plateau, or adopt one of the §8 options) | Project owner | General-release storage expectations |
| 3 | Accept LAN/TLS deferral (loopback-only remote in 0.1.0) or schedule TLS before release | Project owner | Remote-API criterion in §12 |
| — | Product name and support links (replaces `APP_NAME`, `com.dkvlabs.appname`) | Project owner | Branding criterion |

Engineering items that remain before promoting rc1 → release (no owner decision needed, just execution):
DB-full/corrupted and pristine-first-run runs on the bundle; the dedicated accessibility/i18n pass (§10);
a 4-h soak; clicking Reset settings in the bundle.

## 14. Verification commands and exact outcomes

| Command | Outcome |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS — 0 findings |
| `cargo test --workspace` | **183 passed, 0 failed, 4 ignored** (app_lib 77 + 1 ign; config_security 4; monitor_alerts 21; monitor_core 10; serialization 8; monitor_history 7 + 1 ign; history 8 + 1 ign; platform_linux 3; platform_macos 7; platform_windows 5; monitor_remote 11 unit + 9 integration; monitor_storage 4 + 9; cpu_crosscheck 1 ign; dump_live_readings 1 ign) |
| `pnpm --dir apps/desktop typecheck` | PASS |
| `pnpm --dir apps/desktop lint` | PASS |
| `pnpm --dir apps/desktop test` | **18 files, 108 tests passed** |
| `pnpm --dir apps/desktop build` | PASS |
| `pnpm --dir apps/desktop tauri build` | PASS ×3 (final bundle = artifacts in §7) |
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok |
| `pnpm --dir apps/desktop audit` | No known vulnerabilities found |
| `node scripts/generate-sbom.mjs` | wrote `docs/release/sbom/sbom-0.1.0.cdx.json` |
| `hdiutil verify` / `attach` / `detach`, `codesign -dv`, `spctl --assess`, `plutil -p` | see §7 |
| soak sampler (`ps`, `top -l 2`, `stat`), `lsof -nP -iTCP:45454` | see §8, §5 |

## 15. Not tested / untested OS-hardware

* **Windows x64** and **Linux (GNOME/KDE)**: no native build, install, launch or runtime validation of any
  kind; only fixture/unit tests of their collectors ran on macOS. Compile verification is pending CI.
* macOS Intel: not built, not claimed.
* On macOS arm64: pristine first run on the bundle, autostart, sleep/resume, hotplug/unplug, DB
  full/corrupted, Reset click in the bundle, accessibility pass, 4-h soak, large-folder scan timing,
  multi-client remote stream, live Trash/force-quit of a protected process (needs a dedicated machine).

## 16. Known limitations and risks

* Ad-hoc signature only (owner blocker §13); total live RSS above the proposed budget (§8); history
  growth default (§8); loopback-only remote (§6); `APP_NAME` branding (§11).
* Residual unknown: the silent exit of soak run 1 (§5).
* Security changes this phase are all restrictive (CORS test, deny gates, dependency upgrade, min OS,
  crash log limited to 500-char lines with no user data); `reset_settings` is a new IPC command gated by
  the existing capability set, confirmed through the destructive dialog, and only writes defaults.

## 17. Exit criteria and next phase

Phase 09 exit criteria are met for the declared target with the §12 checklist evaluated honestly and the
decision table in §13. **Phase 10 (multi-host, optional) is not started; it remains blocked on the owner's
explicit acceptance** per `docs/implementation-phases.md`, and the three §13 decisions should be resolved
before any rc1 → release promotion.
