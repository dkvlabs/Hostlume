# Phase 10 (optional, **post-v1**) — Multiple-computer monitoring

**Owns:** FX-022 (new product enhancement; **not iStatistica parity**).  
**Outcome:** Securely view a few user-owned macOS, Windows and Linux machines from one console. Do not start until phase 09 release gates pass and the user explicitly wants it.

## Requirements and architectural questions

- Use a small read-only host-agent or the existing server on each machine; create new separate protocol/versioning ADR.
- Require deliberate pairing with short-lived codes or secure keys, device identity, token rotation and per-device revoke.
- Avoid public Internet relay, cloud broker and port forwarding by default; start trusted local LAN mode with TLS and proper cert validation.
- Allow graceful offline state, last-seen timestamp, per-host data retention/labels and capability differences.
- No fleet remote command execution, process termination, file deletion or fan control.

## Implementation path

1. Threat model pairing, discovery, replay/MITM and token theft before code.
2. Versioned read-only agent manifest/capability handshake and host names that do not expose serials/MACs.
3. LAN discovery **only with opt-in**, rate limits and authenticated pairing.
4. Normalized remote metrics ingestion into bounded in-memory/store; local history per host if selected.
5. UI: host switcher and grid, offline/stale warnings, per-host comparison, source label, permissions.
6. Test reconnect, revoked key, clock skew, duplicate host ID, device renamed, compromised LAN device and network partition.

## Exit gate

- [ ] No implicit networking before opt-in.
- [ ] TLS, pairing, revocation and MITM tests validated.
- [ ] User can safely remove hosts/associated history.
- [ ] Each metric shows host identity and timestamp; stale data never presented as live.
- [ ] Resource and privacy impact measured.
