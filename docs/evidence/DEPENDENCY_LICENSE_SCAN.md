# Dependency and license scan — Phase 00

Date inspected: 19.10.2026. Sources: crates.io registry API (`https://crates.io/api/v1/crates/<name>`) and npm registry (`https://registry.npmjs.org/<pkg>/latest`) — machine-readable metadata, not marketing pages.
Policy (AGENTS.md §4): use existing libraries only after checking maintenance, APIs, platform support and licenses; lock versions; **defer unreviewed libraries**.

Legend — Redistribution: ✅ acceptable for a closed-source distributed app under the listed license terms; ⚠️ needs legal review before adoption; ❌ rejected.

## Rust crates (verified this session)

| Crate | Latest verified | License | Updated | Redistribution | Role / decision |
|---|---|---|---|---|---|
| `tauri` | 2.12.4 stable (3.0.0-alpha.5 exists) | Apache-2.0 OR MIT | 2026-10-14 | ✅ | App shell. **Pin to 2.x stable**; 3.0 alphas explicitly out of scope |
| `sysinfo` | 0.39.6 | MIT | 2026-07-09 | ✅ | Portable CPU/memory/disk/network/process/temperature baseline. Verified by live spike (FEASIBILITY_SPIKES.md). No battery module |
| `rusqlite` | 0.40.2 | MIT | 2026-08-08 | ✅ | SQLite layer (ADR-0002); use `bundled` feature to control SQLite version |
| `axum` | 0.8.9 | MIT | 2026-04-14 | ✅ | Optional read-only remote server (Phase 08) |
| `trash` | 5.2.9 | MIT | 2026-09-13 | ✅ | Move-to-Trash across macOS/Windows/Linux (FX-008). License/behavior still needs per-OS manual validation in Phase 04 |
| `battery` | 0.7.8 | Apache-2.0 OR MIT | **2020-11-01** | ❌ | **Rejected — unmaintained (~6 years).** Battery via own IOKit/upower/Win32 adapters (ADR-0004) |

Not yet reviewed / to pin in the phase that needs them (do **not** adopt before review): `tokio` (transitively via axum/tauri — license Apache/MIT, verify exact version at lock time), `serde`/`serde_json`, `thiserror`/`anyhow`, `objc2`/`objc2-io-kit` family (used inside sysinfo; direct use for macOS battery/sensors needs its own check), `windows`/`windows-sys` crate (Microsoft, MIT OR Apache-2.0 — verify version when Windows adapters start), Tauri plugins (`tauri-plugin-autostart`, `tauri-plugin-notification`, `tauri-plugin-updater`, `tauri-plugin-single-instance` — verify each at Phase 01/06/07), migration helpers (`refinery` etc. — custom runner chosen instead, ADR-0002), `csv` export crate (Phase 08).

**Sensor/GPU vendor SDKs:** none adopted. Any vendor graphics/sensor SDK requires a documented license + redistribution review before use (architecture.md §platform boundaries; reference-audit.md qualification 3 — the reference product's sensor plugin must not be shipped or called).

## Frontend packages (verified this session)

| Package | Latest verified | License | Redistribution | Notes |
|---|---|---|---|---|
| `react` | 19.3.1 | MIT | ✅ | UI framework |
| `vite` | 8.3.4 | MIT | ✅ | Build tool |
| `typescript` | 7.0.2 | Apache-2.0 | ✅ | Verify Vite/plugin compatibility with TS 7 at Phase 01; pin exact version in lockfile |
| `tailwindcss` | 4.3.3 | MIT | ✅ | v4 line; shadcn/ui supports it |
| `shadcn` (CLI) | 4.22.1 | MIT | ✅ | Components are copied into the repo → become our code; keep attribution per MIT |
| `echarts` | 6.1.0 | Apache-2.0 | ✅ | Charts; Apache-2.0 requires NOTICE retention in distribution |
| `zustand` | 5.0.15 | MIT | ✅ | Presentation-only state (architecture.md §state boundaries) |

Candidates to evaluate at Phase 01 (unreviewed, not adopted): test runner (`vitest`), E2E (`playwright` — testing-qa.md warns not to assume a Tauri 2 WebDriver integration exists on macOS without testing), lint (`eslint` + typescript-eslint), i18n (message-key format may be hand-rolled per requirements.md).

## OS-level entitlements / signing (not libraries, but license-adjacent)

- macOS: Developer ID signing + notarization for direct distribution; WidgetKit extension needs its own target + App Group review (release-distribution.md §4). No privileged-helper entitlement is planned (security-privacy.md §8). Mac App Store sandbox compatibility is explicitly **not** promised.
- Windows: code-signing certificate and SmartScreen reputation are external release concerns (release-distribution.md §5); no kernel driver in v1.
- Linux: no special entitlements; polkit only if a future optional helper were ever approved.

## Outcome

No license blocker found for the chosen stack. All core dependencies are MIT or Apache-2.0 (OR-MIT), actively maintained (2026 updates), and acceptable for redistribution in a signed closed-source desktop app, subject to Apache-2.0 NOTICE handling (tauri, echarts, typescript). One candidate (`battery`) rejected for maintenance. Vendor sensor/GPU SDKs deferred pending per-SDK legal review.

---

# Phase 01 additions (adopted and verified 19.10.2026)

Dependencies actually adopted by the Phase 01 code (lockfile-pinned). All checked against registry metadata the same day.

## Rust crates newly adopted

| Crate | Pinned | License | Redistribution | Role |
|---|---|---|---|---|
| `tauri` | 2.12.4 | Apache-2.0 OR MIT | ✅ | App shell (pre-verified Phase 00) |
| `tauri-build` | 2.7.3 | Apache-2.0 OR MIT | ✅ | Build script |
| `tauri-plugin-single-instance` | 2.5.4 | Apache-2.0 OR MIT | ✅ | Single-instance lifecycle (phase 01 task 8) |
| `rusqlite` (+ `libsqlite3-sys` bundled) | 0.40.2 | MIT | ✅ | Settings store (ADR-0002) |
| `serde` / `serde_json` | 1.x (lockfile) | MIT OR Apache-2.0 | ✅ | Contract serialization |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | ✅ | Typed errors |
| `sha2` | 0.10.x | MIT OR Apache-2.0 | ✅ | Migration checksums |
| `uuid` | 1.x | Apache-2.0 OR MIT | ✅ | Installation host UUID (contract §8) |
| `tokio` | 1.53.3 | MIT | ✅ | Scheduler timing (features: macros/rt/sync/time) |

## Frontend packages newly adopted

| Package | Pinned | License | Redistribution | Role |
|---|---|---|---|---|
| `typescript` | **5.9.3** | Apache-2.0 | ✅ | **TS 7.0.2 NOT adopted:** `typescript-eslint` 8.71.2 peers `typescript >=4.8.4 <6.1.0` → TS 7 blocked until tooling support; revisit per phase |
| `@vitejs/plugin-react` | 6.1.2 | MIT | ✅ | Vite React plugin |
| `@tailwindcss/vite` | 4.3.3 | MIT | ✅ | Tailwind v4 Vite plugin |
| `@radix-ui/react-{select,switch,slider,tooltip,label,slot}` | 2.4.1 / 1.4.1 / 1.6.1 / 1.4.1 / 2.1.17 / 1.5.0 | MIT | ✅ | shadcn/ui primitives (components hand-adapted into repo → our code) |
| `class-variance-authority` | 0.7.1 | Apache-2.0 | ✅ | Component variants (NOTICE in dist) |
| `clsx` / `tailwind-merge` | 2.1.1 / 3.7.0 | MIT | ✅ | Class utilities |
| `lucide-react` | 1.59.0 | ISC | ✅ | Icons (ISC → retain copyright notice) |
| `vitest` | 5.0.5 | MIT | ✅ | Frontend test runner (vite ^8 peer ✓) |
| `jsdom` | 30.1.3 | MIT | ✅ | Test DOM |
| `@testing-library/react` / `user-event` / `jest-dom` | 16.3.3 / 14.6.8 / 7.0.1 | MIT | ✅ | Component tests |
| `eslint` + `typescript-eslint` | 10.13.0 + 8.71.2 | MIT | ✅ | Lint (dev only) |
| `@tauri-apps/cli` / `@tauri-apps/api` | 2.12.3 / 2.12.4 | Apache-2.0 OR MIT | ✅ | Tauri tooling + IPC |

**E2E (`playwright` etc.):** still not adopted — deferred; testing-qa.md §1 requires verifying a macOS-compatible approach before adoption.

## Enforcement

`deny.toml` (cargo-deny) gates Rust licenses in CI; run locally 19.10.2026: `cargo deny check licenses` → **licenses ok**. npm license review remains manual per-phase (this file); no copyleft dependency entered the frontend tree (GPL-3.0 is the project's own root LICENSE only).

## Phase 09 release review (19.10.2026)

Commands and exact outcomes (cargo-deny 0.20.2, pnpm 10.33.2):

| Check | Command | Result |
|---|---|---|
| Rust advisories (RustSec, fetched live) | `cargo deny check advisories` | **ok** — 0 vulnerabilities, 0 yanked; 4 `unmaintained` notices ignored with reasons in `deny.toml` (below) |
| Rust licenses | `cargo deny check licenses` | **ok** (allow list unchanged; 1 "license not encountered" warning for an allow-list entry no crate uses) |
| Rust sources | `cargo deny check sources` | **ok** — crates.io only; `unknown-git`/`unknown-registry` denied |
| Rust bans | `cargo deny check bans` | **ok** — 20 duplicate-version warnings (policy `warn`); wildcard check now passes because every workspace crate declares `publish = false` (they were previously treated as crates.io-publishable, where path deps are illegal) |
| npm vulnerabilities | `pnpm audit` | **No known vulnerabilities found** after the ECharts upgrade below |
| SBOM | `node scripts/generate-sbom.mjs` | `docs/release/sbom/sbom-0.1.0.cdx.json` (CycloneDX 1.5; 515 crates + 262 npm packages, 57 of them runtime) + `docs/release/sbom/README.md` license tallies |

### Actioned

| Package | Was | Now | Why |
|---|---|---|---|
| `echarts` | 6.0.0 | **6.1.0** (Apache-2.0, published 2026-05-19) | `pnpm audit`: GHSA-fgmj-fm8m-jvvx, moderate XSS in `<6.1.0`. Typecheck/lint/108 vitest tests/`pnpm build` re-run green; Compare page API (`init`/`setOption`/`resize`/`dispose`) unchanged within 6.x |

### Accepted unmaintained notices (transitive, Linux build stack; re-check each phase)

| Advisory | Crate | Path | Why accepted |
|---|---|---|---|
| RUSTSEC-2024-0370 | `proc-macro-error` 1.0.4 | `glib-macros` 0.18 ← gtk (Linux Tauri stack) | compile-time proc macro only; no runtime code shipped |
| RUSTSEC-2026-0370 | `zvariant` 5.x | `zbus` 5.19 ← `notify-rust` (tauri-plugin-notification), `tauri-plugin-single-instance` | Linux D-Bus only; fix requires upstream plugins to move to zbus 6; no vulnerability, "unmaintained" status |
| RUSTSEC-2026-0371 | `zbus_names` 4.x | same | same |
| RUSTSEC-2026-0372 | `zvariant_derive` 5.x | same | same |

These do not affect the macOS release candidate's runtime; they are tracked here so the Linux build (not validated in this release) inherits the review.

### Native sensor SDKs

None bundled: macOS sensors are read through public IOKit/`system_profiler` interfaces (ADR-0004); no vendor GPU SDKs, kernel extensions or privileged helpers are shipped (security-privacy.md §8). Nothing from the reference application (ImageTasks) is present in assets, code or SBOM.
