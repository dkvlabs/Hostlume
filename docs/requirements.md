# Product requirements and acceptance criteria

## Product statement

**Hostlume** is an offline-first desktop system monitor for macOS, Windows and Linux. It provides live metrics, history, process visibility, safe disk cleanup, conditional hardware sensors, configurable tray indicators, alerts and secure optional remote viewing. Target users are developers, power users and administrators monitoring their **own** machines.

## Personas and workflows

1. **Developer:** identify CPU/memory-hungry processes while running a local stack; inspect histories and force-quit a hung process with confirmation.
2. **Laptop user:** watch battery health and discharge, CPU/GPU temperature when available, fans, tray indicators and notifications.
3. **Power user:** find large files, understand network saturation, record a session and export CSV.
4. **Remote viewer:** opt in to LAN monitoring, authenticate from a phone and inspect CPU/RAM/disk history without controlling the machine.

## Functional requirements (FR)

| ID | Requirement | Pass criterion |
|---|---|---|
| FR-01 | Real host identity and uptime | Host/OS/version/architecture resolved or explicitly unavailable |
| FR-02 | Live total/per-core CPU and frequency when available | Correct deltas; first-sample warm-up; 0–100 overall scale |
| FR-03 | Memory, swap and OS-specific pressure/classification | Present meaningful categories with labels and no double counting |
| FR-04 | Filesystems/mounts and capacity | Hot-plug handled; volume selected; units consistent |
| FR-05 | Live network per interface and IP/gateway | Dynamic adapter list; traffic rates use monotonic clocks |
| FR-06 | Application/process list with usage trends | PID/start-time identity; 10k-process stress test; sortable/searchable |
| FR-07 | Guarded process termination | Confirm and revalidate target identity; permission errors visible |
| FR-08 | Directory size scan/visualization | Cancellation, symlink safety, partial results, Trash workflow |
| FR-09 | Disk read/write throughput and counters | Real device/OS source; reset/unavailable states handled |
| FR-10 | GPU and display monitoring | Enumerate adapters/displays; correct VRAM/unified-memory semantics |
| FR-11 | Temperatures/fan telemetry | Actual supported readings; safe missing-hardware behavior |
| FR-12 | Battery statistics | Battery absent state; health/cycles only where exposed |
| FR-13 | Bluetooth accessory battery | Report supported device data and transparent unsupported cases |
| FR-14 | Tray/menu bar monitoring | Background process stays running; popup and settings work |
| FR-15 | Widgets/compact fallback | macOS native widget when approved; OS-appropriate fallback |
| FR-16 | Rule-based notifications | Hold time, hysteresis and cooldown prevent alert storms |
| FR-17 | Local action execution (opt-in) | User-created fixed command; test mode; no remote triggering |
| FR-18 | Historical comparison/recording | Fixed timestamps, zoom ranges, CSV round trip |
| FR-19 | Read-only web/JSON API | Disabled by default; token auth and transport rules validated |
| FR-20 | Settings and accessibility | Persisted, keyboard navigable, unit/theme options supported |

Map FRs to `FX-*` rows in `feature-matrix.md` for phase ownership.

## Non-functional requirements (NFR)

### Reliability and correctness

- Single collector service; all UI panels consume the same latest snapshot.
- Current sample has a collection timestamp and a freshness threshold; charts tolerate gaps instead of connecting across machine sleep.
- No stale sample presented as current; no `0` to mean missing metric.
- An individual collector failure cannot crash the monitoring service.
- Database migrations are transactional and backups are considered before irreversible changes.
- Unplug devices, switch interfaces, sleep/resume and revoke permissions without crashing.

### Performance budgets (targets to **measure**, not guarantees)

- Baseline on reference hardware: average **<2% CPU** during background idle at 3-second base polling; under **150 MB resident memory** during ordinary monitoring.
- Target dashboard responsiveness: input feedback <100 ms under normal load; no 1-second UI stalls from scanning.
- Default: 3 s fast collector tick; slower sensors 5–15 s; expensive static discovery 30–120 s or event-driven where possible.
- Keep rolling chart windows bounded, downsample history, batch SQLite commits and avoid per-process snapshots of every field at 1 Hz.
- Record measurements on a named hardware/OS matrix; never declare universal performance from a single laptop.

### Product constraints

- Local-first operation with zero accounts and zero mandatory servers.
- macOS Apple Silicon and Intel, Windows 10/11 x64, Linux GNOME/KDE on mainstream distros are validation **targets**, subject to up-to-date Tauri/toolchain support; do not promise an older OS until it runs in CI.
- Use user-visible, human-readable error states and explanations when a module requires drivers, OS permissions or optional helpers.
- Theme: dark/light/system; localization architecture from day one; English in v1, translation files prepared for Romanian and other languages.
- All destructive actions must be undoable where supported or clearly confirmed and scoped.

## Accessibility and localization

- WCAG 2.2 AA as design target for contrast/keyboard behavior; visible focus; never encode metric meaning by color only; respect reduced motion.
- i18n message keys; no concatenated translated strings; support comma decimals and unit preference.
- Distinguish SI rates (`MB/s`) from binary capacities (`GiB`), or allow explicit user preference with exact formatting tests.

## Exclusions for first release

- Fan **control**, overclocking, BIOS writes, undervolting.
- Kernel driver development and unsigned system extensions solely to expose sensors.
- Remote process termination, remote shell, remote cleanup and remote management.
- Public Internet-hosted dashboard by default, user accounts, cloud analytics, sync and billing.
- Multi-host fleet orchestration (phase 10 optional).

## Definition of feature complete

Each feature must have: documented expected behavior; native collector and capability status; types and units; UX states; data/security review; deterministic test(s); platform validation logs or an explicit validated limitation; no critical defect. A placeholder view or failing platform is never reported as done.
