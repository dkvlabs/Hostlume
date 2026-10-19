import { beforeEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/lib/ipc/client";
import type { ProcessPage, ProcessRow } from "@/lib/ipc/types";
import { processKey, sortRows, useProcessStore } from "./processStore";

vi.mock("@/lib/ipc/client", async () => {
  const actual =
    await import(/* webpackIgnore: true */ "@/lib/ipc/client");
  return {
    ...actual,
    ipc: {
      ...actual.ipc,
      getProcessPage: vi.fn(),
    },
  };
});

beforeEach(() => {
  vi.mocked(ipc.getProcessPage).mockReset();
});

function row(overrides: Partial<ProcessRow>): ProcessRow {
  return {
    pid: 1,
    startTimeSec: 100,
    name: "proc",
    rssBytes: 1024,
    cpuUsageCores: null,
    cpuUsageHost: null,
    status: "Run",
    parentPid: null,
    ...overrides,
  };
}

function page(rows: ProcessRow[]): ProcessPage {
  return {
    collectedAtUtc: "2026-10-19T01:00:00.000Z",
    cpuWarmup: false,
    logicalCores: 4,
    rows,
  };
}

describe("processKey", () => {
  it("keys by pid + start time, not name", () => {
    expect(processKey(row({ pid: 5, startTimeSec: 10 }))).toBe("5:10");
    // Same PID, different start time (PID reuse) => different key.
    expect(processKey(row({ pid: 5, startTimeSec: 999 }))).toBe("5:999");
  });
});

describe("sortRows", () => {
  const rows = [
    row({ pid: 1, name: "b", rssBytes: 300, cpuUsageCores: 10 }),
    row({ pid: 2, name: "a", rssBytes: 200, cpuUsageCores: 50 }),
    row({ pid: 3, name: "c", rssBytes: 900, cpuUsageCores: 150 }),
  ];

  it("sorts by memory descending when asked", () => {
    expect(sortRows(rows, "memory", false).map((r) => r.pid)).toEqual([3, 1, 2]);
  });

  it("sorts by cpu with core-equivalent values above 100", () => {
    expect(sortRows(rows, "cpu", false).map((r) => r.pid)).toEqual([3, 2, 1]);
    expect(sortRows(rows, "cpu", true).map((r) => r.pid)).toEqual([1, 2, 3]);
  });

  it("sorts by name ascending", () => {
    expect(sortRows(rows, "name", true).map((r) => r.name)).toEqual(["a", "b", "c"]);
  });
});

describe("processStore", () => {
  it("refresh persists rows and pushes CPU samples keyed by identity", async () => {
    useProcessStore.setState({ page: null, error: null, cpuHistory: {} });
    vi.mocked(ipc.getProcessPage).mockResolvedValueOnce(
      page([row({ pid: 7, startTimeSec: 42, cpuUsageCores: 12.5, cpuUsageHost: 3.1 })])
    );
    await useProcessStore.getState().refresh();
    const state = useProcessStore.getState();
    expect(state.page?.rows[0]?.pid).toBe(7);
    const key = "7:42";
    expect(state.cpuHistory[key]).toHaveLength(1);
    expect(state.cpuHistory[key]![0]!.v).toBe(12.5);
  });

  it("keeps separate series across a PID reuse (same pid, new start time)", async () => {
    useProcessStore.setState({ page: null, error: null, cpuHistory: {} });
    vi.mocked(ipc.getProcessPage).mockResolvedValueOnce(
      page([row({ pid: 7, startTimeSec: 42, cpuUsageCores: 1 })])
    );
    await useProcessStore.getState().refresh();
    vi.mocked(ipc.getProcessPage).mockResolvedValueOnce(
      page([row({ pid: 7, startTimeSec: 4242, cpuUsageCores: 2 })])
    );
    await useProcessStore.getState().refresh();
    const history = useProcessStore.getState().cpuHistory;
    expect(history["7:42"]).toBeDefined();
    expect(history["7:4242"]).toBeDefined();
    expect(history["7:42"]!.at(-1)!.v).toBe(1);
    expect(history["7:4242"]!.at(-1)!.v).toBe(2);
  });
});
