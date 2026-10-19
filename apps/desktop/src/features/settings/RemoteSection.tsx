/**
 * Settings → Remote (FX-019, design.md §12/§14): the explicit opt-in for
 * the read-only monitor server. Default OFF — nothing listens unless this
 * section says so. Loopback only in this build (LAN fails closed until
 * TLS provisioning is validated); the access token is shown exactly once
 * and only its hash is persisted; rotation kills old sessions instantly.
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

const inputClass =
  "h-8 w-28 rounded-[8px] border border-stroke bg-surface px-3 text-sm numeric focus-visible:outline-2 focus-visible:outline-accent";

export function RemoteSection() {
  const t = useT();
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [port, setPort] = useState<number | null>(null);
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
              value={status?.mode ?? "off"}
              onValueChange={(value) => void configure(value as RemoteMode)}
            >
              <SelectTrigger id="remote-mode" className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">{t("remote.mode.off")}</SelectItem>
                <SelectItem value="loopback">{t("remote.mode.loopback")}</SelectItem>
                <SelectItem value="lan" disabled>
                  {t("remote.mode.lan")}
                </SelectItem>
              </SelectContent>
            </Select>
            {status?.mode === "lan" ? (
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
              disabled={status?.mode !== "loopback" || port === status?.port}
              onClick={() => void configure("loopback")}
            >
              {t("remote.apply")}
            </Button>
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
              {!status?.token_exists && status?.mode === "loopback" ? (
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
