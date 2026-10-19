/**
 * Sensors screen (FX-010/012/013/014/021): real snapshot values render,
 * unavailable families render reasoned empty states (never zeros), the
 * temperature unit converts at display time, and the Capabilities tab
 * exposes the report with an explicit re-probe. IPC is mocked (test-only).
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CapabilityReport, MetricReading, SystemSnapshot } from "@/lib/ipc/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SensorsPage } from "./SensorsPage";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTelemetryStore } from "@/stores/telemetryStore";

const report: CapabilityReport = {
  schema_version: 1,
  host_id: "h",
  generated_at_utc: "2026-10-19T10:00:00.000Z",
  revision: 3,
  capabilities: [
    {
      metric_id: "sensor.temperature",
      state: "available",
      source: "collector-sensors",
      explanation: "22 temperature sensors reported by the operating system.",
      permissions_required: [],
      optional_helper_required: false,
    },
    {
      metric_id: "sensor.fan.speed",
      state: "unsupported",
      source: "collector-sensors",
      explanation: "macOS does not expose fan RPM through public non-root interfaces.",
      reason_code: "NO_PUBLIC_READABLE_SENSOR",
      permissions_required: [],
      optional_helper_required: false,
    },
    {
      metric_id: "battery.charge",
      state: "available",
      source: "collector-battery",
      permissions_required: [],
      optional_helper_required: false,
    },
  ],
};

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getCapabilities: vi.fn(async () => report),
    reprobeCapabilities: vi.fn(async () => ({
      ...report,
      revision: report.revision + 1,
    })),
    getGpuInfo: vi.fn(async () => ({
      generated_at_utc: "2026-10-19T10:00:00.000Z",
      adapters: [
        {
          id: "gpu:0",
          name: "Apple M3 Max",
          vendor: "sppci_vendor_Apple",
          is_unified_memory: true,
          displays: [
            {
              name: "Color LCD",
              resolution: "2056 x 1329 @ 120.00Hz",
              is_main: true,
              is_online: true,
            },
          ],
        },
      ],
    })),
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
  },
  onTelemetry: vi.fn(async () => () => {}),
}));

function reading(
  metric_id: string,
  entity_id: string,
  value: number | string | null,
  overrides: Partial<MetricReading> = {},
): MetricReading {
  return {
    metric_id,
    entity_id,
    value,
    unit: metric_id === "sensor.temperature" ? "celsius" : "percent",
    availability: value === null ? "temporarily_unavailable" : "available",
    source: "test",
    collected_at_utc: "2026-10-19T10:00:00.000Z",
    monotonic_sequence: 1,
    ...overrides,
  };
}

function snapshotWith(metrics: MetricReading[]): SystemSnapshot {
  return {
    schema_version: 1,
    host_id: "h",
    generation: "g",
    sequence: 9,
    collected_at_utc: "2026-10-19T10:00:00.000Z",
    capabilities_revision: 1,
    metrics,
  };
}

function resetStores(snapshot: SystemSnapshot | null) {
  useTelemetryStore.setState({ snapshot, connection: "live", error: null });
  useSettingsStore.setState({
    settings: {
      schema_version: 1,
      appearance: "dark",
      compact_mode: false,
      polling_base_seconds: 3,
      close_to_tray: false,
      locale: "en",
      overview_cards: [],
      history: { raw_hours: 24, rollup_1m_days: 30, rollup_1h_days: 365 },
      rate_unit: "bytes",
      external_lookup: { enabled: false, provider: "ipify", timeout_secs: 5 },
      temperature_unit: "celsius",
    },
  });
}

function renderPage() {
  return render(
    <TooltipProvider>
      <SensorsPage />
    </TooltipProvider>,
  );
}

describe("SensorsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStores(
      snapshotWith([
        reading("sensor.temperature", "sensor:pmu_tdie6", 64.25),
        reading("sensor.temperature", "sensor:als_temp", null),
        reading("battery.charge", "battery:0", 84),
        reading("battery.status", "battery:0", "discharging", { unit: "text" }),
        reading("battery.health", "battery:0", 85.1),
        reading("battery.cycles", "battery:0", 180),
        reading("battery.time_remaining", "battery:0", 4.5 * 3600),
      ]),
    );
  });

  it("renders live temperatures with values and explicit unavailable states", async () => {
    renderPage();
    expect(await screen.findByText("Pmu Tdie6")).toBeInTheDocument();
    expect(screen.getByText("64.3")).toBeInTheDocument();
    // The silent sensor is a visible state, not a zero.
    expect(screen.getAllByRole("button", { name: /Als Temp/ }).length).toBeGreaterThan(0);
  });

  it("fans tab explains the unsupported state instead of showing zeros", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("tab", { name: "Fans" }));
    await waitFor(() => {
      expect(screen.getByTestId("sensors-no-data")).toBeInTheDocument();
    });
    expect(
      screen.getByText(/macOS does not expose fan RPM/),
    ).toBeInTheDocument();
  });

  it("battery tab shows charge, status, health and cycles from the snapshot", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("tab", { name: "Battery" }));
    expect(await screen.findByText("84%")).toBeInTheDocument();
    expect(screen.getByText("discharging")).toBeInTheDocument();
    expect(screen.getByText("85.1%")).toBeInTheDocument();
    expect(screen.getByText("180")).toBeInTheDocument();
  });

  it("gpu tab shows adapter identity, unified-memory label and display", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("tab", { name: "GPU" }));
    expect(await screen.findByText("Apple M3 Max")).toBeInTheDocument();
    expect(screen.getByText("Unified memory")).toBeInTheDocument();
    expect(screen.getByText(/2056 x 1329/)).toBeInTheDocument();
    expect(screen.getByText(/utilization is not exposed/i)).toBeInTheDocument();
  });

  it("capabilities tab lists every reported row with reasons and re-probes on demand", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("tab", { name: "Capabilities" }));
    expect(await screen.findByText("sensor.temperature")).toBeInTheDocument();
    expect(screen.getByText("battery.charge")).toBeInTheDocument();
    expect(
      screen.getByText(/macOS does not expose fan RPM/),
    ).toBeInTheDocument();

    const { ipc } = await import("@/lib/ipc/client");
    await user.click(screen.getByRole("button", { name: "Re-probe capabilities" }));
    await waitFor(() => {
      expect(ipc.reprobeCapabilities).toHaveBeenCalled();
    });
  });

  it("bluetooth tab renders the no-data notice when nothing is connected", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("tab", { name: "Bluetooth" }));
    await waitFor(() => {
      expect(screen.getByTestId("sensors-no-data")).toBeInTheDocument();
    });
  });
});
