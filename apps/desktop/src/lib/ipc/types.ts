/**
 * TypeScript mirror of the Rust contract in `monitor-core`
 * (docs/metrics-data-contract.md §2). Keep in lockstep; `guards.ts`
 * runtime-validates every payload crossing the IPC boundary.
 */

export const SCHEMA_VERSION = 1;

export type Availability =
  | "available"
  | "unsupported"
  | "permission_required"
  | "temporarily_unavailable"
  | "stale"
  | "error";

export type MetricUnit =
  | "percent"
  | "bytes"
  | "bytes_per_second"
  | "bits_per_second"
  | "celsius"
  | "rpm"
  | "seconds"
  | "count"
  | "watts"
  | "hertz"
  | "boolean"
  | "text";

export type MetricValue = number | string | boolean;

export interface MetricReading {
  metric_id: string;
  entity_id: string;
  /** null is meaningful: no value without `available`. Never fabricated. */
  value: MetricValue | null;
  unit: MetricUnit;
  availability: Availability;
  source: string;
  collected_at_utc: string;
  monotonic_sequence: number;
  reason_code?: string;
  human_message_key?: string;
  precision?: number;
}

export interface SystemSnapshot {
  schema_version: number;
  host_id: string;
  generation: string;
  sequence: number;
  collected_at_utc: string;
  metrics: MetricReading[];
  capabilities_revision: number;
}

export interface MetricCapability {
  metric_id: string;
  state: Availability;
  source: string;
  explanation?: string;
  reason_code?: string;
  remediation?: string;
  human_message_key?: string;
  precision?: number;
  freshness_budget_ms?: number;
  permissions_required: string[];
  optional_helper_required: boolean;
}

export interface CapabilityReport {
  schema_version: number;
  host_id: string;
  generated_at_utc: string;
  revision: number;
  capabilities: MetricCapability[];
}

export type Appearance = "system" | "light" | "dark";

export interface OverviewCardPref {
  id: string;
  visible: boolean;
}

export interface HistoryRetention {
  raw_hours: number;
  rollup_1m_days: number;
  rollup_1h_days: number;
}

export interface Settings {
  schema_version: number;
  appearance: Appearance;
  compact_mode: boolean;
  polling_base_seconds: number;
  close_to_tray: boolean;
  locale: string;
  overview_cards: OverviewCardPref[];
  history: HistoryRetention;
}

export interface SettingsPatch {
  appearance?: Appearance;
  compact_mode?: boolean;
  polling_base_seconds?: number;
  close_to_tray?: boolean;
  locale?: string;
  overview_cards?: OverviewCardPref[];
  history?: HistoryRetention;
}

/** Per-collector health from `get_diagnostics` (no host identifiers). */
export interface CollectorDiagnostic {
  id: string;
  group: string;
  ticks_total: number;
  error_count: number;
  lastOkAtUtc?: string;
  lastDurationMs?: number;
  lastError?: string;
}

export interface DiagnosticsReport {
  collectors: CollectorDiagnostic[];
}

/** Structured error from Rust commands; UI localizes via messageKey. */
export interface CommandErrorPayload {
  code: string;
  messageKey: string;
  reason?: string;
}

/** One row of the process table (FX-007). Identity = pid + startTimeSec. */
export interface ProcessRow {
  pid: number;
  startTimeSec: number;
  name: string;
  rssBytes: number;
  /** Core-equivalent percent (can exceed 100); null during warm-up. */
  cpuUsageCores: number | null;
  /** Host-normalized 0–100; null during warm-up. */
  cpuUsageHost: number | null;
  status: string;
  parentPid: number | null;
}

export interface ProcessPage {
  collectedAtUtc: string;
  cpuWarmup: boolean;
  logicalCores: number;
  rows: ProcessRow[];
}

export type ProcessActionMode = "graceful" | "force";

export interface ProcessActionRequest {
  pid: number;
  startTimeSec: number;
  mode: ProcessActionMode;
  confirmationId: string;
}

export type ActionOutcome =
  | "ok"
  | "identity_changed"
  | "process_exited"
  | "protected_target"
  | "permission_denied"
  | "unsupported_on_platform"
  | "failed";

export interface ProcessActionResult {
  outcome: ActionOutcome;
  pid: number;
  startTimeSec: number;
  mode: string;
}

export interface HistoryPoint {
  ts_utc_ms: number;
  value: number;
  count: number | null;
}

export interface HistoryGap {
  from_utc_ms: number;
  to_utc_ms: number;
  reason: string;
}

export interface HistoryResponse {
  metric_id: string;
  entity_id: string;
  resolution: "raw" | "rollup_1m" | "rollup_1h";
  points: HistoryPoint[];
  gaps: HistoryGap[];
  missing_ratio: number;
}

export interface HistoryQuerySpec {
  metric_id: string;
  entity_id: string;
  from_utc_ms: number;
  to_utc_ms: number;
  max_points?: number;
}

/** Lightweight push on every scheduler tick (ADR-0003). */
export interface TelemetryEventPayload {
  schemaVersion: number;
  hostId: string;
  generation: string;
  sequence: number;
  collectedAtUtc: string;
  capabilitiesRevision: number;
}
