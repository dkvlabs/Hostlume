/**
 * Hosts page (phase 10, FX-022): the paired computers this console reads.
 *
 * Honesty rules enforced here, not just in Rust:
 * * every card carries the host's identity and *two* clocks — ours (last
 *   response, age) and the agent's (collected-at, skew);
 * * `stale`/`offline` hosts keep showing their last values, but under an
 *   explicit warning and never with the "Live" badge;
 * * there is no remote action of any kind on this page (no quit, no trash,
 *   no fan control) — only pair / rename / record / remove.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Sparkline } from "@/components/metric/Sparkline";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ConfirmActionDialog } from "@/components/ui/ConfirmActionDialog";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { formatByUnit } from "@/lib/formatting/units";
import { IpcCommandError, onHostsUpdated } from "@/lib/ipc/client";
import type { HostLiveState, HostStatus, MetricUnit } from "@/lib/ipc/types";
import type { SamplePoint } from "@/lib/metrics";
import { useHostsStore } from "@/stores/hostsStore";

const KNOWN_UNITS: readonly MetricUnit[] = [
  "percent",
  "bytes",
  "bytes_per_second",
  "bits_per_second",
  "celsius",
  "rpm",
  "seconds",
  "count",
  "watts",
  "hertz",
  "boolean",
  "text",
];

function asUnit(unit: string): MetricUnit | null {
  return (KNOWN_UNITS as readonly string[]).includes(unit) ? (unit as MetricUnit) : null;
}

export function formatValue(value: number, unit: string): string {
  const known = asUnit(unit);
  return known ? formatByUnit(value, known) : `${value}`;
}

/** "3 s", "2 min", "1 h" — coarse on purpose; precision comes from the clocks shown. */
export function formatAge(ms: number): string {
  if (ms < 60_000) return `${Math.max(0, Math.round(ms / 1000))} s`;
  if (ms < 3_600_000) return `${Math.round(ms / 60_000)} min`;
  return `${Math.round(ms / 3_600_000)} h`;
}

export function formatSkew(ms: number): string {
  const sign = ms >= 0 ? "+" : "−";
  const abs = Math.abs(ms);
  if (abs < 1000) return `${sign}${abs} ms`;
  return `${sign}${(abs / 1000).toFixed(1)} s`;
}

export function statusVariant(
  status: HostStatus
): "neutral" | "accent" | "warning" | "critical" {
  switch (status) {
    case "online":
      return "accent";
    case "stale":
      return "warning";
    case "never_connected":
      return "neutral";
    default:
      return "critical";
  }
}

function statusKey(status: HostStatus): MessageKey {
  return `hosts.status.${status}` as MessageKey;
}

function headlinePoints(host: HostLiveState, metricId: string): SamplePoint[] {
  const entry = host.headline.find(([id]) => id === metricId);
  return entry ? entry[1].map((s) => ({ t: s.ts_utc_ms, v: s.value })) : [];
}

function latestReading(host: HostLiveState, metricId: string) {
  return host.readings.find((r) => r.metric_id === metricId);
}

function errorKey(err: unknown): MessageKey | null {
  if (err instanceof IpcCommandError) return err.messageKey as MessageKey;
  return null;
}

const INPUT_CLASS =
  "h-8 w-full rounded-[8px] border border-stroke bg-surface px-3 text-sm text-text";

// ---- pairing form -----------------------------------------------------------

function PairForm({ onClose }: { onClose: () => void }) {
  const t = useT();
  const pair = useHostsStore((s) => s.pair);
  const [endpoint, setEndpoint] = useState("");
  const [code, setCode] = useState("");
  const [label, setLabel] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<{ key: MessageKey | null; detail: string } | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const submit = async () => {
    setWorking(true);
    setError(null);
    try {
      const host = await pair(endpoint.trim(), code.trim(), label.trim());
      setDone(host.fingerprint_sha256);
    } catch (err) {
      setError({
        key: errorKey(err),
        detail: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setWorking(false);
    }
  };

  return (
    <Card data-testid="pair-form">
      <CardHeader>
        <CardTitle>{t("hosts.pair.title")}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {done ? (
          <>
            <p className="text-sm">{t("hosts.pair.done")}</p>
            <p className="break-all font-mono text-xs text-muted">{done}</p>
            <div>
              <Button size="sm" onClick={onClose}>
                {t("hosts.detail.back")}
              </Button>
            </div>
          </>
        ) : (
          <>
            <div className="flex flex-col gap-1">
              <Label htmlFor="pair-endpoint">{t("hosts.pair.endpoint")}</Label>
              <input
                id="pair-endpoint"
                className={INPUT_CLASS}
                value={endpoint}
                onChange={(e) => setEndpoint(e.target.value)}
                placeholder="192.168.1.20:45454"
                autoComplete="off"
                spellCheck={false}
              />
              <p className="text-xs text-muted">{t("hosts.pair.endpointHint")}</p>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="pair-code">{t("hosts.pair.code")}</Label>
              <input
                id="pair-code"
                className={`${INPUT_CLASS} font-mono uppercase tracking-widest`}
                value={code}
                onChange={(e) => setCode(e.target.value)}
                maxLength={12}
                autoComplete="off"
                spellCheck={false}
              />
              <p className="text-xs text-muted">{t("hosts.pair.codeHint")}</p>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="pair-label">{t("hosts.pair.label")}</Label>
              <input
                id="pair-label"
                className={INPUT_CLASS}
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                maxLength={48}
              />
            </div>
            {error ? (
              <p role="alert" className="text-sm text-critical">
                {error.key ? t(error.key) : error.detail}
                {error.key && error.detail ? (
                  <span className="block text-xs text-muted">{error.detail}</span>
                ) : null}
              </p>
            ) : null}
            <div className="flex gap-2">
              <Button
                size="sm"
                disabled={working || !endpoint.trim() || !code.trim() || !label.trim()}
                onClick={() => void submit()}
              >
                {working ? t("hosts.pair.working") : t("hosts.pair.submit")}
              </Button>
              <Button size="sm" variant="secondary" disabled={working} onClick={onClose}>
                {t("hosts.detail.back")}
              </Button>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}

// ---- host card ---------------------------------------------------------------

function HostCard({
  host,
  onOpen,
  onRemove,
}: {
  host: HostLiveState;
  onOpen: () => void;
  onRemove: () => void;
}) {
  const t = useT();
  const setRecording = useHostsStore((s) => s.setRecording);
  const cpu = latestReading(host, "cpu.total.usage");
  const memory = latestReading(host, "memory.physical.used");
  const live = host.status === "online";
  return (
    <Card data-testid={`host-card-${host.host_id}`} data-status={host.status}>
      <CardHeader className="flex flex-row items-start justify-between gap-2">
        <div className="min-w-0">
          <CardTitle className="truncate">{host.label}</CardTitle>
          <p className="truncate text-xs text-muted">{host.endpoint}</p>
        </div>
        <Badge variant={statusVariant(host.status)} data-testid="host-status">
          {t(statusKey(host.status))}
        </Badge>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs">
          <dt className="text-muted">{t("hosts.lastResponse")}</dt>
          <dd data-testid="host-age">
            {host.age_ms === null
              ? t("hosts.never")
              : `${formatAge(host.age_ms)} ${t("hosts.ago")}`}
          </dd>
          <dt className="text-muted">{t("hosts.remoteClock")}</dt>
          <dd className="truncate font-mono">{host.remote_collected_at_utc ?? "—"}</dd>
          <dt className="text-muted">{t("hosts.skew")}</dt>
          <dd>{host.clock_skew_ms === null ? "—" : formatSkew(host.clock_skew_ms)}</dd>
        </dl>
        {host.no_sample_yet ? (
          <p className="text-xs text-muted">{t("hosts.noSampleYet")}</p>
        ) : null}
        {host.last_error && !live ? (
          <p className="text-xs text-critical" data-testid="host-error">
            {host.last_error}
          </p>
        ) : null}
        <div className="grid grid-cols-2 gap-3">
          <div>
            <p className="text-xs text-muted">{t("hosts.cpu")}</p>
            <p className="text-lg tabular-nums" data-testid="host-cpu">
              {cpu ? formatValue(cpu.value, cpu.unit) : "—"}
            </p>
            <Sparkline
              points={headlinePoints(host, "cpu.total.usage")}
              ariaLabel={`${host.label} ${t("hosts.cpu")}`}
              height={28}
            />
          </div>
          <div>
            <p className="text-xs text-muted">{t("hosts.memory")}</p>
            <p className="text-lg tabular-nums">
              {memory ? formatValue(memory.value, memory.unit) : "—"}
            </p>
            <Sparkline
              points={headlinePoints(host, "memory.physical.used")}
              ariaLabel={`${host.label} ${t("hosts.memory")}`}
              height={28}
            />
          </div>
        </div>
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Switch
              id={`record-${host.host_id}`}
              checked={host.record_history}
              onCheckedChange={(checked) => void setRecording(host.host_id, checked)}
            />
            <Label htmlFor={`record-${host.host_id}`} className="text-xs">
              {t("hosts.record")}
            </Label>
          </div>
          <div className="flex gap-2">
            <Button size="sm" variant="secondary" onClick={onOpen}>
              {t("hosts.detail.readings")}
            </Button>
            <Button size="sm" variant="ghost" onClick={onRemove}>
              {t("hosts.remove")}
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// ---- detail ------------------------------------------------------------------

function HostDetail({ host, onBack }: { host: HostLiveState; onBack: () => void }) {
  const t = useT();
  const rename = useHostsStore((s) => s.rename);
  const [label, setLabel] = useState(host.label);
  useEffect(() => setLabel(host.label), [host.label]);
  const grouped = useMemo(() => {
    const map = new Map<string, HostLiveState["readings"]>();
    for (const reading of host.readings) {
      const family = reading.metric_id.split(".")[0] ?? reading.metric_id;
      const list = map.get(family) ?? [];
      list.push(reading);
      map.set(family, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [host.readings]);

  return (
    <div className="flex flex-col gap-4" data-testid="host-detail">
      <div className="flex items-center gap-3">
        <Button size="sm" variant="secondary" onClick={onBack}>
          ← {t("hosts.detail.back")}
        </Button>
        <h2 className="text-lg font-semibold">{host.label}</h2>
        <Badge variant={statusVariant(host.status)} data-testid="host-status">
          {t(statusKey(host.status))}
        </Badge>
        <span className="text-xs text-muted">{t("hosts.detail.source")}</span>
      </div>
      {host.status === "stale" ? (
        <p role="status" className="rounded-[8px] border border-warning/40 bg-warning/10 p-2 text-sm">
          {t("hosts.detail.staleWarning")}
        </p>
      ) : null}
      {host.status !== "online" && host.status !== "stale" && host.readings.length > 0 ? (
        <p role="status" className="rounded-[8px] border border-critical/40 bg-critical/10 p-2 text-sm">
          {t("hosts.detail.offlineWarning")}
        </p>
      ) : null}
      <Card>
        <CardContent className="grid grid-cols-1 gap-x-6 gap-y-1 pt-4 text-xs md:grid-cols-2">
          <Row label={t("hosts.endpoint")} value={host.endpoint} mono />
          <Row
            label={t("hosts.lastResponse")}
            value={
              host.age_ms === null
                ? t("hosts.never")
                : `${formatAge(host.age_ms)} ${t("hosts.ago")}`
            }
          />
          <Row label={t("hosts.remoteClock")} value={host.remote_collected_at_utc ?? "—"} mono />
          <Row
            label={t("hosts.skew")}
            value={host.clock_skew_ms === null ? "—" : formatSkew(host.clock_skew_ms)}
            hint={t("hosts.skewHint")}
          />
          <Row label={t("hosts.protocol")} value={`v${host.protocol_version}`} />
          <Row label={t("hosts.agentVersion")} value={host.agent_version ?? "—"} />
          <Row label={t("hosts.families")} value={host.families.join(", ") || "—"} />
          <Row label={t("hosts.fingerprint")} value={host.fingerprint_sha256} mono />
        </CardContent>
      </Card>
      <div className="flex items-end gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="host-rename">{t("hosts.rename")}</Label>
          <input
            id="host-rename"
            className={INPUT_CLASS}
            value={label}
            maxLength={48}
            onChange={(e) => setLabel(e.target.value)}
          />
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={label.trim() === host.label || !label.trim()}
          onClick={() => void rename(host.host_id, label.trim())}
        >
          {t("hosts.rename")}
        </Button>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>{t("hosts.detail.readings")}</CardTitle>
        </CardHeader>
        <CardContent>
          {grouped.length === 0 ? (
            <p className="text-sm text-muted">
              {host.no_sample_yet ? t("hosts.noSampleYet") : "—"}
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead className="text-muted">
                <tr>
                  <th className="py-1 text-left font-normal">metric</th>
                  <th className="py-1 text-left font-normal">entity</th>
                  <th className="py-1 text-right font-normal">value</th>
                  <th className="py-1 text-right font-normal">{t("hosts.remoteClock")}</th>
                </tr>
              </thead>
              <tbody>
                {grouped.map(([family, readings]) => (
                  <ReadingGroup key={family} family={family} readings={readings} />
                ))}
              </tbody>
            </table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function ReadingGroup({
  family,
  readings,
}: {
  family: string;
  readings: HostLiveState["readings"];
}) {
  return (
    <>
      <tr>
        <th colSpan={4} className="pt-2 text-left text-muted">
          {family}
        </th>
      </tr>
      {readings.map((reading) => (
        <tr key={`${reading.metric_id}:${reading.entity_id}`}>
          <td className="py-0.5 font-mono">{reading.metric_id}</td>
          <td className="py-0.5 font-mono text-muted">{reading.entity_id}</td>
          <td className="py-0.5 text-right tabular-nums">
            {formatValue(reading.value, reading.unit)}
          </td>
          <td className="py-0.5 text-right font-mono text-muted">
            {reading.collected_at_utc}
          </td>
        </tr>
      ))}
    </>
  );
}

function Row({
  label,
  value,
  mono,
  hint,
}: {
  label: string;
  value: string;
  mono?: boolean;
  hint?: string;
}) {
  return (
    <div className="flex flex-col py-1">
      <span className="text-muted">{label}</span>
      <span className={mono ? "break-all font-mono" : ""}>{value}</span>
      {hint ? <span className="text-[11px] text-muted">{hint}</span> : null}
    </div>
  );
}

// ---- page --------------------------------------------------------------------

export function HostsPage() {
  const t = useT();
  const hosts = useHostsStore((s) => s.hosts);
  const status = useHostsStore((s) => s.status);
  const error = useHostsStore((s) => s.error);
  const refresh = useHostsStore((s) => s.refresh);
  const remove = useHostsStore((s) => s.remove);
  const [pairing, setPairing] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [purge, setPurge] = useState(false);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5_000);
    const unlistenPromise = onHostsUpdated(() => void refresh());
    unlistenPromise.catch(() => undefined);
    return () => {
      window.clearInterval(timer);
      void unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [refresh]);

  const selectedHost = hosts.find((h) => h.host_id === selected) ?? null;
  const removingHost = hosts.find((h) => h.host_id === removing) ?? null;

  const confirmRemove = useCallback(() => {
    if (!removing) return;
    const id = removing;
    setRemoving(null);
    setSelected((current) => (current === id ? null : current));
    void remove(id, purge);
    setPurge(false);
  }, [remove, removing, purge]);

  if (selectedHost) {
    return (
      <div className="p-4">
        <HostDetail host={selectedHost} onBack={() => setSelected(null)} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">{t("hosts.title")}</h1>
          <p className="max-w-3xl text-sm text-muted">{t("hosts.hint")}</p>
        </div>
        {!pairing ? (
          <Button size="sm" onClick={() => setPairing(true)}>
            {t("hosts.pair")}
          </Button>
        ) : null}
      </div>
      {pairing ? <PairForm onClose={() => setPairing(false)} /> : null}
      {status === "error" && error ? (
        <p role="alert" className="text-sm text-critical">
          {error}
        </p>
      ) : null}
      {hosts.length === 0 && status === "ready" && !pairing ? (
        <p className="text-sm text-muted" data-testid="hosts-empty">
          {t("hosts.empty")}
        </p>
      ) : null}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
        {hosts.map((host) => (
          <HostCard
            key={host.host_id}
            host={host}
            onOpen={() => setSelected(host.host_id)}
            onRemove={() => setRemoving(host.host_id)}
          />
        ))}
      </div>
      {removingHost ? (
        <div className="flex items-center gap-2 text-sm" data-testid="remove-purge-row">
          <input
            id="remove-purge"
            type="checkbox"
            checked={purge}
            onChange={(e) => setPurge(e.target.checked)}
          />
          <Label htmlFor="remove-purge">{t("hosts.remove.purge")}</Label>
        </div>
      ) : null}
      <ConfirmActionDialog
        open={removingHost !== null}
        title={t("hosts.remove.title")}
        body={removingHost ? `${removingHost.label} — ${t("hosts.remove.body")}` : ""}
        confirmLabel={t("hosts.remove.confirm")}
        destructive
        onConfirm={confirmRemove}
        onCancel={() => {
          setRemoving(null);
          setPurge(false);
        }}
      />
    </div>
  );
}
