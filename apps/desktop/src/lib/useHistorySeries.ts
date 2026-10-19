/**
 * History series hook (phase 03 §3C.12): ranges up to 15 min come from the
 * in-memory ring buffers; 1 h / 24 h come from the database through the
 * bounded `get_history` query (auto resolution, gap + missing reporting).
 */
import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc/client";
import { metricKey, type SamplePoint } from "@/lib/metrics";
import { useTelemetryStore } from "@/stores/telemetryStore";

export type HistoryRange = 60 | 300 | 900 | 3_600 | 86_400;

const DB_RANGES: ReadonlySet<number> = new Set([3_600, 86_400]);

export interface HistorySeries {
  points: SamplePoint[];
  /** Database ranges only: gap segments and missing fraction. */
  gaps: { from_utc_ms: number; to_utc_ms: number; reason: string }[];
  missingRatio: number | null;
  loading: boolean;
  error: string | null;
}

const EMPTY: HistorySeries = {
  points: [],
  gaps: [],
  missingRatio: null,
  loading: false,
  error: null,
};

export function useHistorySeries(
  metricId: string,
  entityId: string,
  rangeSeconds: HistoryRange
): HistorySeries {
  const ring = useTelemetryStore((s) => s.history[metricKey(metricId, entityId)]);
  const receivedAtMs = useTelemetryStore((s) => s.receivedAtMs);
  const [db, setDb] = useState<HistorySeries>(EMPTY);

  const fromDb = DB_RANGES.has(rangeSeconds);
  useEffect(() => {
    if (!fromDb) return;
    let disposed = false;
    setDb({ ...EMPTY, loading: true });
    const load = () => {
      const to = Date.now();
      const from = to - rangeSeconds * 1_000;
      void ipc
        .getHistory({ metric_id: metricId, entity_id: entityId, from_utc_ms: from, to_utc_ms: to })
        .then((response) => {
          if (disposed) return;
          setDb({
            points: response.points.map((p) => ({ t: p.ts_utc_ms, v: p.value })),
            gaps: response.gaps,
            missingRatio: response.missing_ratio,
            loading: false,
            error: null,
          });
        })
        .catch((err) => {
          if (disposed) return;
          setDb({
            ...EMPTY,
            error: err instanceof Error ? err.message : String(err),
          });
        });
    };
    load();
    const timer = window.setInterval(load, 60_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [fromDb, metricId, entityId, rangeSeconds, receivedAtMs]);

  if (!fromDb) {
    const points = ring?.last(rangeSeconds / 3) ?? [];
    return { points, gaps: [], missingRatio: null, loading: false, error: null };
  }
  return db;
}
