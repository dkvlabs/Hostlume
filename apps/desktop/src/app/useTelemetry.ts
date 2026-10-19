/**
 * Wires the Rust scheduler to the telemetry store: fetch the current
 * snapshot on mount, refresh on every push event whose sequence advanced,
 * and flag staleness when ticks stop arriving (sleep, suspend, error).
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
    let knownSequence = -1;
    setConnecting();

    const fetchSnapshot = async () => {
      try {
        const snapshot = await ipc.getSnapshot();
        if (disposed) return;
        if (snapshot.sequence > knownSequence) {
          knownSequence = snapshot.sequence;
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
      }
    };

    void fetchSnapshot();
    const unlistenPromise = onTelemetry((payload) => {
      if (payload.sequence > knownSequence) {
        knownSequence = payload.sequence;
        void fetchSnapshot();
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
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [applySnapshot, markStale, setError, setConnecting]);
}
