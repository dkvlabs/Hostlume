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
