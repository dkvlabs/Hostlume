# Phase 01 — Tauri foundation, contracts, settings and CI

Date: 19.10.2026
Commit: this commit (Phase 01; Phase 00 research artifacts landed in the immediately preceding commit)
Feature IDs: FX-020 (→ `in_progress`; retention/recording/export and release polish remain in later phases per the matrix split 01/09)
Status: complete (awaiting owner acceptance)

## Changes implemented (code map)

Workspace (`Cargo.toml`, resolver 2, edition 2021, GPL-3.0-only):

- `crates/monitor-core` — the telemetry contract as compiled Rust (ADR-0001):
  - `Availability`, `MetricUnit`, `MetricValue`, `MetricReading` with `check_invariants()` enforcing the null-not-zero rule in both directions and rejecting non-finite numbers (`metric.rs`).
  - `SystemSnapshot` (schema v1, host UUID, generation, sequence, UTC timestamp, metrics, capabilities revision) with full validation (`snapshot.rs`).
  - `MetricCapability` / `CapabilityReport` incl. the honest `COLLECTOR_NOT_IMPLEMENTED` constructor (`capability.rs`).
  - `Collector` trait, typed `CollectorError` (`collector.rs`, `error.rs`), schema-version guard (`check_schema_version`).
  - `Clock` trait + `SystemClock` (UTC epoch ms + monotonic ns; monotonic anchored via `OnceLock`), self-contained ISO-8601 formatter tested against `date(1)`-verified anchors (`clock.rs`).
  - Canonical metric-id registry from contract §3 (`families.rs`), unit parser with bounded aliases (`units.rs`).
  - Golden serialization fixture `tests/fixtures/snapshot_v1.json` + 8 integration tests (round trip, invariants, guard).
- `crates/monitor-platform-macos` / `-windows` / `-linux` — Phase 01 seams that compile on **every** OS via `cfg` gates: on the native OS capabilities report `COLLECTOR_NOT_IMPLEMENTED`, cross-compiled they report `WRONG_PLATFORM`; `sample()` returns an empty honest set (native) or a typed `Unsupported` error (foreign). Real collectors arrive in Phases 02/05.
- `apps/desktop/src-tauri` — Tauri 2.12.4 shell:
  - `settings.rs`: Settings v1 (appearance system/light/dark, compact mode, polling 1–60 s default 3 s, close-to-tray flag, locale registry en/ro), all-or-nothing `SettingsPatch` with `deny_unknown_fields`, schema version guard.
  - `settings_store.rs`: bundled rusqlite, WAL, **checksummed ordered migrations** (`schema_migrations`, refuse-on-drift, refuse-future-DB), app_settings upsert, installation host UUID (random, never MAC/serial), 100% parameter-bound SQL.
  - `scheduler.rs`: the single empty scheduler — one loop, settings-driven interval, sequence bumps, publishes honest empty snapshots, emits `telemetry://snapshot` push payloads, graceful cancellation (watch channel); emitter seam keeps it unit-testable.
  - `commands.rs`: `get_snapshot` (typed `NO_SNAPSHOT_YET` before first tick), `get_capabilities`, `get_settings`, `update_settings` — every input validated in Rust; structured error payloads carry i18n `messageKey`s.
  - `lib.rs`: single-instance plugin (focus existing window), capability report aggregated from the three platform adapters (`WRONG_PLATFORM` rows dropped), scheduler lifecycle stop on window close.
  - `tauri.conf.json`: strict CSP (`default-src 'self'`, no wildcard, no remote origins), window 1100×720 / min 960×650, `APP_NAME` placeholder branding.
  - `capabilities/default.json`: **`core:default` only** — no shell/fs/http/global-shortcut/process permissions.
  - `tests/config_security.rs`: shipped-config assertions (CSP strictness, capability allowlist, window minimums, icon set).
- `apps/desktop` frontend (React 19.3.1 + TypeScript **5.9.3** + Vite 8 + Tailwind 4):
  - Design tokens from design.md §2 as CSS variables + `@theme inline` utilities; dark canonical, light override, `prefers-reduced-motion` respected, visible focus, tabular-numeral `.numeric` class.
  - `lib/ipc/`: typed mirrors, **runtime guards** (validate every payload incl. the availability invariant), single `ipcClient` adapter, `IpcCommandError` with code/messageKey.
  - `stores/`: `telemetryStore` (lastKnown/stale differentiation, freshness budget 15 s, connection state), `settingsStore` (optimistic update with rollback on rejection).
  - `components/`: shadcn-idiom primitives (button/card/badge/select/switch/slider/label/separator/tooltip on Radix, token-styled) + `MetricCard` & `MetricAvailability` implementing all availability states (dash for missing — never `0`; icon+text chips, not color-only).
  - `app/`: sidebar with 7 pages, status strip, `useTelemetry` hook (snapshot fetch + push subscribe + staleness timer), global shortcuts `Ctrl/Cmd+,` and `Ctrl/Cmd+1..7`.
  - `features/`: Overview (honest waiting/empty states), placeholder pages ("Not implemented yet"), Settings page wiring every v1 preference.
  - i18n: message-key catalogs `en` + `ro`, locale registry shared with the Rust bounds.
  - **No fake provider anywhere in release code** — mocks exist only under `src/**/*.test.*` with `vi.mock`.
- `scripts/generate-icon.mjs` — original icon artwork generated procedurally (pure Node PNG encoder, design-token colors); `tauri icon` derived icns/ico/pngs; unused mobile/store variants removed. No downloaded assets.
- CI: `.github/workflows/ci.yml` — `rust-checks` (fmt/clippy/test) × [macos-15, windows-2025, ubuntu-24.04], `frontend-checks`, `desktop-debug-build` (tauri debug × 3 OS), `dependency-licenses` (cargo-deny); `deny.toml` license policy. Runner labels re-verified (macos-14 deprecated) — see ADR-0005 Amendment 1.
- `.gitignore` extended for target/node/dist (pre-existing user entries preserved).

## Verification

| Environment | Command / scenario | Result | Evidence |
|---|---|---|---|
| macOS arm64 | `cargo fmt --all --check` | PASS | this run |
| macOS arm64 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS | this run |
| macOS arm64 | `cargo test --workspace` | PASS — **34 tests, 0 failed** (core serialization 8, core unit 4, platform gates 3×2, app settings 5, settings store 6 incl. migration-preserve/checksum-drift/future-DB, scheduler tick+graceful-stop 1, config security 4) | this run |
| macOS arm64 | `pnpm lint` | PASS | this run |
| macOS arm64 | `pnpm typecheck` (tsc --noEmit, strict) | PASS | this run |
| macOS arm64 | `pnpm test` (vitest+RTL) | PASS — **49 tests, 0 failed** (guards incl. invariant rejections, stores stale/lastKnown + rollback, theme resolution, MetricCard/MetricAvailability all states, units, App shell nav/shortcuts) | this run |
| macOS arm64 | `pnpm build` (vite) | PASS — dist 385 KB JS / 18 KB CSS | this run |
| macOS arm64 | `pnpm tauri build --debug --no-bundle` | PASS — `target/debug/app` | this run |
| macOS arm64 | Launch smoke (binary): run 6 s, `kill -0` alive, settings DB created, `schema_migrations` = [1], `host_identity` count = 1, then SIGTERM → terminated | PASS | this run |
| macOS arm64 | `pnpm tauri build --debug` (full bundle) | PASS — `APP_NAME.app` (32 MB) + `APP_NAME_0.1.0_aarch64.dmg` (7.7 MB) | this run |
| macOS arm64 | Launch smoke (bundle): `open APP_NAME.app`, process alive after 5 s, AppleScript quit → exited cleanly | PASS | this run |
| macOS arm64 | `cargo deny check licenses` (cargo-deny 0.20.2, brew) | PASS — `licenses ok` | this run |
| Windows x64 | any command | NOT_RUN — no hardware/runner; covered by CI matrix once pushed | R10 / ADR-0005 |
| Linux x64 | any command | NOT_RUN — no hardware/runner; covered by CI matrix once pushed | R10 / ADR-0005 |

Compatibility decisions verified against official sources 19.10.2026: TS 5.9.3 pinned (TS 7 blocked by `typescript-eslint` peer `<6.1.0`), vitest 5 ↔ vite 8 peer ✓, @testing-library/react 16 ↔ React 19 ✓, @tailwindcss/vite 4.3.3 ↔ vite ^8 ✓, runner labels via actions/runner-images README.

## Platform capability findings

No new hardware findings this phase (collectors are Phase 02+). The capability **seam** is live: `get_capabilities` returns 20 honest `COLLECTOR_NOT_IMPLEMENTED` rows derived from the platform adapters on this host (macOS).

## Security and data integrity

- WebView is untrusted input: commands validate in Rust; IPC payloads runtime-validated in TS guards before reaching stores; settings patches `deny_unknown_fields`.
- Least privilege: capabilities grant `core:default` only; no shell/fs/http plugins; CSP strict with no wildcards or remote origins — asserted by `tests/config_security.rs` on every platform.
- Settings SQL: parameter-bound only (no format!/concatenation SQL anywhere); migrations transactional with checksum refusal for drifted or future databases.
- Host identity is a random installation UUID; no MAC/serial/hostname stored or displayed.
- No mock provider exists in release code; mocks confined to `*.test.*` files. Placeholder UI renders dashes + availability states, never zeros.
- Original icon artwork generated procedurally; no reference-app assets, names, or visuals anywhere.
- `close_to_tray` is stored but documented as inactive until Phase 06 (tray); closing the window exits — the honest current behavior, stated in Settings UI.
- No remote listener exists (nothing to audit); Phase 08 gate owns the security test table for it.

## Blockers / untested

- **Windows/Linux:** no local hardware. CI matrix is authored and its pieces verified individually, but the first actual green run requires pushing to the remote. SUPPORTED_TARGETS.md stays "NOT tested" for those rows until then (ADR-0005 Amendment 1); `docs/progress/CI-BASELINE.md` will record the first green run IDs.
- **Window-content screenshot evidence:** the app was launched and quit cleanly headlessly; pixel-level shell review is a manual step for the owner on this machine (vitest asserts the shell content).
- **E2E tooling:** not adopted (deliberately); testing-qa.md §1 requires verifying a macOS-compatible approach first — owning decision: Phase 02.
- echarts intentionally not yet adopted (no charts exist in Phase 01); will get its license/NOTICE review entry when Phase 02 adopts it.

## Feature matrix edits

- FX-020: `planned` → **`in_progress`** — evidence: settings persist + migrate (settings_store tests: ordered migration preserves data, checksum drift refused, future DB refused); theme dark/light/system + persistence (theme tests, App test); i18n registry en/ro (dictionary + registry tests); polling bounds enforced in Rust and mirrored in UI (bounds tests); UI states per design.md §15 (MetricCard/MetricAvailability suites). Retention/recording/export rows stay open for Phases 03/08/09.
- All other FX rows unchanged (no evidence yet).

## Gate review (01-foundation.md §4)

- [x] Actual app starts on available native OS runner, displays app shell and system/empty loading state — launch smoke binary + bundle PASS (macOS arm64).
- [x] Three OS platform adapter crates compile with `cfg` gates and return explicit `unsupported` — proven on macOS build; Windows/Linux compilation is CI's job (authored, labels verified).
- [x] IPC payloads typed, versioned and runtime-validated; initial settings persist and migrate.
- [x] UI has reusable `MetricCard` and `MetricAvailability` components with all states (component tests cover all six availabilities + no-zero invariants).
- [x] Theme, density, nav and accessibility foundations in place (tokens, compact mode, keyboard nav + shortcuts, focus, reduced motion, i18n keys).
- [x] CI config and lockfiles committed; known untested native targets listed (above + SUPPORTED_TARGETS.md unchanged until CI green).
- [x] No mocked metrics in any release code path.

**Handoff:** "Phase 01 completed with green local verification on macOS arm64. Phase 02 (portable collectors & dashboard, FX-001–006) may start after owner acceptance; CI baseline on Windows/Linux lands with the first push."
