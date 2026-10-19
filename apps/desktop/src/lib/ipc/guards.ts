/**
 * Runtime validation for IPC payloads. The WebView is untrusted input
 * (security-privacy.md §2): anything coming back from Rust is checked
 * against the contract shape before it may reach stores or components.
 */
import type {
  AlertActionConfig,
  AlertEventRow,
  AlertRule,
  Appearance,
  Availability,
  CapabilityReport,
  CommandErrorPayload,
  CsvExportResult,
  DiagnosticsReport,
  HistoryResponse,
  GpuAdapter,
  GpuDisplay,
  GpuInfoReport,
  MetricCapability,
  MetricReading,
  NotificationStateInfo,
  ProcessActionResult,
  ProcessPage,
  RecordingSession,
  RemoteSetup,
  RemoteStatus,
  RuleProbe,
  Settings,
  SystemSnapshot,
  TelemetryEventPayload,
} from "./types";
import { SCHEMA_VERSION } from "./types";

const AVAILABILITIES: readonly Availability[] = [
  "available",
  "unsupported",
  "permission_required",
  "temporarily_unavailable",
  "stale",
  "error",
];

const UNITS = [
  "percent",
  "bytes",
  "bytes_per_second",
  "bits_per_second",
  "celsius",
  "rpm",
  "seconds",
  "count",
  "watts",
  "hertz",
  "boolean",
  "text",
] as const;

const APPEARANCES: readonly Appearance[] = ["system", "light", "dark"];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

export function isMetricReading(value: unknown): value is MetricReading {
  if (!isRecord(value)) return false;
  if (typeof value.metric_id !== "string" || typeof value.entity_id !== "string")
    return false;
  if (typeof value.source !== "string") return false;
  if (typeof value.collected_at_utc !== "string") return false;
  if (typeof value.monotonic_sequence !== "number") return false;
  if (!AVAILABILITIES.includes(value.availability as Availability)) return false;
  if (!UNITS.includes(value.unit as (typeof UNITS)[number])) return false;
  const { value: reading, availability } = value as {
    value: unknown;
    availability: Availability;
  };
  // Contract invariant: only an available reading may carry a value, and
  // numeric values must be finite (no NaN — it cannot survive JSON anyway).
  if (availability === "available") {
    if (
      reading === null ||
      !(typeof reading === "number" || typeof reading === "string" || typeof reading === "boolean")
    ) {
      return false;
    }
    if (typeof reading === "number" && !Number.isFinite(reading)) return false;
  } else if (reading !== null) {
    return false;
  }
  return true;
}

export function isSystemSnapshot(value: unknown): value is SystemSnapshot {
  if (!isRecord(value)) return false;
  if (value.schema_version !== SCHEMA_VERSION) return false;
  if (typeof value.host_id !== "string" || value.host_id.length === 0) return false;
  if (typeof value.generation !== "string" || value.generation.length === 0) return false;
  if (typeof value.sequence !== "number") return false;
  if (typeof value.collected_at_utc !== "string") return false;
  if (typeof value.capabilities_revision !== "number") return false;
  if (!Array.isArray(value.metrics)) return false;
  return value.metrics.every((m) => isMetricReading(m));
}

export function isMetricCapability(value: unknown): value is MetricCapability {
  if (!isRecord(value)) return false;
  if (typeof value.metric_id !== "string") return false;
  if (!AVAILABILITIES.includes(value.state as Availability)) return false;
  if (typeof value.source !== "string") return false;
  if (!Array.isArray(value.permissions_required)) return false;
  return true;
}

export function isCapabilityReport(value: unknown): value is CapabilityReport {
  if (!isRecord(value)) return false;
  if (value.schema_version !== SCHEMA_VERSION) return false;
  if (typeof value.host_id !== "string") return false;
  if (typeof value.generated_at_utc !== "string") return false;
  if (typeof value.revision !== "number") return false;
  if (!Array.isArray(value.capabilities)) return false;
  return value.capabilities.every((c) => isMetricCapability(c));
}

export function isOverviewCardPref(value: unknown): value is { id: string; visible: boolean } {
  return (
    isRecord(value) && typeof value.id === "string" && typeof value.visible === "boolean"
  );
}

export function isSettings(value: unknown): value is Settings {
  if (!isRecord(value)) return false;
  if (value.schema_version !== SCHEMA_VERSION) return false;
  if (!APPEARANCES.includes(value.appearance as Appearance)) return false;
  if (typeof value.compact_mode !== "boolean") return false;
  if (!isFiniteNumber(value.polling_base_seconds)) return false;
  if (typeof value.close_to_tray !== "boolean") return false;
  if (typeof value.locale !== "string") return false;
  if (!Array.isArray(value.overview_cards)) return false;
  if (!value.overview_cards.every((card) => isOverviewCardPref(card))) return false;
  const history = value.history;
  if (!isRecord(history)) return false;
  if (
    !isFiniteNumber(history.raw_hours) ||
    !isFiniteNumber(history.rollup_1m_days) ||
    !isFiniteNumber(history.rollup_1h_days)
  ) {
    return false;
  }
  if (!["bytes", "bits"].includes(value.rate_unit as string)) return false;
  if (!["celsius", "fahrenheit"].includes(value.temperature_unit as string)) return false;
  const tray = value.tray;
  if (!isRecord(tray)) return false;
  if (
    typeof tray.enabled !== "boolean" ||
    typeof tray.show_cpu !== "boolean" ||
    typeof tray.show_memory !== "boolean" ||
    typeof tray.show_network !== "boolean" ||
    typeof tray.show_title !== "boolean"
  ) {
    return false;
  }
  if (typeof value.launch_at_login !== "boolean") return false;
  const alerts = value.alerts;
  if (!isRecord(alerts)) return false;
  if (
    typeof alerts.notifications_enabled !== "boolean" ||
    typeof alerts.actions_enabled !== "boolean" ||
    typeof alerts.quiet_hours_enabled !== "boolean" ||
    !isFiniteNumber(alerts.quiet_start_minute) ||
    !isFiniteNumber(alerts.quiet_end_minute)
  ) {
    return false;
  }
  const remote = value.remote;
  if (!isRecord(remote)) return false;
  if (!["off", "loopback", "lan"].includes(remote.mode as string)) return false;
  if (!isFiniteNumber(remote.port)) return false;
  if (value.remote_token_hash !== null && typeof value.remote_token_hash !== "string") {
    return false;
  }
  const lookup = value.external_lookup;
  if (!isRecord(lookup)) return false;
  return (
    typeof lookup.enabled === "boolean" &&
    typeof lookup.provider === "string" &&
    isFiniteNumber(lookup.timeout_secs)
  );
}

export function isGpuDisplay(value: unknown): value is GpuDisplay {
  if (!isRecord(value)) return false;
  return (
    typeof value.name === "string" &&
    (value.resolution === undefined || typeof value.resolution === "string") &&
    typeof value.is_main === "boolean" &&
    typeof value.is_online === "boolean"
  );
}

export function isGpuAdapter(value: unknown): value is GpuAdapter {
  if (!isRecord(value)) return false;
  return (
    typeof value.id === "string" &&
    typeof value.name === "string" &&
    (value.vendor === undefined || typeof value.vendor === "string") &&
    typeof value.is_unified_memory === "boolean" &&
    (value.vram_bytes === undefined ||
      (isFiniteNumber(value.vram_bytes) && Number.isInteger(value.vram_bytes))) &&
    Array.isArray(value.displays) &&
    value.displays.every((display) => isGpuDisplay(display))
  );
}

export function isGpuInfoReport(value: unknown): value is GpuInfoReport {
  if (!isRecord(value)) return false;
  if (typeof value.generated_at_utc !== "string") return false;
  if (!Array.isArray(value.adapters)) return false;
  return value.adapters.every((adapter) => isGpuAdapter(adapter));
}

export function isDiagnosticsReport(value: unknown): value is DiagnosticsReport {
  if (!isRecord(value)) return false;
  if (!Array.isArray(value.collectors)) return false;
  return value.collectors.every((c) => {
    if (!isRecord(c)) return false;
    return (
      typeof c.id === "string" &&
      typeof c.group === "string" &&
      typeof c.ticks_total === "number" &&
      typeof c.error_count === "number"
    );
  });
}

export function isCommandErrorPayload(value: unknown): value is CommandErrorPayload {
  if (!isRecord(value)) return false;
  return (
    typeof value.code === "string" &&
    typeof value.messageKey === "string" &&
    (value.reason === undefined || typeof value.reason === "string")
  );
}

export function isProcessPage(value: unknown): value is ProcessPage {
  if (!isRecord(value)) return false;
  if (typeof value.collected_at_utc !== "string") return false;
  if (typeof value.cpuWarmup !== "boolean") return false;
  if (typeof value.logicalCores !== "number") return false;
  if (!Array.isArray(value.rows)) return false;
  return value.rows.every((row) => {
    if (!isRecord(row)) return false;
    return (
      typeof row.pid === "number" &&
      typeof row.startTimeSec === "number" &&
      typeof row.name === "string" &&
      typeof row.rssBytes === "number" &&
      (row.cpuUsageCores === null || isFiniteNumber(row.cpuUsageCores)) &&
      (row.cpuUsageHost === null || isFiniteNumber(row.cpuUsageHost)) &&
      typeof row.status === "string" &&
      (row.parentPid === null || typeof row.parentPid === "number")
    );
  });
}

const ACTION_OUTCOMES = [
  "ok",
  "identity_changed",
  "process_exited",
  "protected_target",
  "permission_denied",
  "unsupported_on_platform",
  "failed",
] as const;

export function isProcessActionResult(value: unknown): value is ProcessActionResult {
  if (!isRecord(value)) return false;
  return (
    typeof value.outcome === "string" &&
    (ACTION_OUTCOMES as readonly string[]).includes(value.outcome) &&
    typeof value.pid === "number" &&
    typeof value.startTimeSec === "number" &&
    typeof value.mode === "string"
  );
}

export function isHistoryResponse(value: unknown): value is HistoryResponse {
  if (!isRecord(value)) return false;
  if (typeof value.metric_id !== "string" || typeof value.entity_id !== "string") return false;
  if (!["raw", "rollup_1m", "rollup_1h"].includes(value.resolution as string)) return false;
  if (!Array.isArray(value.points) || !Array.isArray(value.gaps)) return false;
  if (typeof value.missing_ratio !== "number") return false;
  return value.points.every(
    (p) =>
      isRecord(p) &&
      typeof p.ts_utc_ms === "number" &&
      typeof p.value === "number" &&
      (p.count === null || typeof p.count === "number")
  );
}

export function isTelemetryEventPayload(value: unknown): value is TelemetryEventPayload {
  if (!isRecord(value)) return false;
  if (value.schemaVersion !== SCHEMA_VERSION) return false;
  return (
    typeof value.hostId === "string" &&
    typeof value.generation === "string" &&
    typeof value.sequence === "number" &&
    typeof value.collectedAtUtc === "string" &&
    typeof value.capabilitiesRevision === "number"
  );
}

// ---- Alerts and approved local actions (FX-017, phase 07) ------------------

export function isAlertActionConfig(value: unknown): value is AlertActionConfig {
  return (
    isRecord(value) &&
    typeof value.program === "string" &&
    isFiniteNumber(value.timeout_secs) &&
    typeof value.approved === "boolean"
  );
}

export function isAlertRule(value: unknown): value is AlertRule {
  if (!isRecord(value)) return false;
  if (typeof value.id !== "string" || typeof value.metric_id !== "string") return false;
  if (value.entity_filter !== null && typeof value.entity_filter !== "string") return false;
  if (!["above", "below"].includes(value.comparator as string)) return false;
  if (!isFiniteNumber(value.threshold)) return false;
  if (
    !isFiniteNumber(value.hold_duration_ms) ||
    !isFiniteNumber(value.hysteresis_delta) ||
    !isFiniteNumber(value.cooldown_ms)
  ) {
    return false;
  }
  if (typeof value.enabled !== "boolean") return false;
  if (value.notification_message !== null && typeof value.notification_message !== "string") {
    return false;
  }
  if (value.action !== null && !isAlertActionConfig(value.action)) return false;
  return true;
}

export function isAlertEventRow(value: unknown): value is AlertEventRow {
  if (!isRecord(value)) return false;
  if (typeof value.id !== "string" || typeof value.rule_id !== "string") return false;
  if (!["firing", "resolved"].includes(value.state as string)) return false;
  if (!isFiniteNumber(value.triggered_at_utc_ms)) return false;
  if (value.resolved_at_utc_ms !== undefined && !isFiniteNumber(value.resolved_at_utc_ms)) {
    return false;
  }
  return value.summary_json === undefined || typeof value.summary_json === "string";
}

export function isRuleProbe(value: unknown): value is RuleProbe {
  if (!isRecord(value)) return false;
  if (value.matched_entity !== null && typeof value.matched_entity !== "string") return false;
  if (value.availability !== null && !AVAILABILITIES.includes(value.availability as Availability)) {
    return false;
  }
  if (value.value !== null && !isFiniteNumber(value.value)) return false;
  return typeof value.crossing_now === "boolean" && typeof value.explanation === "string";
}

export function isRemoteStatus(value: unknown): value is RemoteStatus {
  if (!isRecord(value)) return false;
  if (!["off", "loopback", "lan"].includes(value.mode as string)) return false;
  if (!isFiniteNumber(value.port)) return false;
  if (typeof value.running !== "boolean") return false;
  if (typeof value.token_exists !== "boolean") return false;
  return value.url === undefined || typeof value.url === "string";
}

export function isRemoteSetup(value: unknown): value is RemoteSetup {
  if (!isRecord(value)) return false;
  if (!isRemoteStatus(value.status)) return false;
  return value.token_once === undefined || typeof value.token_once === "string";
}

export function isRecordingSession(value: unknown): value is RecordingSession {
  if (!isRecord(value)) return false;
  if (typeof value.id !== "string" || typeof value.label !== "string") return false;
  if (!Array.isArray(value.metric_ids) || !value.metric_ids.every((m) => typeof m === "string")) {
    return false;
  }
  if (!isFiniteNumber(value.started_at_utc_ms)) return false;
  if (
    value.ended_at_utc_ms !== undefined &&
    value.ended_at_utc_ms !== null &&
    !isFiniteNumber(value.ended_at_utc_ms)
  ) {
    return false;
  }
  return ["recording", "stopped", "interrupted"].includes(value.status as string);
}

export function isCsvExportResult(value: unknown): value is CsvExportResult {
  return (
    isRecord(value) &&
    typeof value.path === "string" &&
    isFiniteNumber(value.rows) &&
    typeof value.truncated === "boolean"
  );
}

export function isNotificationStateInfo(value: unknown): value is NotificationStateInfo {
  if (!isRecord(value)) return false;
  return (
    ["macos", "windows", "linux"].includes(value.platform as string) &&
    ["os_managed", "not_applicable"].includes(value.state as string) &&
    typeof value.detail_key === "string"
  );
}
