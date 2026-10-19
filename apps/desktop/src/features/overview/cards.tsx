/**
 * Overview cards (FX-001..006) — every value comes from the live snapshot
 * store; unavailable data renders as explicit states, never zeros.
 */
import { useEffect, useState } from "react";
import { MetricAvailability } from "@/components/metric/MetricAvailability";
import { Sparkline } from "@/components/metric/Sparkline";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useT } from "@/lib/i18n";
import {
  entityLabel,
  findReading,
  formatUptime,
  numericValue,
  readingsFor,
  textValue,
} from "@/lib/metrics";
import { useHistorySeries, type HistoryRange } from "@/lib/useHistorySeries";
import { formatByUnit } from "@/lib/formatting/units";
import { useTelemetryStore } from "@/stores/telemetryStore";

function UnavailableNote({ reasonCode }: { reasonCode?: string }) {
  return (
    <div className="flex items-center gap-2 text-sm text-muted">
      <span>—</span>
      <MetricAvailability availability="temporarily_unavailable" reasonCode={reasonCode ?? "WARMUP"} />
    </div>
  );
}

export function CpuCard({ range }: { range: HistoryRange }) {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const total = findReading(snapshot, "cpu.total.usage");
  const cores = readingsFor(snapshot, "cpu.core.usage").sort((a, b) =>
    a.entity_id.localeCompare(b.entity_id, undefined, { numeric: true })
  );
  const series = useHistorySeries("cpu.total.usage", "host", range);
  const totalPoints = series.points;
  const totalValue = numericValue(total);

  return (
    <Card data-testid="card-cpu">
      <CardHeader>
        <CardTitle>{t("overview.card.cpu")}</CardTitle>
        {total && total.availability !== "available" ? (
          <MetricAvailability availability={total.availability} reasonCode={total.reason_code} />
        ) : null}
      </CardHeader>
      <CardContent>
        {totalValue === null ? (
          <UnavailableNote reasonCode={total?.reason_code} />
        ) : (
          <>
            <p className="numeric text-2xl font-semibold">{formatByUnit(totalValue, "percent", 1)}</p>
            <Sparkline
              points={totalPoints}
              maxGapMs={Math.max(30_000, (range / 10) * 1_000)}
              ariaLabel={t("overview.card.cpu")}
              className="mt-2 h-10 w-full"
            />
            {series.gaps.length > 0 || series.missingRatio !== null ? (
              <p className="numeric mt-1 text-[10px] text-muted">
                {series.gaps.length} {t("history.gaps")}
                {series.missingRatio !== null
                  ? ` · ${(series.missingRatio * 100).toFixed(0)}% ${t("history.missing")}`
                  : ""}
              </p>
            ) : null}
          </>
        )}
        <p className="mt-3 mb-1 text-xs font-medium text-muted">{t("overview.perCore")}</p>
        <div className="grid gap-1" role="list" aria-label={t("overview.perCore")}>
          {cores.map((core) => {
            const value = numericValue(core);
            return (
              <div key={core.entity_id} role="listitem" className="flex items-center gap-2 text-xs">
                <span className="numeric w-8 text-muted">{entityLabel(core.entity_id)}</span>
                <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-raised">
                  <div
                    className="h-full rounded-full bg-accent"
                    style={{ width: `${value === null ? 0 : Math.min(100, value)}%` }}
                    data-state={value === null ? "unavailable" : "ok"}
                  />
                </div>
                <span className="numeric w-12 text-right">
                  {value === null ? "—" : formatByUnit(value, "percent", 0)}
                </span>
              </div>
            );
          })}
          {cores.length === 0 ? <UnavailableNote /> : null}
        </div>
      </CardContent>
    </Card>
  );
}

export function MemoryCard({ range }: { range: HistoryRange }) {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const used = findReading(snapshot, "memory.physical.used");
  const total = findReading(snapshot, "memory.physical.total");
  const swapUsed = findReading(snapshot, "memory.swap.used");
  const swapTotal = findReading(snapshot, "memory.swap.total");
  const points = useHistorySeries("memory.physical.used", "host", range).points;
  const usedValue = numericValue(used);
  const totalValue = numericValue(total);
  const percent =
    usedValue !== null && totalValue !== null && totalValue > 0 ? (usedValue / totalValue) * 100 : null;

  return (
    <Card data-testid="card-memory">
      <CardHeader>
        <CardTitle>{t("overview.card.memory")}</CardTitle>
        {used && used.availability !== "available" ? (
          <MetricAvailability availability={used.availability} reasonCode={used.reason_code} />
        ) : null}
      </CardHeader>
      <CardContent>
        {usedValue === null || totalValue === null ? (
          <UnavailableNote reasonCode={used?.reason_code} />
        ) : (
          <>
            <p className="numeric text-2xl font-semibold">
              {formatByUnit(usedValue, "bytes")}
              <span className="text-sm text-muted">
                {" "}
                {t("overview.usedOfTotal")} {formatByUnit(totalValue, "bytes")}
              </span>
            </p>
            <div
              className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-raised"
              role="progressbar"
              aria-valuenow={Math.round(percent ?? 0)}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div className="h-full rounded-full bg-accent" style={{ width: `${percent ?? 0}%` }} />
            </div>
            <Sparkline points={points} ariaLabel={t("overview.card.memory")} className="mt-2 h-10 w-full" />
          </>
        )}
        <p className="mt-3 text-xs text-muted">
          {t("overview.swap")}:{" "}
          {swapUsed && numericValue(swapUsed) !== null && swapTotal && numericValue(swapTotal) !== null ? (
            <span className="numeric">
              {formatByUnit(numericValue(swapUsed) as number, "bytes")} /{" "}
              {formatByUnit(numericValue(swapTotal) as number, "bytes")}
            </span>
          ) : (
            "—"
          )}
        </p>
      </CardContent>
    </Card>
  );
}

export function StorageCard() {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const capacities = readingsFor(snapshot, "disk.volume.capacity").sort((a, b) =>
    a.entity_id.localeCompare(b.entity_id)
  );
  return (
    <Card data-testid="card-storage">
      <CardHeader>
        <CardTitle>{t("overview.card.storage")}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-3">
        {capacities.length === 0 ? <p className="text-sm text-muted">{t("overview.noVolumes")}</p> : null}
        {capacities.map((capacity) => {
          const available = findReading(snapshot, "disk.volume.available", capacity.entity_id);
          const filesystem = textValue(
            findReading(snapshot, "disk.volume.filesystem", capacity.entity_id)
          );
          const removable = findReading(snapshot, "disk.volume.removable", capacity.entity_id);
          const readOnly = findReading(snapshot, "disk.volume.read_only", capacity.entity_id);
          const cap = numericValue(capacity);
          const avail = numericValue(available);
          const usedPercent = cap !== null && avail !== null && cap > 0 ? ((cap - avail) / cap) * 100 : null;
          return (
            <div key={capacity.entity_id} className="text-xs">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate font-medium">{entityLabel(capacity.entity_id)}</span>
                <span className="flex items-center gap-1">
                  {filesystem ? <Badge variant="neutral">{filesystem}</Badge> : null}
                  {removable?.value === true ? <Badge variant="accent">{t("overview.removable")}</Badge> : null}
                  {readOnly?.value === true ? <Badge variant="warning">{t("overview.readOnly")}</Badge> : null}
                </span>
              </div>
              <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-raised">
                <div
                  className="h-full rounded-full bg-accent"
                  style={{ width: `${usedPercent ?? 0}%` }}
                  data-state={usedPercent === null ? "unavailable" : "ok"}
                />
              </div>
              <p className="numeric mt-1 text-muted">
                {cap !== null ? `${formatByUnit(cap - (avail ?? 0), "bytes")} / ${formatByUnit(cap, "bytes")}` : "—"}
              </p>
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

export function NetworkCard({ range }: { range: HistoryRange }) {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const interfaces = readingsFor(snapshot, "net.interface.rx_rate");
  const [selected, setSelected] = useState<string | null>(null);

  const entityIds = interfaces.map((i) => i.entity_id);
  // Fall back to the first interface when the selection disconnects
  // (unplug/rename) instead of showing a broken card.
  useEffect(() => {
    if (selected && !entityIds.includes(selected)) {
      setSelected(entityIds[0] ?? null);
    } else if (!selected && entityIds.length > 0) {
      setSelected(entityIds[0]!);
    }
  }, [selected, entityIds.join("|")]);

  const rxRate = selected ? findReading(snapshot, "net.interface.rx_rate", selected) : undefined;
  const txRate = selected ? findReading(snapshot, "net.interface.tx_rate", selected) : undefined;
  const rxTotal = selected ? findReading(snapshot, "net.interface.rx_total", selected) : undefined;
  const txTotal = selected ? findReading(snapshot, "net.interface.tx_total", selected) : undefined;
  const ipv4 = selected ? textValue(findReading(snapshot, "net.interface.ipv4", selected)) : null;
  const ipv6 = selected ? textValue(findReading(snapshot, "net.interface.ipv6", selected)) : null;
  const rxPoints = useHistorySeries("net.interface.rx_rate", selected ?? "", range).points;
  const txPoints = useHistorySeries("net.interface.tx_rate", selected ?? "", range).points;
  const rxValue = numericValue(rxRate);
  const txValue = numericValue(txRate);

  return (
    <Card data-testid="card-network">
      <CardHeader>
        <CardTitle>{t("overview.card.network")}</CardTitle>
        {rxRate && rxRate.availability !== "available" ? (
          <MetricAvailability availability={rxRate.availability} reasonCode={rxRate.reason_code} />
        ) : null}
      </CardHeader>
      <CardContent className="grid gap-3">
        {interfaces.length === 0 ? (
          <p className="text-sm text-muted">{t("overview.noInterfaces")}</p>
        ) : (
          <>
            <div className="grid items-center gap-2 sm:grid-cols-[6rem_1fr]">
              <span className="text-xs text-muted">{t("overview.interface")}</span>
              <Select
                value={selected ?? undefined}
                onValueChange={(value) => setSelected(value)}
              >
                <SelectTrigger aria-label={t("overview.interface")} className="w-64">
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
                  {rxValue === null ? "—" : formatByUnit(rxValue, "bytes_per_second", 1)}
                </p>
                <Sparkline points={rxPoints} ariaLabel={t("overview.down")} className="h-8 w-full" />
              </div>
              <div>
                <p className="text-xs text-muted">{t("overview.up")}</p>
                <p className="numeric text-lg font-semibold">
                  {txValue === null ? "—" : formatByUnit(txValue, "bytes_per_second", 1)}
                </p>
                <Sparkline points={txPoints} ariaLabel={t("overview.up")} className="h-8 w-full" />
              </div>
            </div>
            <p className="numeric text-xs text-muted">
              {rxTotal && numericValue(rxTotal) !== null
                ? `Σ ${t("overview.down")} ${formatByUnit(numericValue(rxTotal) as number, "bytes")}`
                : ""}{" "}
              {txTotal && numericValue(txTotal) !== null
                ? `Σ ${t("overview.up")} ${formatByUnit(numericValue(txTotal) as number, "bytes")}`
                : ""}
            </p>
            {ipv4 || ipv6 ? (
              <p className="numeric text-xs text-muted">
                {t("overview.addresses")}: {[ipv4, ipv6].filter(Boolean).join(" · ")}
              </p>
            ) : null}
          </>
        )}
      </CardContent>
    </Card>
  );
}

export function HostCard() {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const text = (id: string) => textValue(findReading(snapshot, id)) ?? "—";
  const uptime = numericValue(findReading(snapshot, "host.uptime"));
  const logical = numericValue(findReading(snapshot, "host.cpu.cores.logical"));
  const physical = numericValue(findReading(snapshot, "host.cpu.cores.physical"));
  const rows: [string, string][] = [
    [t("nav.overview"), text("host.hostname")],
    ["OS", text("host.os.version")],
    ["Kernel", text("host.os.kernel")],
    ["Arch", text("host.arch")],
    ["CPU", text("host.cpu.brand")],
    [
      t("overview.coreCount"),
      `${logical ?? "—"}${physical !== null ? ` (${physical})` : ""}`,
    ],
    ["Uptime", uptime !== null ? formatUptime(uptime) : "—"],
  ];
  return (
    <Card data-testid="card-host">
      <CardHeader>
        <CardTitle>{t("overview.card.host")}</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-1 text-xs">
          {rows.map(([label, value]) => (
            <div key={label} className="flex justify-between gap-3">
              <dt className="text-muted">{label}</dt>
              <dd className="numeric truncate text-right">{value}</dd>
            </div>
          ))}
        </dl>
      </CardContent>
    </Card>
  );
}
