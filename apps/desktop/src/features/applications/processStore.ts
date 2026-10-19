/**
 * Applications page state (FX-007): polled process rows (single 10 s
 * source — never per-row timers) plus short per-process CPU history keyed
 * by **PID + start time** so exits and PID reuse drop old series instead
 * of mislabeling the new owner (phase 03 §3B.8).
 */
import { create } from "zustand";
import { ipc } from "@/lib/ipc/client";
import type { ProcessPage, ProcessRow } from "@/lib/ipc/types";

export const PROCESS_POLL_MS = 10_000;
const HISTORY_CAPACITY = 120; // ~20 min at 10 s poll
const MAX_TRACKED_SERIES = 3_000;

export type ProcessSortKey = "cpu" | "memory" | "pid" | "name";

interface ProcessIdentity {
  pid: number;
  startTimeSec: number;
}

export function processKey(row: ProcessRow): string {
  return `${row.pid}:${row.startTimeSec}`;
}

/**
 * Value-ascending base comparator (cpu/memory by value, pid/name
 * lexically); `ascending=false` reverses it. Warm-up rows (null CPU)
 * sort below everything for cpu.
 */
export function sortRows(
  rows: ProcessRow[],
  key: ProcessSortKey,
  ascending: boolean
): ProcessRow[] {
  const sorted = [...rows];
  sorted.sort((a, b) => {
    switch (key) {
      case "cpu":
        return (a.cpuUsageCores ?? -1) - (b.cpuUsageCores ?? -1);
      case "memory":
        return a.rssBytes - b.rssBytes;
      case "pid":
        return a.pid - b.pid;
      case "name":
        return a.name.localeCompare(b.name);
    }
  });
  return ascending ? sorted : sorted.reverse();
}

interface ProcessState {
  page: ProcessPage | null;
  error: string | null;
  /** Per-identity recent CPU (core-equivalent) samples, oldest first. */
  cpuHistory: Record<string, { t: number; v: number }[]>;
  refresh: () => Promise<void>;
  pruneHistory: (currentKeys: Set<string>) => void;
  reset: () => void;
}

export const useProcessStore = create<ProcessState>()((set, get) => ({
  page: null,
  error: null,
  cpuHistory: {},

  refresh: async () => {
    try {
      const page = await ipc.getProcessPage();
      const history = { ...get().cpuHistory };
      const seen: string[] = [];
      for (const row of page.rows) {
        const key = processKey(row);
        seen.push(key);
        if (row.cpuUsageCores === null) continue;
        const series = history[key] ? [...history[key]!] : [];
        series.push({ t: Date.parse(page.collectedAtUtc), v: row.cpuUsageCores });
        if (series.length > HISTORY_CAPACITY) {
          series.splice(0, series.length - HISTORY_CAPACITY);
        }
        history[key] = series;
      }
      // Drop series for identities that vanished (exit or PID reuse).
      if (seen.length > 0) {
        for (const key of Object.keys(history)) {
          if (!seen.includes(key) && Object.keys(history).length > MAX_TRACKED_SERIES) {
            delete history[key];
          }
        }
      }
      set({ page, error: null, cpuHistory: history });
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
    }
  },

  pruneHistory: (currentKeys) =>
    set((state) => {
      const history = { ...state.cpuHistory };
      let removed = false;
      for (const key of Object.keys(history)) {
        if (!currentKeys.has(key)) {
          delete history[key];
          removed = true;
        }
      }
      return removed ? { cpuHistory: history } : {};
    }),

  reset: () => set({ page: null, error: null, cpuHistory: {} }),
}));

export function identityOf(row: ProcessRow): ProcessIdentity {
  return { pid: row.pid, startTimeSec: row.startTimeSec };
}
