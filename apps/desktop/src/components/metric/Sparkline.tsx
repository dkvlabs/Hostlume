/**
 * Gap-aware sparkline (phase 02 §2C.12): null points and time gaps break
 * the line — charts never connect through invalid intervals (design.md
 * §15, contract §4). Pure SVG; a full ECharts time-series component lands
 * with persistent history in Phase 03.
 */
import { useId } from "react";
import type { SamplePoint } from "@/lib/metrics";

export interface SparklineProps {
  points: SamplePoint[];
  /** Line breaks when consecutive samples are farther apart than this (ms). */
  maxGapMs?: number;
  width?: number;
  height?: number;
  ariaLabel: string;
  className?: string;
}

interface Segment {
  d: string;
}

export function sparklineSegments(
  points: SamplePoint[],
  width: number,
  height: number,
  maxGapMs: number
): Segment[] {
  const defined = points.filter((p) => p.v !== null) as { t: number; v: number }[];
  if (defined.length < 2) return [];
  const t0 = defined[0]!.t;
  const t1 = defined[defined.length - 1]!.t;
  const span = t1 - t0;
  if (span <= 0) return [];
  let min = defined[0]!.v;
  let max = defined[0]!.v;
  for (const p of defined) {
    if (p.v < min) min = p.v;
    if (p.v > max) max = p.v;
  }
  const range = max - min || 1;
  const pad = 2;
  const x = (t: number) => pad + ((t - t0) / span) * (width - pad * 2);
  const y = (v: number) => height - pad - ((v - min) / range) * (height - pad * 2);

  const segments: Segment[] = [];
  let current = "";
  let previous: { t: number; v: number } | null = null;
  for (const point of points) {
    if (point.v === null) {
      if (current) segments.push({ d: current });
      current = "";
      previous = null;
      continue;
    }
    const cursor = { t: point.t, v: point.v };
    if (!previous) {
      current = `M ${x(cursor.t).toFixed(1)} ${y(cursor.v).toFixed(1)}`;
    } else if (cursor.t - previous.t > maxGapMs) {
      if (current) segments.push({ d: current });
      current = `M ${x(cursor.t).toFixed(1)} ${y(cursor.v).toFixed(1)}`;
    } else {
      current += ` L ${x(cursor.t).toFixed(1)} ${y(cursor.v).toFixed(1)}`;
    }
    previous = cursor;
  }
  if (current) segments.push({ d: current });
  return segments.filter((s) => s.d.includes("L"));
}

export function Sparkline({
  points,
  maxGapMs = 30_000,
  width = 200,
  height = 40,
  ariaLabel,
  className,
}: SparklineProps) {
  const gradientId = useId();
  const segments = sparklineSegments(points, width, height, maxGapMs);
  return (
    <svg
      role="img"
      aria-label={ariaLabel}
      viewBox={`0 0 ${width} ${height}`}
      className={className}
      preserveAspectRatio="none"
    >
      {segments.length === 0 ? (
        <line
          x1={2}
          y1={height / 2}
          x2={width - 2}
          y2={height / 2}
          stroke="var(--stroke)"
          strokeWidth={1}
          strokeDasharray="3 3"
        />
      ) : (
        <>
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--accent)" stopOpacity={0.25} />
              <stop offset="100%" stopColor="var(--accent)" stopOpacity={0} />
            </linearGradient>
          </defs>
          {segments.map((segment, index) => (
            <path
              key={index}
              d={`${segment.d} L ${width - 2} ${height} L 2 ${height} Z`}
              fill={`url(#${gradientId})`}
              stroke="none"
            />
          ))}
          {segments.map((segment, index) => (
            <path
              key={`line-${index}`}
              d={segment.d}
              fill="none"
              stroke="var(--accent)"
              strokeWidth={1.5}
              strokeLinejoin="round"
              strokeLinecap="round"
            />
          ))}
        </>
      )}
    </svg>
  );
}
