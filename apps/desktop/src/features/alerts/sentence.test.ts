/**
 * Sentence builder and template rendering for alert rules (phase 07).
 * The curated metric list must stay inside the Rust validator's registry —
 * a mismatch would let the UI offer rules the backend always rejects.
 */
import { describe, expect, it } from "vitest";
import {
  ALERT_METRIC_OPTIONS,
  formatDuration,
  formatMinuteOfDay,
  metricOption,
  renderTemplate,
  ruleSentence,
} from "./sentence";
import type { AlertRule } from "@/lib/ipc/types";

function rule(overrides: Partial<AlertRule> = {}): AlertRule {
  return {
    id: "r1",
    metric_id: "cpu.total.usage",
    entity_filter: null,
    comparator: "above",
    threshold: 90,
    hold_duration_ms: 60_000,
    hysteresis_delta: 10,
    cooldown_ms: 300_000,
    enabled: true,
    notification_message: null,
    action: null,
    ...overrides,
  };
}

describe("ruleSentence", () => {
  it("renders the canonical sentence with hold and cooldown", () => {
    expect(ruleSentence(rule())).toBe(
      "Notify when CPU total usage rises above 90 % and stays there for 1 min. Re-alerts after 5 min."
    );
  });

  it("renders zero-hold and zero-cooldown honestly", () => {
    const sentence = ruleSentence(rule({ hold_duration_ms: 0, cooldown_ms: 0 }));
    expect(sentence).toBe("Notify when CPU total usage rises above 90 %. Re-alerts immediately.");
  });

  it("renders below-comparator battery rules with humanized bytes where relevant", () => {
    const sentence = ruleSentence(
      rule({
        metric_id: "memory.available",
        comparator: "below",
        threshold: 2_147_483_648,
        hold_duration_ms: 0,
      })
    );
    expect(sentence).toBe(
      "Notify when Memory available falls below 2 GiB. Re-alerts after 5 min."
    );
  });

  it("falls back to the raw metric id for unknown families", () => {
    expect(ruleSentence(rule({ metric_id: "future.metric" }))).toContain("future.metric");
  });
});

describe("formatDuration / formatMinuteOfDay", () => {
  it("formats compact durations", () => {
    expect(formatDuration(0)).toBe("0 s");
    expect(formatDuration(59_000)).toBe("59 s");
    expect(formatDuration(60_000)).toBe("1 min");
    expect(formatDuration(5_400_000)).toBe("1 h 30 min");
  });

  it("formats minute-of-day as HH:mm", () => {
    expect(formatMinuteOfDay(0)).toBe("00:00");
    expect(formatMinuteOfDay(22 * 60)).toBe("22:00");
    expect(formatMinuteOfDay(8 * 60 + 15)).toBe("08:15");
  });
});

describe("renderTemplate", () => {
  it("substitutes notification placeholders only", () => {
    expect(
      renderTemplate("{value} on {entity} crossed {threshold}", {
        value: 94.5,
        entity: "host",
        threshold: 90,
      })
    ).toBe("94.5 on host crossed 90");
  });

  it("leaves unknown placeholders untouched", () => {
    expect(renderTemplate("{cmd} {value}", { value: 1, entity: "e", threshold: 0 })).toBe(
      "{cmd} 1"
    );
  });
});

describe("ALERT_METRIC_OPTIONS", () => {
  it("only offers numeric families the Rust validator accepts", () => {
    // Mirror of monitor_core::families::numeric_unit — keep in lockstep.
    const numericFamilies = new Set([
      "host.uptime",
      "host.cpu.cores.logical",
      "host.cpu.cores.physical",
      "cpu.total.usage",
      "cpu.core.usage",
      "cpu.process.usage_cores",
      "cpu.process.usage_host",
      "memory.physical.total",
      "memory.physical.used",
      "memory.available",
      "memory.swap.used",
      "memory.swap.total",
      "disk.volume.capacity",
      "disk.volume.available",
      "disk.device.read_bytes_total",
      "disk.device.read_rate",
      "disk.device.write_bytes_total",
      "disk.device.write_rate",
      "net.interface.rx_total",
      "net.interface.tx_total",
      "net.interface.rx_rate",
      "net.interface.tx_rate",
      "gpu.engine.utilization",
      "gpu.memory.dedicated_used",
      "sensor.temperature",
      "sensor.fan.speed",
      "battery.charge",
      "battery.cycles",
      "battery.time_remaining",
      "battery.power_draw",
      "bluetooth.device.charge",
    ]);
    for (const option of ALERT_METRIC_OPTIONS) {
      expect(numericFamilies.has(option.id), option.id).toBe(true);
    }
  });

  it("resolves labels for known ids", () => {
    expect(metricOption("cpu.total.usage")?.label).toBe("CPU total usage");
    expect(metricOption("not.a.metric")).toBeUndefined();
  });
});
