# Phase 08 — Comparison/recording + secure optional web monitor

**Owns:** FX-018, FX-019.  
**Outcome:** Durable performance sessions and user-controlled read-only remote visibility without weakening local machine security.

## 1. Preconditions

- History and bounded query APIs from Phase 03; capability data from Phase 05.
- Threat model and remote-server requirements accepted in `docs/security-privacy.md`.

## 2. Comparison and CSV work order

1. Implement series query across `metric_id + entity_id + UTC time range`, with selectable raw/1m/1h resolution, gaps and availability percentages.
2. Implement Compare screen with up to 4 aligned time series; independent labeled axes when units differ, accessible table equivalent and zoom/export controls.
3. Recording service: start/stop named session, selected metrics, persisted UTC session boundaries and metadata; power loss/crash recovers an interrupted recording state.
4. CSV export has documented header, units, time zone, source and missingness; stream large exports rather than building huge arrays. If CSV import is included, validate schema strictly and import into separate recording namespace so it cannot impersonate live telemetry.
5. Retention/session interaction: notify before expiring session data; user may export or pin within storage limits.

## 3. Remote server work order

6. Implement optional Axum server behind explicit Settings opt-in; default OFF and no listener. Initial bind `127.0.0.1`, configurable port with collision error.
7. Implement *read-only* JSON `/api/v1` routes for `capabilities`, `system`, `snapshot`, `cpu`, `memory`, `disks`, `network`, `gpu`, `sensors`, `battery`, `processes` (sanitized fields) and `metrics/history` with explicit whitelist/projection.
8. Add authenticated SSE/WebSocket stream (choose via ADR) with bounded per-client buffers, closed on slow-client overload and no unnecessary host identifiers. No remote calls to Tauri commands.
9. Implement strong token generation/storage, rotation/revocation, request rate limiting, response/query size caps, strict CORS and no token logging/URL tokens.
10. Add LAN opt-in mode **only after TLS + auth validation**; reject non-loopback binds if missing required protection. Provide transparent UI with address/port/certificate trust instructions.
11. Build responsive read-only browser dashboard matching original design tokens, not copying proprietary reference layout. Good mobile views, no process controls/Trash buttons.
12. Create integration tests and security reviews with OS firewall interactions. Verify optional server shuts down on app Quit and doesn't launch at login unless configuration explicitly permits it.

## 4. Tests

- Comparison alignment across UTC/DST, missing data gap, mismatched units, 1M-point fixture query downsampling.
- Recording start/stop/restart, CSV export/import round-trip and malicious CSV formula content handling (escape formula-leading values when spreadsheet-friendly CSV is enabled).
- Remote OFF default yields **no listening port**; loopback explicit; LAN without TLS/token rejected.
- Missing/bad/rotated token blocked, incorrect CORS rejected, oversized requests limited, many clients/backpressure bounded.
- Remote JSON excludes sensitive full process commands, local file paths, tokens, network hardware addresses by default.
- Attempt remote POST/DELETE/process kill/Trash/shell routes; no handler exists.
- TLS certificate verification and firewall/host scoping tested on a *second device* for LAN mode.

## 5. Acceptance gate

- [ ] Compare and history display actual values/units/gaps.
- [ ] Recording persists and exported CSV is documented and reloadable if import is supported.
- [ ] Server OFF by default (confirmed OS port inspection).
- [ ] All remote routes are read-only with bounded queries, auth and privacy projection.
- [ ] LAN mode fails closed without TLS/auth; real external client can view only after deliberate configuration.
- [ ] Tested native targets/security artifacts in `docs/progress/PHASE-08.md`.

**Never:** add remote process termination or dynamic command execution “for convenience.”
