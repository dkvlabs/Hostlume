import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/lib/ipc/types";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
    resetSettings: vi.fn(),
  },
}));

import { ipc } from "@/lib/ipc/client";
import { useSettingsStore } from "./settingsStore";

const mockedGetSettings = vi.mocked(ipc.getSettings);
const mockedUpdateSettings = vi.mocked(ipc.updateSettings);
const mockedResetSettings = vi.mocked(ipc.resetSettings);

function validSettings(): Settings {
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
        remote_display_name: "This computer",
        remote_devices: [],
        remote_hosts: [],
        remote_poll_seconds: 5,
  };
}

function reset() {
  useSettingsStore.setState({
    settings: null,
    status: "idle",
    error: null,
  });
}

beforeEach(() => {
  reset();
  vi.clearAllMocks();
});

describe("settingsStore", () => {
  it("load success transitions idle → ready with persisted settings", async () => {
    mockedGetSettings.mockResolvedValue(validSettings());
    await useSettingsStore.getState().load();
    const state = useSettingsStore.getState();
    expect(state.status).toBe("ready");
    expect(state.settings?.polling_base_seconds).toBe(3);
  });

  it("load failure records the error status", async () => {
    mockedGetSettings.mockRejectedValue(new Error("store unavailable"));
    await useSettingsStore.getState().load();
    const state = useSettingsStore.getState();
    expect(state.status).toBe("error");
    expect(state.error).toContain("store unavailable");
  });

  it("update persists via IPC and adopts the returned server state", async () => {
    const initial = validSettings();
    useSettingsStore.setState({ settings: initial, status: "ready" });
    mockedUpdateSettings.mockResolvedValue({ ...initial, appearance: "dark" });
    const ok = await useSettingsStore.getState().update({ appearance: "dark" });
    expect(ok).toBe(true);
    expect(useSettingsStore.getState().settings?.appearance).toBe("dark");
    expect(mockedUpdateSettings).toHaveBeenCalledWith({ appearance: "dark" });
  });

  it("failed update rolls the UI back to the previous settings", async () => {
    const initial = validSettings();
    useSettingsStore.setState({ settings: initial, status: "ready" });
    mockedUpdateSettings.mockRejectedValue({
      code: "SETTINGS_INVALID",
      messageKey: "errors.settingsInvalid",
    });
    const ok = await useSettingsStore.getState().update({ polling_base_seconds: 999 });
    expect(ok).toBe(false);
    const state = useSettingsStore.getState();
    expect(state.settings?.polling_base_seconds).toBe(3);
    expect(state.error).toBeTruthy();
  });

  it("reset adopts the backend defaults (remote off, token erased)", async () => {
    const customized = {
      ...validSettings(),
      close_to_tray: true,
      remote: { mode: "loopback" as const, port: 51515 },
      remote_token_hash: "abc123",
    };
    useSettingsStore.setState({ settings: customized, status: "ready" });
    const defaults = validSettings();
    mockedResetSettings.mockResolvedValue(defaults);
    const ok = await useSettingsStore.getState().reset();
    expect(ok).toBe(true);
    expect(mockedResetSettings).toHaveBeenCalledWith();
    const state = useSettingsStore.getState();
    expect(state.settings?.close_to_tray).toBe(false);
    expect(state.settings?.remote.mode).toBe("off");
    expect(state.settings?.remote_token_hash).toBeNull();
    expect(state.error).toBeNull();
  });

  it("failed reset keeps the current settings and records the error", async () => {
    const initial = validSettings();
    useSettingsStore.setState({ settings: initial, status: "ready" });
    mockedResetSettings.mockRejectedValue({
      code: "STORE_UNAVAILABLE",
      messageKey: "errors.storeUnavailable",
    });
    const ok = await useSettingsStore.getState().reset();
    expect(ok).toBe(false);
    const state = useSettingsStore.getState();
    expect(state.settings).toBe(initial);
    expect(state.error).toBeTruthy();
  });
});
