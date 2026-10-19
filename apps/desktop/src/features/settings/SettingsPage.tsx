/**
 * Settings v1 (phase 01 §2 task 7): appearance dark/light/system, compact
 * mode, polling bounds, close-to-tray preference, locale registry. Every
 * change is validated in Rust and persisted to SQLite; failures roll the UI
 * back and are surfaced, never silently swallowed.
 */
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { LOCALES, useT } from "@/lib/i18n";
import type { Appearance } from "@/lib/ipc/types";
import { useSettingsStore } from "@/stores/settingsStore";

/** Mirrors apps/desktop/src-tauri/src/settings.rs safe bounds. */
export const POLLING_MIN_SECS = 1;
export const POLLING_MAX_SECS = 60;
export const POLLING_DEFAULT_SECS = 3;

export function SettingsPage() {
  const t = useT();
  const settings = useSettingsStore((s) => s.settings);
  const update = useSettingsStore((s) => s.update);
  const [feedback, setFeedback] = useState<"saved" | "failed" | null>(null);

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
        </CardContent>
      </Card>

      <Separator className="mt-2" />
      <Button variant="ghost" size="sm" className="w-fit" onClick={() => void patch({})}>
        {t("common.retry")}
      </Button>
    </div>
  );
}
