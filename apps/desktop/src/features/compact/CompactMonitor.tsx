/**
 * Compact floating monitor (phase 06 §2.6): the documented fallback for
 * desktop environments without a tray and an on-demand option from the
 * tray menu. Read-only; values come from the same snapshot bus as the
 * main window — unavailable metrics render as dashes, never zeros.
 */
import { useEffect, useState } from "react";
import { MetricAvailability } from "@/components/metric/MetricAvailability";
import { useT } from "@/lib/i18n";
import { firstReading, numericValue, textValue } from "@/lib/metrics";
import { formatByUnit } from "@/lib/formatting/units";
import { useTelemetry } from "@/app/useTelemetry";
import { useTelemetryStore } from "@/stores/telemetryStore";
import { useSettingsStore } from "@/stores/settingsStore";

declare global {
  interface Window {
    __HOSTLUME_COMPACT__?: boolean;
  }
}

export function isCompactMode(): boolean {
  return typeof window !== "undefined" && window.__HOSTLUME_COMPACT__ === true;
}

export function CompactMonitor() {
  const t = useT();
  useTelemetry();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const loadSettings = useSettingsStore((s) => s.load);
  const rateUnit = useSettingsStore((s) => s.settings?.rate_unit ?? "bytes");
  const [updatedAt, setUpdatedAt] = useState<string | null>(null);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (snapshot?.collected_at_utc) {
      // Local time rendering of the UTC stamp (contract §5).
      setUpdatedAt(new Date(snapshot.collected_at_utc).toLocaleTimeString());
    }
  }, [snapshot]);

  const cpu = firstReading(snapshot, "cpu.total.usage");
  const cpuValue = numericValue(cpu);
  const used = firstReading(snapshot, "memory.physical.used");
  const total = firstReading(snapshot, "memory.physical.total");
  const usedValue = numericValue(used);
  const totalValue = numericValue(total);
  const rx = firstReading(snapshot, "net.interface.rx_rate");
  const tx = firstReading(snapshot, "net.interface.tx_rate");
  const rxValue = numericValue(rx);
  const txValue = numericValue(tx);
  const stale = snapshot === null;

  return (
    <div
      data-testid="compact-monitor"
      className="flex h-screen flex-col justify-between gap-2 overflow-hidden bg-surface p-3 text-text select-none"
    >
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold tracking-wide text-muted">
          {t("app.name")}
        </span>
        <span className="numeric text-[10px] text-muted" title={t("status.lastUpdate")}>
          {updatedAt ?? "—"}
        </span>
      </div>
      <dl className="grid gap-1 text-sm">
        <div className="flex items-center justify-between gap-2">
          <dt className="text-muted">{t("overview.card.cpu")}</dt>
          <dd className="numeric flex items-center gap-2">
            {cpu && cpu.availability !== "available" ? (
              <MetricAvailability
                availability={cpu.availability}
                reasonCode={cpu.reason_code}
              />
            ) : null}
            {cpuValue === null ? "—" : formatByUnit(cpuValue, "percent", 0)}
          </dd>
        </div>
        <div className="flex items-center justify-between gap-2">
          <dt className="text-muted">{t("overview.card.memory")}</dt>
          <dd className="numeric">
            {usedValue === null || totalValue === null || totalValue <= 0
              ? "—"
              : `${formatByUnit(usedValue, "bytes", 0)} / ${formatByUnit(totalValue, "bytes", 0)}`}
          </dd>
        </div>
        <div className="flex items-center justify-between gap-2">
          <dt className="text-muted">{t("overview.card.network")}</dt>
          <dd className="numeric flex items-center gap-2">
            {rx && rx.availability !== "available" ? (
              <MetricAvailability
                availability={rx.availability}
                reasonCode={rx.reason_code}
              />
            ) : null}
            {rxValue === null || txValue === null
              ? "—"
              : `↓ ${formatByUnit(rxValue, rateUnit === "bits" ? "bits_per_second" : "bytes_per_second", 0)} ↑ ${formatByUnit(txValue, rateUnit === "bits" ? "bits_per_second" : "bytes_per_second", 0)}`}
          </dd>
        </div>
      </dl>
      <p className="text-[10px] text-muted">
        {stale || snapshot === null
          ? t("states.collecting")
          : textValue(firstReading(snapshot, "host.hostname")) ?? ""}
      </p>
    </div>
  );
}
