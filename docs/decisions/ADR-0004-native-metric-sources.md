# ADR-0004: Native metric sources per OS — portable baseline, platform adapters, no privileged helper in v1

Status: accepted
Date: 19.10.2026
Owner: principal engineer (Phase 00 agent)
Related features: FX-001…FX-004, FX-006…FX-014, FX-021

## Context

Collectors must produce real readings on three OSes with wildly different sensor exposure, while honoring: never fabricate values, never default to root/admin, no unsigned drivers, no unreviewed third-party sensor SDKs (architecture.md, security-privacy.md §8, reference-audit.md qualification 3).

## Decision

**Layering:** `sysinfo` is the portable baseline for CPU (overall/per-core), memory, swap, disks/mounts, network interfaces (cumulative + interval counters), processes (pid, start_time, cpu, memory), and — on macOS — temperature components. Each `monitor-platform-<os>` crate supplements or replaces baseline data with native sources where semantics or coverage require it, and every metric records its `source` string (contract §1).

Per-OS source decisions (all "intended" rows need runner/hardware validation in their phases — see RISK_REGISTER R10):

- **macOS (validated on arm64 this session):**
  - CPU/memory/processes/disks/networks: sysinfo (spike-proven, FEASIBILITY_SPIKES.md).
  - Temperatures: sysinfo `Components` → IOKit HID Apple-vendor temperature sensors, unprivileged, 22 sensors proven on M3 Max.
  - Battery: **own IOKit adapter** reading `AppleSmartBattery` registry keys (CycleCount, DesignCapacity, FullCharge/NominalChargeCapacity, IsCharging, ExternalConnected, AdapterDetails, InstantAmperage — all proven via `ioreg`, unprivileged). The `battery` crate is **rejected** (unmaintained since 2020-11-01).
  - Fans: no supportable unprivileged source found → `unsupported` state by default; research item, likely `platform_limited` (R1). `powermetrics` is root-gated and unusable.
  - Disk I/O: rate-only `iostat`-equivalent is insufficient provenance-wise; Phase 04 must evaluate IOKit `IOBlockStorageDriver` statistics for cumulative counters (R12).
  - GPU utilization: unproven; Phase 05 spike required before any GPU% claim on Apple Silicon (R1).
- **Windows (NOT TESTED — docs verified only):**
  - CPU/memory/processes: direct Win32 APIs (`GetSystemTimes`, `GlobalMemoryStatusEx`, psapi/Toolhelp or Process V2 counterset) — Microsoft explicitly recommends these over PDH for higher-frequency collection (verified 19.10.2026).
  - PDH only where necessary (e.g., "GPU Engine" counters), sampled ≤1 Hz per MS guidance, and never as the sole 3 s source.
  - Battery: Win32 `GetSystemPowerStatus` + `IOCTL_BATTERY_*`/WMI `BatteryStatus` — cycles/design capacity conditional per device (capability-probed).
  - Sensors/fans: motherboard exposure varies; no bundled driver; `unsupported`/`permission_required` states instead. Vendor SDKs only after license review (none adopted in Phase 00).
- **Linux (NOT TESTED — docs verified only):**
  - CPU/memory/processes: /proc (or sysinfo baseline where semantics match).
  - Sensors/fans: `/sys/class/hwmon` per kernel sysfs ABI (world-readable standard attributes; temperatures in millidegrees Celsius — verified 19.10.2026; fan unit convention RPM to be re-confirmed against the same page at implementation time since our fetch did not surface an explicit unit statement).
  - Battery: `/sys/class/power_supply` (+ UPower where session services exist).
  - Disk I/O: `/proc/diskstats` cumulative counters; device-mapper/loop/RAID de-duplication rules required (platform-capabilities.md).
  - Network: `/proc/net/dev` or netlink; handle renames/namespaces.
  - GPU: DRM sysfs (`/sys/class/drm/card*/device/gpu_busy_percent` on amdgpu, Intel/NVIDIA differ) — strictly capability-probed per driver; no invented aggregates.

**Helper policy:** no privileged helper ships in v1. If a future metric demonstrably requires one, it is a separate signed, consented component with its own ADR and threat review (security-privacy.md §8).

**Mock policy:** mock collectors exist behind `#[cfg(feature = "mock")]`/test-only flags; production builds fail loudly rather than fall back to mocks (AGENTS.md §5).

## Alternatives considered

- **sysinfo-only everywhere:** rejected — no battery on macOS, no fan/GPU nuance, Windows PDH semantics need native care.
- **Third-party cross-platform sensor suites (e.g., LibreHardwareMonitor-style):** rejected for v1 — Windows service/driver implications, redistribution licensing unclear, maintenance risk (architecture.md platform boundaries).
- **Fork/vendor `battery` crate:** rejected — 6 years stale; a focused IOKit/Win32/sysfs battery adapter is smaller than resurrecting it.

## Consequences

Capability probing (`MetricCapability` per architecture.md) is mandatory for every conditionally-available metric; unit tests use recorded fixtures (SENSOR-01, BAT-01, MEM-01…); per-OS `source` strings surface in the Capabilities UI (FX-021).

## Evidence

- macOS rows: live spikes on macOS 27.0.1 / M3 Max, 19.10.2026 — docs/evidence/FEASIBILITY_SPIKES.md (all unprivileged).
- sysinfo 0.39.6 source inspection: `src/unix/apple/macos/component/{arm,x86}.rs` (IOHID temperature), `src/common/` module list (no battery).
- Windows PDH guidance: https://learn.microsoft.com/en-us/windows/win32/perfctrs/about-performance-counters (fetched 19.10.2026: "not designed to be collected more than once per second"; recommends GetSystemTimes/GlobalMemoryStatusEx/psapi).
- Linux hwmon ABI: https://docs.kernel.org/hwmon/sysfs-interface.html (fetched 19.10.2026: naming `<type><number>_<item>`, millidegree Celsius for temp channels, standard attributes world-readable).
- `battery` crate status: https://crates.io/api/v1/crates/battery (0.7.8, updated 2020-11-01).

## Follow-up

Unproven items with owning phases: macOS GPU% + IOKit disk counters (05/04), macOS fan research (05), all Windows rows (02/05 on runners), all Linux rows incl. explicit fan RPM unit confirmation (02/05/06 on runners). Any newly adopted native library requires a DEPENDENCY_LICENSE_SCAN.md entry first.
