# Supported targets — validated vs intended

Date: 19.10.2026. Phase 00 output. Rule: a target is **validated** only when a build or tests actually ran on that OS/hardware; everything else is **intended** or **not promised**. Cross-compiling is never hardware validation (AGENTS.md §7).

## Framework baseline (official docs, verified 19.10.2026)

Tauri 2 prerequisites (https://v2.tauri.app/start/prerequisites/):

- macOS: Catalina 10.15+; system WebKit via Xcode.
- Windows: Windows 7+ per Tauri docs, with Microsoft Edge WebView2 — preinstalled on Windows 10 1803+; MSVC toolchain (`*-pc-windows-msvc`) + Microsoft C++ Build Tools required.
- Linux: WebKitGTK **4.1** (`libwebkit2gtk-4.1-dev` / `webkit2gtk-4.1` / `webkit2gtk4.1-devel`); distros listed by Tauri include Debian, Arch, Fedora, openSUSE, Ubuntu-family bases.
- Rust via rustup; Node LTS for the frontend.

Tauri crate: **2.12.4 stable** pinned (3.0.0-alpha.5 exists and is explicitly out of scope).

## Status table

| Target | Status | Basis |
|---|---|---|
| macOS arm64 (Apple Silicon), macOS 13+ | **Spike-validated** (collectors only) | Real telemetry + temperature spikes ran on macOS 27.0.1 / M3 Max, 19.10.2026 (docs/evidence/FEASIBILITY_SPIKES.md). No Tauri app built yet — that is Phase 01 |
| macOS arm64, minimum OS version | **Intended: macOS 11 (Big Sur)+** | Tauri supports 10.15+; we pick 11 as product floor to bound WebKit/API variance. Must be re-confirmed when the first Tauri build runs (Phase 01) |
| macOS x86_64 (Intel) | **Intended, NOT validated — no Intel hardware this session** | Do not promise until an Intel (or CI x86_64 macOS) build + smoke test exists |
| macOS universal2 | **Not promised** | release-distribution.md §2: only after both slices validated |
| Windows 11 x64 | **Intended, NOT tested — no hardware/runner** | WebView2 present by default; PDH ≤1 Hz guidance verified via MS docs 19.10.2026 |
| Windows 10 x64 (1803+) | **Intended, NOT tested** | WebView2 preinstalled from 1803 per Tauri docs; older builds may need the Evergreen bootstrapper — verify at Phase 01 |
| Windows ARM64 | **Not promised** | release-distribution.md §2 conditional |
| Linux x86_64 — Ubuntu LTS (GNOME, Wayland) | **Intended, NOT tested — no hardware/runner** | WebKitGTK 4.1 available on current LTS |
| Linux x86_64 — Fedora (GNOME/KDE, Wayland) | **Intended, NOT tested** | WebKitGTK 4.1 packaged |
| Linux x86_64 — KDE Plasma (Wayland) | **Intended, NOT tested** | tray via StatusNotifier assumed present — must be spiked (Phase 06) |
| Linux x86_64 — X11 sessions | **Intended (secondary), NOT tested** | |
| Linux ARM64 | **Not promised** | conditional per release-distribution.md §2 |
| Flatpak / Snap / Mac App Store / Microsoft Store | **Out of v1 channel scope** | Direct distribution first (release-distribution.md §1); sandboxing affects sensors/remote server |

## Toolchain actually present on the Phase 00 machine

| Tool | Version | Note |
|---|---|---|
| macOS | 27.0.1 (26A434), arm64 | Apple M3 Max, Mac15,11 |
| rustc / cargo | 1.99.1 (Homebrew) | consider rustup-managed toolchain in CI for target flexibility |
| node | v24.22.0 | ≥ LTS, OK for Vite 8 |
| pnpm | 10.33.2 | |
| Xcode | full Xcode 26 (`xcode-select -p` → /Applications/Xcode.app) | required for macOS/WidgetKit |
| git | 2.56.0 | |

Windows and Linux toolchains: **NOT PRESENT on this machine** — first verification happens on CI runners (ADR-0005) or loaned hardware.

## Validation obligations before any public OS-support claim

1. Phase 01: Tauri 2 app builds + tests green on macOS arm64 (local), Windows x64 and Linux x86_64 (CI runners).
2. Phase 02: live CPU/memory/disk/network parity cross-check against OS tools on each available target (testing-qa.md §4).
3. Phase 05+: per-host sensor capability matrix from real hardware, with `unsupported` states where hardware/drivers are silent.
4. Phase 09: signed installers built and manually launched per target before any release claim.

## Phase 09 release-candidate status (19.10.2026)

| Target | Status after phase 09 | Basis |
|---|---|---|
| macOS arm64 — macOS 27.0.1, Apple M3 Max | **Release candidate (unsigned)** — `.app` + `.dmg` built, DMG checksum verified and mounted, launched three times, 30 + 10 min soaks, OS quit exit 0, no panics | docs/progress/PHASE-09.md, docs/progress/PERF-2026-10-19.md, CHANGELOG.md |
| macOS arm64 — declared minimum | `LSMinimumSystemVersion` **11.0** (documented product floor) | Not tested below 27.0.1; the floor is a declaration, not a validated minimum |
| macOS arm64 — signing | **Blocked**: ad-hoc linker signature only; `spctl --assess` fails; no notarization | Owner's Apple Developer identity + notarization run required before distribution |
| macOS x86_64 (Intel) | **Not built, not claimed** | No Intel hardware/runner in any phase |
| Windows 11/10 x64 | **Not built, not claimed** | Code compiles behind `cfg(windows)` gates in earlier phases; no native build, install or runtime evidence (ADR-0005) |
| Linux x86_64 (GNOME / KDE) | **Not built, not claimed** | Fixture-tested parsers only; WebKitGTK/tray/packaging never exercised (ADR-0005) |

Rollout posture (phase 09 §5): ship the macOS arm64 candidate once signed, labelled as the only supported target; other targets stay "intended" with transparent gating until native evidence exists.
