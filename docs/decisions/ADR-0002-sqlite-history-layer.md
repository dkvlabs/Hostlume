# ADR-0002: SQLite layer — rusqlite (bundled) with a custom versioned migration runner

Status: accepted
Date: 19.10.2026
Owner: principal engineer (Phase 00 agent)
Related features: FX-018, FX-020 (history, retention, settings); consumed by all metric phases

## Context

History, rollups, recordings, alert events and durable settings live in local SQLite (WAL). We need: deterministic migrations testable against old fixtures, bounded queries, no SQL reachable from the WebView, and no secrets in the database export surface. The crate must be maintained and permissively licensed.

## Decision

- **Library:** `rusqlite` **0.40.2** (MIT, updated 2026-08-08 — verified via crates.io API on 19.10.2026) with the `bundled` feature so the SQLite version is compiled in and identical on all targets.
- **Migrations:** a small **custom migration runner** in `monitor-storage`: ordered `V<n>__<name>.sql` files embedded via `include_str!`, applied inside a transaction, recorded in `schema_migrations(version, applied_at_utc, checksum)`. Runner refuses to start the app on checksum mismatch of an applied migration (tamper/accident detection) and supports a documented repair path.
- **Concurrency:** one dedicated writer thread with a bounded mpsc queue (drop policy instrumented); readers open separate connections in WAL mode with `busy_timeout`. All queries use **parameter binding only** — no string-formatted SQL anywhere (also a project security constraint).
- **Schema:** tables per metrics-data-contract.md §6 (devices, metric_definitions, samples_raw, samples_rollup_1m/1h, recording_sessions, alert_rules, alert_events, app_settings) with composite index `(host_id, metric_id, entity_id, ts_utc_ms)`, bounded `LIMIT`/time-range enforcement in the query layer.
- **Secrets:** LAN tokens/credentials never stored in this database or CSV exports (contract §6); OS credential storage instead (Phase 08).

## Alternatives considered

- **`sqlx` (async):** capable, but async-runtime coupling for a single-writer embedded store adds complexity without benefit; compile-time query checking is nice-to-have. Deferred, not rejected forever.
- **`tauri-plugin-sql`:** rejected — exposes SQL execution to the WebView layer, violating "Rust core owns state" and least-privilege IPC (security-privacy.md §2).
- **`refinery` / third-party migration crates:** plausible (MIT), but a ~200-line runner we fully control and test avoids another dependency; revisit only if migration needs grow (e.g., versioned Rust-based migrations).
- **Other embedded-DB wrappers or ORM layers:** not evaluated in depth; none change the ownership-boundary argument.

## Consequences

`monitor-storage` is the only crate allowed to open the database file; `monitor-history` builds queries on top of it; migrations gain fixture tests (restore old DB → migrate → assert schema + data). WAL checkpointing and retention jobs are scheduled by the core service, not the UI.

## Evidence

- rusqlite 0.40.2 / MIT / 2026-08-08: https://crates.io/api/v1/crates/rusqlite (fetched 19.10.2026).
- Schema and query contract: docs/metrics-data-contract.md §5–7.
- SQLite UPSERT/on-conflict form to be confirmed against sqlite.org docs when writing V1 migration (contract §6 note) — Phase 01 task.

## Follow-up

Revisit if: dataset growth measurements (Phase 08/09) show rollup strategy insufficient; or if we need multi-writer throughput (we should not). Migration runner correctness must be proven with downgrade/upgrade fixtures before Phase 03 exit.
