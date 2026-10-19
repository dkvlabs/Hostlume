# ADR-0005: CI platform matrix and validation ladder

Status: accepted
Date: 19.10.2026
Owner: principal engineer (Phase 00 agent)
Related features: all (release gates), FX-020

## Context

Only one machine is available in the current working environment (macOS arm64, M3 Max). Windows and Linux are **NOT TESTED — no hardware/runner** (RISK_REGISTER R10). Requirements demand native builds/tests on each supported target, and AGENTS.md forbids calling cross-compilation "validation". Phase 01 exit gate: "build/test on all available native targets".

## Decision

1. **CI provider:** GitHub Actions (repo is git-hosted; workflows under `.github/workflows/`).
2. **Intended matrix** (runner labels are *intended* — exact current availability must be re-verified against https://docs.github.com/en/actions/using-github-hosted-runners at Phase 01 setup, because runner images change):
   - `macos-14`-class arm64 runner: `cargo fmt/clippy/test`, frontend lint/typecheck/test/build, `tauri build` smoke (unsigned).
   - `windows-2022`-class x64 MSVC runner: same gates; WebView2 present on runner images (verify).
   - `ubuntu-22.04`/`ubuntu-24.04`-class x64 runner: same gates + WebKitGTK 4.1 system deps installed explicitly; headless smoke only (no tray/GPU on runners).
3. **What CI proves vs what it cannot:** CI proves compilation, deterministic logic tests (contract fixtures), packaging artifacts, and headless startup. CI runners have no batteries, limited/no sensors, virtualized GPUs — collector integration evidence on real hardware remains a **manual gate** recorded in `docs/progress/` with the testing-qa.md §8 evidence template (CPU model, OS, driver, display server, date, capabilities discovered).
4. **Intel macOS:** intended via `macos-13`-class runner *if still offered*; otherwise documented as unvalidated and dropped from promised targets (SUPPORTED_TARGETS.md).
5. **Secrets:** signing keys/notarization credentials live only in CI secret stores, never in the repo; workflows pin action versions and use least-privilege `permissions:` blocks (release-distribution.md §3).
6. **PR gate:** fmt + clippy `-D warnings` + `cargo test --workspace` + frontend lint/typecheck/test/build on all three OSes before merge; nightly/weekly full `tauri build` matrix to catch packaging drift.

## Alternatives considered

- **Local-only validation (macOS machine):** rejected — cannot produce honest Windows/Linux claims; the whole product premise is cross-platform truthfulness.
- **Self-hosted hardware lab now:** deferred — real hardware for manual gates will be recruited as available; CI carries the automated load first.
- **Cross-compilation from macOS:** rejected as validation (AGENTS.md §7); acceptable only as an extra artifact convenience, never as test evidence.

## Consequences

Phase 01 must add: workspace CI workflow file, per-OS dependency setup steps, artifact upload, and the first honest SUPPORTED_TARGETS.md status flips (Windows x64 / Linux x86_64 from "NOT tested" to "CI-validated (headless)" — with wording that never implies hardware sensor validation).

## Evidence

- Local toolchain recorded: rustc 1.99.1, cargo 1.99.1, node v24.22.0, pnpm 10.33.2, Xcode 26 (SUPPORTED_TARGETS.md).
- Tauri 2 prerequisites per OS (WebView2, WebKitGTK 4.1, MSVC): https://v2.tauri.app/start/prerequisites/ (fetched 19.10.2026).
- No Windows/Linux machine present in this environment — stated, not assumed (Phase 00 precondition).

## Follow-up

Verify runner labels/images and macOS Intel availability in the first week of Phase 01; add a `docs/progress/CI-BASELINE.md` record with the first green run IDs. Manual hardware matrix (testing-qa.md §3) is recruited per phase: battery-present vs battery-absent hosts before Phase 05, GNOME/KDE Wayland sessions before Phase 06.
