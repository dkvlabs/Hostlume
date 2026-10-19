# ADR-0001: Adopt the telemetry data contract as the normative Phase 01 type foundation

Status: accepted
Date: 19.10.2026
Owner: principal engineer (Phase 00 agent)
Related features: all (FX-001…FX-022)

## Context

Every later phase consumes one shared metric/snapshot bus. Without a frozen contract, Phase 02+ collectors, history, UI and the remote API would each invent their own shapes, and the "never fabricate values" rule would be unenforceable. `docs/metrics-data-contract.md` already defines availability states, units, snapshot schema, rate math, retention and entity keys.

## Decision

Adopt `docs/metrics-data-contract.md` (schemaVersion 1) as **normative** for `monitor-core` types, with these Phase 00 resolutions of its open points:

1. **Per-process CPU display:** both metric IDs are collected (`cpu.process.usage_cores` 0–100×cores and `cpu.process.usage_host` 0–100). The Applications table displays **host-normalized** (`usage_host`) by default with a user-visible semantic toggle (design.md §6); charts label which semantic is shown. Windows core-equivalent values are never silently clamped.
2. **Warm-up:** first sample of any interval-derived metric (CPU, per-process CPU, rates) is emitted as `temporarily_unavailable` with reason `WARMUP` — confirmed necessary by spike evidence (sysinfo returned 0.0% process CPU on first refresh; global CPU returned a value on the very first refresh that must still be treated as untrusted).
3. **Missingness:** `availability !== 'available'` ⇒ `value === null`. No NaN, no -1, no zero-fills, in Rust, SQLite and UI (invariant from contract §2; enforced by contract tests).
4. **Timestamps:** storage uses UTC epoch milliseconds (`ts_utc_ms`); intervals use the monotonic clock only; UI renders viewer-local with DST-safe labels; discontinuity markers on sleep/resume/collector-restart/hotplug/counter-reset.
5. **Memory:** OS-specific `MemoryBreakdown` structs per platform (macOS vm_stat categories, Linux meminfo, Windows counters); never cross-OS category sums; pressure ≠ percent-used.
6. **Retention defaults (configurable):** raw 24 h; 1-minute rollups 30 d; 1-hour rollups 365 d. Rollups store count/ok_count/min/max/avg/last so partial intervals are not misrepresented.
7. **Entity keys:** host = random install UUID; process = pid + start_time (+ session); network = OS interface id + generation; disk = stable OS volume/device id.

## Alternatives considered

- **Free-form per-collector JSON:** rejected — kills queryability, history rollups and contract tests.
- **Protobuf/flatbuffers schema now:** rejected for v1 — serde JSON over Tauri IPC is adequate at 3 s cadence; revisit only if measured serialization cost demands it (Phase 09 perf gate).

## Consequences

`monitor-core` owns the canonical Rust types mirroring the contract (Availability, MetricUnit, MetricReading, SystemSnapshot); all crates depend on it; frontend consumes generated/shared TypeScript types; contract fixture catalog (metrics-data-contract.md §9) becomes the Phase 01/02 test backbone.

## Evidence

- Contract text: docs/metrics-data-contract.md (repo, drafted 18.10.2026).
- Warm-up and pid+start_time behavior proven on macOS 27.0.1 / M3 Max, 19.10.2026: docs/evidence/FEASIBILITY_SPIKES.md (spike 1).
- Windows PDH ≤1 Hz constraint supporting interval-based rate math: https://learn.microsoft.com/en-us/windows/win32/perfctrs/about-performance-counters (fetched 19.10.2026).

## Follow-up

Type freeze happens in Phase 01; any schema change afterwards requires a new ADR and a `schemaVersion` bump with migration tests. Linux/Windows semantic details (meminfo fields, PDH counter names) get per-platform annexes in Phase 02 when runners exist.
