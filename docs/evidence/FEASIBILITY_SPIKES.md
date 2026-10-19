# Feasibility spikes — Phase 00

Date: 19.10.2026 · Machine: MacBook Pro (Mac15,11), Apple M3 Max, 14 logical cores (10P+4E), 36 GiB RAM
OS: macOS 27.0.1 (build 26A434), arm64 · Toolchain: rustc 1.99.1 / cargo 1.99.1 (Homebrew), node v24.22.0, pnpm 10.33.2, Xcode 26 at `/Applications/Xcode.app`
Spike location: `/tmp/hostlume-spike` (throwaway, intentionally **outside** the repository — Phase 00 produces no production code).

**No root/sudo was used anywhere in this session.** All results below are from an unprivileged user process.

## Spike 1 — sysinfo 0.39.6 core telemetry snapshot (Rust)

Method: `cargo build` + run a binary using only APIs the compiler accepted (fabricated APIs fail the build; three did — see "API corrections"). Command:

```text
cd /tmp/hostlume-spike && cargo build && ./target/debug/hostlume-spike
```

Result: **PASS — real readings obtained.** Key output (19.10.2026):

| Area | Evidence | Notes |
|---|---|---|
| Host identity | `host_name`, OS "Darwin" "27.0.1", kernel "27.0.0", arch "arm64" | all `Some(...)` real values |
| Uptime | `uptime_s: 199886`, `boot_time: 1792171723` | boot_time exactly matches `sysctl -n kern.boottime` (see cross-check 5) |
| CPU warm-up | first `refresh_cpu_all()` returned a number (48.6) but per contract the first interval is not trustworthy | Product must still enforce its own two-sample WARMUP policy; do not surface sample 1 |
| CPU valid | after 1.5 s second sample: global 57.2%, per-core 82–89% on cores 0–3 (machine under compile load), 14 logical cores, frequency 4056 MHz | per-core works |
| Load average | `LoadAvg { one: 15.95, five: 17.16, fifteen: 15.96 }` | API returns a struct in 0.39 |
| Memory | total 36864 MiB, used 31005, available 13992, free 334; swap 5120/4427/692 MiB | consistent with `vm_stat` (page size 16384) |
| Disks | 3 mounts incl. external exFAT volume "PlayStation" (1862 GiB total / 1336 GiB avail) and both APFS system volumes | hot-plugged external drive enumerated without privileges |
| Networks | 29 interfaces; cumulative (`total_received/transmitted`) **and** interval (`received/transmitted`) byte counters, e.g. `utun8 rx_total=41221375KiB` | rate math possible in-process |
| Processes | 836 processes; each with `pid`, `start_time` (unix epoch), name, memory | PID-reuse identity guard has the raw material it needs |
| Process CPU warm-up | first `refresh_processes` reported 0.0% CPU for all top processes | confirms per-process CPU also needs a previous sample — matches fixture CPU-01 |
| Cost | `System::new_all()` + 500 ms sleep + `refresh_all()` = 537 ms wall → ≈37 ms actual work on M3 Max | well inside the 3 s fast-poll budget; formal soak test deferred to Phase 09 |

### sysinfo 0.39 API corrections discovered by the compiler (recorded so nobody re-invents them)

- `Cpu` has **no** `core_id()` and no `usage()`; it is `cpu_usage()`, and per-core identity = index in `cpus()`.
- `System::load_average()` returns a `LoadAvg` struct (`.one/.five/.fifteen`) — not an out-array, not an `Option`.
- `Disk::file_system()` returns `&OsStr`, not `&[u8]`.
- `System::refresh_usage()` does **not** exist.
- sysinfo 0.39.6 has **no battery module** (`src/common/` contains only component, disk, network, system, user). Battery must come from a platform adapter (see cross-check 1).

## Spike 2 — temperature sensors on Apple Silicon via sysinfo Components

```text
cargo build --bin components && ./target/debug/components
```

Result: **PASS — 22 real temperature sensors, unprivileged, Apple Silicon.** Sample output:

```text
label="gas gauge battery" temp=Some(34.0)°C
label="PMU tdie6"         temp=Some(64.2)°C   (tdie1..tdie10, tdev1..tdev8, tcal)
label="NAND CH0 temp"     temp=Some(33.0)°C
label="als-temp"          temp=None           (sensor present but no reading — availability state material)
```

Mechanism (read from the crate source, `sysinfo-0.39.6/src/unix/apple/macos/component/arm.rs`): IOKit HID
`IOHIDEventSystemClientCreate` + Apple-vendor temperature usage page (`kHIDUsage_AppleVendor_TemperatureSensor`,
`kIOHIDEventTypeTemperature`) via `objc2-io-kit`. No root, no private SMC protocol observed in this path.

**Not exposed by this path:** fan RPM (none returned), GPU utilization, per-process disk I/O.

## Native cross-checks (macOS commands, unprivileged)

1. **Battery — `ioreg -rn AppleSmartBattery`:** real values without root: `CycleCount=180`, `BatteryData{DesignCapacity=8579 mAh, FullChargeCapacity=7300, NominalChargeCapacity=7544, RemainingCapacity=7221, CurrentCapacity=100, FullyCharged=1}`, `IsCharging=No`, `ExternalConnected=Yes`, `AdapterDetails{Watts=140,...}`, `InstantAmperage=0`. → Battery health/cycles/charge/time-estimate inputs are obtainable; Rust access needs an IOKit adapter (AppleSmartBattery registry keys) since sysinfo lacks battery support.
2. **`battery` crate (crates.io):** latest 0.7.8, **last updated 2020-11-01** → unmaintained, **deferred/rejected**; use own IOKit access instead (decision recorded in ADR-0004).
3. **Disk I/O — `iostat -d -w 1 -c 3`:** per-device KB/t, tps, MB/s for disk0/4/5/7 without root. This is a **rate-only** source; per the metrics contract its provenance must be marked (not derived from monotonic counters). Cumulative block-device counters would need IOKit `IOBlockStorageDriver` statistics — **not proven yet** (open item for Phase 04/05).
4. **Memory — `vm_stat`:** page size 16384; active/inactive/wired/purgeable categories present → OS-specific `MemoryBreakdown` mapping required (contract §4: never sum categories across OSes).
5. **Uptime — `sysctl -n kern.boottime`:** `sec = 1792171723` — byte-identical to sysinfo `boot_time`. Cross-source consistency proven.
6. **Temperature/fans via Apple tooling — `powermetrics`:** without sudo: `powermetrics must be invoked as the superuser`; `--samplers smc` is `unrecognized` on macOS 27. → Apple's own sampler tooling is root-gated; our non-root path is the IOKit HID sensors above. Deeper thermal/power data remains root-gated and out of v1 scope (no privileged helper by default — security-privacy.md §8).
7. **Core topology — `sysctl hw.ncpu hw.perflevel0.logicalcpu hw.perflevel1.logicalcpu`:** 14 / 10 / 4 (performance/efficiency split) → P/E core labeling is possible from public sysctls.

## High-risk feasibility outcomes (task 6)

| Question | Status on this machine | Evidence |
|---|---|---|
| Apple Silicon temperatures | **Proven** without root (22 sensors) | Spike 2 |
| Apple Silicon fan RPM | **Not available** via sysinfo/IOHID path; root-gated tooling rejected → research item, likely `platform_limited` on many Macs | Spike 2, cross-check 6 |
| Apple Silicon GPU busy % | **Unproven** — no public API verified this session; IOKit Accelerator statistics need a dedicated Phase 05 spike | — |
| Battery cycles/health | **Proven** via `AppleSmartBattery` (ioreg), Rust adapter pending | Cross-check 1 |
| Windows sensor/GPU coverage | **NOT TESTED — no hardware/runner in this environment.** Only doc-level facts verified (PDH ≤1 Hz guidance) | MS docs, see PHASE-00 source log |
| Linux tray / hwmon | **NOT TESTED — no hardware/runner.** hwmon sysfs doc verified (millidegree Celsius convention) | kernel docs, see PHASE-00 source log |
| macOS WidgetKit | Doc-verified: extension target + SwiftUI + timeline mechanism. No packaging spike run (needs a signing identity) | Apple docs, see PHASE-00 source log |

## Reproduction

```bash
cd /tmp/hostlume-spike && cargo run --bin hostlume-spike && cargo run --bin components
ioreg -rn AppleSmartBattery | grep -E 'CycleCount|DesignCapacity|IsCharging'
iostat -d -w 1 -c 3; vm_stat | head; sysctl -n kern.boottime
```

Spike sources are preserved at `/tmp/hostlume-spike` on this machine (not committed; they are throwaway code, not the Phase 01 implementation).
