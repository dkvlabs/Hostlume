# Phase 10 report — optional multi-host extension (FX-022)

**Date:** 2026-10-20 · **Owning features:** FX-022 · **Validated target:** macOS 27.0.1 (Apple M3 Max, arm64) ·
**New ADR:** [ADR-0009](../decisions/ADR-0009-multi-host-pairing-protocol.md) — protocol v1, pairing, TLS pinning, per-device tokens

Phase 10 adds what the phase doc asks for and nothing more: a console that can **read** a few user-owned
machines running this app's agent over the local network — TLS with certificate pinning, single-use
pairing codes bound to the certificate fingerprint, per-device revocation, bounded polling with honest
liveness (stale is never rendered as live), optional per-host history recording, and safe host removal
including its recorded history. No discovery, no relay, no cloud, **no remote action of any kind**.

The headline evidence: the **release bundle** was launched as a console against a labelled demo agent
(synthetic values, loopback bind) running the real `monitor-remote` TLS server — the Hosts page showed
the agent **Live** with polled values and both clocks (`docs/evidence/phase-10-hosts-live.png`), the
TLS connection was confirmed with `lsof`, **2,600+ readings were ingested into local history under the
agent's host id**, the poller cost measured **0.0–1.7 %** CPU, and the owner's settings row was restored
byte-identical afterwards.

## 1. What shipped

### Agent side (`crates/monitor-remote`)

* **`tls.rs`** — self-signed ECDSA P-256 identity via `rcgen` (generated at first LAN enable, persisted
  as DER, key file 0600, self-healing regeneration if stored material is unusable);
  `TlsListener` for `axum::serve` with off-loop handshakes (10 s timeout, 32-connection semaphore cap);
  `ClientAddr` connect-info for both listeners; fingerprint = SHA-256 of the DER certificate.
* **`pairing.rs`** — HMAC-SHA256 over the existing `sha2` crate (pinned to RFC 4231 vectors); 8-symbol
  (40-bit) codes from the OS RNG, 120 s TTL, **one attempt per code** (any verification consumes the
  window); console proof and agent acknowledgement on distinct domain strings; constant-shape compares.
* **`protocol.rs`** — `PROTOCOL_VERSION = 1`, authenticated `/api/v1/manifest` (host id, user-chosen
  display name, cadence, available families, TLS fingerprint — no hostname/serials/MACs), device
  records (hash only), `MAX_DEVICES = 16`.
* **`auth.rs`** — owner token + per-device bearer tokens behind one `authenticate_bearer`; last-seen
  tracking; hot-swappable device set (revoke takes effect on the next request).
* **`routes.rs`** — new `POST /api/v1/pair` (rate-limited, code-gated, LAN-only: plain loopback
  answers `409 pairing_unavailable`) and `GET /api/v1/manifest`; `requests_seen` diagnostic counter.
* **LAN mode** now actually starts — over TLS only (`LanRequiresTls` still fails closed without an
  identity); loopback stays plain HTTP exactly as phase 08 shipped it.

### Console side

* **`client.rs`** (in `monitor-remote`) — `PinnedVerifier` (trust = pinned fingerprint; handshake
  signatures verified through the ring provider; a mismatch records the presented fingerprint and fails
  before any request), `ObservingVerifier` (pairing-time TOFU that pins after first sight), `pair()`,
  `AgentClient::{manifest,snapshot}`, pure liveness derivation (`online ≤ 2 polls, stale ≤ 6, else
  offline; unauthorized/cert_mismatch/incompatible are terminal`), capped backoff (2 s → 60 s).
* **`apps/desktop/src-tauri/src/hosts.rs`** — credential store (separate `remote-credentials.json`,
  0600, never in settings/WebView/logs), one poller thread per host (≤ 8 hosts), bounded headline
  rings, `pair_host` (async, off the main thread; refuses a host id already paired at another
  endpoint), `remove_host` (+ optional `delete_host_samples`), `rename_host`, `set_host_recording`,
  `get_hosts_state`; startup starts workers **only** for paired hosts — an empty registry opens no
  socket.
* **Settings v9** — `remote_display_name`, `remote_devices`, `remote_hosts`, `remote_poll_seconds`;
  v8 rows migrate on load (config and token hash preserved); device/host records are **not**
  user-patchable — only the dedicated commands write them.
* **Frontend** — Hosts page (nav + ⌘7): card grid with status badges, both clocks (last response age +
  agent clock with skew), headline sparklines, record-history toggle; detail view with per-family
  readings table (every row carries the agent's timestamp) and explicit stale/offline warnings; pair
  form; remove flow with purge choice; Settings → Remote gains LAN mode, the machine's certificate
  fingerprint, the pairing window (code + countdown) and the paired-devices list with per-device
  revoke; en + ro dictionaries for all of it.

## 2. Threat model → tests (ADR-0009 §1)

Every adversary row in the ADR table lands on a test:

| Threat | Test |
|---|---|
| MITM at pairing (A2) | `pairing::proof_bound_to_another_fingerprint_is_rejected_and_burns`, integration `mitm_at_pairing_fails_and_burns_code` |
| MITM after pairing (A3) | `compromised_lan_device_never_receives_the_token` — the impostor agent's `requests_seen` stays **0** |
| Code guessing (A4) | `wrong_code_burns_the_window`, `version_and_shape_rejections_do_not_burn_the_window` (probes can't deny service to the real console) |
| Rogue agent (A8) | ack-proof verification; `AgentProofInvalid` never stores a token |
| Replay (A7) | single-use window + nonce |
| Token theft console-side (A5) | credential file 0600 + never in settings shape (compile-time) + WebView never sees tokens; per-device revocation (`revoked_device_is_unauthorized_immediately`) |
| Token theft agent-side (A6) | hashes only (`pair_over_tls…` asserts the persisted record is the hash) |
| Duplicate host id (A9) | `duplicate_device_replaces_credential_and_rename_keeps_identity` + console-side `HOST_ALREADY_PAIRED` |
| Clock skew / stale-as-live (A10) | `clock_skew_does_not_affect_liveness`, `liveness_ages_from_local_clock…`, UI "stale never rendered as live" |
| Discovery exposure (A11) | absent by design; `pairing_is_unavailable_on_plain_loopback` |

## 3. Defect found and fixed (pre-existing)

**Phase-06 regression:** `SettingsPatch` accepted `tray` and `launch_at_login` but `apply_patch` never
applied them — tray configuration changes and the launch-at-login toggle were silently dropped since
phase 06 (the OS login item toggled, the persisted setting did not). Fixed, pinned by
`patch_applies_tray_and_launch_at_login`, and recorded in the feature matrix.

## 4. Verification commands and exact outcomes

| Command | Outcome |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS — 0 findings |
| `cargo test --workspace` | **223 passed, 0 failed, 4 ignored** (was 183+4: +29 monitor-remote units, +11 multi-host integration, +4 app hosts/settings, +3 monitor-history/settings-store, −3 absorbed) |
| `pnpm --dir apps/desktop typecheck` / `lint` | PASS |
| `pnpm --dir apps/desktop test` | **19 files, 117 tests passed** (was 108) |
| `pnpm --dir apps/desktop build` | PASS |
| `pnpm --dir apps/desktop tauri build` | PASS — DMG `44f7a91e05f20d2359caab3b09355890039f69f87374fa25aed41f4069e69dc3` (6,827,582 B), binary `7f81d03d2b5ee1d7b634a25eccfbd060cb8f60f8a6fe1ab02c96d2d898f2d9bc` (19,420,992 B), ad-hoc linker-signed, `hdiutil verify` VALID, `LSMinimumSystemVersion 11.0` |
| `cargo run -p monitor-remote --example demo-agent` | live harness (see §6) |

Measurement-methodology correction (phase 09 note): `top -l 2 -stats cpu` prints **system-wide** CPU
lines; phase 10's process numbers use `ps -o %cpu` spaced samples instead. The 40–58 % figures observed
mid-smoke were the system line while the machine was busy, not the app.

## 5. Exit gate (phase doc §"Exit gate")

- [x] **No implicit networking before opt-in** — the console connects only to hosts the user paired
      (missing credential → no worker, unit-tested); the agent listens only in LAN mode with a TLS
      identity (fail-closed retained).
- [x] **TLS, pairing, revocation and MITM tests validated** — the 11 `tests/multi_host.rs` scenarios
      over real loopback HTTPS with pinned clients (§2).
- [x] **User can safely remove hosts and associated history** — `remove_host` stops the poller, drops
      the credential and the record, optionally purges all three sample tiers scoped to that host id
      (`deleting_one_host_leaves_every_other_host_intact`); the local machine's history is refused.
- [x] **Each metric shows host identity and timestamp; stale data never presented as live** — every
      reading row carries the agent timestamp; the card carries both clocks; `stale`/`offline` render
      warnings and never the Live badge (UI tests + live smoke).
- [x] **Resource and privacy impact measured** — §6/§7/§8.

## 6. Live validation — release bundle as console (macOS 27.0.1, M3 Max)

Setup: the rebuilt release bundle; `demo-agent` (labelled DEMO, synthetic sine-wave values, loopback
bind) on 127.0.0.1:45993 with a pre-registered device credential; the owner's settings row backed up,
a v9 row seeded with one host (`record_history: true`, 2 s poll), the credential file written 0600.

* Hosts page (⌘7) rendered the agent card **Live** — CPU 38.9 %, memory 12.6 GiB, sparklines, last
  response "1 s ago", agent clock shown, skew −2.0 s displayed as display-only
  (`docs/evidence/phase-10-hosts-live.png`).
* `lsof`: `app 40012 → 127.0.0.1:45993 ESTABLISHED` — the poller's TLS connection.
* History ingestion: **2,600 readings / 5 series** under the agent's host_id (timestamps = the agent's
  own clock), proving per-host recording works end-to-end.
* Clean shutdown (⌘Q) and teardown: settings row restored byte-identical (schema 8, 82 bytes —
  re-verified by re-reading), credential file removed, no listener left.

## 7. Resource impact

* Console, polling one agent at a 2 s interval (worst case in this phase): main process
  **0.0–1.7 %** CPU (`ps` instantaneous, spaced samples; 0.0 % after the agent stops), RSS
  **94–101 MB** — the poller thread sleeps between polls; per-host cost is one HTTPS request per
  interval plus a bounded store update.
* Agent: unchanged phase-08 server surface plus TLS framing; the demo harness is not part of the app.
* History growth from a recorded host is the same 250 B/row economics as local rows — bounded by the
  same retention controls, and removal purges it on request.

## 8. Privacy review

What leaves a machine in LAN mode: the numeric-only snapshot projection (unchanged from phase 08), the
sanitized process page, the bounded history queries, and the manifest — whose only identity fields are
the installation UUID and a **user-chosen** display name. Hostnames, serials, MACs, interface
addresses, filesystem names and text metrics are structurally excluded (phase-08 projection tests
still green). Console-side: device tokens live only in the 0600 credential file and never reach the
WebView, settings, exports or logs; pairing codes exist only in the pairing window and the one request.

## 9. Untested / not in this phase

* **A real second machine over physical LAN** — no second device was available; the LAN bind
  (0.0.0.0), addressing on a real network, and Wi-Fi interruption behaviour are unvalidated on
  hardware. The loopback-bound TLS path (production code path, test-seam address only) is what ran.
* Windows / Linux: compile verification pending CI as in every prior phase; no runtime validation.
* Discovery, keychain storage, SSE-based ingestion, remote Applications/Storage pages — explicitly
  not in v1 (ADR-0009 §6).
* The phase-09 owner decisions (signing/notarization, history-growth default, LAN/TLS deferral) are
  unchanged by this phase; LAN/TLS is now *implemented* but still needs the real-device validation
  before the deferral can be considered closed.

## 10. Blockers and next steps

No engineering blocker. Before calling FX-022 verified on hardware: two-device LAN validation
(pairing from the UI, revoke from the agent, remove-with-purge, sleep/resume of both sides).
Owner decisions from phase 09 still stand. Phase 10 was the last phase in `implementation-phases.md`;
the remaining work is the rc1 → release checklist (PHASE-09.md §13) plus the two-device validation
above.

```text
PHASE: 10 — optional multi-host extension
Implemented: crates/monitor-remote (tls/pairing/protocol/client modules, LAN-over-TLS, manifest+pair routes, per-device tokens), crates/monitor-history (delete_host_samples), apps/desktop/src-tauri (hosts.rs runtime+commands, settings v9, remote.rs LAN/pairing/devices), frontend (Hosts page, hostsStore, RemoteSection LAN/pairing/devices, nav, en/ro); ADR-0009; demo-agent example (dev tool). FX-022.
Tests run: cargo fmt --check PASS; clippy -D warnings 0 findings; cargo test --workspace 223 passed/0 failed/4 ignored; pnpm typecheck+lint PASS; vitest 19 files/117 passed; pnpm build PASS; tauri build PASS (DMG 44f7a91e…69dc3, binary 7f81d03d…f2d9bc, hdiutil VALID).
Manual validation: macOS 27.0.1 / M3 Max — release bundle as console vs demo TLS agent on loopback: Hosts page Live with polled values + both clocks + skew; lsof ESTABLISHED; 2600+ readings ingested under the agent host id; poller 0.0–1.7% CPU, RSS 94–101 MB; clean quit; owner settings restored byte-identical.
Not tested: real second machine over physical LAN; Windows/Linux runtime; discovery/keychain/SSE ingestion (out of scope v1).
Known limitations: LAN bind 0.0.0.0 unvalidated on hardware; device tokens in a 0600 file (keychain follow-up); demo-agent harness is synthetic and dev-only.
Risks/security: no remote action routes; pairing bound to certificate fingerprint with burn-on-failure; revocation immediate; console credential file 0600 and never in the WebView; projection privacy unchanged (phase-08 tests green); phase-06 settings-patch defect fixed.
Feature matrix updated: yes (FX-022 implemented; phase-10 evidence records)
Next phase: none defined — remaining: rc1→release checklist + two-device LAN validation
```
