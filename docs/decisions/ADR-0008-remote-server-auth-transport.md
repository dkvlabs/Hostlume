# ADR-0008 — Remote server transport, auth and the LAN gate

**Status:** accepted (phase 08); LAN gate lifted in phase 10 by [ADR-0009](ADR-0009-multi-host-pairing-protocol.md) — LAN mode ships over pinned TLS only, and still fails closed without a TLS identity
**Date:** 2026-10-19
**Context:** FX-019, `docs/phases/08-comparison-remote.md` §3, `docs/security-privacy.md` §6

## Decision

### 1. Axum 0.8 in its own crate, owned runtime thread

`crates/monitor-remote` is a pure library: the app hands it read-only data
seams (shared snapshot `Arc<RwLock<…>>`, shared capability report, history
*directory* for per-query WAL connections, and a sanitized process-row
closure). The server runs on its own thread with a current-thread tokio
runtime and a std `TcpListener` converted to non-blocking — no runtime is
shared with Tauri's, and there is **no route into Tauri commands**.

### 2. SSE over WebSocket

The live stream is **SSE** (`GET /api/v1/stream`): one-way push matches the
need (snapshots to viewers), needs no upgrade handshake or extra protocol
state, survives proxies better, and the browser side is a native
`EventSource`. Boundedness: max 8 concurrent clients (further subscribers
get an explicit `503 client_limit`), a 4-slot per-client buffer, and the
pusher drops a client whose buffer cannot drain within the 5 s send budget.
Events carry the numeric-only projection — no host identifiers.

### 3. Token auth: OS RNG, hash-only persistence, cookie bridge

Tokens come from UUIDv4 (122 OS-random bits) and are persisted **only as
SHA-256** in the settings row; the plaintext is returned to the UI exactly
once at generation/rotation and never logged or stored. Every data route
accepts `Authorization: Bearer …`. Because `EventSource` cannot set
headers, `POST /api/v1/session` exchanges the token for an in-memory,
`HttpOnly; SameSite=Strict` session cookie (12 h TTL, ≤32 sessions,
oldest-evicted). This is the only POST route. Auth attempts are
rate-limited per IP (10 / 10 s) separately from the general limiter
(240 / 10 s). Rotation swaps the hash and clears all sessions atomically;
**revocation stops the server** — running with a sentinel credential would
be a guessable secret.

### 4. Read-only and private by construction

Routes are `GET` only (+ the one POST); anything else is 405/404 at the
router — there is no kill/Trash/shell handler and no IPC bridge. The
projection layer (`projection.rs`) emits **numeric readings only** on
family/snapshot/stream routes, so interface addresses, filesystem names,
Bluetooth names and the hostname cannot leak; `/system` is the single,
explicit identity projection. `/processes` re-serializes a sanitized row
shape (pid, name, host-normalized CPU, RSS, status — no paths, no command
lines, no parent pids). History is whitelisted to known families, bounded
to 90 days / 400 points, and opens its own WAL connection. No CORS headers
are emitted anywhere, so browsers refuse cross-origin reads.

### 5. LAN mode ships disabled — fails closed

TLS provisioning (certificate generation, trust UX, renewal) was not
implemented or validated in this phase, and phase §3.10 forbids LAN
without validated TLS + auth. Therefore `RemoteMode::Lan` exists in the
schema (so the setting round-trips) but every path that could start a
non-loopback listener returns `LanRequiresTls` and the UI marks the option
unavailable. The loopback server itself is live-validated (PHASE-08.md);
a second-device LAN test is explicitly deferred until TLS ships.

## Consequences

* Settings schema advances to **v8** (`remote: {mode, port}`,
  `remote_token_hash: Option<String>`); v1→v8 load-time migration chain.
* The WebView cannot set arbitrary listen ports: `configure_remote`
  validates 1024–65535, persists, and starts/stops the listener. Port
  collisions surface as a typed `REMOTE_PORT_UNAVAILABLE` error.
* The default remains **no listener**; startup applies the configuration
  only when mode = loopback *and* a token hash exists (starting an open
  server is a typed failure, never a fallback), and app exit stops it.
* `result_large_err` is consciously allowed in the routes module: the
  reject paths are cold and `Box<Response>` has no IntoResponse impl in
  axum, so boxing would buy nothing.
