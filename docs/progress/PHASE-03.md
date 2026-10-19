# Phase 03 — Processes, historical telemetry and recording foundations

Date: 19.10.2026
Commit: this commit
Feature IDs: FX-007 (→ `in_progress`), FX-018 substrate (history store + bounded query; recording UI stays FX-018/Phase 08), FX-020 continuation (retention settings)
Status: complete (awaiting owner acceptance)

## Changes implemented (code map)

### 3A. SQLite history

- **Layer/migration choice:** already fixed by ADR-0002 (bundled rusqlite 0.40.2, custom checksummed migrations, single writer). No new ADR needed; this phase implements it as a real crate.
- **`crates/monitor-history`** (new workspace crate):
  - `migrations.rs` — V1 `history_core`: `metric_definitions`, `samples_raw` (+ composite index `(host_id, metric_id, entity_id, ts_utc_ms)` + ts index), `samples_rollup_1m`, `samples_rollup_1h` (count/ok_count/min/max/avg/last, PK = bucket), `recording_sessions`, `alert_rules`, `alert_events` (placeholders for later phases), `action_audit`. Checksum refusal + future-DB refusal, per-migration transactions.
  - `store.rs` — `HistoryStore::open/with_connection` with WAL + busy timeout; `checkpoint_on_close`.
  - `writer.rs` — `BatchWriter`: **synchronously migrates its connection at spawn** (bug found by the live smoke below and fixed), single writer thread, bounded channel (4096), batched prepared-statement transactions (256/batch), `enqueue` never blocks (drop + `dropped_full` counter), `shutdown()`/`close_and_flush()` drain + WAL checkpoint.
  - `retention.rs` — deterministic Rust-side rollup computation from raw over a recompute window (min/max/avg/last/count/ok_count; last = newest by timestamp), idempotent UPSERTs; `retain(now, raw_hours, r1m_days, r1h_days)` (0 = keep tier forever); `clear_samples` for the explicit Settings action.
  - `query.rs` — `get_history`: resolution auto-selection (raw ≤ 30 min → 1m rollups ≤ 7 d → 1h rollups), stride decimation to `max_points` (≤ 2000), gap detection (cadence multiple / `GENERATION_CHANGE` from persisted generation), missing ratio, full validation (31-day range cap, point cap).
- **App wiring** (`apps/desktop/src-tauri/src/lib.rs`): history DB under `app_data/history/`, writer spawned at setup, **snapshot tap** on the scheduler publishes each snapshot's available numeric readings to the writer (drops counted, never blocking); maintenance task every 60 s (1m rollups, 5 min recompute window) and every 10 min (1h rollups over 25 h + retention from settings); `RunEvent::Exit` stops the scheduler and flushes the writer.
- **Settings v3** (`settings.rs` + store): `history` retention block with bounds (raw ≤ 720 h, 1m ≤ 730 d, 1h ≤ 3650 d, 0 = keep forever) and **load-time migration chain v1→latest and v2→v3**, both fixture-tested.

### 3B. Process collector and manager

- **`processes.rs`**: `ProcessManager::page()` — PID, start time (seconds), name, RSS, status, parent, CPU in **both** contract semantics: `cpu_usage_cores` (sysinfo verified core-equivalent, can exceed 100 — verified against vendored `compute_cpu_usage`) and `cpu_usage_host` (÷ logical cores). First page is a declared WARMUP (CPU fields null; identity+memory real). Rows are NOT part of the metric snapshot (contract §5: avoid recording all per-process metrics forever); served via `get_process_page` polled by the UI at 10 s while the page is visible.
- **`request_process_action`**: PID + start time re-query immediately before any signal — start-time mismatch → `identity_changed` (PID reuse guard, test-proven against a real process), absent → `process_exited`; PID 0/1/self → `protected_target` refused before any OS call; graceful = SIGTERM on unix (`unsupported_on_platform` documented on Windows v1), force = SIGKILL/TerminateProcess; **no elevation anywhere**. Every attempt writes an `action_audit` row (pid + start + mode + result — no command lines, no paths).
- **Commands**: `get_process_page`, `request_process_action`, `get_history` (metric_id checked against the family registry), `clear_history` (requires non-empty confirmation id).

### 3C. History visualization

- **Applications page** (`features/applications/`): custom **virtualized** table (windowed rendering, 32 px rows + overscan — 10 k rows stay cheap without a new dependency), sort by name/pid/cpu/memory, search filter, detail drawer with identity, CPU hint (core-equivalent semantics), and per-process CPU history **keyed `pid:startTime`** — a PID reuse creates a new series instead of contaminating the old (store test).
- **Quit / force quit** flow through `ConfirmActionDialog` (explicit target, effect explanation, Esc/Cancel safe default, destructive treatment for force); refusal outcomes surfaced in plain language ("Refused: the process exited and its PID was reused." etc.).
- **Overview**: `useHistorySeries` — ≤ 15 min from ring buffers, **1 h / 24 h from the DB** through the bounded query, with gap count + missing-ratio displayed under the sparkline; time control now offers 1m/5m/15m/1h/24h.
- **Settings → History**: retention sliders (bounds enforced in Rust) + Clear-history with confirmation dialog (samples only; settings/audits/schema kept).
- **i18n**: all new strings keyed in en + ro.

## Verification

| Environment | Command / scenario | Result | Evidence |
|---|---|---|---|
| macOS arm64 | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` | PASS | this run |
| macOS arm64 | `cargo test --workspace` | PASS — **66 tests, 0 failed**: history migrations fresh/drift/future, writer batch+drop counts, rollup determinism (out-of-order insert → last=40.0, idempotent recompute), retention tier cutoffs, clear, bounded query with MISSING_SAMPLES + GENERATION_CHANGE gaps + missing ratio, rollup resolution engagement, settings v1→v3 + v2→v3 migrations + bounds, process warm-up/identity/PID-reuse-refusal/protected/exited/invalid-mode, scheduler regression suite | this run |
| macOS arm64 | `cargo test -p monitor-history --test history -- --ignored` (1 M synthetic rows) | PASS — insert 3.3 s, **bounded query 1.3 ms, 360 points returned ≤ max_points** (contract §7: never flood JS) | this run |
| macOS arm64 | `pnpm lint` / `pnpm typecheck` / `pnpm test` / `pnpm build` | PASS — **70 frontend tests**: sort semantics (incl. >100% core-equivalent CPU), identity-keyed history across PID reuse, warm-up nulls, guards for process/history payloads, confirm-dialog flow, Applications page renders real table | this run |
| macOS arm64 | **Live persistence smoke** (fresh app data, binary run 20 s, SIGTERM): `samples_raw` = **1063 rows / 12 distinct metrics** during run, count stable after exit | PASS — history persists across restart path; writer flush + WAL checkpoint on shutdown | this run |
| macOS arm64 | **Corruption-adjacent check**: writer-migration bug found by the first live run (fresh DB had no schema) → `BatchWriter::spawn` now migrates synchronously; maintenance connection also self-migrates | fixed + covered by suite | this run |
| macOS arm64 | **Process-poll overhead** (phase deliverable): `cargo test -p app --lib processes::tests::process_poll_overhead_measurement -- --ignored --nocapture` | **940 rows in 13.5 ms per page(); cadence 10 s (medium group, slower than 3 s core CPU)** — ≈0.14% duty cycle | this run |
| macOS arm64 | Background overhead: app 45 s, no UI interaction — CPU ≈ 0%, RSS ≈ 105–120 MB (debug build), samples accumulating (~60/s) | PASS | this run |
| Windows / Linux | any build/test | NOT_RUN — no hardware/runner; portable code compiles in CI matrix (authored, not yet executed) | R10 |

## Security and data integrity

- **PID reuse guard test-proven**: wrong start time on a real process → `identity_changed`, no signal sent; protected targets refused before any OS call; own PID in the protected set.
- **No elevation**; permission shortfalls surface as `permission_denied` with no retry-as-root path.
- **Audit trail**: every action attempt persisted (mode + outcome + identity); command lines and paths are never stored, satisfying the testing-qa security case.
- **History integrity**: UTC epoch-ms only; generation stamped per sample → restarts appear as GENERATION_CHANGE gaps; rollups idempotent (recompute windows); retention bounded by explicit settings; clear-history requires confirmation and never touches settings/audits.
- **Writer backpressure**: bounded queue with counted drops — a slow/full disk degrades to instrumented sample loss, never a crashed UI (writer thread isolated).
- Process pages are read-only; **no process action exists over any remote surface** (the remote API does not exist yet; the action command is a local Tauri command and will never be bridged per phase doc security note).

## Blockers / untested

- **Windows / Linux**: process adapter compiles platform-neutrally but is NOT exercised there; graceful-quit on Windows is `unsupported_on_platform` by design until a Win32 path lands (documented platform_limited). History crate is platform-neutral SQLite.
- **Icon column** (design §6): deferred — macOS icon extraction needs an AppKit adapter; rows render name-only (documented limitation, not a fake).
- **1M-row test** runs only when explicitly invoked (`--ignored`) to keep CI fast; CI runs the 100k-equivalent normal suites.
- **PID-reuse live force-quit** (an actual SIGKILL of a reused PID) is covered by the refusal test at the guard level; a real kill E2E on protected/system processes requires native test machines (Phase 09 hardware matrix).
- Recording sessions/alerts tables exist as placeholders; their engines are FX-017/FX-018 scope.

## Feature matrix edits

- FX-007: `planned` → **`in_progress`** — process list with stable identity, dual CPU semantics, guarded quit/force-quit with audit + UI, macOS-validated.
- FX-020 remains `in_progress` (history retention settings + confirmed clear added).
- FX-018 substrate (persistence + bounded query + gaps) landed as cross-cutting work; FX-018 itself stays `planned` until comparison/recording UI exists.

## Gate review (phase doc §5)

- [x] History persists across restart (live smoke), UTC timestamps, bounded retention (settings-driven worker).
- [x] Process metrics use stable identity (pid + start time keyed everywhere); CPU semantics correct and labeled (core-equivalent + host-normalized).
- [x] Graceful/force quit only after local confirmation; protected targets fail safely (refusal tests).
- [x] History query bounded (stride decimation; 1M-row test: 1.3 ms / ≤ max_points).
- [x] DB issues do not crash the main window (writer thread isolated, errors instrumented; corruption of a migrated DB refused loudly by checksum).
- [x] Test commands/results and permission limits recorded (this file).

**Handoff:** "Phase 03 completed with persisted, gap-honest history and a guarded process manager on macOS arm64. Phase 04 (storage analyzer + network detail, FX-008/009/011) may start after owner acceptance; CI baseline for Windows/Linux still lands with the first push."
