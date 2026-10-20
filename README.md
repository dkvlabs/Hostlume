# Cross-platform System Monitor — Build Kit

**Purpose:** Build an original, production-grade desktop monitoring application with functionality comparable to [iStatistica Pro](https://www.imagetasks.com/istatistica/pro/) for **macOS, Windows, and Linux**.

**Deliverable:** This repository is the **documentation and execution package** for **Hostlume**, an original cross-platform system monitor. The recommended stack is **Tauri 2 + Rust + React/TypeScript + Vite + Tailwind CSS + shadcn/ui + SQLite**. (Named 20.10.2026; earlier builds used the `APP_NAME` placeholder and the app migrates their data directory on first start.)

## Start here

1. Unzip into an **empty Git repository** (or copy its contents into your app repository).
2. Open the repository in Codex. Codex automatically discovers the root [`AGENTS.md`](AGENTS.md).
3. Give Codex the prompt in [`docs/prompts/START_CODEX.md`](docs/prompts/START_CODEX.md).
4. Execute phases **00 → 09 in order** using [`docs/implementation-phases.md`](docs/implementation-phases.md) and [`docs/prompts/PHASE_RUNNER.md`](docs/prompts/PHASE_RUNNER.md).
5. Do not begin the next phase until the current phase's exit criteria and evidence are recorded. Phase 10 is an optional post-v1 extension.

## Documentation map

| File | What it defines |
|---|---|
| [`AGENTS.md`](AGENTS.md) | Non-negotiable agent rules, work protocol, DoD, security rules |
| [`docs/reference-audit.md`](docs/reference-audit.md) | Verified reference app capabilities and research boundaries |
| [`docs/requirements.md`](docs/requirements.md) | Functional, performance, quality, and acceptance requirements |
| [`docs/feature-matrix.md`](docs/feature-matrix.md) | Feature IDs, reference parity, priority, owning phase |
| [`docs/architecture.md`](docs/architecture.md) | Runtime, Rust crates, OS adapters, IPC, eventing, lifecycle |
| [`docs/metrics-data-contract.md`](docs/metrics-data-contract.md) | Metric schema, units, storage, calculation invariants, queries |
| [`docs/design.md`](docs/design.md) | UI design system, page blueprints, interaction specifications |
| [`docs/platform-capabilities.md`](docs/platform-capabilities.md) | Supported OS paths, native API candidates, hardware limitations |
| [`docs/security-privacy.md`](docs/security-privacy.md) | Threat model, permission model, remote API and workflow safety |
| [`docs/testing-qa.md`](docs/testing-qa.md) | Tests, fixtures, hardware matrix, benchmarks, pass criteria |
| [`docs/release-distribution.md`](docs/release-distribution.md) | CI, signing, installer strategy, update, release gates |
| [`docs/implementation-phases.md`](docs/implementation-phases.md) | Milestones, dependencies, sequencing, scope control |
| [`docs/phases/`](docs/phases/README.md) | Detailed, executable phase instructions and acceptance gates |
| [`docs/prompts/`](docs/prompts/START_CODEX.md) | Ready-to-paste Codex prompts |

## Scope

### Reference feature parity (supported where technically possible)

- CPU, per-core and process usage; RAM and memory pressure; disks and I/O; network interfaces and traffic; system information.
- GPU and displays; hardware temperatures and fans; battery and Bluetooth accessory charge when accessible.
- Running applications, usage history, quit/force quit; storage visualizer with **Move to Trash**.
- Menu bar/tray monitors; configurable dashboards; macOS widgets; notifications and optional approved local actions.
- Comparison charts, recording, CSV export; **opt-in** web monitoring and read-only REST API.

### Original improvements (not claimed as iStatistica features)

- Windows and Linux support; explicit hardware capability explanations; consistent historical retention controls.
- Optional future multi-computer monitoring (phase 10).

## Non-negotiable constraints

- **No fake system values.** If an OS/driver blocks a metric, show `unsupported` / `permission_required` / `temporarily_unavailable` / `error` with context.
- **No forced UI imitation.** Reproduce function and workflow, not proprietary branding, imagery, or exact visual assets.
- **No unverified “all-platform” claims.** Compile, package and test in native OS environments.
- **No silent deletion, elevated helper installation, command execution or LAN listener.** Require explicit opt-in and appropriate safeguards.
- **No requirement for cloud services** for local monitoring.

## Suggested milestones

- **M1** (phases 00–02): Real CPU/RAM/disk/network metrics on all target systems, dashboard, settings, usable application shell.
- **M2** (03–04): Process inspector and safe termination, persistent history, advanced network, storage scan/Trash.
- **M3** (05–07): Conditional native sensors, GPU/battery/Bluetooth, tray/widgets, alerts.
- **M4** (08–09): Comparison, remote read-only dashboard, tests, security review, installers and release evidence.

## Source & research note

Primary source: [ImageTasks iStatistica Pro](https://www.imagetasks.com/istatistica/pro/). App Store listing: [iStatistica Pro](https://apps.apple.com/us/app/istatistica-pro/id1447778660). These documents were drafted **18.10.2026** using public descriptions. They do not imply access to the app's source code, protocol internals, or binary. Validate changing dependencies/API support before implementation.

## Licensing and distribution

No license to use ImageTasks trademarks, code or graphics is conferred by this package. Review licenses for all dependencies and sensor integrations. Signing keys, certificates, App Store entitlements and package-distribution credentials are **not** included.
