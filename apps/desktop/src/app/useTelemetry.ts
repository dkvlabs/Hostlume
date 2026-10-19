/**
 * Wires the Rust scheduler to the telemetry store: fetch the current
 * snapshot on mount, refresh on every push event whose sequence advanced,
 * and flag staleness when ticks stop arriving (sleep, suspend, error).
 *
 * Sequence gating (phase 09 release smoke fix): `appliedSequence` is the
 * sequence of the snapshot the store currently holds and only advances when
 * a fetched snapshot is applied. The push event merely signals that a newer
 * snapshot exists; it must not pre-advance the applied sequence, otherwise
 * the fetch that follows (same sequence as the event) is discarded and the
 * UI only refreshes when a tick happens to land between emit and fetch.
 */
import { useEffect } from "react";
import { ipc, onTelemetry } from "@/lib/ipc/client";
import { isOlderThanFreshness, useTelemetryStore } from "@/stores/telemetryStore";

export function useTelemetry(): void {
  const applySnapshot = useTelemetryStore((s) => s.applySnapshot);
  const markStale = useTelemetryStore((s) => s.markStale);
  const setError = useTelemetryStore((s) => s.setError);
  const setConnecting = useTelemetryStore((s) => s.setConnecting);

  useEffect(() => {
    let disposed = false;
    let appliedSequence = -1;
    let inFlight = false;
    let pendingSequence = -1;
    setConnecting();

    const fetchSnapshot = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const snapshot = await ipc.getSnapshot();
        if (disposed) return;
        if (snapshot.sequence > appliedSequence) {
          appliedSequence = snapshot.sequence;
          applySnapshot(snapshot);
        }
      } catch (err) {
        if (disposed) return;
        // NO_SNAPSHOT_YET before the first tick is a normal Phase 01 state,
        // not an error banner; anything else is reported.
        const code = (err as { code?: string }).code;
        if (code !== "NO_SNAPSHOT_YET") {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        inFlight = false;
        // A newer tick was announced while this fetch was in flight.
        if (!disposed && pendingSequence > appliedSequence) {
          pendingSequence = -1;
          void fetchSnapshot();
        }
      }
    };

    void fetchSnapshot();
    const unlistenPromise = onTelemetry((payload) => {
      if (payload.sequence > appliedSequence) {
        pendingSequence = payload.sequence;
        void fetchSnapshot();
      }
    });
    unlistenPromise.catch((err: unknown) => {
      if (!disposed) {
        setError(`telemetry subscription failed: ${err instanceof Error ? err.message : String(err)}`);
      }
    });

    const stalenessTimer = window.setInterval(() => {
      const { receivedAtMs, connection } = useTelemetryStore.getState();
      if (connection === "live" && isOlderThanFreshness(receivedAtMs, Date.now())) {
        markStale();
      }
    }, 3_000);

    return () => {
      disposed = true;
      window.clearInterval(stalenessTimer);
      void unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [applySnapshot, markStale, setError, setConnecting]);
}
