/**
 * Overview (FX-005): real values from the subscribed snapshot; visible
 * cards and their order come from persisted settings. Sparkline windows
 * are bounded by the in-memory ring buffer availability.
 */
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { useT } from "@/lib/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTelemetryStore } from "@/stores/telemetryStore";
import { CpuCard, HostCard, MemoryCard, NetworkCard, StorageCard } from "./cards";

/** Sparkline window -> sample counts, bounded by ring-buffer span (3 s cadence). */
const RANGE_SAMPLES: Record<number, number> = {
  60: 20,
  300: 100,
  900: 300,
};

function TimeRangeControl({
  range,
  onChange,
}: {
  range: number;
  onChange: (seconds: number) => void;
}) {
  const t = useT();
  return (
    <div className="flex items-center gap-1" role="group" aria-label="Time range">
      {[60, 300, 900].map((seconds) => (
        <Button
          key={seconds}
          size="sm"
          variant={range === seconds ? "default" : "ghost"}
          onClick={() => onChange(seconds)}
          aria-pressed={range === seconds}
          title={t("status.lastUpdate")}
        >
          {seconds < 60 ? `${seconds}s` : `${seconds / 60}m`}
        </Button>
      ))}
    </div>
  );
}

export function OverviewPage() {
  const t = useT();
  const connection = useTelemetryStore((s) => s.connection);
  const error = useTelemetryStore((s) => s.error);
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const cards = useSettingsStore((s) => s.settings?.overview_cards);
  const [range, setRange] = useState(60);
  const samples = RANGE_SAMPLES[range] ?? 20;

  if (connection === "error") {
    return (
      <div className="grid gap-4 p-5">
        <Card data-testid="overview-state">
          <CardContent className="pt-5 text-sm">
            <p className="text-critical" role="alert">
              {t("status.error")}
              {error ? ` — ${error}` : ""}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (snapshot === null) {
    return (
      <div className="grid gap-4 p-5">
        <Card data-testid="overview-state">
          <CardContent className="pt-5 text-sm">
            <p className="text-muted" role="status">
              {t("overview.waiting")}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  const cardMap: Record<string, React.ReactNode> = {
    cpu: <CpuCard samples={samples} />,
    memory: <MemoryCard samples={samples} />,
    storage: <StorageCard />,
    network: <NetworkCard samples={samples} />,
    host: <HostCard />,
  };

  const visibleCards = (cards ?? []).filter((card) => card.visible);

  return (
    <div className="grid gap-4 p-5">
      <div className="flex items-center justify-end">
        <TimeRangeControl range={range} onChange={setRange} />
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        {visibleCards.map((card) => (
          <div key={card.id} data-card={card.id}>
            {cardMap[card.id] ?? null}
          </div>
        ))}
      </div>
    </div>
  );
}
