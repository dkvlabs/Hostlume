# ADR-0003: IPC transport — Tauri commands + one broadcast snapshot event stream

Status: accepted
Date: 19.10.2026
Owner: principal engineer (Phase 00 agent)
Related features: FX-001…FX-006, FX-019 (all live-data consumers)

## Context

The core service collects on a fixed cadence (fast 3 s / medium 5–10 s / slow 30–120 s per architecture.md) and must feed several independent consumers: main window, tray popover, future widget snapshot cache, and the optional remote server projection. Consumers appear/disappear (windows open/close, sleep/resume). The WebView is untrusted input; nothing stateful may live in JS beyond presentation.

## Decision

1. **Request/response:** Tauri 2 `invoke` commands with Rust-side schema validation for every argument (`get_capabilities`, `get_snapshot`, `get_history`, `get_processes`, action commands with confirmation ids, settings commands) exactly as sketched in architecture.md §IPC contract.
2. **Live telemetry:** a single global event **`telemetry://snapshot`** emitted per collection tick, payload carrying `{schemaVersion, hostId, generation, sequence, collectedAtUtc, metrics[] (delta-capable), capabilitiesRevision}` per ADR-0001. Bounded broadcast channel with `drop_oldest` policy and backlog instrumentation; the emitter never blocks collectors.
3. **Resubscribe protocol:** on window load, event-listener re-attach, or detected `sequence` gap/generation change, the client calls `get_snapshot` for full state before applying further deltas. Stale events (older sequence) are ignored client-side.
4. **Remote server** (Phase 08) subscribes to the same in-process snapshot stream — it never polls hardware and never reaches action handlers.
5. **No per-card timers, no per-metric channels.** Poll groups live in the core scheduler only.

## Alternatives considered

- **`tauri::ipc::Channel` per command:** viable in Tauri 2 for streaming responses, but a channel is tied to one invoking window; multi-consumer broadcast (tray + window + widget cache + remote) would need a fan-out layer anyway. Events give us one well-understood broadcast primitive. Revisit only if payload size at 3 s cadence becomes a measured problem.
- **Polling `get_snapshot` from JS at 1 s:** rejected — wasted work, jitter, and it breaks the "single scheduler" rule.
- **Direct Zustand ↔ Rust state sync library:** rejected — no maintained, reviewed option; Zustand stays presentation-only.

## Consequences

Frontend needs a thin typed IPC adapter (subscribe/resubscribe/gap logic) shared by all windows; sequence/generation handling is unit-testable without Tauri by feeding the adapter recorded event fixtures. Payload size must be monitored (delta encoding of unchanged static metadata is allowed since `capabilitiesRevision` separates capability churn from metric churn).

## Evidence

- Tauri 2 stable 2.12.4 verified (crates.io API, 19.10.2026); events/commands are core documented Tauri 2 features (v2.tauri.app).
- Transport design constraints: docs/architecture.md §event transport, §IPC contract; docs/metrics-data-contract.md §2 (sequence/generation fields).

## Follow-up

Phase 01 must prove: emission cost at full metric cardinality on the slowest target, listener behavior across sleep/resume, and WebView resubscribe after window reload. If `Channel` proves materially cheaper for large payloads, supersede this ADR with measurements, not assumptions.
