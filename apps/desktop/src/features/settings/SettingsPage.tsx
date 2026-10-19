/**
 * Settings v1 (phase 01 §2 task 7): appearance dark/light/system, compact
 * mode, polling bounds, close-to-tray preference, locale registry. Every
 * change is validated in Rust and persisted to SQLite; failures roll the UI
 * back and are surfaced, never silently swallowed.
 */
import { useState } from "react";
import { ConfirmActionDialog } from "@/components/ui/ConfirmActionDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { LOCALES, useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { ipc } from "@/lib/ipc/client";
import type { Appearance, TemperatureUnit } from "@/lib/ipc/types";
import { useSettingsStore } from "@/stores/settingsStore";
import { AlertsSection } from "@/features/alerts/AlertsSection";
import { RemoteSection } from "./RemoteSection";

/** Mirrors apps/desktop/src-tauri/src/settings.rs safe bounds. */
export const POLLING_MIN_SECS = 1;
export const POLLING_MAX_SECS = 60;
export const POLLING_DEFAULT_SECS = 3;

export function SettingsPage() {
  const t = useT();
  const settings = useSettingsStore((s) => s.settings);
  const update = useSettingsStore((s) => s.update);
  const [feedback, setFeedback] = useState<"saved" | "failed" | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearFeedback, setClearFeedback] = useState<boolean | null>(null);

  if (!settings) {
    return (
      <div className="p-5 text-sm text-muted" role="status">
        {t("status.connecting")}
      </div>
    );
  }

  const patch = async (partial: Parameters<typeof update>[0]) => {
    const ok = await update(partial);
    setFeedback(ok ? "saved" : "failed");
  };

  return (
    <div className="grid max-w-2xl gap-4 p-5">
      {feedback ? (
        <p role="status" className={feedback === "saved" ? "text-xs text-muted" : "text-xs text-critical"}>
          {feedback === "saved" ? t("settings.saved") : t("settings.saveFailed")}
        </p>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.appearance")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-theme">{t("settings.theme")}</Label>
            <Select
              value={settings.appearance}
              onValueChange={(value) => void patch({ appearance: value as Appearance })}
            >
              <SelectTrigger id="setting-theme" className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="system">{t("settings.theme.system")}</SelectItem>
                <SelectItem value="light">{t("settings.theme.light")}</SelectItem>
                <SelectItem value="dark">{t("settings.theme.dark")}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-locale">{t("settings.locale")}</Label>
            <Select
              value={settings.locale}
              onValueChange={(value) => void patch({ locale: value })}
            >
              <SelectTrigger id="setting-locale" className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {LOCALES.map((locale) => (
                  <SelectItem key={locale} value={locale}>
                    {locale === "en" ? "English" : "Română"}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-temperature-unit">
              {t("settings.temperatureUnit")}
            </Label>
            <Select
              value={settings.temperature_unit}
              onValueChange={(value) =>
                void patch({ temperature_unit: value as TemperatureUnit })
              }
            >
              <SelectTrigger id="setting-temperature-unit" className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="celsius">
                  {t("settings.temperatureUnit.celsius")}
                </SelectItem>
                <SelectItem value="fahrenheit">
                  {t("settings.temperatureUnit.fahrenheit")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-compact">{t("settings.compactMode")}</Label>
            <div className="flex items-center gap-3">
              <Switch
                id="setting-compact"
                checked={settings.compact_mode}
                onCheckedChange={(checked) => void patch({ compact_mode: checked })}
              />
              <span className="text-xs text-muted">{t("settings.compactModeHint")}</span>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.monitoring")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-polling">{t("settings.polling")}</Label>
            <div className="flex items-center gap-3">
              <Slider
                id="setting-polling"
                min={POLLING_MIN_SECS}
                max={POLLING_MAX_SECS}
                step={1}
                value={[settings.polling_base_seconds]}
                aria-label={t("settings.polling")}
                onValueCommit={([value]) => {
                  if (value !== undefined) void patch({ polling_base_seconds: value });
                }}
              />
              <Badge variant="neutral" className="numeric w-14 justify-center">
                {settings.polling_base_seconds}s
              </Badge>
            </div>
            <p className="text-xs text-muted sm:col-start-2">
              {t("settings.pollingHint")}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.general")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-close-tray">{t("settings.closeToTray")}</Label>
            <div className="flex items-center gap-3">
              <Switch
                id="setting-close-tray"
                checked={settings.close_to_tray}
                onCheckedChange={(checked) => void patch({ close_to_tray: checked })}
              />
              <span className="text-xs text-muted">{t("settings.closeToTrayHint")}</span>
            </div>
          </div>

          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="setting-launch-login">{t("settings.launchAtLogin")}</Label>
            <div className="flex items-center gap-3">
              <Switch
                id="setting-launch-login"
                checked={settings.launch_at_login}
                onCheckedChange={(checked) => void patch({ launch_at_login: checked })}
              />
              <span className="text-xs text-muted">{t("settings.launchAtLoginHint")}</span>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.tray")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4">
          <p className="text-xs text-muted">{t("settings.trayHint")}</p>
          {(
            [
              ["setting-tray-enabled", "enabled", "settings.tray.enabled"],
              ["setting-tray-cpu", "show_cpu", "settings.tray.showCpu"],
              ["setting-tray-memory", "show_memory", "settings.tray.showMemory"],
              ["setting-tray-network", "show_network", "settings.tray.showNetwork"],
              ["setting-tray-title", "show_title", "settings.tray.showTitle"],
            ] as const
          ).map(([id, key, labelKey]) => (
            <div
              key={id}
              className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]"
            >
              <Label htmlFor={id}>{t(labelKey)}</Label>
              <Switch
                id={id}
                checked={settings.tray[key]}
                onCheckedChange={(checked) =>
                  void patch({ tray: { ...settings.tray, [key]: checked } })
                }
              />
            </div>
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.overviewLayout")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-2">
          <p className="text-xs text-muted">{t("settings.overviewLayoutHint")}</p>
          {settings.overview_cards.map((card, index) => (
            <div key={card.id} className="flex items-center gap-3 text-sm">
              <Switch
                id={`card-visible-${card.id}`}
                checked={card.visible}
                aria-label={`${t("settings.overviewLayout")}: ${t(("overview.card." + card.id) as MessageKey)}`}
                onCheckedChange={(visible) => {
                  const next = settings.overview_cards.map((c, i) =>
                    i === index ? { ...c, visible } : c
                  );
                  void patch({ overview_cards: next });
                }}
              />
              <Label htmlFor={`card-visible-${card.id}`} className="flex-1">
                {t(("overview.card." + card.id) as MessageKey)}
              </Label>
              <Button
                variant="ghost"
                size="icon"
                disabled={index === 0}
                aria-label={t("common.moveUp")}
                onClick={() => {
                  const next = [...settings.overview_cards];
                  const [moved] = next.splice(index, 1);
                  next.splice(index - 1, 0, moved!);
                  void patch({ overview_cards: next });
                }}
              >
                ↑
              </Button>
              <Button
                variant="ghost"
                size="icon"
                disabled={index === settings.overview_cards.length - 1}
                aria-label={t("common.moveDown")}
                onClick={() => {
                  const next = [...settings.overview_cards];
                  const [moved] = next.splice(index, 1);
                  next.splice(index + 1, 0, moved!);
                  void patch({ overview_cards: next });
                }}
              >
                ↓
              </Button>
            </div>
          ))}
        </CardContent>
      </Card>

      {/* Alerts (FX-017, design.md §11): rules, notifications, actions. */}
      <AlertsSection />

      {/* Remote read-only monitor (FX-019): explicit opt-in, default OFF. */}
      <RemoteSection />

      <Card>
        <CardHeader>
          <CardTitle>{t("history.title")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4">
          <p className="text-xs text-muted">{t("history.retention")}</p>
          {(
            [
              ["raw_hours", "history.rawHours", 720],
              ["rollup_1m_days", "history.rollup1m", 730],
              ["rollup_1h_days", "history.rollup1h", 3650],
            ] as const
          ).map(([field, labelKey, max]) => (
            <div key={field} className="grid items-center gap-2 sm:grid-cols-[16rem_1fr]">
              <Label htmlFor={`history-${field}`}>{t(labelKey)}</Label>
              <Slider
                id={`history-${field}`}
                min={0}
                max={max}
                step={field === "raw_hours" ? 1 : 30}
                value={[settings.history[field]]}
                aria-label={t(labelKey)}
                onValueCommit={([value]) => {
                  if (value !== undefined) {
                    void patch({ history: { ...settings.history, [field]: value } });
                  }
                }}
              />
              <span className="numeric text-xs text-muted sm:col-start-2">
                {settings.history[field]}
              </span>
            </div>
          ))}
          <Separator />
          <Button
            variant="destructive"
            className="w-fit"
            onClick={() => setConfirmClear(true)}
          >
            {t("history.clear")}
          </Button>
          {clearFeedback ? (
            <p className="text-xs text-muted" role="status">
              {t("history.cleared")}
            </p>
          ) : null}
        </CardContent>
      </Card>

      {confirmClear ? (
        <ConfirmActionDialog
          open
          destructive
          title={t("history.clearConfirmTitle")}
          body={t("history.clearConfirmBody")}
          confirmLabel={t("history.clear")}
          onConfirm={() => {
            void ipc
              .clearHistory(`settings-${Date.now()}`)
              .then(() => setClearFeedback(true))
              .catch(() => setClearFeedback(false));
          }}
          onCancel={() => setConfirmClear(false)}
        />
      ) : null}

      <Separator className="mt-2" />
      <Button variant="ghost" size="sm" className="w-fit" onClick={() => void patch({})}>
        {t("common.retry")}
      </Button>
    </div>
  );
}
