# Phase 08 report — comparison/recording + secure optional web monitor

**Date:** 2026-10-19 · **Owning features:** FX-018, FX-019

## 1. What shipped

### FX-018 — Comparison, recording, CSV export

* **`monitor-history` additions** (schema migration V2: `recording_sessions.status`):
  * `sessions.rs` — `start/stop/get/list_sessions`, typed double-stop errors, bounded
    lists; **crash recovery**: sessions left open by a previous run are re-marked
    `interrupted` at startup (never silently closed).
  * `export.rs` — paged raw-row reads (`export_page` with exclusive cursor,
    5 000-row pages) so exports stream instead of building arrays; `csv_field`
    with RFC 4180 quoting **plus formula-leading escape** (`=`,`+`,`-`,`@`,tab →
    `'` prefix); documented header `timestamp_utc,host_id,entity_id,metric_id,
    value,unit,availability,source`; hard ceiling 2 000 000 rows with an
    in-file truncation note. **CSV import is intentionally not implemented**
    (phase §2.4 makes it conditional).
* **App commands** (`recordings.rs`): `start_recording` (metric whitelist, UUID id,
  UTC window), `stop_recording`, `list_recordings`, `export_csv` (single series)
  and `export_recording_csv` (session window × selected metrics × distinct
  entities, 64 entities/metric cap). Files are written **only** under the app's
  own `exports/` directory — the WebView never picks a write path.
* **Compare screen** (`features/compare/ComparePage.tsx`): up to **4** aligned
  series on a shared UTC time axis with **one labeled y-axis per distinct unit**,
  range selection (15 m/1 h/6 h/24 h/7 d) with resolution auto-selection matching
  the query layer (raw ≤30 min, 1 m ≤7 d, 1 h beyond, ≤400 points), gap-aware
  rendering (`connectNulls: false` — gaps are never bridged), an accessible data
  table equivalent, per-series CSV export and **recording controls** (labelled
  start from the charted series, stop, interrupted status, session export).
  ECharts 6.0.0 (Apache-2.0) is the chart engine specified by the stack.

### FX-019 — Secure optional remote monitor

* **`crates/monitor-remote`** (new; architecture §3's `monitor-remote` seam):
  * Axum 0.8.6 server on a dedicated thread + current-thread tokio runtime.
  * **Default OFF** — nothing constructs a listener without explicit opt-in;
    startup applies the configuration only when mode = loopback AND a token
    exists (starting token-less fails closed); app exit stops it unconditionally.
  * Read-only `GET /api/v1/{capabilities,system,snapshot,cpu,memory,disks,
    network,gpu,sensors,battery,bluetooth,processes,metrics/history}` + SSE
    `GET /api/v1/stream` + exactly one POST: `/api/v1/session`.
  * **Auth**: UUIDv4 tokens (OS RNG), persisted **only as SHA-256**; bearer
    header or HttpOnly+SameSite=Strict session cookie (in-memory, 12 h, ≤32,
    brute-force-limited); rotation swaps the hash + clears sessions atomically;
    revocation **stops the server** (no sentinel credentials).
  * **Bounded**: history capped at 90 days/400 points with metric whitelist;
    per-IP rate limiter (240/10 s general, 10/10 s auth) with bounded key map;
    SSE ≤8 clients, 4-slot buffers, 5 s slow-client drop.
  * **Private**: numeric-only projection (no addresses/names/hostname on data
    routes; `/system` is the explicit identity block); sanitized process rows
    (no paths/commands/parents); no CORS headers (browsers block cross-origin).
  * **Embedded read-only dashboard** (`/`): original design tokens, token →
    session → live SSE cards + table; no controls of any kind.
  * **LAN fails closed**: `RemoteMode::Lan` returns `LanRequiresTls` everywhere;
    the UI renders the option disabled. See ADR-0008.
* **App integration**: settings **v8** (`remote.mode/port`, `remote_token_hash`);
  `configure_remote` (validates 1024–65535, persists, starts/stops, mints the
  first token once), `remote_rotate_token`, `remote_revoke_token`,
  `get_remote_status`; port collisions surface as typed errors; startup
  `apply_remote_at_startup` + exit `shutdown_remote`.

## 2. Deterministic test coverage (phase §4 mapping)

| Scenario | Test |
|---|---|
| Remote OFF → no listening port | `monitor-remote` `off_mode_never_binds`; **live lsof** (below) |
| LAN without TLS rejected | `lan_fails_closed_even_with_valid_port` (both crates) |
| Missing/bad/rotated token blocked | `missing_and_bad_tokens_are_blocked`, `rotation_invalidates_old_token_and_sessions` |
| Non-GET actions rejected; no kill/Trash route | `authed_reads_work_and_writes_do_not_exist` (405 + 404) |
| Remote JSON excludes commands/paths/addresses | projection tests (`numeric_projection_excludes_all_text_metrics`) + live grep (below) |
| Oversized/bounded history | `history_route_is_bounded_and_whitelisted` |
| Session/cors/cookie handling | `session_exchange_and_dashboard` (HttpOnly+SameSite=Strict asserted) |
| Many clients/backpressure bounded | `sse_client_cap_holds`, flood → 429 |
| Comparison alignment/gaps/units | bounded query machinery unit-tested (phase 03) + `connectNulls:false`; table view |
| Recording start/stop/restart | `session_lifecycle_and_validation`, `open_sessions_are_marked_interrupted_on_startup` |
| CSV round trip + malicious formula content | `pages_are_ordered_bounded_and_resume_after_cursor`, `csv_escaping_quotes_and_neutralizes_formulas` |
| 1M-point downsampling | phase 03 stride decimation tests (query layer shared) |

## 3. Verification commands and exact outcomes

| Command | Result |
|---|---|
| `cargo fmt --all` + `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS — 0 findings |
| `cargo test --workspace` | PASS — **179 passed / 0 failed** / 3 ignored (monitor-remote 11 unit + 8 integration; monitor-history 7+9; app lib incl. settings v8) |
| `pnpm typecheck` / `pnpm lint` | PASS |
| `pnpm test` | PASS — 15 files, 97 passed / 0 failed |
| `pnpm build` | PASS |
| cross-target cargo checks | NOT RUN — targets not installed on this host (E0463), CI pending |

## 4. Native validation — macOS (Apple M3 Max, macOS 26, arm64)

Live smoke of the built binary over **real loopback HTTP**:

1. **OFF default confirmed by OS inspection**: with the shipped defaults,
   `lsof -iTCP:45454 -sTCP:LISTEN` reports nothing.
2. **Opt-in binds loopback**: with a seeded v8 settings row
   (`mode=loopback, port=45454, token_hash=sha256(token)`) the app started and
   `lsof` shows `app … TCP 127.0.0.1:45454 (LISTEN)`.
3. **Auth negatives**: no token → 401; wrong token → 401 (both `snapshot` and
   every other data route).
4. **Real data with a valid token**: `/api/v1/cpu` returned live per-core
   readings (`cpu.core.usage`, core:7 = 69.7 %, real values).
5. **Read-only enforcement**: POST `/api/v1/snapshot` → 405; POST
   `/api/v1/processes/kill` and `/api/v1/trash` → 404 (no such routes).
6. **Privacy projection**: the full `/snapshot` payload greps **0** hits for
   `net.interface.ipv4` and `host.hostname`.
7. **Bounded history**: real persisted samples returned with gaps/missing
   ratio (`missing_ratio: 0.895` honest for the young DB); 200-day range → 400.
8. **SSE**: `GET /stream` streamed `event: snapshot` frames with numeric
   readings.
9. **Dashboard**: `/` serves the read-only HTML; `POST /session` set
   `hostlume_session=…; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200`.
10. **Clean exit**: SIGTERM → process gone, `lsof` count 0 (port freed).
11. **Smoke hygiene**: the seeded settings row was restored to
    `remote.mode=off`, token cleared, and the smoke token file deleted.

## 5. Untested OS / hardware

| Target | State |
|---|---|
| LAN mode on a second device | **deferred with TLS** (fails closed; ADR-0008) |
| Remote server on Windows/Linux runtimes | compile-gated (cross targets not installed); pure library, no platform gates |
| Compare chart interaction / CSV export via UI clicks | manual pass pending (commands + format unit-covered) |
| 1M-point fixture downsampling at scale | shared query layer tested with bounded fixtures; no large synthetic soak this phase |

## 6. Security review (source locations)

* No write/control routes: `crates/monitor-remote/src/routes.rs` registers GET
  handlers + one POST; `fallback_404`; method routing 405. No handler reaches
  process actions, Trash, actions runner or the shell (the only spawn in the
  app remains `actions.rs`, unreachable from HTTP).
* Token lifecycle: `auth.rs` (`generate_token`, `hash_token`, constant-shape
  compare), settings store hash-only, `remote.rs` commands; `ServerShared`
  `Debug` skips token material.
* Rate limiting: `auth::RateLimiter` bounded key map (fail-closed under flood).
* Privacy: `projection.rs` numeric-only filter; `/system` identity block;
  process rows re-shaped in `remote::remote_data` closure.
* WebView cannot write files to arbitrary paths: CSV export writes only under
  the app data `exports/` dir (`recordings.rs::exports_dir`).
* Production never fabricates: server serves the shared real snapshot or an
  explicit `no_sample_yet` 503; recording is an annotation window, never a
  data source.

## 7. Blockers

None for the loopback gate. LAN + TLS remains a deliberate, documented gap
(ADR-0008) with its second-device validation deferred alongside it.

```text
PHASE: 08 — comparison/recording + secure optional web monitor
Implemented: monitor-history {sessions, export, migration V2}; monitor-remote (axum loopback server, token auth, read-only projection routes, SSE, dashboard); app settings v8 + configure/rotate/revoke + recording/CSV commands + startup/exit lifecycle; Compare screen (ECharts ≤4 series, table, exports, recording UI); Settings→Remote section; en+ro catalogs
Tests run: cargo fmt --check PASS; cargo clippy --workspace --all-targets -D warnings PASS (0); cargo test --workspace PASS 179/0/3 ign; pnpm typecheck PASS; pnpm lint PASS; pnpm test PASS 97/0; pnpm build PASS; cross-target NOT RUN (no targets installed)
Manual validation: macOS 26 / Apple M3 Max live loopback smoke (10 checks above, all PASS) incl. lsof OFF-default proof, 401/405/404 negatives, privacy grep, SSE frame capture, cookie flags, exit port-free; settings restored to off
Not tested: LAN on a second device (deferred with TLS), Windows/Linux runtimes (compile-gated), UI-click paths for Compare/export
Known limitations: LAN mode fails closed (ADR-0008); CSV import not implemented (optional per phase); session pinning vs retention not implemented — export covers recovery, warn-before-expiry is the retention hint shown in Compare
Risks/security: server default OFF; hash-only token storage; revocation stops the listener; rate limiting fail-closed; no CORS; projections exclude identifiers; no path from HTTP to local controls
Feature matrix updated: yes (FX-018 implemented, FX-019 implemented with LAN platform_limited)
Next phase: 09 — hardening and release (installers, perf/security/a11y QA, docs), ready pending owner acceptance
```
