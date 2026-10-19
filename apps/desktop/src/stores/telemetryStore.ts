/**
 * Presentation state for the live telemetry stream (architecture.md §7:
 * Zustand holds cached read-only snapshots for display only).
 *
 * `stale` vs `lastKnown` differentiation: when samples stop arriving the
 * snapshot is *kept* (it is the last known truth, tagged with its original
 * timestamp) but flagged stale so the UI renders it desaturated with the
 * stale chip — never as current, never dropped silently.
 *
 * History: bounded in-memory ring buffers per watched metric+entity feed
 * the Overview sparklines until the Phase 03 persistent store exists.
 * Non-available or vanished readings push `null` gap markers.
 */
import { create } from "zustand";
import type { SystemSnapshot } from "@/lib/ipc/types";
import {
  HISTORY_WATCHED,
  MetricHistory,
  metricKey,
  numericValue,
  type SamplePoint,
} from "@/lib/metrics";

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
  /** Ring buffers keyed by `<metric_id>|<entity_id>`. */
  history: Record<string, MetricHistory>;
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
  history: {},

  applySnapshot: (snapshot) =>
    set((state) => ({
      snapshot,
      stale: false,
      connection: "live",
      error: null,
      receivedAtMs: Date.now(),
      history: pushHistory(state.history, snapshot),
    })),

  markStale: () =>
    set((state) => (state.snapshot && !state.stale ? { stale: true } : {})),

  setConnecting: () => set({ connection: "connecting", error: null }),

  setError: (message) => set({ connection: "error", error: message }),
}));

/** Sample time from the reading's own collection timestamp. */
function sampleTime(snapshot: SystemSnapshot): number {
  const parsed = Date.parse(snapshot.collected_at_utc);
  return Number.isNaN(parsed) ? Date.now() : parsed;
}

function pushHistory(
  history: Record<string, MetricHistory>,
  snapshot: SystemSnapshot
): Record<string, MetricHistory> {
  const t = sampleTime(snapshot);
  const next = { ...history };
  for (const reading of snapshot.metrics) {
    if (!HISTORY_WATCHED.has(reading.metric_id)) continue;
    const key = metricKey(reading.metric_id, reading.entity_id);
    let series = next[key];
    if (!series) {
      series = new MetricHistory();
      next[key] = series;
    }
    const value = numericValue(reading);
    series.push({ t, v: value } satisfies SamplePoint);
  }
  return next;
}

/** True when the current snapshot is older than the freshness budget. */
export function isOlderThanFreshness(receivedAtMs: number | null, now: number): boolean {
  return receivedAtMs !== null && now - receivedAtMs > STALE_AFTER_MS;
}
