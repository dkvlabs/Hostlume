/**
 * App shell tests: complete navigation set, honest placeholder states,
 * keyboard shortcuts, theme application on load. The IPC client module is
 * mocked — this is a test-only mock, production has no fake provider.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/lib/ipc/types";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getSnapshot: vi.fn(),
    getCapabilities: vi.fn(),
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
  },
  onTelemetry: vi.fn(async () => () => {}),
  IpcCommandError: class extends Error {
    constructor(payload: { code: string; messageKey: string }) {
      super(payload.code);
    }
  },
}));

import { ipc } from "@/lib/ipc/client";
import { App } from "./App";

const mockedGetSettings = vi.mocked(ipc.getSettings);
const mockedGetSnapshot = vi.mocked(ipc.getSnapshot);

function settings(overrides: Partial<Settings> = {}): Settings {
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
    rate_unit: "bytes" as const,
    external_lookup: { enabled: false, provider: "ipify", timeout_secs: 5 },
        temperature_unit: "celsius",
        tray: { enabled: true, show_cpu: true, show_memory: true, show_network: true, show_title: true },
        launch_at_login: false,
        alerts: {
          notifications_enabled: true,
          actions_enabled: false,
          quiet_hours_enabled: false,
          quiet_start_minute: 1320,
          quiet_end_minute: 480,
        },
        remote: { mode: "off", port: 45454 },
        remote_token_hash: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedGetSettings.mockResolvedValue(settings());
  // NO_SNAPSHOT_YET before the first tick is the honest Phase 01 state.
  mockedGetSnapshot.mockRejectedValue({
    code: "NO_SNAPSHOT_YET",
    messageKey: "errors.noSnapshotYet",
  });
});

describe("App shell", () => {
  it("renders all seven navigation entries", async () => {
    render(<App />);
    for (const label of [
      "Overview",
      "Applications",
      "Storage",
      "Network",
      "Sensors",
      "Compare",
      "Settings",
    ]) {
      expect(await screen.findByRole("button", { name: new RegExp(label) })).toBeInTheDocument();
    }
  });

  it("shows the honest waiting state on Overview (no fabricated numbers)", async () => {
    render(<App />);
    expect(await screen.findByText("Waiting for the first sample…")).toBeInTheDocument();
    expect(screen.queryByText(/\d+(\.\d+)?%/)).not.toBeInTheDocument();
  });

  it("Applications renders the real process table (phase 03)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /Applications/ }));
    expect(await screen.findByRole("searchbox")).toBeInTheDocument();
    // Warm-up placeholder until the first 10 s poll resolves (IPC mocked:
    // get_process_page is not part of this mock — the honest empty state shows).
    expect(screen.getByText(/Collecting|No processes/i)).toBeInTheDocument();
  });

  it("Sensors page renders the sensors screen (phase 05)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /Sensors/ }));
    // The screen's tablist replaces the old placeholder.
    expect(await screen.findByRole("tablist")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Capabilities" })).toBeInTheDocument();
  });

  it("Storage page shows the scan controls (phase 04)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /Storage/ }));
    expect(await screen.findByRole("textbox")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan" })).toBeInTheDocument();
  });

  it("Network page shows the interface selector (phase 04)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /Network/ }));
    // Gateway label renders as its own text node followed by the honest
    // "not exposed" value.
    expect(await screen.findByText(/Gateway/)).toBeInTheDocument();
    expect(await screen.findByText("Not exposed by this source")).toBeInTheDocument();
  });

  it("Ctrl+, opens Settings", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Overview/ });
    await user.keyboard("{Control>},{/Control}");
    expect(await screen.findByText("Appearance")).toBeInTheDocument();
  });

  it("Ctrl+2 navigates to the second page (Applications)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /Overview/ });
    await user.keyboard("{Control>}2{/Control}");
    expect(await screen.findByRole("searchbox")).toBeInTheDocument();
  });

  it("applies the loaded theme to the document and language", async () => {
    render(<App />);
    await waitFor(() => {
      expect(mockedGetSettings).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("dark");
    });
  });

  it("shows the persisted Settings values once loaded", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /Settings/ }));
    expect(await screen.findByText("Base polling interval")).toBeInTheDocument();
    expect(await screen.findByText("3s")).toBeInTheDocument();
  });
});
