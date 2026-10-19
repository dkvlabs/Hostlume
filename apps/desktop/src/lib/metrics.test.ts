import { describe, expect, it } from "vitest";
import {
  entityLabel,
  formatUptime,
  MetricHistory,
  metricKey,
  numericValue,
} from "./metrics";
import type { MetricReading } from "@/lib/ipc/types";

describe("MetricHistory", () => {
  it("is bounded at capacity, dropping oldest points", () => {
    const history = new MetricHistory(3);
    for (let i = 0; i < 5; i++) {
      history.push({ t: i * 1000, v: i });
    }
    expect(history.length).toBe(3);
    expect(history.all().map((p) => p.t)).toEqual([2000, 3000, 4000]);
  });

  it("keeps null gap markers (charts must break, not bridge)", () => {
    const history = new MetricHistory(10);
    history.push({ t: 0, v: 1 });
    history.push({ t: 1000, v: null });
    history.push({ t: 2000, v: 3 });
    expect(history.all()).toEqual([
      { t: 0, v: 1 },
      { t: 1000, v: null },
      { t: 2000, v: 3 },
    ]);
    expect(history.last(2)).toEqual([
      { t: 1000, v: null },
      { t: 2000, v: 3 },
    ]);
  });
});

describe("numericValue", () => {
  const reading = (overrides: Partial<MetricReading>): MetricReading => ({
    metric_id: "m",
    entity_id: "e",
    value: 5,
    unit: "count",
    availability: "available",
    source: "s",
    collected_at_utc: "2026-10-19T00:00:00.000Z",
    monotonic_sequence: 1,
    ...overrides,
  });

  it("returns numbers only for available readings", () => {
    expect(numericValue(reading({}))).toBe(5);
    expect(
      numericValue(reading({ value: null, availability: "temporarily_unavailable" }))
    ).toBeNull();
    expect(numericValue(reading({ value: 42, availability: "unsupported" }))).toBeNull();
  });
});

describe("entityLabel", () => {
  it("strips typed prefixes and generations", () => {
    expect(entityLabel("core:3")).toBe("3");
    expect(entityLabel("if:en0:1")).toBe("en0");
    expect(entityLabel("vol:/")).toBe("/");
    expect(entityLabel("host")).toBe("host");
  });
});

describe("metricKey", () => {
  it("joins id and entity", () => {
    expect(metricKey("cpu.total.usage", "host")).toBe("cpu.total.usage|host");
  });
});

describe("formatUptime", () => {
  it("formats human uptime", () => {
    expect(formatUptime(59)).toBe("0m");
    expect(formatUptime(3_600)).toBe("1h 0m");
    expect(formatUptime(93_600)).toBe("1d 2h 0m");
    expect(formatUptime(Number.NaN)).toBe("—");
  });
});
