# Changelog

All notable changes to **Hostlume**. Versions follow SemVer; dates are UTC. The product was named on 20.10.2026; builds before that shipped as the `APP_NAME` placeholder with identifier `com.dkvlabs.appname`, and the app migrates that older data directory (settings, history, exports, credentials) into the new one on first start.

## 0.1.0 — release candidate 1 (2026-10-19, unreleased)

First feature-complete build of the cross-platform system monitor described in `docs/`. This is a **release candidate for one target**; see the support matrix — claims are limited to what was built, installed, launched and measured on real hardware.

### Supported targets in this candidate

| Target | Status | Evidence |
|---|---|---|
| **macOS, Apple Silicon (arm64)** — validated on macOS 27.0.1 (M3 Max); declared floor 11.0 is the documented product floor, not a tested minimum | **Release candidate** — `.app` and `.dmg` built, DMG verified and mounted, app launched, 30 + 10 min soaks, clean quit (exit 0) | `docs/progress/PHASE-09.md`, `docs/progress/PERF-2026-10-19.md` |
| macOS, Intel (x86_64) | **Not included** — no Intel hardware or runner was available; not built | — |
| Windows 11/10 x64 | **Not included** — code compiles behind `cfg(windows)` gates in earlier phases but no native build, install or runtime validation exists | ADR-0005 (CI matrix pending) |
| Linux x86_64 (GNOME / KDE) | **Not included** — fixture-tested parsers only; no native build, packaging or desktop validation | ADR-0005 |

**Distribution caveat:** the macOS bundle is **ad-hoc signed only** (linker signature, no Developer ID, not notarized). Gatekeeper will refuse it on other Macs until the owner signs and notarizes it with their Apple Developer identity; see "Release blockers" in `docs/progress/PHASE-09.md`.

### Features (macOS arm64, live-validated unless noted)

- **Overview dashboard** with configurable, reorderable cards: CPU total + per-core, memory + swap + pressure, disks/volumes, network interface throughput, host identity/uptime, GPU/displays, sensors, battery. Real readings only; unavailable metrics show `unsupported` / `permission required` / `temporarily unavailable` / `error` with reasons.
- **Applications**: virtualized process table with per-process CPU history; quit / force-quit behind explicit confirmation with PID + start-time identity recheck and local audit.
- **Storage**: cancellable folder scan with lazy tree, large-file view and **Move to Trash** (OS Trash only, scoped to the scan root, fail-closed).
- **Network**: per-interface counters, rates, addresses; external IP lookup switch present but **no transport implemented** (explicitly reported as unavailable).
- **Sensors** (macOS M3 Max: 21 temperature sensors), **battery** (cycles, health, charge), **Bluetooth accessory battery** (probe implemented; needs an accessory to show a value), **GPU** adapter/display inventory (GPU utilisation `platform_limited` — no public non-root source on macOS).
- **Capabilities** view explaining, per metric family, what this machine can and cannot report and why.
- **Menu-bar / tray** monitor with live CPU title, pause/resume sampling, close-to-tray, launch-at-login; compact floating monitor as the widget fallback (WidgetKit gated on code signing).
- **Alerts**: threshold rules with hold time, hysteresis and cooldown; OS notifications; quiet hours; **optional approved local actions** (default off, double-gated, no arguments, no shell, hard timeout, audited).
- **Compare**: up to four aligned series, unit axes, data-table view; **recordings** with crash-safe status; **CSV export** (RFC 4180, formula-injection escaping) into the app's own `exports/` folder.
- **Remote read-only monitor** (default **OFF**): loopback-only Axum server, random token stored only as a hash, rotation/revocation, bounded history, SSE stream, embedded read-only dashboard, no write/control routes. LAN mode is refused until TLS exists (ADR-0008).
- **Settings**: appearance (dark/light/system), compact mode, polling 1–60 s, locale (English, Romanian), temperature unit, rate unit, retention horizons, tray options, history clear and — new in this release — **Reset settings** (restores defaults, stops remote, erases its token, disables launch-at-login; keeps history and rules).
- **History**: local SQLite (WAL) with raw samples, per-minute and hourly rollups, retention trimming; schema migrations with checksums.
- **Crash handling**: panics append one line to a local `crash.log`; never transmitted.

### Fixed in the release-candidate smoke (phase 09)

Both defects below were present since phase 02 and were masked by test fixtures and headless validation; they were found by launching the actual release bundle.

- The frontend rejected every real settings payload because the settings guard compared the settings schema version (8) with the telemetry contract version (1). Settings never loaded, the Overview rendered no cards and the Settings page showed "Connecting…". Fixed; a cross-language contract fixture (`settings-default.json`, generated from Rust's `Settings::default()` and asserted on both sides) now pins the real shape.
- The dashboard refreshed only once every few minutes instead of every tick: the push event pre-advanced the "known sequence" so the snapshot fetched for that event (same sequence) was discarded. Fixed with separate applied/pending sequences, in-flight coalescing, and regression tests that reproduce the exact scenario.
- Failures of the Rust→WebView telemetry emit and of the JS subscription are now reported (stderr / UI error state) instead of being silently discarded.

### Security and privacy

- No network activity by default; no telemetry upload, analytics, crash upload or update checks. See `PRIVACY.md`.
- Remote server: OFF by default, loopback-only, token-hashed, read-only, rate-limited, no CORS grants (new test), no credentials in URLs or logs.
- Dependency review: `cargo deny check` (advisories/bans/licenses/sources) **ok**; `pnpm audit` clean after upgrading ECharts 6.0.0 → **6.1.0** (GHSA-fgmj-fm8m-jvvx, moderate XSS). SBOM: `docs/release/sbom/`.
- Untrusted process names render as inert text (new test).

### Known limitations

- **History database growth:** ≈73 MB/hour of raw samples on a 14-core laptop with 180 series; with the default 24 h raw retention the database plateaus near **1.7–1.8 GB**. Bounded, but large — a retention/schema plan is documented in `docs/progress/PERF-2026-10-19.md` and should be decided before general release.
- Memory footprint with the window open: ≈100 MB main process + ≈85 MB WebKit content process (above the proposed 150 MB total target; CPU ≈0.5 %).
- GPU utilisation % and fan RPM are `platform_limited` on macOS (no public non-root source). Bluetooth accessory charge requires a connected accessory exposing a battery characteristic.
- Remote **LAN** mode, TLS and second-device validation are deferred (ADR-0008). Alert actions take **no arguments** (ADR-0007).
- CSV **import**, native **WidgetKit** widget, Windows/Linux runtime behaviour, 4-hour soak, 200 % scaling / screen-reader passes: not done in this candidate.
- Dev-mode notifications attribute to Terminal; bundled builds use the app identifier.

### Artifacts built in this session (macOS arm64, from the phase 09 tree)

| Artifact | Size | SHA-256 |
|---|---|---|
| `APP_NAME_0.1.0_aarch64.dmg` | 5,594,313 B | `e5e5f3d158c44ceb2c925bc99eddb334f838faa3466b66848b33a720341cd4ab` |
| `APP_NAME.app/Contents/MacOS/app` (release binary) | 16,503,616 B | `3b7ee96710e084c373aa0458fa3a6d562ad24541a0287496293231052c802c0e` |

Bundle metadata: `CFBundleIdentifier com.dkvlabs.hostlume`, `CFBundleName Hostlume`, `CFBundleShortVersionString 0.1.0`, `LSMinimumSystemVersion 11.0` (the documented product floor; only macOS 27.0.1 was actually exercised). Signature: `adhoc, linker-signed`; TeamIdentifier not set. Release builds for distribution must be regenerated by CI at tag time (`v0.1.0-rc.1`) with the signing identity, and checksums re-published.

### Named: Hostlume (20.10.2026)

The product name is **Hostlume** (identifier `com.dkvlabs.hostlume`). The bundle, window title, tray, notifications, agent certificate and default host display name all use it; builds from before this date shipped as the `APP_NAME` placeholder (their data directories migrate automatically on first start — see Upgrade below).

### Phase 10 additions (20.10.2026, unreleased) — multi-host preview (FX-022)

- **Other computers (read-only):** pair this console with a few user-owned machines running Hostlume's agent on the local network — **TLS with certificate pinning** (self-signed agent certificate, fingerprint fixed at pairing), **single-use pairing codes** bound to that fingerprint (one attempt, 2-minute window), **per-device revocation**, per-host labels and a versioned capability handshake (ADR-0009). No discovery, no relay, no cloud, **no remote action of any kind**.
- **Honest liveness:** every remote value carries the host identity and both clocks — last-response age on this machine and the agent's timestamp with the measured clock skew; stale/offline hosts keep their last values under explicit warnings, never as live.
- **Per-host history recording (opt-in per host)** into the same local history under the agent's host id; **removing a host** can delete those rows; removal is double-confirmed.
- **LAN mode now starts** — over TLS only (it still refuses to bind without a TLS identity). Settings → Remote shows this machine's certificate fingerprint, the pairing window and the paired-devices list with per-device revoke. A real two-machine LAN validation is still pending (`docs/progress/PHASE-10.md` §9); the verified path is the release bundle as console against a labelled demo TLS agent on loopback (`docs/evidence/phase-10-hosts-live.png`).
- Settings schema moves to **v9** (rows from v1–v8 migrate at load as before). Device tokens are stored only in `remote-credentials.json` (owner-only file) — never in settings, exports or the WebView.
- Fixed since rc1: tray-configuration and launch-at-login patches were accepted but never persisted (phase 06 regression); remote sample ingestion also gained `delete_host_samples`.

| Artifact (rebuilt from the phase 10 tree — **supersedes the hashes above**) | Size | SHA-256 |
|---|---|---|
| `Hostlume_0.1.0_aarch64.dmg` | 6,838,021 B | `e67abc562e62ff32c7ed1ea1bd65668b1800022662476a29eb5004c1542d3ee1` |
| `Hostlume.app/Contents/MacOS/app` (release binary) | 19,428,352 B | `2d2d31224c58256ab8f1e71b26adb9207850e026bb20422b9f3171023ab0a930` |

(The interim multi-host build above these was labelled `APP_NAME`; hashes: dmg `44f7a91e…9e69dc3`, binary `7f81d03d…0a930`.)

### Upgrade, migration and rollback

- **Naming migration:** builds before 20.10.2026 stored data under `com.dkvlabs.appname`; the named build moves that directory's contents into `com.dkvlabs.hostlume` on first start and never overwrites an existing installation there.
- **Settings** live in `settings.sqlite3` with a schema version (now **9**). Rows from any earlier version (1–7) migrate forward automatically at load; the migrated row is written back once. Partial or hand-edited rows load with defaults for missing fields (`#[serde(default)]`), and **Reset settings** restores a complete default row. A newer schema than the binary supports is refused with `UnsupportedSchemaVersion` rather than guessed.
- **History** lives in `history/history.sqlite3` with checksummed ordered migrations (V1 base schema, V2 `recording_sessions.status`). Migrations run transactionally at startup; a database from a *newer* build is refused (`DatabaseTooNew`) and left untouched; checksum drift is refused.
- **Rollback:** reinstall the previous `.app`. Settings from v8 cannot be read by a binary that only knows ≤v7 (it reports `UnsupportedSchemaVersion`); use **Reset settings** before downgrading, or delete `settings.sqlite3` (history is a separate file and is kept). History V2 → a V1-only binary refuses the database (`DatabaseTooNew`); keep a copy of `history/` before downgrading if the samples matter.
- **Naming (20.10.2026):** the app is **Hostlume** with identifier `com.dkvlabs.hostlume`. On first start it moves the previous `com.dkvlabs.appname` data directory's contents across (never overwriting an existing installation); if both directories exist, the old one is left in place for manual inspection.
- Uninstalling the app does not delete the data directory (`~/Library/Application Support/com.dkvlabs.hostlume/`); remove it manually to erase all data.
- Updates never enable the remote server or local actions, never reset privacy choices and install no helpers (release-distribution.md §7). No auto-updater is configured in this candidate.
