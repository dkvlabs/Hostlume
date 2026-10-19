/**
 * Snapshot selectors and the in-memory ring buffers that feed sparklines
 * (phase 02 §2C.12: history lives in memory until the Phase 03 store).
 *
 * Ring discipline: every push appends a sample point; non-available or
 * missing readings push `null`, which charts treat as a gap and never
 * connect through (design.md §15 / contract §4).
 */
import type { MetricReading, MetricValue, SystemSnapshot } from "@/lib/ipc/types";

export function findReading(
  snapshot: SystemSnapshot | null,
  metricId: string,
  entityId = "host"
): MetricReading | undefined {
  return snapshot?.metrics.find((m) => m.metric_id === metricId && m.entity_id === entityId);
}

export function readingsFor(snapshot: SystemSnapshot | null, metricId: string): MetricReading[] {
  return snapshot ? snapshot.metrics.filter((m) => m.metric_id === metricId) : [];
}

/** Numeric value of a reading, or null unless available+finite. */
export function numericValue(reading: MetricReading | undefined): number | null {
  if (!reading || reading.availability !== "available") return null;
  const { value } = reading;
  if (isNumberValue(value)) return value;
  return null;
}

export function isNumberValue(value: MetricValue | null): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

export function textValue(reading: MetricReading | undefined): string | null {
  if (!reading || reading.availability !== "available") return null;
  return typeof reading.value === "string" ? reading.value : null;
}

/** Human label for an entity key: `core:3` → `3`, `if:en0:1` → `en0`. */
export function entityLabel(entityId: string): string {
  if (entityId.startsWith("core:")) return entityId.slice("core:".length);
  if (entityId.startsWith("if:")) {
    const rest = entityId.slice("if:".length);
    const lastColon = rest.lastIndexOf(":");
    return lastColon === -1 ? rest : rest.slice(0, lastColon);
  }
  if (entityId.startsWith("vol:")) return entityId.slice("vol:".length);
  return entityId;
}

export function metricKey(metricId: string, entityId: string): string {
  return `${metricId}|${entityId}`;
}

export interface SamplePoint {
  /** Sample time (ms since epoch, from collectedAtUtc). */
  t: number;
  /** null = unavailable/missing ⇒ charts must break the line here. */
  v: number | null;
}

export const HISTORY_CAPACITY = 400;

/** Metric families tracked in the in-memory ring buffers. */
export const HISTORY_WATCHED = new Set([
  "cpu.total.usage",
  "cpu.core.usage",
  "memory.physical.used",
  "net.interface.rx_rate",
  "net.interface.tx_rate",
]);

export class MetricHistory {
  private readonly points: SamplePoint[] = [];

  constructor(private readonly capacity: number = HISTORY_CAPACITY) {}

  push(point: SamplePoint): void {
    this.points.push(point);
    if (this.points.length > this.capacity) {
      this.points.splice(0, this.points.length - this.capacity);
    }
  }

  /** Oldest-first copy, safe for render passes. */
  all(): SamplePoint[] {
    return [...this.points];
  }

  /** Last `count` points (or fewer). */
  last(count: number): SamplePoint[] {
    return this.points.slice(Math.max(0, this.points.length - count));
  }

  get length(): number {
    return this.points.length;
  }
}

/** Format seconds as `1d 2h 3m` style uptime. */
export function formatUptime(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return "—";
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const parts: string[] = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0 || days > 0) parts.push(`${hours}h`);
  parts.push(`${minutes}m`);
  return parts.join(" ");
}
