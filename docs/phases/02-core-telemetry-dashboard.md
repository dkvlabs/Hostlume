# Phase 02 — Real portable telemetry and Overview dashboard

**Owns:** FX-001, FX-002, FX-003, FX-004, FX-005, FX-006.  
**Outcome:** First genuinely useful system monitor with real readings on macOS, Windows and Linux (where native builders are available).

## 1. Preconditions

- Phase 01 typed metric envelope, capability registry and IPC interface exist.
- Platform support targets and live validation machines have been identified.

## 2. Vertical-slice implementation

### 2A. Sampling core

1. Implement one Rust scheduler with fast (initial 3s), medium and slow groups; monotonic `Instant` for deltas and UTC for timestamps; collector registration by capability.
2. Define sample generations and `WARMUP`, `SLEEP_GAP`, `COUNTER_RESET`, `PERMISSION_DENIED`, `DRIVER_UNSUPPORTED` statuses.
3. Implement bounded async snapshot bus: latest snapshot retained; UI event coalescing; subscribe/resync/sequence handling; slow collector errors isolated.
4. Add collector diagnostics: last successful tick, work duration, missed ticks and dropped messages; avoid logging full host identifiers by default.

### 2B. Real collectors

5. **Host:** hostname/OS/version/architecture/uptime, CPU description and logical/physical core counts when available.
6. **CPU:** total and per-core usage, optional frequency and user/system usage. Check `sysinfo` update/warm-up semantics against current crate docs; use native fallbacks when meanings differ.
7. **Memory:** RAM used/available/total, swap; map OS-native memory pressure/classifications through named platform-specific fields, without cross-OS invented equivalence.
8. **Volumes/disks:** mount paths, filesystem/volume capacity, free/available bytes, removable state; handle mount/unmount. Eject is a guarded later enhancement if platform-appropriate.
9. **Network:** enumerate interfaces with stable keys, addresses and counters. Compute rates from per-interface byte differences, reset baseline on rename/hotplug/sleep. Local/gateway IP if OS exposes reliably; external IP remains OFF.
10. Store unit/capability origin in each measurement. Hardware not exposed → `unsupported`, not a blank zero.

### 2C. Overview UI

11. Build responsive grid with CPU, Memory, Storage, Network and Host cards, using actual `get_snapshot` and subscribed events.
12. Add per-core bars and bounded ring-buffer sparklines (history in memory before Phase 03).
13. Add configurable visible card order and time range within current ring-buffer availability.
14. Implement states when a network interface drops, no battery exists, CPU sample warms up or app resumes after sleep.
15. Wire settings polling update (bounded) to scheduler without duplicate timers.

## 3. Tests and validation

- Unit: CPU warm-up and multi-core normalization; exact math for 1000 byte delta over 0.5s = 2000 B/s; counter reset; sleep gap; interface change; invalid duration; missing metric serializes `null`.
- Integration: Linux `/proc` counters (where available), Win32/PDH (on Windows), Mac host source (on macOS), async scheduler cancellation and error isolation.
- UI: hidden/visible cards; no fake data on unsupported; network interface selection changes graph source without data leakage.
- Native smoke: run a controlled CPU load, memory allocation and file/download workload, verify direction/range against OS monitor with matched sampling window; identify tested host/OS.
- Benchmark: process CPU/RSS in background with dashboard closed. Record measured figures, no made-up pass.

## 4. Deliverables

- Rust core scheduler and portable/native collector adapters.
- API/IPC: `get_snapshot`, `get_capabilities`, `telemetry://snapshot` (or approved equivalent).
- Overview dashboard with real CPU/RAM/disk/net; baseline charts; capability status.
- Unit/integration tests and a tested/unverified platform matrix report.

## 5. Phase exit gate

- [ ] Live readings verified on **each available** native test OS, and untested OS clearly listed.
- [ ] First CPU sample correctly warm-up state; second sample gives sensible CPU percent.
- [ ] Rate math uses **elapsed monotonic time** and handles device reset/unplug/sleep.
- [ ] Charts never connect through invalid gaps; app does not crash when adapter fails.
- [ ] Dashboard reconfigures and retains settings; no redundant hardware polling per card.
- [ ] Benchmark and test commands/outcomes captured in `docs/progress/PHASE-02.md`.

**Stop and fix:** any production card using fixture data or a hardcoded CPU/temperature value.
