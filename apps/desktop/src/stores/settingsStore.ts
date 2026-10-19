/**
 * Settings presentation store. Single source of truth is the Rust settings
 * store (ADR-0002); this cache applies optimistic updates and rolls back on
 * rejection so validation failures never leave the UI contradicting disk.
 */
import { create } from "zustand";
import { ipc } from "@/lib/ipc/client";
import type { Settings, SettingsPatch } from "@/lib/ipc/types";

interface SettingsState {
  settings: Settings | null;
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
  load: () => Promise<void>;
  /** Optimistic update; returns true when the backend accepted it. */
  update: (patch: SettingsPatch) => Promise<boolean>;
  /** Restore factory settings; returns true when the backend accepted. */
  reset: () => Promise<boolean>;
}

export const useSettingsStore = create<SettingsState>()((set, get) => ({
  settings: null,
  status: "idle",
  error: null,

  load: async () => {
    if (get().status === "loading") return;
    set({ status: "loading", error: null });
    try {
      const settings = await ipc.getSettings();
      set({ settings, status: "ready" });
    } catch (err) {
      set({ status: "error", error: err instanceof Error ? err.message : String(err) });
    }
  },

  update: async (patch) => {
    const previous = get().settings;
    if (!previous) return false;
    const optimistic: Settings = {
      ...previous,
      ...(patch.appearance !== undefined ? { appearance: patch.appearance } : {}),
      ...(patch.compact_mode !== undefined ? { compact_mode: patch.compact_mode } : {}),
      ...(patch.polling_base_seconds !== undefined
        ? { polling_base_seconds: patch.polling_base_seconds }
        : {}),
      ...(patch.close_to_tray !== undefined ? { close_to_tray: patch.close_to_tray } : {}),
      ...(patch.locale !== undefined ? { locale: patch.locale } : {}),
    };
    set({ settings: optimistic });
    try {
      const persisted = await ipc.updateSettings(patch);
      set({ settings: persisted, error: null });
      return true;
    } catch (err) {
      set({
        settings: previous,
        error: err instanceof Error ? err.message : String(err),
      });
      return false;
    }
  },

  reset: async () => {
    try {
      const defaults = await ipc.resetSettings();
      set({ settings: defaults, error: null });
      return true;
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
      return false;
    }
  },
}));
