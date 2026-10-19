import { describe, expect, it } from "vitest";
import { sparklineSegments } from "./Sparkline";
import type { SamplePoint } from "@/lib/metrics";

function ramp(): SamplePoint[] {
  return [0, 1, 2, 3].map((i) => ({ t: i * 1000, v: i * 10 }));
}

describe("sparklineSegments", () => {
  it("builds a single continuous segment for clean data", () => {
    const segments = sparklineSegments(ramp(), 200, 40, 30_000);
    expect(segments).toHaveLength(1);
    expect(segments[0]!.d.startsWith("M ")).toBe(true);
    expect(segments[0]!.d.includes(" L ")).toBe(true);
  });

  it("splits at null gap markers — never bridges across them", () => {
    const points: SamplePoint[] = [
      { t: 0, v: 1 },
      { t: 1000, v: 2 },
      { t: 2000, v: null }, // gap
      { t: 3000, v: 5 },
      { t: 4000, v: 6 },
    ];
    const segments = sparklineSegments(points, 200, 40, 30_000);
    expect(segments).toHaveLength(2);
    // Each segment restarts with M (move), never connecting over the gap.
    for (const segment of segments) {
      expect(segment.d.startsWith("M ")).toBe(true);
    }
  });

  it("splits when the time distance exceeds maxGapMs", () => {
    const points: SamplePoint[] = [
      { t: 0, v: 1 },
      { t: 1000, v: 2 },
      { t: 60_000, v: 3 }, // sleep-sized hole
      { t: 61_000, v: 4 },
    ];
    const segments = sparklineSegments(points, 200, 40, 30_000);
    expect(segments).toHaveLength(2);
  });

  it("returns no drawable segment for fewer than two defined points", () => {
    expect(sparklineSegments([{ t: 0, v: 1 }], 200, 40, 30_000)).toHaveLength(0);
    expect(sparklineSegments([{ t: 0, v: null }], 200, 40, 30_000)).toHaveLength(0);
    expect(sparklineSegments([], 200, 40, 30_000)).toHaveLength(0);
  });
});
