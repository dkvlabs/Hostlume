/**
 * Settings page: the factory-reset lifecycle (phase 09 §2.3) must be
 * double-confirmed, call the backend exactly once and render an honest
 * outcome in both directions. Alerts and remote sections render with
 * empty fixture data — their behavior is covered by their own tests.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/lib/ipc/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SettingsPage } from "./SettingsPage";
import { useSettingsStore } from "@/stores/settingsStore";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
    resetSettings: vi.fn(),
    clearHistory: vi.fn(),
    getAlertRules: vi.fn(async () => []),
    getAlertEvents: vi.fn(async () => []),
    getNotificationState: vi.fn(async () => ({
      platform: "macos",
      state: "os_managed",
      detail_key: "settings.notifications.macosManaged",
    })),
    getRemoteStatus: vi.fn(async () => ({
      running: false,
      mode: "off",
      port: 45454,
      token_exists: false,
    })),
    configureRemote: vi.fn(),
    rotateRemoteToken: vi.fn(),
    revokeRemoteToken: vi.fn(),
    startRecording: vi.fn(),
    stopRecording: vi.fn(),
    listRecordings: vi.fn(async () => []),
    exportCsv: vi.fn(),
    exportRecordingCsv: vi.fn(),
  },
}));

import { ipc } from "@/lib/ipc/client";

const mockedResetSettings = vi.mocked(ipc.resetSettings);

function settingsFixture(overrides: Partial<Settings> = {}): Settings {
  return {
    schema_version: 1,
    appearance: "system",
    compact_mode: false,
    polling_base_seconds: 3,
    close_to_tray: false,
    locale: "en",
    overview_cards: [
      { id: "cpu", visible: true },
      { id: "memory", visible: true },
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

function renderPage() {
  return render(
    <TooltipProvider>
      <SettingsPage />
    </TooltipProvider>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  useSettingsStore.setState({
    settings: settingsFixture(),
    status: "ready",
    error: null,
  });
});

describe("SettingsPage factory reset", () => {
  it("requires confirmation, resets once and reports success", async () => {
    const user = userEvent.setup();
    const customized = settingsFixture({
      close_to_tray: true,
      remote: { mode: "loopback", port: 51515 },
      remote_token_hash: "abc",
    });
    useSettingsStore.setState({ settings: customized, status: "ready", error: null });
    const defaults = settingsFixture();
    mockedResetSettings.mockResolvedValue(defaults);

    renderPage();
    await user.click(await screen.findByRole("button", { name: "Reset settings…" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeTruthy();
    expect(mockedResetSettings).not.toHaveBeenCalled();

    await user.click(
      within(dialog).getByRole("button", { name: "Reset settings…" })
    );
    await waitFor(() => expect(mockedResetSettings).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(useSettingsStore.getState().settings?.close_to_tray).toBe(false)
    );
    expect(
      await screen.findByText("Settings restored to defaults")
    ).toBeTruthy();
  });

  it("cancel performs no reset", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Reset settings…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /cancel/i }));
    expect(mockedResetSettings).not.toHaveBeenCalled();
  });

  it("a rejected reset reports failure and keeps current settings", async () => {
    const user = userEvent.setup();
    mockedResetSettings.mockRejectedValue({
      code: "STORE_UNAVAILABLE",
      messageKey: "errors.storeUnavailable",
    });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Reset settings…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(
      within(dialog).getByRole("button", { name: "Reset settings…" })
    );
    expect(
      await screen.findByText(
        "Reset was rejected; your settings are unchanged."
      )
    ).toBeTruthy();
    expect(useSettingsStore.getState().settings?.polling_base_seconds).toBe(3);
  });
});
