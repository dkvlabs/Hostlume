# Build, signing and release specification

## 1. Initial channel policy

First supported channel: **direct distribution** with clear signed installers where platform signing is practical. App Store / Microsoft Store / Flatpak are **separate** policy and packaging decisions because sandboxing and helper restrictions can affect sensors, remote server and OS integrations. Avoid promising universal bundles before proof-of-concept packaging.

## 2. Target release artifacts

| OS | Architecture | Candidate artifact | Gate |
|---|---|---|---|
| macOS | Apple Silicon arm64 | signed/notarized `.dmg` / `.app` | native UI, tray and optional helper entitlements |
| macOS | Intel x86_64 | signed/notarized `.dmg` / `.app` | Intel native testing, if target supported |
| Windows | x64 | MSI/NSIS EXE based on verified Tauri 2 setup | signed binary and clean install/uninstall |
| Windows | ARM64 | conditional | toolchain and dependency validation; don't assert parity |
| Linux | x86_64 | `.deb`, `.rpm`, AppImage where supported | GNOME/KDE and dependency checks |
| Linux | ARM64 | conditional | actual artifact, dependencies and hardware validation |

Do not claim `universal2` macOS or Windows ARM64 support until build inputs and native libraries have been validated. Some native GPU libraries may ship only for selected architectures.

## 3. CI pipeline design

1. **PR validation:** formatting, Rust lint/tests, TypeScript type/lint/tests, security scan, docs link/path lint.
2. **Platform builds:** macOS runner (arm64 + Intel as available), Windows runner, Linux runner with required WebKitGTK/system libs.
3. **Integration smoke:** startup, one real CPU/memory sample, capability report and process list (CI hardware may lack sensors).
4. **Release candidate:** signed builds, installed on physical machines, tray and state lifecycle tests, optional WidgetKit/helper packaging, migration test.
5. **Release:** provenance/SBOM, hashes, changelog, update manifest signature and staged rollout controls.

Cache dependencies without caching secrets. Protect signing keys with CI secret store; don't commit certificates, passwords or tokens. Pin action versions and review workflow permissions.

## 4. macOS specifics

- Sign application and nested helper/extension independently, then notarize and verify distribution artifact.
- WidgetKit extension and App Group entitlements need review; the desktop app and widget may not share memory directly.
- Native helper enrollment and removal must be documented, visible and optional.
- Hardened Runtime, sandbox requirements for selected channel and Info.plist privacy permission texts should match actual API use.
- Check menu-bar behavior with app closed/background monitoring enabled; Dock/window close semantics must be consistent.

## 5. Windows specifics

- Validate MSI/EXE installation in user account, start-at-login behavior, update flow and clean uninstallation.
- Signing certificate/SmartScreen reputation are external release concerns, not things Codex can fabricate.
- Test Windows security protections and process kill failures without triggering UAC by default.
- Check DLL search paths, GPU vendor SDK distribution rights and runtime dependencies.

## 6. Linux specifics

- Validate WebKitGTK/Tauri prerequisites for supported distros, dynamic library dependencies and desktop file integration.
- Test .deb/.rpm installs, AppImage as permitted, StatusNotifier/AppIndicator behavior and floating mini-monitor fallback.
- Test headless or minimal DE startup: show actionable error rather than crashing.
- Review Wayland restrictions and polkit requirements before considering optional elevated helpers.

## 7. Updates and rollback

- Use a signed Tauri updater configuration only after checking current Tauri 2 installer/updater compatibility for each target.
- Updates must not silently enable LAN web access or workflows, reset privacy consents or install privileged helpers.
- Migrate SQLite schema transactionally; backup risky migrations. Preserve history across upgrades or explain retention changes.
- Add crash recovery: safe startup with remote listener off if remote configuration is invalid, ability to reset corrupted settings without deleting historical data.

## 8. Release checklist

- [ ] Feature matrix accurately reflects each platform; all P0 done or explicitly documented blocker approved.
- [ ] 3 native OS build logs and smoke-test reports stored.
- [ ] Sensor support matrix documented for actual tested hardware.
- [ ] Security tests, dependency licenses and SBOM reviewed.
- [ ] Icons/visual assets original and product branding finalized.
- [ ] Version/changelog matches application and binary metadata.
- [ ] Installer uninstallation, startup and updater paths tested.
- [ ] macOS signing/notarization verified for intended channel.
- [ ] Windows signed distribution decision reviewed; warning states explained.
- [ ] Linux package dependencies and startup tested in target distros.
- [ ] Remote server defaults OFF; workflow actions defaults OFF.
- [ ] Optional external IP lookup defaults OFF and is disclosed.
- [ ] Release notes include OS/hardware limits, not blanket support claims.
