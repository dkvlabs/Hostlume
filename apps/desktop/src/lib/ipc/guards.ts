/**
 * Runtime validation for IPC payloads. The WebView is untrusted input
 * (security-privacy.md §2): anything coming back from Rust is checked
 * against the contract shape before it may reach stores or components.
 */
import type {
  Appearance,
  Availability,
  CapabilityReport,
  CommandErrorPayload,
  DiagnosticsReport,
  HistoryResponse,
  MetricCapability,
  MetricReading,
  ProcessActionResult,
  ProcessPage,
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
  return (
    isFiniteNumber(history.raw_hours) &&
    isFiniteNumber(history.rollup_1m_days) &&
    isFiniteNumber(history.rollup_1h_days)
  );
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
