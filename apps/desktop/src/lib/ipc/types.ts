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

export interface Settings {
  schema_version: number;
  appearance: Appearance;
  compact_mode: boolean;
  polling_base_seconds: number;
  close_to_tray: boolean;
  locale: string;
  overview_cards: OverviewCardPref[];
}

export interface SettingsPatch {
  appearance?: Appearance;
  compact_mode?: boolean;
  polling_base_seconds?: number;
  close_to_tray?: boolean;
  locale?: string;
  overview_cards?: OverviewCardPref[];
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

/** Lightweight push on every scheduler tick (ADR-0003). */
export interface TelemetryEventPayload {
  schemaVersion: number;
  hostId: string;
  generation: string;
  sequence: number;
  collectedAtUtc: string;
  capabilitiesRevision: number;
}
