import { describe, expect, it } from "vitest";
import {
  isOlderThanFreshness,
  useTelemetryStore,
  STALE_AFTER_MS,
} from "./telemetryStore";
import type { SystemSnapshot } from "@/lib/ipc/types";

function snapshot(sequence: number): SystemSnapshot {
  return {
    schema_version: 1,
    host_id: "h",
    generation: "g",
    sequence,
    collected_at_utc: "2026-10-19T00:00:00.000Z",
    capabilities_revision: 1,
    metrics: [],
  };
}

function reset() {
  useTelemetryStore.setState({
    snapshot: null,
    stale: false,
    connection: "connecting",
    error: null,
    receivedAtMs: null,
    history: {},
  });
}

describe("telemetryStore", () => {
  it("starts connecting with no snapshot (honest empty)", () => {
    reset();
    const state = useTelemetryStore.getState();
    expect(state.snapshot).toBeNull();
    expect(state.connection).toBe("connecting");
    expect(state.stale).toBe(false);
  });

  it("applySnapshot marks the stream live and clears staleness", () => {
    reset();
    useTelemetryStore.getState().applySnapshot(snapshot(1));
    const state = useTelemetryStore.getState();
    expect(state.connection).toBe("live");
    expect(state.stale).toBe(false);
    expect(state.snapshot?.sequence).toBe(1);
    expect(state.receivedAtMs).not.toBeNull();
  });

  it("markStale keeps the last-known snapshot but flags it", () => {
    reset();
    const store = useTelemetryStore.getState();
    store.applySnapshot(snapshot(2));
    store.markStale();
    let state = useTelemetryStore.getState();
    expect(state.stale).toBe(true);
    expect(state.snapshot?.sequence).toBe(2);
    // Idempotent: stays stale.
    useTelemetryStore.getState().markStale();
    state = useTelemetryStore.getState();
    expect(state.stale).toBe(true);
  });

  it("a new snapshot unsets the stale flag", () => {
    reset();
    useTelemetryStore.getState().applySnapshot(snapshot(3));
    useTelemetryStore.getState().markStale();
    useTelemetryStore.getState().applySnapshot(snapshot(4));
    expect(useTelemetryStore.getState().stale).toBe(false);
  });

  it("setError records the failure state", () => {
    reset();
    useTelemetryStore.getState().setError("boom");
    const state = useTelemetryStore.getState();
    expect(state.connection).toBe("error");
    expect(state.error).toBe("boom");
  });

  it("freshness check respects the staleness budget", () => {
    const now = 1_000_000;
    expect(isOlderThanFreshness(null, now)).toBe(false);
    expect(isOlderThanFreshness(now, now)).toBe(false);
    expect(isOlderThanFreshness(now - STALE_AFTER_MS, now)).toBe(false);
    expect(isOlderThanFreshness(now - STALE_AFTER_MS - 1, now)).toBe(true);
  });
});
