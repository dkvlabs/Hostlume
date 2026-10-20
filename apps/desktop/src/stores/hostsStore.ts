/**
 * Paired-host presentation store (phase 10, FX-022). The Rust runtime owns
 * polling, credentials and liveness; this cache mirrors `get_hosts_state`
 * and refreshes on `hosts-updated` events. Nothing here ever sees a device
 * token or a pairing code after submission.
 */
import { create } from "zustand";
import { ipc, IpcCommandError } from "@/lib/ipc/client";
import type { HostLiveState, RemoveHostResult } from "@/lib/ipc/types";

interface HostsState {
  hosts: HostLiveState[];
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
  refresh: () => Promise<void>;
  /** Runs the pairing exchange; rethrows so the dialog can show the cause. */
  pair: (endpoint: string, code: string, label: string) => Promise<HostLiveState>;
  remove: (hostId: string, purgeHistory: boolean) => Promise<RemoveHostResult | null>;
  rename: (hostId: string, label: string) => Promise<boolean>;
  setRecording: (hostId: string, enabled: boolean) => Promise<boolean>;
}

function describe(err: unknown): string {
  if (err instanceof IpcCommandError) return err.messageKey;
  return err instanceof Error ? err.message : String(err);
}

function upsert(hosts: HostLiveState[], host: HostLiveState): HostLiveState[] {
  const next = hosts.filter((h) => h.host_id !== host.host_id);
  next.push(host);
  next.sort((a, b) => a.label.localeCompare(b.label) || a.host_id.localeCompare(b.host_id));
  return next;
}

export const useHostsStore = create<HostsState>()((set, get) => ({
  hosts: [],
  status: "idle",
  error: null,

  refresh: async () => {
    if (get().status === "idle") set({ status: "loading" });
    try {
      const hosts = await ipc.getHostsState();
      set({ hosts, status: "ready", error: null });
    } catch (err) {
      set({ status: "error", error: describe(err) });
    }
  },

  pair: async (endpoint, code, label) => {
    const host = await ipc.pairHost(endpoint, code, label);
    set((state) => ({ hosts: upsert(state.hosts, host), status: "ready", error: null }));
    return host;
  },

  remove: async (hostId, purgeHistory) => {
    try {
      const result = await ipc.removeHost(hostId, purgeHistory);
      set((state) => ({
        hosts: state.hosts.filter((h) => h.host_id !== hostId),
        error: null,
      }));
      return result;
    } catch (err) {
      set({ error: describe(err) });
      return null;
    }
  },

  rename: async (hostId, label) => {
    try {
      const host = await ipc.renameHost(hostId, label);
      set((state) => ({ hosts: upsert(state.hosts, host), error: null }));
      return true;
    } catch (err) {
      set({ error: describe(err) });
      return false;
    }
  },

  setRecording: async (hostId, enabled) => {
    try {
      const host = await ipc.setHostRecording(hostId, enabled);
      set((state) => ({ hosts: upsert(state.hosts, host), error: null }));
      return true;
    } catch (err) {
      set({ error: describe(err) });
      return false;
    }
  },
}));
