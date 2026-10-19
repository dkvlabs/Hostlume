import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/lib/ipc/types";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
  },
}));

import { ipc } from "@/lib/ipc/client";
import { useSettingsStore } from "./settingsStore";

const mockedGetSettings = vi.mocked(ipc.getSettings);
const mockedUpdateSettings = vi.mocked(ipc.updateSettings);

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
});
