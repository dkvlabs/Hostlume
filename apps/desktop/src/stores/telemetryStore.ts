/**
 * Presentation state for the live telemetry stream (architecture.md §7:
 * Zustand holds cached read-only snapshots for display only).
 *
 * `stale` vs `lastKnown` differentiation: when samples stop arriving the
 * snapshot is *kept* (it is the last known truth, tagged with its original
 * timestamp) but flagged stale so the UI renders it desaturated with the
 * stale chip — never as current, never dropped silently.
 */
import { create } from "zustand";
import type { SystemSnapshot } from "@/lib/ipc/types";

/** No fresh snapshot for this long ⇒ UI must show the stale treatment. */
export const STALE_AFTER_MS = 15_000;

export type ConnectionState = "connecting" | "live" | "error";

interface TelemetryState {
  snapshot: SystemSnapshot | null;
  stale: boolean;
  connection: ConnectionState;
  error: string | null;
  /** Local reception time of the current snapshot (Date.now() domain). */
  receivedAtMs: number | null;
  applySnapshot: (snapshot: SystemSnapshot) => void;
  markStale: () => void;
  setConnecting: () => void;
  setError: (message: string) => void;
}

export const useTelemetryStore = create<TelemetryState>()((set) => ({
  snapshot: null,
  stale: false,
  connection: "connecting",
  error: null,
  receivedAtMs: null,

  applySnapshot: (snapshot) =>
    set({
      snapshot,
      stale: false,
      connection: "live",
      error: null,
      receivedAtMs: Date.now(),
    }),

  markStale: () =>
    set((state) => (state.snapshot && !state.stale ? { stale: true } : {})),

  setConnecting: () => set({ connection: "connecting", error: null }),

  setError: (message) => set({ connection: "error", error: message }),
}));

/** True when the current snapshot is older than the freshness budget. */
export function isOlderThanFreshness(receivedAtMs: number | null, now: number): boolean {
  return receivedAtMs !== null && now - receivedAtMs > STALE_AFTER_MS;
}
