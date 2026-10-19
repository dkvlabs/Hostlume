# Architecture — cross-platform desktop system monitor

> **Status: proposed architecture; not implemented.** Architect for feature parity without pretending that private macOS sensors, Windows GPU counters or every Linux desktop widget are uniformly available.

## 1. Design drivers

1. **Truthful telemetry:** clear metric semantics, proven collection source and explicit missingness.
2. **Native where essential:** system calls and hardware vendor integration in platform adapters, not in React.
3. **Low overhead:** shared collection schedule, bounded streams, batch persistence, minimal polling.
4. **Security by default:** local/offline operation, read-only remote API, guarded destructive operations.
5. **Capability negotiation:** same UI/components with platform-specific availability.
6. **Independent implementation:** no ImageTasks proprietary code or dependency on its sensors plugin.

## 2. Runtime diagram

```text
                  Tauri Desktop Host (one process, optional helper)
┌───────────────────────────────────────────────────────────────────────────┐
│ WebView: React/TypeScript                                                 │
│  Dashboard | Processes | Storage | Network | Sensors | Compare | Settings│
│  Typed IPC client -> Zustand (derived UI state) -> bounded ECharts series │
├─────────────────────── Tauri IPC / events ────────────────────────────────┤
│ Rust App Runtime: shared state / commands / lifecycle                     │
│     │                         │                   │                       │
│     ▼                         ▼                   ▼                       │
│ Collector Scheduler       Action Gate         Tray / Popover / Widgets    │
│     │                         │                   │                       │
│     ├── portable sysinfo      ├─ PID validated    └── consume snapshots    │
│     ├── macOS native          ├─ OS Trash                                 │
│     ├── Windows native        └─ opt-in workflow                          │
│     └── Linux native                                                     │
│     │                                                                     │
│ Normalizer -> Snapshot Bus ──┬── live IPC events -> React                   │
│                              ├── alerts engine -> OS notifications         │
│                              ├── history writer -> SQLite (WAL)           │
│                              └── optional Axum server -> read-only API    │
└───────────────────────────────────────────────────────────────────────────┘

Web remote clients connect only if explicitly enabled. Native widget/OS extension may
be a separate signed target that reads restricted shared snapshots where supported.
```

## 3. Suggested repository structure

```text
AGENTS.md
README.md
docs/
apps/
  desktop/
    src/
      app/                # routing, layouts and provider setup
      components/         # shared UI, metric states, charts, dialogs
      features/           # dashboard, processes, storage, network, etc.
      lib/ipc/            # one typed Tauri IPC adapter
      lib/formatting/     # units, locales, time
      stores/             # Zustand presentation state
      styles/             # tokens + Tailwind theme
    src-tauri/
      src/                # Tauri app startup, command handlers
      capabilities/       # explicit least-privilege IPC permissions
      tauri.conf.json
crates/
  monitor-core/           # MetricEnvelope, Clock, Snapshot, Collector trait
  monitor-platform-macos/ # Apple-specific collectors
  monitor-platform-windows/
  monitor-platform-linux/
  monitor-history/        # SQLite migrations, rollups, retention
  monitor-storage/        # safe async scanning and Trash coordinator
  monitor-alerts/         # rule evaluator
  monitor-remote/         # optional Axum read-only server
native/
  macos/                  # optional native helper / WidgetKit extension
  windows/                # conditional integrations
  linux/                  # tray environment integration
scripts/                  # dev checks, release helpers
.github/workflows/        # native build/test pipelines
```

These are **recommended** names; document justified changes before applying them. Avoid adding extra crates until a real build seam exists. Rust libraries must compile with `cfg(target_os = ...)` gates, and the overall workspace needs CI jobs for each native OS.

## 4. Collector interfaces and capabilities

The Rust core should define behavior, not OS implementation. Conceptual contracts (pseudocode, **not compiled Rust**):

```text
trait Collector {
  id() -> CollectorId
  capabilities(host) -> Vec<MetricCapability>
  sample(clock, previous_sample) -> Result<Vec<MetricEnvelope>, CollectorError>
}

trait PlatformOperations {
  processes() -> ProcessSnapshot
  request_quit(pid, start_time) -> ActionResult
  force_quit(pid, start_time) -> ActionResult
  move_to_trash(path) -> TrashResult
  eject_volume(volume_id) -> ActionResult
}

MetricCapability {
  metric_id, state, source, explanation, remediation, precision,
  freshness_budget_ms, permissions_required, optional_helper_required
}
```

Implement adapters per module and OS. Recheck capability after sleep/resume, driver/device change, permission change or helper restart. System information should be collected at startup and refreshed rarely; network bytes often; fans/temperature less frequently. Collectors that fail must return typed errors, not poison the entire snapshot.

## 5. Sampling and lifecycle

### Poll groups (initial design)

| Poll group | Starting interval | Examples | Notes |
|---|---:|---|---|
| Fast | 3 s | overall CPU, memory, interface counters, disk counters | Sampling needs a warm-up read |
| Medium | 5–10 s | processes, GPU, battery, sensors | Limit expensive enumeration |
| Slow | 30–120 s/events | OS info, drives, displays, Bluetooth enumeration | Prefer OS notifications/events |
| On demand | user action | folder scan, process details, external IP | Cancelable, distinct task |

**Never add one timer per card.** Scheduler broadcasts to multiple consumers. Monotonic clock drives deltas; UTC timestamps are used in persisted records. When machine sleeps: detect large timing gap and emit discontinuity, not a misleading throughput average. On resume, reset counter baselines and refresh capabilities. For a hidden window: continue the Rust sampler if background operation is enabled, but suspend/reduce UI chart renders.

### Event transport

- Snapshot of latest values exposed via bounded Tauri command `get_snapshot`.
- Push a lightweight `telemetry://snapshot` event with sequence number, generation, collection timestamp and changes. On subscriber resubscribe, fetch full snapshot to avoid loss.
- A per-feature UI service validates incoming typed payloads and applies unit conversions at display only.
- Remote server gets the **same normalized snapshot**; it never polls hardware independently.
- Separate high-volume process table refresh from fast time series to avoid UI churn.
- Use bounded channels (`drop_oldest` for preview streams) and an isolated bounded DB writer queue with backlog instrumentation.

## 6. Tauri IPC contract (illustrative)

```text
get_capabilities() -> CapabilityReport
get_snapshot() -> SystemSnapshot
get_history(metric_ids, from, to, resolution, limit) -> HistorySeries[]
get_processes(sort, filter, page, size) -> ProcessPage
request_process_action({pid,start_time,mode,confirmation_id}) -> ActionResult
start_scan({volume_or_path,options}) -> ScanHandle
cancel_scan(scan_id) -> Result
request_trash({selection,confirmation_id}) -> ActionResult
get_settings() -> Settings
update_settings(validated_patch) -> Settings
configure_remote_access(validated_patch) -> RemoteStatus
```

All command inputs need schema validation and authorization checks in Rust. A confirmation token/dialog alone is insufficient: the backend must verify process identity and file path ownership/scope before mutation. Build capability-specific permissions; **do not enable broad shell/filesystem access by default**.

## 7. State boundaries

- **Rust core**: owns authoritative metric and capability state.
- **SQLite**: historical data and durable settings or data-store configuration, with explicit migrations.
- **Zustand**: selected time range, charts visibility, layout arrangement, UI filters, modal state; cached read-only snapshots for presentation only.
- **Native OS APIs**: source of truth for device lists and ephemeral process identity.
- **Remote server**: read-only serialization of normalized data; no action handlers wired into its router.

Persist settings using one versioned schema; avoid two independent sources of truth between JS plugin storage and SQLite. Secrets go in OS credential storage, never in plain settings exports.

## 8. SQLite history architecture

Tables (schema defined in [`metrics-data-contract.md`](metrics-data-contract.md)):

- `devices`: stable local installation/device IDs and host context.
- `metric_definitions`: stable IDs, unit, category and source metadata.
- `samples_raw`: point-in-time records with device, metric, UTC timestamp, numeric value and status.
- `samples_rollup_1m`, `samples_rollup_1h`: min/max/avg/last/count aggregated in UTC bins.
- `recording_sessions`: annotated start/end times and selected metrics.
- `alert_rules`, `alert_events`: evaluator state and audit.
- `schema_migrations`, `app_settings`: state migrations and durable preferences.

Use WAL and batches; never store per-sample giant JSON blobs as the only query path. Rate calculations should happen **before** persistence, retaining raw cumulative counters as separate metric IDs when useful. Database writes must have retention ceilings, maintenance checkpoints and testable migration plans.

## 9. Platform boundaries

`monitor-platform-macos` uses vetted public API providers for system statistics; optional native Swift code may be needed for widgets or OS-specific UI. Sensors on Apple Silicon can be restricted. `monitor-platform-windows` uses suitable Win32, Pdh/ETW, DXGI/vendor paths when warranted. `monitor-platform-linux` uses `/proc`, `/sys`, UPower and kernel/driver interfaces plus desktop-environment checks. Native collectors **must not lie**: unsupported is a first-class successful diagnostic result.

Never dynamically link a third-party sensor monitoring library without checking its redistribution license, security model, maintenance and target coverage. A privileged helper is a separate signed installer component and a **product decision**, not a default implementation shortcut.

## 10. Storage scanner architecture

- User supplies a chosen root. Resolve/canonicalize appropriately, inspect permissions and volume boundaries; do not scan the whole filesystem automatically.
- Worker traverses with bounded concurrency and cancellation, handles symlinks/reparse points and hardlinks without loops, and returns partial accessible-size totals with warnings.
- `ScanNode` carries path token, parent, known logical/allocated size, file count, errors and progress (not eagerly serializing millions of files to the UI).
- Treemap returns paginated/lazy children. Cache scan results per root + timestamp; invalidate on user refresh or known filesystem changes.
- Trash requests come from an explicit selection and reconfirmation; call the OS-native Trash/Recycle Bin capability, never `rm -rf`.

## 11. Process action architecture

List each process using `pid + start_time + executable/app identity` when OS permits. Before quit/force quit, re-query and reject if identity changed. Try graceful quit when feasible, then clearly request explicit force action. System/root/admin process failures are safe errors, not reasons to silently elevate. Keep action result codes (`permission_denied`, `process_exited`, `identity_changed`, `protected`, `failed`) visible.

## 12. Optional remote access

Separate Axum module with its own startup/shutdown and bounded resources. Disabled by default; listen `127.0.0.1` unless LAN explicitly enabled. `/api/v1` is **read-only**. Token authentication, no URL tokens, IP filtering as an additional control, TLS mandatory for non-loopback binding, rate limiting, strict CORS and configurable trusted origins. No shell, process termination or trash route. See [`security-privacy.md`](security-privacy.md).

## 13. Native widget plan

Native WidgetKit requires a macOS-specific extension, signing/entitlements and a carefully designed shared snapshot cache; don't simply assume a WebView widget is a WidgetKit extension. Windows native widgets may require packaging/provider constraints and separate validation; fallback floating mini-dashboard must be supported. Linux desktop widgets vary dramatically: prefer a floating always-on-top mini-monitor with transparent limitations instead of a fictional universal widget API.

## 14. Top architecture risks and mitigation

| Risk | Mitigation / gate |
|---|---|
| Apple Silicon sensor read APIs not universally public | discovery spike and capability report before making UX promises |
| Windows GPU stats differ by driver/WDDM/hardware | prototype target GPUs, compare against native diagnostic utilities, preserve source labels |
| Linux distro/tray differences | GNOME/KDE hardware/desktop matrix, explicit fallback window |
| CPU usage spikes due to collection | single scheduler, adaptive polling, performance regression tests |
| Stale process PID used for kill | re-verify PID + start time immediately before action |
| Scan hangs on mounts/huge directory trees | cancellation, bounded traversal, access timeouts where possible |
| Secret leakage via remote API/logs | route whitelist, explicit field projection, log redaction, secret store |
| Fake “green” tests from mock providers | separate unit/mock, OS integration and real hardware verification labels |

## 15. Architecture decisions that need sign-off

Document `docs/decisions/ADR-*.md` once resolved:

- Choice of SQLite wrapper and migration tooling.
- IPC event transport vs streaming channel implementation.
- Native sensor helper feasibility and packaging per OS.
- Chart aggregation and retention policy after measured dataset growth.
- Tray popup vs window implementation across DEs.
- Remote TLS configuration and OS credential-storage backend.

See [`platform-capabilities.md`](platform-capabilities.md) and [`metrics-data-contract.md`](metrics-data-contract.md) for lower-level contracts.
