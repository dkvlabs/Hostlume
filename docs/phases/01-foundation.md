# Phase 01 — Tauri foundation, contracts, settings and CI

**Owns:** FX-020 foundation; unblocks all later phases.  
**Outcome:** A buildable Tauri 2 + Rust + React project shell with typed IPC and honest `no sample yet` UI states.

## 1. Preconditions

- Phase 00 signed off and target OS list documented.
- Verify toolchains and current dependency versions from official docs; pin through lockfiles.

## 2. Implementation sequence

1. Initialize Git-aware workspace; preserve docs and `AGENTS.md` at root. Add `apps/desktop` and Rust `crates/` skeletons based on `architecture.md` (start with only required crates).
2. Create Tauri 2 application, React/TS/Vite frontend, Tailwind/shadcn setup, icon placeholders (original assets), theme tokens from `design.md`.
3. Implement Rust `MetricReading`, `Availability`, `SystemSnapshot`, `CapabilityReport`, `Collector` contract, typed errors, monotonic and UTC clock wrappers. Add schema serialization fixtures.
4. Implement Tauri app state and restricted commands: `get_snapshot`, `get_capabilities`, `get_settings`, `update_settings`; do not enable `fs-all` / `shell-all` permissions.
5. Create React typed `ipcClient` service and Zustand UI stores with `lastKnown` and `stale` differentiation; no production fake provider.
6. Build base navigation: Overview, Applications, Storage, Network, Sensors, Compare, Settings. Render empty **honest** data states for not-yet-implemented pages.
7. Implement Settings v1: appearance dark/light/system, compact mode, polling interval bounds (safe min/max), close-to-tray preference, locale key registry, settings schema migration.
8. Add app lifecycle: single-instance behavior if supported by chosen plugin, predictable close/quit handling, graceful cancellation of empty scheduler.
9. Create CI for native OS build matrix (runner targets may be staged), TypeScript checks, cargo checks/tests/lint, dependency/license review jobs.
10. Create `docs/progress/PHASE-01.md` with code map and tested targets.

## 3. Suggested technical tests

- Rust: JSON round trip for availability, null/nonavailable invariant, metric unit parser, schema version guard, settings migration.
- UI: theme persistence, navigation keyboard focus, inactive views show `Not implemented yet` in development and not simulated values, IPC error display.
- Security: capabilities only expose intended commands, no arbitrary shell/FS by default.
- Build: `cargo fmt --all --check`, `cargo test --workspace`, frontend lint/typecheck/build, Tauri native app launch. Fix script names based on created package scripts.

## 4. Deliverables and acceptance

- [ ] Actual app starts on available native OS runner, displays app shell and system/empty loading state.
- [ ] Three OS platform adapter crates/interfaces compile with `cfg` gates (platform-specific code can be empty but must return explicit `unsupported`).
- [ ] IPC payloads typed, versioned and runtime-validated; initial settings persist and migrate.
- [ ] UI has reusable `MetricCard` and `MetricAvailability` components with all states.
- [ ] Theme, density, nav and accessibility foundations in place.
- [ ] CI config and lockfiles committed; known untested native targets listed.
- [ ] No mocked metrics in any release code path.

**Do not:** implement a visually complete fake dashboard, download original icons, enable remote listener, or commit placeholders reported as finished collectors.
