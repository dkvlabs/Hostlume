/**
 * Metric card (design.md §4): label, current value, unit, availability
 * indicator, last-sampled timestamp. The hard rule: a missing reading is a
 * dash plus an availability state — never `0`, never a stale value posed as
 * live. All of loading/ready/empty/unsupported/permission_required/
 * temporarily_unavailable/stale/error are expressible here and covered by
 * component tests.
 */
import type { ReactNode } from "react";
import { MetricAvailability } from "@/components/metric/MetricAvailability";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useT } from "@/lib/i18n";
import { formatByUnit } from "@/lib/formatting/units";
import { cn } from "@/lib/utils";
import type { MetricReading, MetricUnit } from "@/lib/ipc/types";

export interface MetricCardProps {
  label: string;
  unit: MetricUnit;
  /** null ⇒ no sample yet (loading/empty treatment). */
  reading: MetricReading | null;
  /** Timestamp of the reading to show when the snapshot is stale. */
  footer?: ReactNode;
  compact?: boolean;
  className?: string;
}

function renderValue(reading: MetricReading | null, unit: MetricUnit): string {
  if (!reading || reading.value === null) return "—";
  if (reading.availability !== "available") return "—";
  const { value } = reading;
  if (typeof value === "number") {
    return formatByUnit(value, unit, reading.precision ?? 1);
  }
  return String(value);
}

export function MetricCard({ label, unit, reading, footer, compact, className }: MetricCardProps) {
  const t = useT();
  const availability = reading?.availability ?? "temporarily_unavailable";
  const reasonCode = reading?.reason_code ?? "WARMUP";
  const isStale = availability === "stale";
  const showLive = reading !== null && reading.availability === "available";

  return (
    <Card
      className={cn("min-w-0", compact ? "p-3" : "p-5", isStale && "opacity-60", className)}
      data-availability={availability}
    >
      <CardHeader className={compact ? "mb-2" : undefined}>
        <CardTitle>{label}</CardTitle>
        <MetricAvailability availability={availability} reasonCode={reasonCode} />
      </CardHeader>
      <CardContent>
        <p className="numeric text-2xl font-semibold" aria-live="polite">
          {renderValue(reading, unit)}
        </p>
        <p className="mt-1 text-xs text-muted">
          {showLive
            ? `${t("status.lastUpdate")}: ${reading.collected_at_utc}`
            : t("states.collecting")}
        </p>
        {footer}
      </CardContent>
    </Card>
  );
}
