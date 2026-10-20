/**
 * Settings → Remote (FX-019 phase 08, FX-022 phase 10): the explicit
 * opt-in for the read-only monitor server. Default OFF — nothing listens
 * unless this section says so. Loopback is plain HTTP on 127.0.0.1; LAN is
 * HTTPS with this machine's own certificate, whose fingerprint consoles
 * pin at pairing time. The owner token is shown exactly once and only its
 * hash is persisted; paired consoles have their own revocable credentials.
 */
import { useCallback, useEffect, useState } from "react";
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
import { useT } from "@/lib/i18n";
import { ipc } from "@/lib/ipc/client";
import type { RemoteMode, RemoteStatus } from "@/lib/ipc/types";
import { useSettingsStore } from "@/stores/settingsStore";

const inputClass =
  "h-8 w-28 rounded-[8px] border border-stroke bg-surface px-3 text-sm numeric focus-visible:outline-2 focus-visible:outline-accent";
const wideInputClass =
  "h-8 w-full max-w-md rounded-[8px] border border-stroke bg-surface px-3 text-sm focus-visible:outline-2 focus-visible:outline-accent";

function PairingWindow({ status }: { status: RemoteStatus }) {
  const t = useT();
  const [remaining, setRemaining] = useState(status.pairing?.secs_remaining ?? 0);
  const pairing = status.pairing;

  useEffect(() => {
    setRemaining(pairing?.secs_remaining ?? 0);
    if (!pairing) return;
    const timer = window.setInterval(
      () => setRemaining((s) => (s > 0 ? s - 1 : 0)),
      1_000
    );
    return () => window.clearInterval(timer);
  }, [pairing]);

  if (!pairing) {
    return (
      <div className="grid items-start gap-2 sm:grid-cols-[10rem_1fr]" data-testid="pairing-closed">
        <Label htmlFor="remote-pairing">{t("remote.pairing.title")}</Label>
        <div className="flex flex-col gap-1">
          <Button
            id="remote-pairing"
            size="sm"
            variant="secondary"
            className="w-fit"
            onClick={() => void ipc.openPairingWindow()}
          >
            {t("remote.pairing.open")}
          </Button>
          <p className="text-xs text-muted">{t("remote.pairing.hint")}</p>
        </div>
      </div>
    );
  }
  return (
    <div className="grid items-start gap-2 sm:grid-cols-[10rem_1fr]" data-testid="pairing-open">
      <Label htmlFor="remote-pairing-code">{t("remote.pairing.code")}</Label>
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-3">
          <code
            id="remote-pairing-code"
            data-testid="pairing-code"
            className="select-all rounded-[8px] border border-accent/40 bg-accent/10 px-3 py-1 text-lg font-semibold tracking-[0.3em]"
          >
            {pairing.code}
          </code>
          <span className="text-xs text-muted">
            {t("remote.pairing.expires")} {remaining} s
          </span>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => void ipc.closePairingWindow().catch(() => undefined)}
          >
            {t("remote.pairing.close")}
          </Button>
        </div>
        <p className="text-xs text-muted">{t("remote.pairing.hint")}</p>
      </div>
    </div>
  );
}

function DevicesList({ status }: { status: RemoteStatus }) {
  const t = useT();
  const [busy, setBusy] = useState<string | null>(null);
  if (status.devices.length === 0) {
    return (
      <div className="grid items-start gap-2 sm:grid-cols-[10rem_1fr]" data-testid="devices-empty">
        <span className="text-xs font-medium">{t("remote.devices.title")}</span>
        <p className="text-xs text-muted">{t("remote.devices.empty")}</p>
      </div>
    );
  }
  return (
    <div className="grid items-start gap-2 sm:grid-cols-[10rem_1fr]">
      <span className="text-xs font-medium">{t("remote.devices.title")}</span>
      <ul className="flex flex-col gap-2">
        {status.devices.map((device) => (
          <li
            key={device.device_id}
            className="flex flex-wrap items-center justify-between gap-2 rounded-[8px] border border-stroke p-2"
            data-testid="device-row"
          >
            <div className="min-w-0">
              <p className="truncate text-sm">{device.label}</p>
              <p className="text-[11px] text-muted">
                {t("remote.devices.lastSeen")}:{" "}
                {device.last_seen_secs === null
                  ? t("remote.devices.never")
                  : `${device.last_seen_secs} s`}{" "}
                · v{device.protocol_version} ·{" "}
                <span className="font-mono">{device.device_id}</span>
              </p>
            </div>
            <Button
              size="sm"
              variant="destructive"
              disabled={busy === device.device_id}
              data-testid={`revoke-device-${device.device_id}`}
              onClick={() => {
                setBusy(device.device_id);
                void ipc
                  .revokeRemoteDevice(device.device_id)
                  .catch(() => undefined)
                  .finally(() => setBusy(null));
              }}
            >
              {t("remote.devices.revoke")}
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function RemoteSection() {
  const t = useT();
  const updateSettings = useSettingsStore((s) => s.update);
  const settings = useSettingsStore((s) => s.settings);
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [port, setPort] = useState<number | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [nameSaved, setNameSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tokenOnce, setTokenOnce] = useState<string | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const next = await ipc.getRemoteStatus();
      setStatus(next);
      setPort(next.port);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (settings?.remote_display_name && !displayName) {
      setDisplayName(settings.remote_display_name);
    }
  }, [settings?.remote_display_name, displayName]);

  const saveDisplayName = async () => {
    const ok = await updateSettings({ remote_display_name: displayName.trim() });
    setNameSaved(ok);
  };

  const configure = async (mode: RemoteMode) => {
    setError(null);
    setTokenOnce(null);
    try {
      const setup = await ipc.configureRemote(mode, port ?? 0);
      setStatus(setup.status);
      if (setup.token_once) {
        setTokenOnce(setup.token_once);
        setAcknowledged(false);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const rotate = async () => {
    setError(null);
    try {
      const setup = await ipc.rotateRemoteToken();
      setStatus(setup.status);
      setTokenOnce(setup.token_once ?? null);
      setAcknowledged(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const revoke = async () => {
    setError(null);
    try {
      setStatus(await ipc.revokeRemoteToken());
      setTokenOnce(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const mode = status?.mode ?? "off";
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          {t("remote.title")}
          {status?.running ? (
            <Badge variant="accent">{t("remote.running")}</Badge>
          ) : (
            <Badge variant="neutral">{t("remote.off")}</Badge>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        <p className="text-xs text-muted">{t("remote.hint")}</p>
        {error ? (
          <p role="alert" className="text-xs text-critical">
            {error}
          </p>
        ) : null}

        <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
          <Label htmlFor="remote-mode">{t("remote.mode")}</Label>
          <div className="flex items-center gap-3">
            <Select
              value={mode}
              onValueChange={(value) => void configure(value as RemoteMode)}
            >
              <SelectTrigger id="remote-mode" className="w-56">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">{t("remote.mode.off")}</SelectItem>
                <SelectItem value="loopback">{t("remote.mode.loopback")}</SelectItem>
                <SelectItem value="lan">{t("remote.mode.lan")}</SelectItem>
              </SelectContent>
            </Select>
            {status?.mode === "lan" && !status.running ? (
              <span className="text-xs text-warning">{t("remote.lanRefused")}</span>
            ) : null}
          </div>
        </div>

        <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
          <Label htmlFor="remote-port">{t("remote.port")}</Label>
          <div className="flex items-center gap-3">
            <input
              id="remote-port"
              type="number"
              min={1024}
              max={65535}
              value={port ?? 0}
              onChange={(event) => setPort(Number(event.target.value))}
              className={inputClass}
            />
            <Button
              size="sm"
              variant="secondary"
              disabled={mode === "off" || port === status?.port}
              onClick={() => void configure(mode)}
            >
              {t("remote.apply")}
            </Button>
          </div>
        </div>

        <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
          <Label htmlFor="remote-display-name">{t("remote.displayName")}</Label>
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <input
                id="remote-display-name"
                className={wideInputClass}
                value={displayName}
                maxLength={48}
                onChange={(event) => {
                  setDisplayName(event.target.value);
                  setNameSaved(false);
                }}
              />
              <Button
                size="sm"
                variant="secondary"
                disabled={
                  displayName.trim() === settings?.remote_display_name || !displayName.trim()
                }
                onClick={() => void saveDisplayName()}
              >
                {nameSaved ? "✓" : t("remote.apply")}
              </Button>
            </div>
            <p className="text-xs text-muted">{t("remote.displayNameHint")}</p>
          </div>
        </div>

        {status?.running && status.url ? (
          <p className="text-xs text-muted">
            {t("remote.address")}{" "}
            <a
              href={status.url}
              className="text-accent underline"
              target="_blank"
              rel="noreferrer noopener"
            >
              {status.url}
            </a>
          </p>
        ) : null}

        {mode === "lan" ? (
          <>
            <div className="grid items-start gap-2 sm:grid-cols-[10rem_1fr]">
              <span className="text-xs font-medium">{t("remote.fingerprint")}</span>
              <div className="flex flex-col gap-1">
                <code
                  data-testid="tls-fingerprint"
                  className="select-all break-all font-mono text-xs"
                >
                  {status?.tls_fingerprint ?? "—"}
                </code>
                <p className="text-xs text-muted">{t("remote.fingerprintHint")}</p>
                <p className="text-xs text-muted">{t("remote.lanAddressHint")}</p>
              </div>
            </div>
            {status?.running ? <PairingWindow status={status} /> : null}
            {status?.running ? <DevicesList status={status} /> : null}
          </>
        ) : null}

        <div className="grid items-start gap-2 sm:grid-cols-[10rem_1fr]">
          <Label htmlFor="remote-token">{t("remote.token")}</Label>
          <div className="grid gap-2">
            <div className="flex flex-wrap items-center gap-2">
              <Button
                id="remote-token"
                size="sm"
                variant="secondary"
                onClick={() => void rotate()}
              >
                {status?.token_exists ? t("remote.rotate") : t("remote.generate")}
              </Button>
              {status?.token_exists ? (
                <Button size="sm" variant="destructive" onClick={() => void revoke()}>
                  {t("remote.revoke")}
                </Button>
              ) : null}
              {!status?.token_exists && status?.running ? (
                <span className="text-xs text-warning">{t("remote.noToken")}</span>
              ) : null}
            </div>
            {tokenOnce ? (
              <div className="grid gap-2 rounded-[10px] border border-warning bg-surface p-3">
                <p className="text-xs text-warning">{t("remote.tokenOnceWarning")}</p>
                <code className="select-all break-all text-xs">{tokenOnce}</code>
                {acknowledged ? null : (
                  <Button size="sm" variant="secondary" className="w-fit" onClick={() => setAcknowledged(true)}>
                    {t("remote.tokenSaved")}
                  </Button>
                )}
              </div>
            ) : null}
          </div>
        </div>

        <p className="text-xs text-muted">{t("remote.scopeNote")}</p>
      </CardContent>
    </Card>
  );
}
