# Phase 09 — Hardening, QA, release pipelines and docs

**Owns:** all P0/P1 features + FX-020 completion.  
**Outcome:** Evidence-backed release candidate(s), not just a compiled development project.

## 1. Preconditions

- Phases 00–08 accepted; all known deviations and `platform_limited` metrics cataloged.
- Real release target machines/runners and signing strategy available or explicitly documented as blocked.

## 2. Implementation and QA sequence

1. **Feature audit:** review each FX/FR row against code, native behavior, tests and known OS limitations; no placeholder cards listed as verified.
2. **Native build grid:** build and install on macOS Apple Silicon, Intel if claimed, Windows x64, GNOME and KDE target distro(s). Collect logs and package metadata. Hardware unavailable → release claim excluded.
3. **System lifecycle:** first run, login launch, close-to-tray, quit, sleep/resume, interface hotplug, removable drive unplug, GPU driver missing, DB full/corrupted, settings reset.
4. **Safety checks:** protected process rejection; PID reuse; scan symlink/loop/permission failures; Trash cancel/failure; alerts run actions only when local approved.
5. **Remote threat review:** OFF by default; LAN needs opt-in/token/TLS; rotate token, stress backpressure, review projection; no remote action routes.
6. **Performance:** 30m/4h idle RSS+CPU on named hardware, CPU usage with open charts, high-history database, large folder scan, multi-client remote stream.
7. **Accessibility and i18n:** keyboard navigation, high contrast, screen reader, chart data table, min-window size, 200% scaling, English/RO number formatting and reduced motion.
8. **Dependencies:** current Tauri updater and OS plugin compatibility, vulnerability checks, license/SBOM review including native sensor SDKs.
9. **Branding:** replace APP_NAME placeholder, unique app IDs, original icon/graphics, legal notices/Privacy document and support links.
10. **Sign/package:** macOS code signing/notarization (and widget/helper extensions), Windows signing decision and installer smoke test, Linux package dependencies, optional updater signatures.
11. **Release notes:** exact supported OS/architectures/hardware sensors and known limitations; user-facing data privacy and remote opt-in documentation.
12. **Deployment:** version tags and checksums, rollback/migration plan, staged release strategy and support diagnostics.

## 3. Mandatory test environments

Use `docs/testing-qa.md` matrix, recording what **actually** ran. `cargo test`, lint/typecheck, native Tauri build and signed installer smoke results must have captured logs. A Windows-only or macOS-only passing test run cannot establish three-platform completion.

## 4. Release candidate criteria

- [ ] All P0 FX rows verified on declared supported OS targets.
- [ ] All P1 rows either verified or have truthful platform limitation documented and accepted.
- [ ] No high-risk unresolved vulnerability, unsafe Trash/quit, misleading metric, data-loss defect or crash under typical lifecycle.
- [ ] Bundled assets and SDK licenses reviewed; no ImageTasks proprietary materials present.
- [ ] Remote API meets opt-in, token, TLS, read-only and privacy gates.
- [ ] App remains usable and low overhead when sensors unavailable.
- [ ] Native installers actually built, installed, launched, uninstalled and verified.
- [ ] CPU/RSS budget measured and reviewed; if target exceeded, results and plan documented.
- [ ] Changelog, support diagnostics, privacy policy, release matrix and migration plan provided.
- [ ] `docs/progress/PHASE-09.md` includes release decision and owners.

## 5. Rollout posture

If one platform is blocked, ship only verified targets or a clearly labeled beta with transparent feature gating; don't mask a failing Windows build with a green macOS CI badge. Optional native widgets and advanced sensors can remain capability-limited while core monitoring ships, provided public feature claims accurately reflect this.
