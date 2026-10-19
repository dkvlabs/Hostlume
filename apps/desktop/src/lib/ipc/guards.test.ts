import { describe, expect, it } from "vitest";
import {
  isCapabilityReport,
  isMetricReading,
  isSettings,
  isSystemSnapshot,
  isTelemetryEventPayload,
} from "./guards";
import type { SystemSnapshot } from "./types";

function validSnapshot(): SystemSnapshot {
  return {
    schema_version: 1,
    host_id: "0f0e3a2c-1111-4222-8333-444455556666",
    generation: "gen-1",
    sequence: 7,
    collected_at_utc: "2026-10-19T00:00:00.000Z",
    capabilities_revision: 1,
    metrics: [
      {
        metric_id: "cpu.total.usage",
        entity_id: "host",
        value: 12.5,
        unit: "percent",
        availability: "available",
        source: "test",
        collected_at_utc: "2026-10-19T00:00:00.000Z",
        monotonic_sequence: 7,
      },
      {
        metric_id: "sensor.temperature",
        entity_id: "sensor:cpu",
        value: null,
        unit: "celsius",
        availability: "unsupported",
        source: "test",
        collected_at_utc: "2026-10-19T00:00:00.000Z",
        monotonic_sequence: 7,
        reason_code: "NO_PUBLIC_READABLE_SENSOR",
      },
    ],
  };
}

describe("isSystemSnapshot", () => {
  it("accepts a contract-valid snapshot", () => {
    expect(isSystemSnapshot(validSnapshot())).toBe(true);
  });

  it("rejects a wrong schema version", () => {
    expect(isSystemSnapshot({ ...validSnapshot(), schema_version: 2 })).toBe(false);
  });

  it("rejects an available reading with null value", () => {
    const s = validSnapshot();
    (s.metrics[0] as { value: unknown }).value = null;
    expect(isSystemSnapshot(s)).toBe(false);
  });

  it("rejects a non-available reading carrying a value", () => {
    const s = validSnapshot();
    (s.metrics[1] as { value: unknown }).value = 42;
    expect(isSystemSnapshot(s)).toBe(false);
  });

  it("rejects an unknown availability", () => {
    const s = validSnapshot();
    (s.metrics[0] as { availability: string }).availability = "fine_i_guess";
    expect(isSystemSnapshot(s)).toBe(false);
  });

  it("rejects a NaN value even under available", () => {
    const s = validSnapshot();
    (s.metrics[0] as { value: unknown }).value = Number.NaN;
    expect(isSystemSnapshot(s)).toBe(false);
  });

  it("rejects empty host identity", () => {
    expect(isSystemSnapshot({ ...validSnapshot(), host_id: "" })).toBe(false);
  });

  it("rejects non-objects", () => {
    expect(isSystemSnapshot(null)).toBe(false);
    expect(isSystemSnapshot("snapshot")).toBe(false);
    expect(isSystemSnapshot([])).toBe(false);
  });
});

describe("isMetricReading", () => {
  it("accepts boolean and text values when available", () => {
    expect(
      isMetricReading({
        metric_id: "m",
        entity_id: "e",
        value: true,
        unit: "boolean",
        availability: "available",
        source: "s",
        collected_at_utc: "2026-10-19T00:00:00.000Z",
        monotonic_sequence: 1,
      })
    ).toBe(true);
    expect(
      isMetricReading({
        metric_id: "m",
        entity_id: "e",
        value: "hello",
        unit: "text",
        availability: "available",
        source: "s",
        collected_at_utc: "2026-10-19T00:00:00.000Z",
        monotonic_sequence: 1,
      })
    ).toBe(true);
  });

  it("rejects unknown units", () => {
    expect(
      isMetricReading({
        metric_id: "m",
        entity_id: "e",
        value: 1,
        unit: "furlongs",
        availability: "available",
        source: "s",
        collected_at_utc: "2026-10-19T00:00:00.000Z",
        monotonic_sequence: 1,
      })
    ).toBe(false);
  });
});

describe("isSettings", () => {
  const valid = {
    schema_version: 1,
    appearance: "system",
    compact_mode: false,
    polling_base_seconds: 3,
    close_to_tray: false,
    locale: "en",
  };

  it("accepts valid settings", () => {
    expect(isSettings(valid)).toBe(true);
  });

  it("rejects wrong schema or unknown appearance", () => {
    expect(isSettings({ ...valid, schema_version: 9 })).toBe(false);
    expect(isSettings({ ...valid, appearance: "sepia" })).toBe(false);
    expect(isSettings({ ...valid, polling_base_seconds: "3" })).toBe(false);
  });
});

describe("isCapabilityReport", () => {
  it("accepts a valid report and rejects a wrong schema", () => {
    const report = {
      schema_version: 1,
      host_id: "h",
      generated_at_utc: "2026-10-19T00:00:00.000Z",
      revision: 1,
      capabilities: [
        {
          metric_id: "cpu.total.usage",
          state: "unsupported",
          source: "platform-macos",
          reason_code: "COLLECTOR_NOT_IMPLEMENTED",
          permissions_required: [],
          optional_helper_required: false,
        },
      ],
    };
    expect(isCapabilityReport(report)).toBe(true);
    expect(isCapabilityReport({ ...report, schema_version: 2 })).toBe(false);
  });
});

describe("isTelemetryEventPayload", () => {
  it("validates the lightweight push payload", () => {
    expect(
      isTelemetryEventPayload({
        schemaVersion: 1,
        hostId: "h",
        generation: "g",
        sequence: 1,
        collectedAtUtc: "2026-10-19T00:00:00.000Z",
        capabilitiesRevision: 1,
      })
    ).toBe(true);
    expect(
      isTelemetryEventPayload({
        schemaVersion: 2,
        hostId: "h",
        generation: "g",
        sequence: 1,
        collectedAtUtc: "x",
        capabilitiesRevision: 1,
      })
    ).toBe(false);
  });
});
