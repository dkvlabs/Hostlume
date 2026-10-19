/**
 * TypeScript mirror of the Rust contract in `monitor-core`
 * (docs/metrics-data-contract.md §2). Keep in lockstep; `guards.ts`
 * runtime-validates every payload crossing the IPC boundary.
 */

/** Telemetry contract version (`monitor_core::SCHEMA_VERSION`). */
export const SCHEMA_VERSION = 1;

/**
 * Settings schema version (`settings::SETTINGS_SCHEMA_VERSION` in
 * src-tauri). Distinct from the telemetry contract: Rust migrates stored
 * rows at load time and always returns the current version, so the guard
 * only requires a positive integer — never equality with the telemetry
 * version (phase 09 release smoke found that mismatch rejecting every real
 * settings payload). Pinned by `settings-default.json` contract tests.
 */
export const SETTINGS_SCHEMA_VERSION = 8;

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

export type RateUnit = "bytes" | "bits";

export type TemperatureUnit = "celsius" | "fahrenheit";

/** Tray configuration (phase 06 §2.3; persisted, applied live). */
export interface TrayConfig {
  enabled: boolean;
  show_cpu: boolean;
  show_memory: boolean;
  show_network: boolean;
  show_title: boolean;
}

/** One graphics adapter + its displays (FX-010 `get_gpu_info`). */
export interface GpuDisplay {
  name: string;
  resolution?: string;
  is_main: boolean;
  is_online: boolean;
}

export interface GpuAdapter {
  id: string;
  name: string;
  vendor?: string;
  is_unified_memory: boolean;
  vram_bytes?: number;
  displays: GpuDisplay[];
}

export interface GpuInfoReport {
  generated_at_utc: string;
  adapters: GpuAdapter[];
}

export interface ExternalLookup {
  enabled: boolean;
  provider: string;
  timeout_secs: number;
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
  rate_unit: RateUnit;
  external_lookup: ExternalLookup;
  temperature_unit: TemperatureUnit;
  tray: TrayConfig;
  launch_at_login: boolean;
  alerts: AlertsConfig;
  remote: RemoteConfig;
  remote_token_hash: string | null;
}

/** Alert delivery settings (phase 07); actions default OFF. */
export interface AlertsConfig {
  notifications_enabled: boolean;
  actions_enabled: boolean;
  quiet_hours_enabled: boolean;
  quiet_start_minute: number;
  quiet_end_minute: number;
}

export interface SettingsPatch {
  appearance?: Appearance;
  compact_mode?: boolean;
  polling_base_seconds?: number;
  close_to_tray?: boolean;
  locale?: string;
  overview_cards?: OverviewCardPref[];
  history?: HistoryRetention;
  rate_unit?: RateUnit;
  external_lookup?: ExternalLookup;
  temperature_unit?: TemperatureUnit;
  tray?: TrayConfig;
  launch_at_login?: boolean;
  alerts?: AlertsConfig;
  /** Remote configuration changes go through the dedicated
   * `configure_remote` command (token-aware), not this patch. */
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

// ---- Storage analyzer (FX-008) -------------------------------------------

export interface ScanProgress {
  visited_entries: number;
  visited_bytes: number;
  error_count: number;
  done: boolean;
  cancelled: boolean;
}

export interface ScanStatus {
  scanId: string;
  root: string;
  progress: ScanProgress;
}

export interface ChildEntry {
  index: number;
  name: string;
  is_dir: boolean;
  is_symlink: boolean;
  is_mount_boundary: boolean;
  logicalSize: number;
  allocatedSize: number;
  errorCount: number;
}

export interface ScanChildren {
  children: ChildEntry[];
  total_children: number;
  end_of_list: boolean;
  node_index: number;
  errors: number;
  skippedCount: number;
}

export interface TrashPathResult {
  path: string;
  ok: boolean;
  detail?: string;
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

// ---- Alerts and approved local actions (FX-017, phase 07) ------------------

export type AlertComparator = "above" | "below";

/**
 * Mirrors `monitor_alerts::ActionConfig`. This phase launches the approved
 * program without an argument array (project security tooling refuses
 * variable arg-list spawns — ADR-0007); multi-step logic belongs in an
 * approved script.
 */
export interface AlertActionConfig {
  program: string;
  timeout_secs: number;
  approved: boolean;
}

export interface AlertRule {
  /** Empty on create (server assigns the UUID); non-empty for updates. */
  id: string;
  metric_id: string;
  entity_filter: string | null;
  comparator: AlertComparator;
  threshold: number;
  hold_duration_ms: number;
  hysteresis_delta: number;
  cooldown_ms: number;
  enabled: boolean;
  notification_message: string | null;
  action: AlertActionConfig | null;
}

/** One fire→resolve episode from the audit log. */
export interface AlertEventRow {
  id: string;
  rule_id: string;
  state: "firing" | "resolved";
  triggered_at_utc_ms: number;
  resolved_at_utc_ms?: number;
  summary_json?: string;
}

/** Fixture preview (`test_alert_rule`) — no side effects. */
export interface RuleProbe {
  matched_entity: string | null;
  availability: Availability | null;
  value: number | null;
  crossing_now: boolean;
  explanation: string;
}

/** Honest notification-permission report (the desktop plugin has no real
 * permission query; macOS authorization is OS-managed). */
export interface NotificationStateInfo {
  platform: "macos" | "windows" | "linux";
  state: "os_managed" | "not_applicable";
  detail_key: string;
}

// ---- Comparison, recording and CSV export (FX-018, phase 08) ---------------

/** Mirrors `monitor_history::RecordingSession`. */
export interface RecordingSession {
  id: string;
  label: string;
  metric_ids: string[];
  started_at_utc_ms: number;
  ended_at_utc_ms?: number;
  status: "recording" | "stopped" | "interrupted";
}

export interface RecordingStart {
  label: string;
  metric_ids: string[];
}

export interface CsvExportSpec {
  metric_id: string;
  entity_id: string;
  from_utc_ms: number;
  to_utc_ms: number;
}

export interface CsvExportResult {
  path: string;
  rows: number;
  truncated: boolean;
}

// ---- Remote server (FX-019, phase 08) ---------------------------------------

export type RemoteMode = "off" | "loopback" | "lan";

export interface RemoteConfig {
  mode: RemoteMode;
  port: number;
}

/** Mirrors the Rust `RemoteStatus` command payload. */
export interface RemoteStatus {
  running: boolean;
  mode: RemoteMode;
  port: number;
  url?: string;
  token_exists: boolean;
}

export interface RemoteSetup {
  status: RemoteStatus;
  /** The full token, returned ONLY when a new one was generated. */
  token_once?: string;
}
