/**
 * Compare screen (FX-018, design.md §10): up to four aligned time series
 * on shared UTC axes — one labeled axis per distinct unit — with gap
 * awareness, an accessible data-table equivalent, per-series CSV export
 * and recording sessions (start/stop, interrupted recovery, session
 * export). All data comes from the bounded history queries; nothing here
 * simulates values.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as echarts from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useT } from "@/lib/i18n";
import { METRIC_CATALOG, metricEntry } from "@/lib/metricCatalog";
import { ipc } from "@/lib/ipc/client";
import type {
  CsvExportResult,
  HistoryResponse,
  RecordingSession,
} from "@/lib/ipc/types";
import { useTelemetryStore } from "@/stores/telemetryStore";

echarts.use([
  LineChart,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  DataZoomComponent,
  CanvasRenderer,
]);

/** Upper bound of aligned series (design.md §10). */
const MAX_SERIES = 4;

const RANGES = [
  { id: "15m", ms: 15 * 60_000 },
  { id: "1h", ms: 3_600_000 },
  { id: "6h", ms: 6 * 3_600_000 },
  { id: "24h", ms: 24 * 3_600_000 },
  { id: "7d", ms: 7 * 24 * 3_600_000 },
] as const;
type RangeId = (typeof RANGES)[number]["id"];

interface SeriesSpec {
  metric_id: string;
  entity_id: string;
}

const PALETTE = ["#67D4D0", "#B39DDB", "#F4BC6C", "#FB7C80"];

function defaultEntityFor(metricId: string, entities: string[]): string {
  const hint = metricEntry(metricId)?.entityHint ?? "";
  const match = entities.find((entity) => entity.startsWith(hint));
  return match ?? entities[0] ?? "host";
}

function formatBytes(value: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let scaled = value;
  let index = 0;
  while (scaled >= 1_024 && index < units.length - 1) {
    scaled /= 1_024;
    index += 1;
  }
  return `${index === 0 ? scaled : scaled.toFixed(1)} ${units[index]}`;
}

function formatValue(value: number, unit: string): string {
  switch (unit) {
    case "percent":
      return `${value.toFixed(1)} %`;
    case "bytes":
      return formatBytes(value);
    case "bytes_per_second":
      return `${formatBytes(value)}/s`;
    default:
      return `${Number(value.toFixed(2))} ${unit}`;
  }
}

export function ComparePage() {
  const t = useT();
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const [rangeId, setRangeId] = useState<RangeId>("1h");
  const [series, setSeries] = useState<SeriesSpec[]>([
    { metric_id: "cpu.total.usage", entity_id: "host" },
  ]);
  const [data, setData] = useState<Map<string, HistoryResponse>>(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showTable, setShowTable] = useState(false);

  // Recording state (FX-018 §2.3).
  const [sessions, setSessions] = useState<RecordingSession[]>([]);
  const [recordingLabel, setRecordingLabel] = useState("");
  const [exports, setExports] = useState<CsvExportResult[]>([]);

  const chartRef = useRef<HTMLDivElement>(null);
  const chartInstance = useRef<echarts.ECharts | null>(null);

  const range = RANGES.find((r) => r.id === rangeId) ?? RANGES[1];
  const entities = useMemo(() => {
    const set = new Set<string>();
    for (const reading of snapshot?.metrics ?? []) {
      set.add(reading.entity_id);
    }
    return [...set].sort();
  }, [snapshot]);

  const refreshSessions = useCallback(async () => {
    try {
      setSessions(await ipc.listRecordings(20));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const loadSeries = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const now = Date.now();
      const results = await Promise.all(
        series.map((spec) =>
          ipc.getHistory({
            metric_id: spec.metric_id,
            entity_id: spec.entity_id,
            from_utc_ms: now - range.ms,
            to_utc_ms: now,
            max_points: 400,
          })
        )
      );
      const next = new Map<string, HistoryResponse>();
      series.forEach((spec, index) => {
        next.set(
          `${spec.metric_id}@${spec.entity_id}`,
          results[index] ?? (results[0] as HistoryResponse),
        );
      });
      setData(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [series, range.ms]);

  useEffect(() => {
    void loadSeries();
  }, [loadSeries]);

  useEffect(() => {
    void refreshSessions();
  }, [refreshSessions]);

  // Chart rendering: shared UTC time axis, one labeled y-axis per unit.
  useEffect(() => {
    if (!chartRef.current) return;
    if (!chartInstance.current) {
      chartInstance.current = echarts.init(chartRef.current);
    }
    const chart = chartInstance.current;

    const entries = [...data.entries()];
    if (entries.length === 0) {
      chart.clear();
      return;
    }

    const unitsInOrder: string[] = [];
    for (const [key] of entries) {
      const unit = metricEntry(key.split("@")[0] ?? "")?.unit ?? "count";
      if (!unitsInOrder.includes(unit)) unitsInOrder.push(unit);
    }
    const axisIndex = (unit: string) => unitsInOrder.indexOf(unit);

    const chartSeries = entries.map(([key, response], index) => {
      const metricId = key.split("@")[0] ?? "";
      const unit = metricEntry(metricId)?.unit ?? "count";
      const label = metricEntry(metricId)?.label ?? metricId;
      return {
        name: `${label} · ${response.entity_id}`,
        type: "line" as const,
        yAxisIndex: axisIndex(unit),
        showSymbol: false,
        connectNulls: false,
        data: response.points.map((point) => [point.ts_utc_ms, point.value]),
        itemStyle: { color: PALETTE[index % PALETTE.length] },
        lineStyle: { color: PALETTE[index % PALETTE.length], width: 1.5 },
      };
    });

    chart.setOption(
      {
        backgroundColor: "transparent",
        animation: false,
        grid: { left: 64, right: unitsInOrder.length > 1 ? 64 : 32, top: 40, bottom: 48 },
        legend: { top: 0, textStyle: { color: "#A6B0BD", fontSize: 11 } },
        tooltip: {
          trigger: "axis",
          formatter: (params: unknown) => {
            const list = params as {
              seriesName: string;
              value: [number, number];
              axisValue: number;
            }[];
            if (!Array.isArray(list) || list.length === 0) return "";
            const time = new Date(list[0]?.axisValue ?? 0).toISOString().slice(11, 19);
            const lines = list.map((param) => `${param.seriesName}: ${param.value[1]}`);
            return `${time} UTC<br/>${lines.join("<br/>")}`;
          },
        },
        xAxis: {
          type: "time",
          axisLabel: {
            color: "#A6B0BD",
            formatter: (value: number) => new Date(value).toISOString().slice(11, 16),
          },
          splitLine: { show: false },
        },
        yAxis: unitsInOrder.map((unit) => ({
          type: "value",
          name: unit,
          nameTextStyle: { color: "#A6B0BD" },
          axisLabel: { color: "#A6B0BD", formatter: (value: number) => formatValue(value, unit) },
          splitLine: { lineStyle: { color: "#313B47" } },
        })),
        series: chartSeries,
      },
      { notMerge: true }
    );

    const onResize = () => chart.resize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [data, t]);

  const addSeries = (metricId: string) => {
    if (series.length >= MAX_SERIES) return;
    const entity = defaultEntityFor(metricId, entities);
    if (series.some((s) => s.metric_id === metricId && s.entity_id === entity)) return;
    setSeries([...series, { metric_id: metricId, entity_id: entity }]);
  };

  const removeSeries = (index: number) => {
    setSeries(series.filter((_, i) => i !== index));
  };

  const exportOne = async (spec: SeriesSpec) => {
    try {
      const result = await ipc.exportCsv({
        metric_id: spec.metric_id,
        entity_id: spec.entity_id,
        from_utc_ms: Date.now() - range.ms,
        to_utc_ms: Date.now(),
      });
      setExports((prev) => [result, ...prev].slice(0, 5));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const startRecording = async () => {
    try {
      await ipc.startRecording({
        label: recordingLabel.trim() || "recording",
        metric_ids: series.map((s) => s.metric_id),
      });
      setRecordingLabel("");
      await refreshSessions();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const stopRecording = async (sessionId: string) => {
    try {
      await ipc.stopRecording(sessionId);
      await refreshSessions();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const exportSession = async (sessionId: string) => {
    try {
      const result = await ipc.exportRecordingCsv(sessionId);
      setExports((prev) => [result, ...prev].slice(0, 5));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const openExport = (path: string) => {
    // Reveal the containing folder via the existing safe opener (named
    // binary + argument array; the folder, not the file).
    void import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke("open_in_manager", {
        path: path.split("/").slice(0, -1).join("/"),
      }).catch(() => undefined)
    );
  };

  return (
    <div className="grid gap-4 p-5" data-page="compare">
      {error ? (
        <p role="alert" className="text-xs text-critical">
          {error}
        </p>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span>{t("compare.title")}</span>
            <div className="flex items-center gap-2">
              <Select value={rangeId} onValueChange={(value) => setRangeId(value as RangeId)}>
                <SelectTrigger className="w-28" aria-label={t("compare.range")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {RANGES.map((r) => (
                    <SelectItem key={r.id} value={r.id}>
                      {t(("compare.range." + r.id) as never)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Switch
                id="compare-table"
                checked={showTable}
                aria-label={t("compare.showTable")}
                onCheckedChange={setShowTable}
              />
              <Label htmlFor="compare-table" className="text-xs text-muted">
                {t("compare.showTable")}
              </Label>
            </div>
          </CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3">
          <div className="flex flex-wrap items-center gap-2">
            {series.map((spec, index) => {
              const entry = metricEntry(spec.metric_id);
              return (
                <Badge key={`${spec.metric_id}@${spec.entity_id}`} variant="neutral" className="gap-2">
                  <span
                    aria-hidden
                    className="inline-block h-2 w-2 rounded-full"
                    style={{ backgroundColor: PALETTE[index % PALETTE.length] }}
                  />
                  {entry?.label ?? spec.metric_id} · {spec.entity_id}
                  <button
                    type="button"
                    aria-label={t("compare.removeSeries")}
                    className="text-muted hover:text-critical"
                    onClick={() => removeSeries(index)}
                  >
                    ×
                  </button>
                </Badge>
              );
            })}
            {series.length < MAX_SERIES ? (
              <Select
                value=""
                onValueChange={(value) => {
                  if (value) addSeries(value);
                }}
              >
                <SelectTrigger className="w-52" aria-label={t("compare.addSeries")}>
                  <SelectValue placeholder={t("compare.addSeries")} />
                </SelectTrigger>
                <SelectContent>
                  {METRIC_CATALOG.filter(
                    (entry) => !series.some((s) => s.metric_id === entry.id)
                  ).map((entry) => (
                    <SelectItem key={entry.id} value={entry.id}>
                      {entry.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : null}
            <span className="text-xs text-muted">
              {series.length}/{MAX_SERIES}
            </span>
            <Button size="sm" variant="secondary" onClick={() => void loadSeries()} disabled={loading}>
              {loading ? t("compare.loading") : t("common.retry")}
            </Button>
          </div>

          <div ref={chartRef} className="h-80 w-full" role="img" aria-label={t("compare.chartAria")} />

          {showTable ? (
            <table className="w-full text-sm" aria-label={t("compare.tableAria")}>
              <thead>
                <tr className="text-left text-muted">
                  <th className="p-1.5">{t("compare.colTime")}</th>
                  <th className="p-1.5">{t("compare.colSeries")}</th>
                  <th className="p-1.5 text-right">{t("compare.colValue")}</th>
                  <th className="p-1.5">{t("compare.colUnit")}</th>
                </tr>
              </thead>
              <tbody>
                {[...data.entries()].flatMap(([key, response]) => {
                  const metricId = key.split("@")[0] ?? "";
                  const unit = metricEntry(metricId)?.unit ?? "";
                  return response.points.slice(-20).map((point) => (
                    <tr key={`${key}-${point.ts_utc_ms}`} className="border-t border-stroke">
                      <td className="p-1.5 numeric">
                        {new Date(point.ts_utc_ms).toISOString()}
                      </td>
                      <td className="p-1.5">
                        {metricEntry(metricId)?.label ?? metricId} · {response.entity_id}
                      </td>
                      <td className="p-1.5 text-right numeric">{point.value}</td>
                      <td className="p-1.5">{unit}</td>
                    </tr>
                  ));
                })}
              </tbody>
            </table>
          ) : null}

          <div className="flex flex-wrap gap-2">
            {series.map((spec) => (
              <Button
                key={`export-${spec.metric_id}@${spec.entity_id}`}
                size="sm"
                variant="ghost"
                onClick={() => void exportOne(spec)}
              >
                {t("compare.export")} {metricEntry(spec.metric_id)?.label ?? spec.metric_id}
              </Button>
            ))}
          </div>
          {exports.length > 0 ? (
            <div className="grid gap-1 text-xs text-muted">
              {exports.map((result) => (
                <div key={result.path} className="flex items-center gap-2">
                  <Badge variant={result.truncated ? "warning" : "neutral"}>
                    {result.rows} rows
                  </Badge>
                  <span className="truncate">{result.path}</span>
                  <Button size="sm" variant="ghost" onClick={() => openExport(result.path)}>
                    {t("compare.reveal")}
                  </Button>
                </div>
              ))}
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("recording.title")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3">
          <p className="text-xs text-muted">{t("recording.hint")}</p>
          <div className="flex flex-wrap items-center gap-2">
            <input
              type="text"
              value={recordingLabel}
              onChange={(event) => setRecordingLabel(event.target.value)}
              placeholder={t("recording.labelPlaceholder")}
              aria-label={t("recording.labelPlaceholder")}
              className="h-8 w-64 rounded-[8px] border border-stroke bg-surface px-3 text-sm focus-visible:outline-2 focus-visible:outline-accent"
            />
            <Button size="sm" onClick={() => void startRecording()}>
              {t("recording.start")}
            </Button>
            <span className="text-xs text-muted">
              {t("recording.metricsNotePrefix")} ({series.length})
            </span>
          </div>

          {sessions.length === 0 ? (
            <p className="text-sm text-muted">{t("recording.empty")}</p>
          ) : (
            <div className="grid gap-2">
              {sessions.map((session) => (
                <div
                  key={session.id}
                  className="flex flex-wrap items-center gap-2 border-b border-stroke pb-2 text-sm last:border-b-0"
                >
                  <Badge
                    variant={
                      session.status === "recording"
                        ? "accent"
                        : session.status === "interrupted"
                          ? "warning"
                          : "neutral"
                    }
                  >
                    {t(("recording.status." + session.status) as never)}
                  </Badge>
                  <span className="flex-1">{session.label}</span>
                  <span className="numeric text-xs text-muted">
                    {new Date(session.started_at_utc_ms).toLocaleString()}
                    {session.ended_at_utc_ms
                      ? ` → ${new Date(session.ended_at_utc_ms).toLocaleString()}`
                      : ""}
                  </span>
                  {session.status === "recording" ? (
                    <Button size="sm" variant="secondary" onClick={() => void stopRecording(session.id)}>
                      {t("recording.stop")}
                    </Button>
                  ) : null}
                  <Button size="sm" variant="ghost" onClick={() => void exportSession(session.id)}>
                    {t("recording.export")}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
