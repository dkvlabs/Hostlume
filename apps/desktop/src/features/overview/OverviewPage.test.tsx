/**
 * Overview integration: real snapshot values render, hidden cards stay
 * hidden, ordering follows settings, and the network selector switches
 * source. The IPC module is mocked (test-only) to feed a crafted snapshot.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings, SystemSnapshot } from "@/lib/ipc/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { OverviewPage } from "./OverviewPage";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTelemetryStore } from "@/stores/telemetryStore";

// The IPC module is mocked (test-only): Overview reads persisted settings
// from the store, which the real App loads over IPC.
vi.mock("@/lib/ipc/client", () => ({
  ipc: { getSettings: vi.fn(), updateSettings: vi.fn() },
  onTelemetry: vi.fn(async () => () => {}),
}));

const mockedGetSettings = vi.mocked(
  (await import("@/lib/ipc/client")).ipc.getSettings
);

function defaultSettings(): Settings {
  return {
    schema_version: 1,
    appearance: "dark",
    compact_mode: false,
    polling_base_seconds: 3,
    close_to_tray: false,
    locale: "en",
    overview_cards: [
      { id: "cpu", visible: true },
      { id: "memory", visible: true },
      { id: "storage", visible: true },
      { id: "network", visible: true },
      { id: "host", visible: true },
    ],
    history: { raw_hours: 24, rollup_1m_days: 30, rollup_1h_days: 365 },
  };
}

function snapshotWith(metrics: SystemSnapshot["metrics"]): SystemSnapshot {
  return {
    schema_version: 1,
    host_id: "h",
    generation: "g",
    sequence: 5,
    collected_at_utc: "2026-10-19T01:02:03.000Z",
    capabilities_revision: 1,
    metrics,
  };
}

function reading(
  metric_id: string,
  entity_id: string,
  value: number | string | null,
  overrides: Partial<SystemSnapshot["metrics"][number]> = {}
): SystemSnapshot["metrics"][number] {
  return {
    metric_id,
    entity_id,
    value,
    unit: "percent",
    availability: value === null ? "temporarily_unavailable" : "available",
    source: "test",
    collected_at_utc: "2026-10-19T01:02:03.000Z",
    monotonic_sequence: 5,
    ...overrides,
  };
}

function resetStores(snapshot: SystemSnapshot | null, settings?: Settings) {
  useTelemetryStore.setState({
    snapshot,
    stale: false,
    connection: snapshot ? "live" : "connecting",
    error: null,
    receivedAtMs: snapshot ? Date.now() : null,
    history: {},
  });
  useSettingsStore.setState({
    settings: settings ?? defaultSettings(),
    status: "ready",
    error: null,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedGetSettings.mockResolvedValue(defaultSettings());
});

describe("OverviewPage", () => {
  it("renders real snapshot values on the cards", async () => {
    resetStores(
      snapshotWith([
        reading("cpu.total.usage", "host", 42.5, { unit: "percent" }),
        reading("cpu.core.usage", "core:0", 10, { unit: "percent" }),
        reading("cpu.core.usage", "core:1", 90, { unit: "percent" }),
        reading("memory.physical.used", "host", 8 * 1024 ** 3, { unit: "bytes" }),
        reading("memory.physical.total", "host", 32 * 1024 ** 3, { unit: "bytes" }),
        reading("memory.swap.used", "host", 1024, { unit: "bytes" }),
        reading("memory.swap.total", "host", 4096, { unit: "bytes" }),
        reading("host.hostname", "host", "testbook", { unit: "text" }),
        reading("host.uptime", "host", 93_600, { unit: "seconds" }),
      ])
    );
    render(
      <TooltipProvider>
        <OverviewPage />
      </TooltipProvider>
    );
    expect(await screen.findByTestId("card-cpu")).toBeInTheDocument();
    expect(screen.getByText("42.5%")).toBeInTheDocument();
    // Per-core bars render both cores.
    expect(screen.getByRole("list", { name: "Per-core usage" })).toBeInTheDocument();
    // Memory: 8 GiB used of 32 GiB.
    expect(screen.getByText(/8\.0 GiB/)).toBeInTheDocument();
    // Host card identity from the snapshot.
    expect(screen.getByText("testbook")).toBeInTheDocument();
    expect(screen.getByText("1d 2h 0m")).toBeInTheDocument();
  });

  it("shows warm-up states as Collecting — never zeros", async () => {
    resetStores(
      snapshotWith([
        reading("cpu.total.usage", "host", null, {
          unit: "percent",
          availability: "temporarily_unavailable",
          reason_code: "WARMUP",
        }),
      ])
    );
    render(
      <TooltipProvider>
        <OverviewPage />
      </TooltipProvider>
    );
    expect(await screen.findByTestId("card-cpu")).toBeInTheDocument();
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
    expect(screen.queryByText(/0(\.\d+)?%/)).not.toBeInTheDocument();
  });

  it("hides cards marked invisible in settings and keeps settings order", async () => {
    const settings = defaultSettings();
    settings.overview_cards = [
      { id: "host", visible: true },
      { id: "cpu", visible: false },
      { id: "memory", visible: true },
    ];
    resetStores(snapshotWith([reading("host.hostname", "host", "h", { unit: "text" })]), settings);
    render(
      <TooltipProvider>
        <OverviewPage />
      </TooltipProvider>
    );
    await waitFor(() => {
      expect(screen.queryByTestId("card-cpu")).not.toBeInTheDocument();
    });
    expect(screen.getByTestId("card-host")).toBeInTheDocument();
    expect(screen.getByTestId("card-memory")).toBeInTheDocument();
    const hostWrapper = screen.getByTestId("card-host").closest("[data-card]")!;
    const memoryWrapper = screen.getByTestId("card-memory").closest("[data-card]")!;
    expect(hostWrapper.compareDocumentPosition(memoryWrapper) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("switches network interface source on selection without leaking the other's data", async () => {
    const user = userEvent.setup();
    resetStores(
      snapshotWith([
        reading("net.interface.rx_rate", "if:en0:0", 1000, { unit: "bytes_per_second" }),
        reading("net.interface.tx_rate", "if:en0:0", 2000, { unit: "bytes_per_second" }),
        reading("net.interface.ipv4", "if:en0:0", "192.168.1.10/24", { unit: "text" }),
        reading("net.interface.rx_rate", "if:utun4:1", 10, { unit: "bytes_per_second" }),
        reading("net.interface.tx_rate", "if:utun4:1", 20, { unit: "bytes_per_second" }),
        reading("net.interface.ipv4", "if:utun4:1", "10.8.0.2/32", { unit: "text" }),
      ])
    );
    render(
      <TooltipProvider>
        <OverviewPage />
      </TooltipProvider>
    );
    // Default selection is the first interface (en0).
    expect(await screen.findByText("1.0 KB/s")).toBeInTheDocument();
    await user.click(await screen.findByRole("combobox", { name: "Interface" }));
    await user.click(await screen.findByRole("option", { name: "utun4" }));
    await waitFor(() => {
      expect(screen.getByText("10 B/s")).toBeInTheDocument();
    });
    // The previous interface's address must be gone from the card.
    expect(screen.queryByText(/192\.168\.1\.10/)).not.toBeInTheDocument();
    expect(screen.getByText(/10\.8\.0\.2/)).toBeInTheDocument();
  });
});
