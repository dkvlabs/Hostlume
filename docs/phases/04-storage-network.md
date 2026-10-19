# Phase 04 — Storage treemap, safe cleanup, disk I/O and network details

**Owns:** FX-008, FX-009, FX-011.  
**Outcome:** Understand where storage goes, clean up safely, and diagnose per-interface traffic and physical-device throughput.

## 1. Preconditions

- Phase 03 database/history support and guarded Rust action pattern.
- User consent and permissions framework are available for scanning/Trash.

## 2. Storage analyzer work order

1. Implement `ScanOptions`: chosen root, follow_symlinks **false by default**, cross_filesystems false by default, error policy, max concurrency and cancellation token.
2. Write traversal engine with bounded queue, iterative walk, symlink/reparse/hardlink handling, hidden/inaccessible directory status and reason codes.
3. Track `logical_size`, `allocated_size` if available, file and directory count, visited bytes and errors; document uncertainty for APFS clones, deduplication and snapshots.
4. Build paginated/lazy scan tree API (`start_scan`, `cancel_scan`, `get_scan_node`, `get_scan_children`, `get_scan_status`) to prevent huge JSON payloads.
5. Implement interactive treemap with breadcrumbs, top-N aggregation, table alternative, path metadata and search within known results. No unbounded DOM nodes.
6. Implement OS-native **Move to Trash / Recycle Bin** only for explicitly selected items. Revalidate file identity/path scope after selection; reject deletion of app config directories/critical locations without explicit policy. No `rm -rf`, no unsafe delete fallback.
7. Add open-in-file-manager and optional guarded eject flow only where supported. Log errors in local UI without revealing sensitive paths externally.
8. Cache scan results within configured bounds; invalidate on root changes and manual rescan.

## 3. Disk I/O and network work order

9. Add per-physical/logical-device read/write byte counters or documented best available OS source; calculate throughput via monotonic interval and device generation.
10. Build disk I/O charts and read/write unit toggles; do not duplicate counts when device mapper/virtual volume layers overlap.
11. Extend Network page: interface selector/cards, addresses, gateway, link state, bytes/bits rate preference, receive/send cumulative totals and meaningful chart ranges.
12. Handle Wi-Fi/Ethernet/VPN/hotplug, interface ID reuse, counters changing after wake; show disconnected state and retain correctly timestamped history.
13. **External IP and country:** default off. Only perform explicit user opt-in lookup; record provider, timeout and privacy text; do not claim lookup is local.

## 4. Required tests

- Filesystem fixtures: deeply nested trees, permission denied, Unicode paths, symlink loop, junction/reparse points (Windows), hardlinked files, large sparse file, mount boundary, scan cancel midwalk, delete-between-selection-and-action.
- Trash: OS-native transfer confirmed; permission failure preserved; impossible Trash does **not** permanently delete file.
- Network: 1s/3s rate math, counter wraps/resets, virtual interfaces, two simultaneous adapters, repeated interface toggle without stale overlay.
- Disk: USB unplug while graph open, process resumes after sleep, disk failure/source unavailable, storage volume IDs remain stable.
- UI: scan progress updates smoothly without blocking large directory UI, keyboard-accessible breadcrumb and table fallback.

## 5. Acceptance gate

- [ ] Disk scan of user-selected root completes or cancels cleanly, reports partial errors.
- [ ] Treemap and detail panel accurately use reported sizing semantics.
- [ ] Trash request requires explicit confirmation, never runs permanent delete implicitly.
- [ ] Disk I/O only shown when actual device counters exist; correct reset gap handling.
- [ ] Network remains accurate with VPN/virtual adapters and physical changes.
- [ ] External lookups remain opt-in and disabled in default settings.
- [ ] Evidence in `docs/progress/PHASE-04.md` with real tested filesystems/OSes.
