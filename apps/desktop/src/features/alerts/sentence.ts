/**
 * Human sentence preview for alert rules (design.md §11) and the curated
 * numeric metric list the rule editor offers. Thresholds must use the same
 * canonical units as the watched family (mirrors
 * `monitor_core::families::numeric_unit`, the Rust validator's registry).
 */
import type { AlertRule } from "@/lib/ipc/types";

export interface AlertMetricOption {
  id: string;
  label: string;
  unit: string;
}

/** Only numeric families the Rust validator accepts are offered. */
export const ALERT_METRIC_OPTIONS: readonly AlertMetricOption[] = [
  { id: "cpu.total.usage", label: "CPU total usage", unit: "%" },
  { id: "cpu.core.usage", label: "CPU core usage", unit: "%" },
  { id: "memory.physical.used", label: "Memory used", unit: "bytes" },
  { id: "memory.available", label: "Memory available", unit: "bytes" },
  { id: "memory.swap.used", label: "Swap used", unit: "bytes" },
  { id: "disk.volume.available", label: "Volume free space", unit: "bytes" },
  { id: "net.interface.rx_rate", label: "Network download rate", unit: "B/s" },
  { id: "net.interface.tx_rate", label: "Network upload rate", unit: "B/s" },
  { id: "sensor.temperature", label: "Temperature", unit: "°C" },
  { id: "sensor.fan.speed", label: "Fan speed", unit: "RPM" },
  { id: "battery.charge", label: "Battery charge", unit: "%" },
  { id: "gpu.engine.utilization", label: "GPU utilization", unit: "%" },
  { id: "host.uptime", label: "Uptime", unit: "s" },
];

export function metricOption(metricId: string): AlertMetricOption | undefined {
  return ALERT_METRIC_OPTIONS.find((option) => option.id === metricId);
}

/** "90 s" / "5 min" / "1 h 30 min" — compact, locale-neutral. */
export function formatDuration(ms: number): string {
  if (ms <= 0) return "0 s";
  const totalSeconds = Math.round(ms / 1_000);
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  const parts: string[] = [];
  if (hours > 0) parts.push(`${hours} h`);
  if (minutes > 0) parts.push(`${minutes} min`);
  if (seconds > 0 || parts.length === 0) parts.push(`${seconds} s`);
  return parts.join(" ");
}

function formatThreshold(value: number, unit: string): string {
  if (unit === "bytes") {
    const units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let scaled = value;
    let index = 0;
    while (scaled >= 1_024 && index < units.length - 1) {
      scaled /= 1_024;
      index += 1;
    }
    return `${Number.isInteger(scaled) ? scaled : scaled.toFixed(1)} ${units[index]}`;
  }
  return `${value} ${unit}`;
}

/**
 * "Notify when CPU total usage rises above 90 % for 60 s. Re-alerts after
 * 5 min." Composed, never concatenated from translated fragments.
 */
export function ruleSentence(rule: AlertRule): string {
  const option = metricOption(rule.metric_id);
  const metricLabel = option?.label ?? rule.metric_id;
  const unit = option?.unit ?? "";
  const comparator = rule.comparator === "above" ? "rises above" : "falls below";
  const hold =
    rule.hold_duration_ms > 0
      ? ` and stays there for ${formatDuration(rule.hold_duration_ms)}`
      : "";
  const cooldown =
    rule.cooldown_ms > 0
      ? ` Re-alerts after ${formatDuration(rule.cooldown_ms)}.`
      : " Re-alerts immediately.";
  return `Notify when ${metricLabel} ${comparator} ${formatThreshold(rule.threshold, unit)}${hold}.${cooldown}`;
}

/** Placeholder substitution for the optional notification template. */
export function renderTemplate(
  template: string,
  values: { value: number; entity: string; threshold: number }
): string {
  return template
    .replaceAll("{value}", String(values.value))
    .replaceAll("{entity}", values.entity)
    .replaceAll("{threshold}", String(values.threshold));
}

/** "22:00" from minute-of-day, for the quiet-hours selects. */
export function formatMinuteOfDay(minute: number): string {
  const hours = Math.floor(minute / 60);
  const minutes = minute % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

export const MINUTES_OF_DAY: readonly number[] = Array.from({ length: 96 }, (_, i) => i * 15);
