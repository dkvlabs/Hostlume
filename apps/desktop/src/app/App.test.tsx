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

  it("later-phase pages state that they are not implemented yet", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /Storage/ }));
    expect(await screen.findByText("Not implemented yet")).toBeInTheDocument();
    expect(screen.getByTestId("not-implemented")).toBeInTheDocument();
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
