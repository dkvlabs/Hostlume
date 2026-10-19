# Telemetry data contract, calculations and persistence

## 1. Principles

All metrics need typed units, origin, collection time, availability and an entity key. A **missing** reading never means zero. Store canonical numeric units and convert only at display/export boundaries. Metric names are stable strings; providers may change.

## 2. Canonical snapshot schema (normative conceptual TypeScript)

```ts
type Availability =
  | 'available'
  | 'unsupported'
  | 'permission_required'
  | 'temporarily_unavailable'
  | 'stale'
  | 'error';

type MetricUnit =
  | 'percent' | 'bytes' | 'bytes_per_second' | 'bits_per_second'
  | 'celsius' | 'rpm' | 'seconds' | 'count' | 'watts'
  | 'hertz' | 'boolean' | 'text';

type MetricReading = {
  metricId: string;                 // e.g. cpu.total.usage
  entityId: string;                 // host, core:2, if:..., disk:...
  value: number | string | boolean | null;
  unit: MetricUnit;
  availability: Availability;
  source: string;                   // e.g. sysinfo, Windows PDH, Linux hwmon
  collectedAtUtc: string;           // ISO-8601 UTC
  monotonicSequence: number;
  reasonCode?: string;              // NO_DRIVER, PERMISSION_DENIED, etc.
  humanMessageKey?: string;         // localized in UI
  precision?: number;
};

type SystemSnapshot = {
  schemaVersion: 1;
  hostId: string;
  generation: string;               // changes after collector reset
  sequence: number;
  collectedAtUtc: string;
  metrics: MetricReading[];
  capabilitiesRevision: number;
};
```

**Invariant:** when `availability !== 'available'`, `value === null` for numeric measurements. Do not use `NaN`, sentinel `-1`, stale `0` or silent guessed values. Static strings can be provided via separate metadata while measurement availability remains explicit. Long-running `stale` readings may carry a separate `lastKnownValue` tagged with its original timestamp, never represented as live.

## 3. Key metric families

| Metric ID example | Entity | Unit | Semantic rule |
|---|---|---|---|
| `cpu.total.usage` | host | percent | Overall normalized 0–100 of machine capacity |
| `cpu.core.usage` | logical core | percent | Independent 0–100 per core |
| `cpu.process.usage_cores` | PID+start | percent | Can reach `100 × logical_core_count` |
| `cpu.process.usage_host` | PID+start | percent | Host-normalized `usage_cores / logical_cores` |
| `memory.physical.total` | host | bytes | Physical RAM, nonnegative |
| `memory.available` | host | bytes | OS-specific meaning; document provider |
| `memory.swap.used` | host | bytes | Swap used, never added to physical used |
| `disk.volume.capacity` | volume | bytes | Filesystem logical capacity |
| `disk.volume.available` | volume | bytes | Available to user; provider-specific |
| `disk.device.read_bytes_total` | device | bytes | Monotonic when supplied; allow resets |
| `disk.device.read_rate` | device | bytes_per_second | Byte counter delta/elapsed seconds |
| `net.interface.rx_total` | interface | bytes | Cumulative received bytes for this interface |
| `net.interface.rx_rate` | interface | bytes_per_second | Per-interface delta calculation |
| `gpu.engine.utilization` | GPU/engine | percent | Explicit engine/source, avoid invented aggregate |
| `gpu.memory.dedicated_used` | GPU | bytes | Do not equate shared unified memory with VRAM |
| `sensor.temperature` | sensor | celsius | Read-only device-native sensor |
| `sensor.fan.speed` | fan | rpm | Read-only; missing fan isn't zero RPM |
| `battery.charge` | battery | percent | Current state of charge |
| `battery.cycles` | battery | count | Unsupported if cycles unavailable |
| `bluetooth.device.charge` | Bluetooth device | percent | Only if device exposes charge |

## 4. Sampling math

### CPU

- Two samples required to calculate utilization correctly; first sample is `temporarily_unavailable` with `WARMUP` reason.
- Machine-wide CPU is average of logical-core capacity during interval, capped to valid provider range after checking counter semantics.
- Explicitly distinguish host-normalized process CPU (0–100) from core-equivalent process CPU (0–N×100). Windows performance counters can exceed 100% per process because they sum activity across cores; never clamp that value without choosing a normalized definition.

### Rates

```text
rate_bytes_per_second = max(0, new_counter - old_counter) / monotonic_elapsed_seconds
```

Compute only when `elapsed > 0`, both readings target the **same stable interface/device generation**, counters are monotonic, and no sleep/discontinuity or reset occurred. Otherwise mark rate temporarily unavailable and reset baseline; do not invent a zero rate. Cumulative bytes displayed separately from rates. If OS provides only instantaneous rate, mark provenance accordingly.

### Memory

Do not derive used memory by summing wired + cached + compressed across OSes. Build separate OS-specific `MemoryBreakdown` mappings. Memory pressure is **not** the same metric as percentage RAM consumed.

### Storage

Logical size and allocated size differ for sparse files, compression, hardlinks and APFS clones. Visualizations state which unit is shown and cannot guarantee precisely reclaimable bytes. File deletion may not reclaim measured logical size due to snapshots/shared blocks.

## 5. Clock, retention and rollups

- Timestamp raw readings in UTC (`epoch_ms` integer preferred for storage); use monotonic clock exclusively for interval calculations.
- Always insert a gap marker/discontinuity on sleep/resume, collector restart, hotplug or counter reset.
- Proposed default retention for **sampled time-series**: raw 24h; 1-minute aggregates 30d; 1-hour aggregates 365d. Make configurable and test impact before finalizing. Avoid recording all per-process metrics forever by default.
- Aggregate min/max/avg/last/count per metric/entity/time bin. Record sample count and unavailable ratio so partially missing intervals are not misrepresented as reliable averages.
- Use UTC-based grouping; render in the viewer's local time with DST-safe labels.

## 6. Proposed database tables (not exact SQL migration)

```text
devices(id, first_seen_utc, last_seen_utc, platform, platform_version)
metric_definitions(metric_id PK, description, unit, entity_type, semantic_version)
samples_raw(host_id, entity_id, metric_id, ts_utc_ms, value_real,
            availability, source, generation, sequence)
samples_rollup_1m(host_id, entity_id, metric_id, bucket_utc_ms,
                  count, ok_count, min, max, avg, last)
samples_rollup_1h(... same rollup columns ...)
recording_sessions(id, started_at, ended_at, metric_ids_json, label)
alert_rules(id, condition_json, enabled, created_at, updated_at)
alert_events(id, rule_id, state, triggered_at, resolved_at, summary_json)
app_settings(key, schema_version, json_value)
```

Use composite indexes on `(host_id,metric_id,entity_id,ts_utc_ms)` and enforce bounded query `limit` and time range. Check SQLite's supported UPSERT/on-conflict form when generating actual migrations. Store a schema version and verify migrations with old fixtures. Credentials and LAN access tokens are **never** stored in CSV exports or general SQLite settings.

## 7. History query contract

```http
GET /api/v1/metrics/history?metric_id=cpu.total.usage&from=<utc>&to=<utc>&resolution=1m&limit=2000
```

Local typed IPC uses the same semantics. Validate range <= configurable maximum and metrics <= allowed whitelist; aggregate in Rust/SQL rather than sending millions of points to the WebView. Return `unit`, `source(s)`, number of missing samples and `gap` segments. Chart consumers must not connect line segments over gaps. Export CSV with `timestamp_utc,host_id,entity_id,metric_id,value,unit,availability,source` columns plus encoding/documentation.

## 8. Entity IDs and privacy

- Host identity uses random installation UUID, **not** MAC address or serial number.
- Process key: PID + start time + host session; do not store executable path by default in high-frequency history.
- Network key: OS interface ID + generation; do not rely on display name alone.
- Disk key: stable OS volume/device identifier if available; avoid exposing device serials through remote API.
- Hide MAC addresses, usernames, private paths and process full command lines by default from remote API and diagnostic exports.

## 9. Contract tests

Required deterministic fixtures: stable counters, overflow/wrap, reset, device recreation, clock jump, sleep gap, CPU two-sample warm-up, >100% core-equivalent process CPU, permission-restricted sensor, missing battery, Unicode/interface names, huge disk size and aggregate partial availability. Add schema snapshot tests on serializations and CSV decoding.
