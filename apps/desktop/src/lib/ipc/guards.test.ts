import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  isCapabilityReport,
  isMetricReading,
  isSettings,
  isSystemSnapshot,
  isTelemetryEventPayload,
} from "./guards";
import { SETTINGS_SCHEMA_VERSION } from "./types";
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
    overview_cards: [{ id: "cpu", visible: true }],
    history: { raw_hours: 24, rollup_1m_days: 30, rollup_1h_days: 365 },
    rate_unit: "bytes",
    external_lookup: { enabled: false, provider: "ipify", timeout_secs: 5 },
    temperature_unit: "celsius",
    tray: { enabled: true, show_cpu: true, show_memory: true, show_network: true, show_title: true },
    launch_at_login: false,
    remote: { mode: "off", port: 45454 },
    remote_token_hash: null,
    alerts: {
      notifications_enabled: true,
      actions_enabled: false,
      quiet_hours_enabled: false,
      quiet_start_minute: 1320,
      quiet_end_minute: 480,
    },
  };

  it("accepts valid settings", () => {
    expect(isSettings(valid)).toBe(true);
  });

  it("accepts the real Settings::default() serialization from Rust (contract fixture)", () => {
    // Pinned by apps/desktop/src-tauri/src/settings.rs
    // `default_settings_fixture_matches_frontend_contract`. Vitest runs with
    // cwd = apps/desktop (jsdom gives import.meta.url an http scheme).
    const fixture: unknown = JSON.parse(
      readFileSync(
        join(process.cwd(), "src/lib/ipc/__fixtures__/settings-default.json"),
        "utf8"
      )
    );
    expect(isSettings(fixture)).toBe(true);
    expect((fixture as { schema_version: number }).schema_version).toBe(
      SETTINGS_SCHEMA_VERSION
    );
  });

  it("accepts any current-or-future positive settings schema version", () => {
    // Rust migrates rows at load time and owns the version number; the
    // guard must not pin it to the telemetry contract version.
    expect(isSettings({ ...valid, schema_version: SETTINGS_SCHEMA_VERSION })).toBe(true);
    expect(isSettings({ ...valid, schema_version: 9 })).toBe(true);
  });

  it("rejects non-positive or non-integer schema, unknown appearance or bad card list", () => {
    expect(isSettings({ ...valid, schema_version: 0 })).toBe(false);
    expect(isSettings({ ...valid, schema_version: 1.5 })).toBe(false);
    expect(isSettings({ ...valid, schema_version: "8" })).toBe(false);
    expect(isSettings({ ...valid, appearance: "sepia" })).toBe(false);
    expect(isSettings({ ...valid, polling_base_seconds: "3" })).toBe(false);
    expect(isSettings({ ...valid, overview_cards: "cpu" })).toBe(false);
    expect(isSettings({ ...valid, overview_cards: [{ id: 3, visible: true }] })).toBe(false);
  });

  it("rejects malformed alert settings", () => {
    expect(isSettings({ ...valid, alerts: null })).toBe(false);
    expect(
      isSettings({
        ...valid,
        alerts: { ...valid.alerts, actions_enabled: "no" },
      })
    ).toBe(false);
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
