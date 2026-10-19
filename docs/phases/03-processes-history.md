# Phase 03 — Processes, historical telemetry and recording foundations

**Owns:** FX-007 and historical substrate for FX-018.  
**Outcome:** Investigate resource-consuming processes and retain normalized, bounded metric history safely.

## 1. Preconditions

- Phase 02 scheduler/metric schema stable; native CPU/memory probes validated.
- Adopt typed process identity (`PID + creation/start time`, plus host generation).

## 2. Implementation sequence

### 3A. SQLite history

1. Choose SQLite Rust layer + migration library with current maintenance/license check, record ADR.
2. Add schema/migrations for `metric_definitions`, `samples_raw`, 1m/1h rollups, session metadata, settings, alert rules/events placeholders. Use WAL and indexes documented in metrics contract.
3. Build bounded async batch writer; backpressure counters; crash-safe flush/shutdown; retained sampling timestamps and source.
4. Implement retention (proposed raw 24h, 1m 30d, 1h 365d) as user setting with deterministic rollup tests.
5. Implement a bounded `get_history(metric_ids, range, resolution, max_points)` typed IPC endpoint; return gaps and missing ratios.
6. Add migrations from old fixture DBs, disk-full/corrupt DB warnings, WAL checkpoint policy and repair/backup guidance. Never discard data silently.

### 3B. Process collector and manager

7. Enumerate processes: PID, start time, name, icon when available, CPU (core-equivalent + host-normalized where possible), RSS, state, optional parent and thread count.
8. Keep short per-process history keyed by **PID + start time**, not by display name; handle process exits and PID reuse.
9. Build virtualized sortable/filterable process table and detail drawer with per-process CPU/memory chart and permission statuses.
10. Build **request graceful quit** and **force quit** through isolated Rust commands with explicit target confirmation, OS access checks and re-query of start time. No elevation.
11. Add action audit record in a privacy-respecting local log; surface rejected targets and OS error codes.

### 3C. History visualization

12. Upgrade Overview charts to database-backed time ranges (1m, 5m, 15m, 1h, 24h), select automatic resolution to bounded points, and show data gaps.
13. Handle sample generation changes after sleep/resume; chart backend returns discontinuities rather than interpolating.
14. Add a Settings → History view for retention and clear-history action (clear requires confirmation).

## 3. Tests and negative cases

- Migration: fresh DB, upgrade fixture, rollback after simulated failure, compatible schema version handling.
- Retention: raw → rollup with min/max/avg/last/count, missing samples and daylight-saving display safe.
- Load: 1M synthetic history points, bounded query result/time/memory.
- Processes: rapid start/exit, same-name multiple PIDs, PID reuse, protected process; wrong creation time must **refuse** quit.
- UI: sort/search/filter persistence, process exits while drawer open, process list 10k synthetic row virtual-scrolling.
- Security: process full command line not leaked via remote schema or logging.

## 4. Deliverables

- SQLite migrations, writer, query API, retention worker and tests.
- Native process adapter(s), safe termination command interface and UI.
- Historical charts on Overview and process detail views.
- Documented memory/CPU overhead of process polling (default less frequent than core CPU).

## 5. Exit criteria

- [ ] History persists across restart, has UTC timestamps and bounded retention.
- [ ] Process metrics use stable identity and show CPU semantics correctly.
- [ ] Graceful/force quit work only after local confirmation; protected targets fail safely.
- [ ] New history query does not flood JS with millions of points.
- [ ] DB corruption/full disk does not crash main window; clear error surfaced.
- [ ] Actual test commands/results and OS permission limitations in `docs/progress/PHASE-03.md`.

**Security note:** remote endpoints must never bridge to process actions, including in later phases.
