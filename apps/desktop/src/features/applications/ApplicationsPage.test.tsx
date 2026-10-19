/**
 * Security-privacy.md §9: "XSS through process name/path — rendered as
 * inert text". Process names are untrusted OS strings; they must appear
 * as literal text and never become live markup, in either the table or
 * the detail drawer.
 */
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProcessPage } from "@/lib/ipc/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ApplicationsPage } from "./ApplicationsPage";
import { useProcessStore } from "./processStore";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getProcessPage: vi.fn(),
    requestProcessAction: vi.fn(),
    getSettings: vi.fn(),
  },
}));

import { ipc } from "@/lib/ipc/client";

const HOSTILE_NAME = '<img src=x onerror=window.__pwned=1><script>window.__pwned=2</script>';

function pageWith(rows: ProcessPage["rows"]): ProcessPage {
  return {
    collectedAtUtc: "2026-10-19T12:00:00.000Z",
    cpuWarmup: false,
    logicalCores: 14,
    rows,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useProcessStore.setState({ page: null, error: null, cpuHistory: {} });
});

describe("applications page hostile process names", () => {
  it("renders markup-shaped names as inert text, never as elements", async () => {
    vi.mocked(ipc.getProcessPage).mockResolvedValue(
      pageWith([
        {
          pid: 4242,
          startTimeSec: 1_700_000_000,
          name: HOSTILE_NAME,
          rssBytes: 12_345_678,
          cpuUsageCores: 1.5,
          cpuUsageHost: 10,
          status: "Running",
          parentPid: 1,
        },
      ])
    );

    render(
      <TooltipProvider>
        <ApplicationsPage />
      </TooltipProvider>
    );

    // The literal text is present (React escaped it).
    await waitFor(() =>
      expect(screen.getAllByText(HOSTILE_NAME).length).toBeGreaterThan(0)
    );
    // And no element was created from it — no img, no script, no handler.
    expect(document.querySelector("img[src='x']")).toBeNull();
    expect(document.querySelector("script")).toBeNull();
    expect((window as unknown as Record<string, never>).__pwned).toBeUndefined();
  });
});
