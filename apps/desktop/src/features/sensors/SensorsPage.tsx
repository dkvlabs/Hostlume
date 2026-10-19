/**
 * Sensors screen (FX-010, FX-012–014, FX-021; design.md §9). Tabs:
 * Temperatures, Fans, GPU, Battery, Bluetooth, Displays, Capabilities.
 * Every panel renders real readings or an explicit capability explanation —
 * unavailable data never renders as zeros, and the Capabilities tab offers
 * a manual re-probe after driver/permission/device changes.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { MetricAvailability } from "@/components/metric/MetricAvailability";
import { Sparkline } from "@/components/metric/Sparkline";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { ipc } from "@/lib/ipc/client";
import type {
  CapabilityReport,
  GpuInfoReport,
  MetricCapability,
  MetricReading,
} from "@/lib/ipc/types";
import {
  convertTemperature,
  entityLabel,
  firstReading,
  numericValue,
  readingsFor,
  textValue,
} from "@/lib/metrics";
import { formatByUnit } from "@/lib/formatting/units";
import { useHistorySeries } from "@/lib/useHistorySeries";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTelemetryStore } from "@/stores/telemetryStore";

const TABS = [
  "temperatures",
  "fans",
  "gpu",
  "battery",
  "bluetooth",
  "displays",
  "capabilities",
] as const;
type SensorTab = (typeof TABS)[number];

const TAB_LABEL_KEYS: Record<SensorTab, MessageKey> = {
  temperatures: "sensors.tab.temperatures",
  fans: "sensors.tab.fans",
  gpu: "sensors.tab.gpu",
  battery: "sensors.tab.battery",
  bluetooth: "sensors.tab.bluetooth",
  displays: "sensors.tab.displays",
  capabilities: "sensors.tab.capabilities",
};

/** Readings for one metric family keyed by entity, sorted by label. */
function useFamily(metricId: string): MetricReading[] {
  const snapshot = useTelemetryStore((s) => s.snapshot);
  return useMemo(
    () =>
      readingsFor(snapshot, metricId).sort((a, b) =>
        a.entity_id.localeCompare(b.entity_id, undefined, { numeric: true }),
      ),
    [snapshot, metricId],
  );
}

function ReadingRow({
  label,
  reading,
  formatted,
  extra,
}: {
  label: string;
  reading: MetricReading;
  formatted: string | null;
  extra?: string | null;
}) {
  return (
    <div
      className="flex items-center justify-between gap-3 border-b border-stroke py-2 text-sm last:border-b-0"
      data-entity={reading.entity_id}
    >
      <span className="min-w-0 flex-1 truncate" title={label}>
        {label}
      </span>
      {reading.availability === "available" ? (
        <span className="numeric text-right font-medium">
          {formatted ?? "—"}
          {extra ? <span className="ml-2 text-xs text-muted">{extra}</span> : null}
        </span>
      ) : (
        <MetricAvailability availability={reading.availability} reasonCode={reading.reason_code} />
      )}
    </div>
  );
}

/** The capability row for one family; drives reasoned empty states. */
function useCapability(metricId: string): MetricCapability | undefined {
  const [capability, setCapability] = useState<MetricCapability | undefined>();
  useEffect(() => {
    let alive = true;
    ipc
      .getCapabilities()
      .then((report) => {
        if (alive) {
          setCapability(report.capabilities.find((row) => row.metric_id === metricId));
        }
      })
      .catch(() => {
        // Absence of a report renders the generic notice, never a value.
      });
    return () => {
      alive = false;
    };
  }, [metricId]);
  return capability;
}

function NoDataNotice({ capability }: { capability?: MetricCapability }) {
  const t = useT();
  return (
    <div className="grid gap-2 text-sm" data-testid="sensors-no-data">
      <div className="flex items-center gap-2">
        <Badge variant="neutral">{t("states.unsupported")}</Badge>
        <span className="text-muted">{t("sensors.noData")}</span>
      </div>
      {capability?.explanation ? (
        <p className="text-muted">{capability.explanation}</p>
      ) : null}
    </div>
  );
}

function TemperaturesPanel({ unit }: { unit: "celsius" | "fahrenheit" }) {
  const t = useT();
  const readings = useFamily("sensor.temperature");
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    const first = readings[0];
    if (selected === null && first) {
      setSelected(first.entity_id);
    }
  }, [readings, selected]);
  const series = useHistorySeries(
    "sensor.temperature",
    selected ?? "sensor:none",
    900,
  );
  const unitLabel = unit === "fahrenheit" ? "°F" : "°C";

  if (readings.length === 0) {
    return <NoDataNotice />;
  }
  const selectedReading = readings.find((reading) => reading.entity_id === selected);
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle>{t("sensors.temperatures")}</CardTitle>
          <span className="text-xs text-muted">{unitLabel}</span>
        </CardHeader>
        <CardContent>
          {readings.map((reading) => {
            const value = numericValue(reading);
            return (
              <button
                key={reading.entity_id}
                type="button"
                className="w-full text-left"
                onClick={() => setSelected(reading.entity_id)}
                aria-pressed={selected === reading.entity_id}
              >
                <ReadingRow
                  label={entityLabel(reading.entity_id)}
                  reading={reading}
                  formatted={
                    value === null ? null : convertTemperature(value, unit).toFixed(1)
                  }
                />
              </button>
            );
          })}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>
            {t("sensors.history")}
            {selectedReading ? ` — ${entityLabel(selectedReading.entity_id)}` : ""}
          </CardTitle>
        </CardHeader>
        <CardContent>
          {selected ? (
            <Sparkline
              points={series.points}
              maxGapMs={60_000}
              ariaLabel={t("sensors.history")}
              className="h-24 w-full"
            />
          ) : (
            <p className="text-sm text-muted">{t("sensors.pickSensor")}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function FansPanel() {
  const t = useT();
  const readings = useFamily("sensor.fan.speed");
  const capability = useCapability("sensor.fan.speed");
  if (readings.length === 0) {
    return <NoDataNotice capability={capability} />;
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("sensors.fans")}</CardTitle>
      </CardHeader>
      <CardContent>
        {readings.map((reading) => (
          <ReadingRow
            key={reading.entity_id}
            label={entityLabel(reading.entity_id)}
            reading={reading}
            formatted={
              numericValue(reading) === null
                ? null
                : `${t("sensors.rpm")} ${numericValue(reading)}`
            }
          />
        ))}
        <p className="mt-3 text-xs text-muted">{t("sensors.fansReadonly")}</p>
      </CardContent>
    </Card>
  );
}

function GpuPanel() {
  const t = useT();
  const [info, setInfo] = useState<GpuInfoReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const utilization = useFamily("gpu.engine.utilization");
  const vram = useFamily("gpu.memory.dedicated_used");

  const refresh = useCallback(() => {
    setError(null);
    ipc
      .getGpuInfo()
      .then((report) => setInfo(report))
      .catch((err: Error) => setError(err.message));
  }, []);
  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div className="grid gap-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("sensors.gpu.adapters")}</h3>
        <Button size="sm" variant="secondary" onClick={refresh}>
          {t("sensors.refresh")}
        </Button>
      </div>
      {error ? (
        <p className="text-sm text-critical" role="alert">
          {t("status.error")}: {error}
        </p>
      ) : null}
      {(info?.adapters ?? []).length === 0 && !error ? (
        <p className="text-sm text-muted" role="status">
          {t("sensors.gpu.none")}
        </p>
      ) : null}
      {(info?.adapters ?? []).map((adapter) => (
        <Card key={adapter.id} data-testid={`gpu-adapter-${adapter.id}`}>
          <CardHeader>
            <CardTitle>{adapter.name}</CardTitle>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
              {adapter.vendor ? <Badge variant="neutral">{adapter.vendor}</Badge> : null}
              {adapter.is_unified_memory ? (
                <Badge variant="neutral">{t("sensors.gpu.unified")}</Badge>
              ) : null}
              {!adapter.is_unified_memory && adapter.vram_bytes !== undefined ? (
                <span className="numeric">
                  {t("sensors.gpu.vram")}: {formatByUnit(adapter.vram_bytes, "bytes", 1)}
                </span>
              ) : null}
            </div>
          </CardHeader>
          <CardContent className="grid gap-1 text-sm">
            {utilization.length > 0 ? (
              utilization.map((reading) => {
                const value = numericValue(reading);
                return (
                  <ReadingRow
                    key={reading.entity_id}
                    label={t("sensors.gpu.utilization")}
                    reading={reading}
                    formatted={value === null ? null : formatByUnit(value, "percent", 0)}
                  />
                );
              })
            ) : (
              <p className="text-muted">{t("sensors.gpu.utilUnsupported")}</p>
            )}
            {vram.length > 0
              ? vram.map((reading) => {
                  const value = numericValue(reading);
                  return (
                    <ReadingRow
                      key={reading.entity_id}
                      label={t("sensors.gpu.vramUsed")}
                      reading={reading}
                      formatted={value === null ? null : formatByUnit(value, "bytes", 1)}
                    />
                  );
                })
              : null}
            {adapter.displays.map((display) => (
              <div
                key={display.name}
                className="flex items-center justify-between gap-3 border-t border-stroke pt-1 text-xs text-muted"
              >
                <span>{display.name}</span>
                <span className="numeric">
                  {display.resolution ?? "—"}
                  {display.is_main ? ` · ${t("sensors.gpu.main")}` : ""}
                  {display.is_online ? "" : ` · ${t("sensors.gpu.offline")}`}
                </span>
              </div>
            ))}
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function BatteryPanel() {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  // Battery readings live on the `battery:0` entity, not `host`.
  const charge = firstReading(snapshot, "battery.charge");
  const status = firstReading(snapshot, "battery.status");
  const health = firstReading(snapshot, "battery.health");
  const cycles = firstReading(snapshot, "battery.cycles");
  const time = firstReading(snapshot, "battery.time_remaining");
  const draw = firstReading(snapshot, "battery.power_draw");
  const capability = useCapability("battery.charge");

  if (!charge && !status) {
    return <NoDataNotice capability={capability} />;
  }
  const chargeValue = numericValue(charge);
  const healthValue = numericValue(health);
  const cyclesValue = numericValue(cycles);
  const timeValue = numericValue(time);
  const drawValue = numericValue(draw);
  const statusText = textValue(status);
  return (
    <Card data-testid="battery-panel">
      <CardHeader>
        <CardTitle>{t("sensors.battery")}</CardTitle>
        {charge && charge.availability !== "available" ? (
          <MetricAvailability
            availability={charge.availability}
            reasonCode={charge.reason_code}
          />
        ) : null}
      </CardHeader>
      <CardContent>
        <ReadingRow
          label={t("sensors.battery.charge")}
          reading={charge ?? { ...status!, entity_id: "battery:0" }}
          formatted={chargeValue === null ? null : formatByUnit(chargeValue, "percent", 0)}
        />
        {status ? (
          <ReadingRow
            label={t("sensors.battery.status")}
            reading={status}
            formatted={statusText}
          />
        ) : null}
        {health ? (
          <ReadingRow
            label={t("sensors.battery.health")}
            reading={health}
            formatted={healthValue === null ? null : formatByUnit(healthValue, "percent", 1)}
            extra={healthValue === null ? null : t("sensors.battery.healthNote")}
          />
        ) : (
          <p className="py-1 text-xs text-muted">{t("sensors.battery.healthUnsupported")}</p>
        )}
        {cycles ? (
          <ReadingRow
            label={t("sensors.battery.cycles")}
            reading={cycles}
            formatted={cyclesValue === null ? null : String(Math.round(cyclesValue))}
          />
        ) : (
          <p className="py-1 text-xs text-muted">{t("sensors.battery.cyclesUnsupported")}</p>
        )}
        {time ? (
          <ReadingRow
            label={t("sensors.battery.time")}
            reading={time}
            formatted={timeValue === null ? null : formatByUnit(timeValue, "seconds", 0)}
          />
        ) : null}
        {draw ? (
          <ReadingRow
            label={t("sensors.battery.draw")}
            reading={draw}
            formatted={drawValue === null ? null : formatByUnit(Math.abs(drawValue), "watts", 1)}
            extra={
              drawValue === null
                ? null
                : drawValue < 0
                  ? t("sensors.battery.discharging")
                  : t("sensors.battery.chargingWord")
            }
          />
        ) : null}
      </CardContent>
    </Card>
  );
}

function BluetoothPanel() {
  const t = useT();
  const names = useFamily("bluetooth.device.name");
  const charges = useFamily("bluetooth.device.charge");
  const capability = useCapability("bluetooth.device.charge");
  if (names.length === 0) {
    return <NoDataNotice capability={capability} />;
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("sensors.bluetooth")}</CardTitle>
      </CardHeader>
      <CardContent>
        {names.map((device) => {
          const perDevice = charges.filter((charge) =>
            charge.entity_id.startsWith(`${device.entity_id}:`),
          );
          return (
            <div key={device.entity_id} className="border-b border-stroke py-2 last:border-b-0">
              <div className="flex items-center justify-between gap-3 text-sm">
                <span className="truncate">{textValue(device) ?? device.entity_id}</span>
                {perDevice.length === 0 ? (
                  <span className="text-xs text-muted">{t("sensors.bluetooth.noCharge")}</span>
                ) : null}
              </div>
              {perDevice.map((charge) => (
                <ReadingRow
                  key={charge.entity_id}
                  label={entityLabel(charge.entity_id).split(":").pop() ?? ""}
                  reading={charge}
                  formatted={
                    numericValue(charge) === null
                      ? null
                      : formatByUnit(numericValue(charge)!, "percent", 0)
                  }
                />
              ))}
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

function DisplaysPanel() {
  const t = useT();
  const [info, setInfo] = useState<GpuInfoReport | null>(null);
  useEffect(() => {
    ipc
      .getGpuInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, []);
  const displays = (info?.adapters ?? []).flatMap((adapter) =>
    adapter.displays.map((display) => ({ adapter: adapter.name, display })),
  );
  if (displays.length === 0) {
    return <NoDataNotice />;
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("sensors.displays")}</CardTitle>
      </CardHeader>
      <CardContent>
        {displays.map(({ adapter, display }) => (
          <div
            key={`${adapter}/${display.name}`}
            className="flex items-center justify-between gap-3 border-b border-stroke py-2 text-sm last:border-b-0"
          >
            <span>
              {display.name}
              <span className="ml-2 text-xs text-muted">{adapter}</span>
            </span>
            <span className="numeric text-xs">
              {display.resolution ?? "—"}
              {display.is_main ? ` · ${t("sensors.gpu.main")}` : ""}
              {display.is_online ? "" : ` · ${t("sensors.gpu.offline")}`}
            </span>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

function CapabilitiesPanel() {
  const t = useT();
  const [report, setReport] = useState<CapabilityReport | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(async (reprobe: boolean) => {
    setBusy(true);
    try {
      setReport(reprobe ? await ipc.reprobeCapabilities() : await ipc.getCapabilities());
    } catch {
      // The tab keeps the previous report; absence renders as a notice.
      setReport(null);
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void load(false);
  }, [load]);

  const rows = [...(report?.capabilities ?? [])].sort((a, b) =>
    a.metric_id.localeCompare(b.metric_id),
  );
  return (
    <Card data-testid="capabilities-panel">
      <CardHeader className="flex-row items-center justify-between gap-2">
        <CardTitle>{t("sensors.capabilities")}</CardTitle>
        <Button size="sm" variant="secondary" disabled={busy} onClick={() => void load(true)}>
          {t("sensors.reprobe")}
        </Button>
      </CardHeader>
      <CardContent>
        {report ? (
          <p className="numeric pb-2 text-xs text-muted">
            {t("sensors.capabilitiesProbedAt")}: {report.generated_at_utc}
          </p>
        ) : (
          <p className="pb-2 text-sm text-muted" role="status">
            {t("status.connecting")}
          </p>
        )}
        <div className="grid gap-1">
          {rows.map((capability) => (
            <div
              key={capability.metric_id}
              className="grid gap-1 border-b border-stroke py-2 text-sm last:border-b-0"
              data-metric={capability.metric_id}
            >
              <div className="flex items-center justify-between gap-3">
                <span className="numeric text-xs">{capability.metric_id}</span>
                <MetricAvailability
                  availability={capability.state}
                  reasonCode={capability.reason_code}
                />
              </div>
              {capability.explanation ? (
                <p className="text-xs text-muted">{capability.explanation}</p>
              ) : null}
              <p className="text-xs text-muted">{t("sensors.source")}: {capability.source}</p>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

export function SensorsPage() {
  const t = useT();
  const [tab, setTab] = useState<SensorTab>("temperatures");
  const temperatureUnit =
    useSettingsStore((s) => s.settings?.temperature_unit) ?? "celsius";

  return (
    <div className="grid gap-4 p-5" data-testid="sensors-page">
      <h1 className="text-lg font-semibold">{t("nav.sensors")}</h1>
      <div role="tablist" aria-label={t("nav.sensors")} className="flex flex-wrap gap-1">
        {TABS.map((entry) => (
          <button
            key={entry}
            type="button"
            role="tab"
            aria-selected={tab === entry}
            onClick={() => setTab(entry)}
            className={
              tab === entry
                ? "rounded-[8px] bg-surface-raised px-3 py-1.5 text-sm font-medium"
                : "rounded-[8px] px-3 py-1.5 text-sm text-muted hover:bg-surface-raised hover:text-text"
            }
          >
            {t(TAB_LABEL_KEYS[entry])}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {tab === "temperatures" ? <TemperaturesPanel unit={temperatureUnit} /> : null}
        {tab === "fans" ? <FansPanel /> : null}
        {tab === "gpu" ? <GpuPanel /> : null}
        {tab === "battery" ? <BatteryPanel /> : null}
        {tab === "bluetooth" ? <BluetoothPanel /> : null}
        {tab === "displays" ? <DisplaysPanel /> : null}
        {tab === "capabilities" ? <CapabilitiesPanel /> : null}
      </div>
    </div>
  );
}
