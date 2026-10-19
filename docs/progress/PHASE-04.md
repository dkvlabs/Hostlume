# Phase 04 — Storage treemap, safe cleanup, disk I/O and network details

Date: 19.10.2026
Commit: this commit
Feature IDs: FX-008, FX-009, FX-011 (→ `in_progress`: macOS-validated scan/Trash/network detail; Linux disk I/O code compiles + unit-tested but NOT hardware-run; FX-020 continuation for display preferences)
Status: complete (awaiting owner acceptance)

## Changes implemented (code map)

### Storage analyzer (FX-008)

- **`crates/monitor-storage`** (new crate; trash 5.2.9 MIT — reviewed in the Phase 00 scan):
  - `scan.rs` — `ScanOptions` (root; `follow_symlinks` **false by default**; `cross_filesystems` **false by default**; error policy ContinueAndCount/Stop), iterative BFS walk with cancellation token, flat-tree model with parent/children indices. Symlinks recorded with their own size as **skipped statuses** (`SYMLINK_NOT_FOLLOWED`), never followed — loop-proof (test). Mount boundaries detected via device id and recorded `MOUNT_BOUNDARY_NOT_CROSSED`, not crossed. Logical size per entry + best-effort allocated size (unix blocks; documented APFS clone/dedup/sparse uncertainty with a sparse-file test proving logical > allocated). Hardlinks counted per path with explicit uncertainty (test). Rollup pass computes totals bottom-up (children always have higher indices).
  - `trash.rs` — OS-native-only Move to Trash: canonicalize → **scope check against session scan roots** → protected-location list (true system dirs only, so macOS temp stays usable) → `trash::delete`. **Fail closed**: missing/refused/failed leaves the file in place; there is no permanent-delete fallback. Delete-between-selection-and-action test-proven.
- **App wiring** (`storage.rs` + commands): scan registry with per-scan threads and live progress; `start_scan` (typed invalid-root rejection), `cancel_scan`, `get_scan_status`, `get_scan_children(scan_id, node_index, offset, limit ≤ 1000)` — **paginated, size-descending slices; the UI never receives the whole tree**; `move_to_trash` (1..=100 paths, confirmation id required, scope-validated, per-path results); `open_in_manager` (named opener binary + arg array — no shell interpolation); `lookup_external_ip` — **opt-in only**, typed refusal when disabled, and a typed `EXTERNAL_LOOKUP_TRANSPORT_UNAVAILABLE` because no HTTP client has been license/approved-reviewed yet (documented, honest).

### Disk I/O (FX-009)

- `collectors/disk_io.rs` — **Linux**: real `/proc/diskstats` counters (sectors × 512), synthetic layers (loop/ram/dm-/md*) excluded so device-mapper stacks are not double counted; per-device `RateTracker`s with `WARMUP`/`COUNTER_RESET`/`SLEEP_GAP` semantics identical to network. **macOS/Windows: explicit `unsupported` capability with `SOURCE_NOT_AVAILABLE_IN_BUILD`** — sysinfo 0.39 has no disk I/O surface (verified) and no IOKit/Win32 adapter is wired; zero fabricated readings (empty sample, unit-tested).
- Family ids `disk.device.write_bytes_total`/`write_rate` added (read-side ids existed).

### Network detail (FX-011)

- **Network page**: interface selector (auto-fallback on unplug/rename), down/up rates with **bytes/s ↔ bits/s display preference** (settings v4 `rate_unit`), cumulative totals, IPv4/IPv6 addresses, link state, and an honest gateway row ("not exposed by this source" — sysinfo has no gateway API).
- **External IP**: settings v4 `external_lookup` (**disabled by default**, fixed provider ipify, bounded timeout); the page button is disabled until opt-in, and the command reports the typed unavailable state — no unreviewed network transport ships in this build.
- **Settings v4** + tested migration chain v1→latest, v2→v4, v3→v4 (defaults for rate unit + external lookup; opt-in stays OFF through every migration path).

## Verification

| Environment | Command / scenario | Result | Evidence |
|---|---|---|---|
| macOS arm64 | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` | PASS | this run |
| macOS arm64 | `cargo test --workspace` | PASS — **81 tests, 0 failed** (storage: deep-tree totals, Unicode names, symlink loop termination + statuses, hardlink uncertainty, sparse logical>allocated, cancellation, permission-continue, stop-policy, invalid root; trash: scope/missing/protected/vanished + **live macOS trash move**; diskstats parse incl. synthetic exclusion; per-platform honest capabilities; settings v1→v4 migrations; disk_io unsupported platform emits nothing) | this run |
| macOS arm64 | Trash crate license re-checked: MIT (trash 5.2.9 vendored Cargo.toml) | PASS | this run |
| macOS arm64 | `pnpm lint` / `typecheck` / `test` / `build` | PASS — **72 tests**: scan lifecycle UI, treemap/table views, breadcrumb navigation, trash confirmation flow, network selector, honest gateway/external-IP rows, settings v4 guards/fixtures | this run |
| macOS arm64 | Live app run (fresh history, 12 s): CPU ≈ 0%, RSS 122 MB, clean exit | PASS | this run |
| Linux x86_64 | /proc/diskstats collector against a real kernel | NOT_RUN — no Linux hardware/runner; parser + exclusion logic unit-tested; diskstats format per procfs man page | R10 |
| Windows x64 | any | NOT_RUN — no hardware/runner | R10 |

## Security and data integrity

- **Trash is fail-closed and scoped**: canonicalized paths must live inside a session scan root; scan roots themselves, missing paths (delete-between-selection-and-action), and system locations are refused; OS-trash failure is surfaced, never replaced by permanent deletion. All refusals/audit rows keep full paths local (UI only).
- **Scans stay in scope**: no symlink following, no mount crossing, no absolute paths in scan payloads (children carry names + indices only) — nothing sensitive leaks into the UI layer or beyond.
- **External IP stays opt-in and transport-less**: default OFF in every migration path; the command is typed-refusing without an approved HTTP client — no undocumented network call exists.
- **No double counting**: Linux synthetic layers excluded; the macOS empty/unsupported path is explicit.
- `open_in_manager` uses a fixed binary + argument array; no command interpolation.

## Blockers / untested

- **Linux disk I/O hardware run**: parser unit-tested, but the collector has not read a real `/proc/diskstats` — first Linux CI/hardware run must verify sector sizes (the 512 B convention) and exclusions on real mounts.
- **Windows**: scan engine's `device_id` returns None (mount boundary detection off) — documented; allocated size = logical (best effort), reparse-point handling follows in a Windows pass.
- **Treemap**: compact flex-tile view of top-level children (bounded DOM); a full squarified hierarchy with zoom is a follow-up refinement — the table alternative and drill-down already satisfy the phase's pagination/no-unbounded-DOM requirements.
- **External IP transport**: needs an approved HTTP client dependency (license + audit) before opt-in can actually resolve; until then the command honestly reports transport unavailability.
- **Eject flow**: deferred (phase doc marks it optional/where-supported); guarded command slot reserved.

## Feature matrix edits

- FX-008: `planned` → **`in_progress`** — scan engine + scoped Trash + lazy tree API + UI, macOS-validated with live trash move.
- FX-009: `planned` → **`in_progress`** — Linux collector implemented + unit-tested (hardware run pending); macOS honestly unsupported with reason codes.
- FX-011: `planned` → **`in_progress`** — network detail page with selector/addresses/totals/rate preference; gateway absent-by-source documented; external IP opt-in plumbing complete, transport pending.
- FX-020 remains `in_progress` (rate-unit display preference added).

## Gate review (phase doc §5)

- [x] User-selected scan completes or cancels cleanly with partial-error reporting (progress + error counts + skip statuses).
- [x] Treemap/detail use reported sizing semantics (logical vs allocated labeled; uncertainty documented and tested).
- [x] Trash requires explicit confirmation; no implicit/permanent delete path exists (fail-closed tests).
- [x] Disk I/O only when real device counters exist (Linux capability; macOS explicit unsupported with reason).
- [x] Network accurate across adapters with honest gateway/external-IP states.
- [x] External lookups opt-in and disabled by default (setting + typed refusal).
- [x] Evidence recorded in this file with real OS runs (macOS) and explicit NOT_RUN rows.

**Handoff:** "Phase 04 completed with a tested storage scanner, scoped OS-native Trash, honest disk-I/O capabilities, and a detailed network page on macOS arm64. Phase 05 (GPU, sensors & power, FX-010/012–014/021) may start after owner acceptance; CI baseline for Windows/Linux still lands with the first push."
