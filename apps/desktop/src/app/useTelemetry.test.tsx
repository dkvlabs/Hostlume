/**
 * Regression for the phase 09 release-smoke finding: the push event only
 * announces that a newer snapshot exists. The fetch it triggers returns a
 * snapshot whose sequence equals the event's sequence, and that snapshot
 * MUST be applied. The original code pre-advanced the known sequence from
 * the event, so the fetched snapshot failed the strict `>` check and the UI
 * only refreshed when a tick happened to land between emit and fetch.
 */
import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SystemSnapshot, TelemetryEventPayload } from "@/lib/ipc/types";
import { useTelemetryStore } from "@/stores/telemetryStore";

type TelemetryCallback = (payload: TelemetryEventPayload) => void;
let captured: TelemetryCallback | null = null;

vi.mock("@/lib/ipc/client", () => ({
  ipc: { getSnapshot: vi.fn() },
  onTelemetry: vi.fn(async (callback: TelemetryCallback) => {
    captured = callback;
    return () => {};
  }),
}));

import { ipc } from "@/lib/ipc/client";
import { useTelemetry } from "./useTelemetry";

const mockedGetSnapshot = vi.mocked(ipc.getSnapshot);

function snapshot(sequence: number): SystemSnapshot {
  return {
    schema_version: 1,
    host_id: "host",
    generation: "gen",
    sequence,
    collected_at_utc: `2026-10-19T18:00:${String(sequence).padStart(2, "0")}.000Z`,
    capabilities_revision: 1,
    metrics: [],
  };
}

function event(sequence: number): TelemetryEventPayload {
  return {
    schemaVersion: 1,
    hostId: "host",
    generation: "gen",
    sequence,
    collectedAtUtc: `2026-10-19T18:00:${String(sequence).padStart(2, "0")}.000Z`,
    capabilitiesRevision: 1,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  captured = null;
  useTelemetryStore.setState({
    snapshot: null,
    stale: false,
    connection: "connecting",
    error: null,
    receivedAtMs: null,
    history: {},
  });
});

describe("useTelemetry sequence gating", () => {
  it("applies the fetched snapshot when its sequence equals the announcing event", async () => {
    mockedGetSnapshot.mockResolvedValueOnce(snapshot(1));
    renderHook(() => useTelemetry());
    await waitFor(() => expect(useTelemetryStore.getState().snapshot?.sequence).toBe(1));
    await waitFor(() => expect(captured).not.toBeNull());

    // Tick 2: Rust emits seq=2, get_snapshot returns the same seq=2.
    mockedGetSnapshot.mockResolvedValueOnce(snapshot(2));
    captured!(event(2));
    await waitFor(() => expect(useTelemetryStore.getState().snapshot?.sequence).toBe(2));
    expect(useTelemetryStore.getState().connection).toBe("live");

    // Tick 3 likewise — every tick must refresh, not every other tick.
    mockedGetSnapshot.mockResolvedValueOnce(snapshot(3));
    captured!(event(3));
    await waitFor(() => expect(useTelemetryStore.getState().snapshot?.sequence).toBe(3));
    expect(mockedGetSnapshot).toHaveBeenCalledTimes(3);
  });

  it("ignores events that do not advance past the applied snapshot", async () => {
    mockedGetSnapshot.mockResolvedValueOnce(snapshot(5));
    renderHook(() => useTelemetry());
    await waitFor(() => expect(useTelemetryStore.getState().snapshot?.sequence).toBe(5));
    await waitFor(() => expect(captured).not.toBeNull());

    captured!(event(5));
    captured!(event(4));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(mockedGetSnapshot).toHaveBeenCalledTimes(1);
  });

  it("coalesces an event that arrives while a fetch is in flight and refetches once", async () => {
    let resolveFirst: ((value: SystemSnapshot) => void) | null = null;
    mockedGetSnapshot.mockImplementationOnce(
      () => new Promise<SystemSnapshot>((resolve) => { resolveFirst = resolve; })
    );
    renderHook(() => useTelemetry());
    await waitFor(() => expect(captured).not.toBeNull());

    // Two ticks announced while the mount fetch is still pending.
    captured!(event(1));
    captured!(event(2));
    mockedGetSnapshot.mockResolvedValueOnce(snapshot(2));
    resolveFirst!(snapshot(1));
    await waitFor(() => expect(useTelemetryStore.getState().snapshot?.sequence).toBe(2));
    // mount fetch + exactly one follow-up fetch for the coalesced ticks
    expect(mockedGetSnapshot).toHaveBeenCalledTimes(2);
  });
});
