/**
 * Network details (FX-006/011): interface selector, addresses, cumulative
 * totals, rate charts, and the opt-in-only external IP lookup. All live
 * data comes from the subscribed snapshot; gateway is honestly absent
 * (the source does not expose it).
 */
import { useEffect, useState } from "react";
import { Sparkline } from "@/components/metric/Sparkline";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useT } from "@/lib/i18n";
import { formatByUnit } from "@/lib/formatting/units";
import {
  entityLabel,
  findReading,
  numericValue,
  readingsFor,
  textValue,
} from "@/lib/metrics";
import { useHistorySeries, type HistoryRange } from "@/lib/useHistorySeries";
import { ipc } from "@/lib/ipc/client";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTelemetryStore } from "@/stores/telemetryStore";

export function NetworkPage() {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const settings = useSettingsStore((s) => s.settings);
  const interfaces = readingsFor(snapshot, "net.interface.rx_rate");
  const [selected, setSelected] = useState<string | null>(null);
  const range: HistoryRange = 300;
  const [externalNote, setExternalNote] = useState<string | null>(null);

  const entityIds = interfaces.map((i) => i.entity_id);
  useEffect(() => {
    if (selected && !entityIds.includes(selected)) setSelected(entityIds[0] ?? null);
    else if (!selected && entityIds.length > 0) setSelected(entityIds[0]!);
  }, [selected, entityIds.join("|")]);

  const rxRate = selected ? findReading(snapshot, "net.interface.rx_rate", selected) : undefined;
  const txRate = selected ? findReading(snapshot, "net.interface.tx_rate", selected) : undefined;
  const rxTotal = selected ? findReading(snapshot, "net.interface.rx_total", selected) : undefined;
  const txTotal = selected ? findReading(snapshot, "net.interface.tx_total", selected) : undefined;
  const ipv4 = selected ? textValue(findReading(snapshot, "net.interface.ipv4", selected)) : null;
  const ipv6 = selected ? textValue(findReading(snapshot, "net.interface.ipv6", selected)) : null;
  const rxPoints = useTelemetryHistory("net.interface.rx_rate", selected, range);
  const txPoints = useTelemetryHistory("net.interface.tx_rate", selected, range);
  const rxValue = numericValue(rxRate);
  const txValue = numericValue(txRate);
  const rateUnit = settings?.rate_unit ?? "bytes";
  const rate = (value: number) =>
    rateUnit === "bits"
      ? `${formatByUnit(value * 8, "bits_per_second", 1)}`
      : formatByUnit(value, "bytes_per_second", 1);

  const requestExternalIp = async () => {
    setExternalNote(null);
    try {
      const ip = await ipc.lookupExternalIp();
      setExternalNote(ip);
    } catch (err) {
      setExternalNote(
        err instanceof Error ? err.message : String(err)
      );
    }
  };

  return (
    <div className="grid gap-4 p-5">
      <Card>
        <CardHeader>
          <CardTitle>{t("network.title")}</CardTitle>
          {rxRate && rxRate.availability !== "available" ? (
            <Badge variant="accent">{t("states.collecting")}</Badge>
          ) : null}
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
            <span className="text-xs text-muted">{t("network.interface")}</span>
            <Select value={selected ?? undefined} onValueChange={(v) => setSelected(v)}>
              <SelectTrigger aria-label={t("network.interface")} className="w-72">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {entityIds.map((id) => (
                  <SelectItem key={id} value={id}>
                    {entityLabel(id)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-xs text-muted">{t("overview.down")}</p>
              <p className="numeric text-lg font-semibold">
                {rxValue === null ? "—" : rate(rxValue)}
              </p>
              <Sparkline points={rxPoints} ariaLabel={t("overview.down")} className="h-10 w-full" />
            </div>
            <div>
              <p className="text-xs text-muted">{t("overview.up")}</p>
              <p className="numeric text-lg font-semibold">
                {txValue === null ? "—" : rate(txValue)}
              </p>
              <Sparkline points={txPoints} ariaLabel={t("overview.up")} className="h-10 w-full" />
            </div>
          </div>

          <p className="numeric text-xs text-muted">
            {t("network.downTotal")}:{" "}
            {rxTotal && numericValue(rxTotal) !== null
              ? formatByUnit(numericValue(rxTotal) as number, "bytes")
              : "—"}{" "}
            · {t("network.upTotal")}:{" "}
            {txTotal && numericValue(txTotal) !== null
              ? formatByUnit(numericValue(txTotal) as number, "bytes")
              : "—"}
          </p>

          <p className="text-xs text-muted">
            {t("overview.addresses")}:{" "}
            <span className="numeric">{[ipv4, ipv6].filter(Boolean).join(" · ") || "—"}</span>
          </p>

          <p className="text-xs text-muted">
            {t("network.linkState")}:{" "}
            <span>{selected ? "up" : "—"}</span> · {t("network.gateway")}:{" "}
            <span>{t("network.gatewayUnavailable")}</span>
          </p>

          <div>
            <Button
              size="sm"
              variant="secondary"
              disabled={!settings?.external_lookup.enabled}
              title={
                settings?.external_lookup.enabled
                  ? undefined
                  : t("network.externalIpOff").replace(
                      "{provider}",
                      settings?.external_lookup.provider ?? "ipify"
                    )
              }
              onClick={() => void requestExternalIp()}
            >
              {t("network.externalIp")}
            </Button>
            {externalNote ? (
              <span className="numeric ml-2 text-xs text-muted">{externalNote}</span>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function useTelemetryHistory(
  metricId: string,
  entityId: string | null,
  range: HistoryRange
) {
  return useHistorySeries(metricId, entityId ?? "", range).points;
}
