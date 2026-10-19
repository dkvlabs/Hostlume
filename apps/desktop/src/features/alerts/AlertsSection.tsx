/**
 * Settings → Alerts (FX-017, design.md §11): rule editor with a human
 * sentence preview, fixture-driven test (no live actions), audit event
 * log, notification troubleshooting with honest permission reporting, and
 * the disabled-by-default local action gates. All mutations go through the
 * validated Rust commands; the WebView never shapes executable config.
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
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { ipc } from "@/lib/ipc/client";
import type {
  AlertComparator,
  AlertEventRow,
  AlertRule,
  NotificationStateInfo,
  RuleProbe,
} from "@/lib/ipc/types";
import { useSettingsStore } from "@/stores/settingsStore";
import {
  ALERT_METRIC_OPTIONS,
  MINUTES_OF_DAY,
  formatMinuteOfDay,
  ruleSentence,
} from "./sentence";

const inputClass =
  "h-8 rounded-[8px] border border-stroke bg-surface px-3 text-sm focus-visible:outline-2 focus-visible:outline-accent";

function blankRule(): AlertRule {
  return {
    id: "",
    metric_id: "cpu.total.usage",
    entity_filter: null,
    comparator: "above",
    threshold: 90,
    hold_duration_ms: 60_000,
    hysteresis_delta: 10,
    cooldown_ms: 300_000,
    enabled: true,
    notification_message: null,
    action: null,
  };
}

export function AlertsSection() {
  const t = useT();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.update);
  const [rules, setRules] = useState<AlertRule[] | null>(null);
  const [events, setEvents] = useState<AlertEventRow[] | null>(null);
  const [notifState, setNotifState] = useState<NotificationStateInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<AlertRule | null>(null);
  const [draftIsNew, setDraftIsNew] = useState(false);
  const [probe, setProbe] = useState<RuleProbe | null>(null);
  const [testNotif, setTestNotif] = useState<"ok" | "failed" | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [nextRules, nextEvents, nextState] = await Promise.all([
        ipc.getAlertRules(),
        ipc.getAlertEvents(20),
        ipc.getNotificationState(),
      ]);
      setRules(nextRules);
      setEvents(nextEvents);
      setNotifState(nextState);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const patchAlerts = (field: keyof NonNullable<typeof settings>["alerts"], value: boolean | number) => {
    if (!settings) return;
    void updateSettings({ alerts: { ...settings.alerts, [field]: value } });
  };

  const ruleLabel = (ruleId: string): string => {
    const rule = rules?.find((r) => r.id === ruleId);
    return rule ? rule.metric_id : ruleId;
  };

  const saveDraft = async () => {
    if (!draft) return;
    try {
      if (draftIsNew) {
        await ipc.createAlertRule({ ...draft, id: "" });
      } else {
        await ipc.updateAlertRule(draft);
      }
      setDraft(null);
      setProbe(null);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const testDraft = async () => {
    if (!draft) return;
    setProbe(null);
    try {
      setProbe(await ipc.testAlertRule(draft));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="grid gap-4">
      {error ? (
        <p role="alert" className="text-xs text-critical">
          {error}
        </p>
      ) : null}

      {/* Rules */}
      <Card>
        <CardHeader>
          <CardTitle>{t("alerts.rules")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3">
          {rules === null ? (
            <p role="status" className="text-sm text-muted">
              {t("status.connecting")}
            </p>
          ) : rules.length === 0 ? (
            <p className="text-sm text-muted">{t("alerts.noRules")}</p>
          ) : (
            rules.map((rule) => (
              <div key={rule.id} className="grid gap-1 border-b border-stroke pb-3 last:border-b-0">
                <div className="flex items-center gap-3">
                  <Switch
                    id={`rule-enabled-${rule.id}`}
                    checked={rule.enabled}
                    aria-label={t("alerts.enabled")}
                    onCheckedChange={(enabled) => {
                      void ipc
                        .updateAlertRule({ ...rule, enabled })
                        .then(refresh)
                        .catch((err) => setError(String(err)));
                    }}
                  />
                  <Label htmlFor={`rule-enabled-${rule.id}`} className="flex-1 text-sm font-normal">
                    {ruleSentence(rule)}
                  </Label>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setDraft({ ...rule });
                      setDraftIsNew(false);
                      setProbe(null);
                    }}
                  >
                    {t("alerts.edit")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setDraft({ ...rule, id: "", enabled: false });
                      setDraftIsNew(true);
                      setProbe(null);
                    }}
                  >
                    {t("alerts.duplicate")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      void ipc.deleteAlertRule(rule.id).then(refresh).catch((err) => setError(String(err)));
                    }}
                  >
                    {t("alerts.delete")}
                  </Button>
                </div>
                {rule.action ? (
                  <p className="text-xs text-muted sm:pl-9">
                    <Badge variant={rule.action.approved ? "warning" : "neutral"}>
                      {rule.action.approved
                        ? t("alerts.actionEnabled")
                        : t("alerts.actionStored")}
                    </Badge>{" "}
                    {rule.action.program} · {t("alerts.actionTimeout")} {rule.action.timeout_secs}s
                  </p>
                ) : null}
              </div>
            ))
          )}
          <Button
            variant="secondary"
            size="sm"
            className="w-fit"
            onClick={() => {
              setDraft(blankRule());
              setDraftIsNew(true);
              setProbe(null);
            }}
          >
            {t("alerts.addRule")}
          </Button>
        </CardContent>
      </Card>

      {/* Rule editor (inline card — no modal dependency) */}
      {draft ? (
        <Card>
          <CardHeader>
            <CardTitle>
              {draftIsNew ? t("alerts.editorNew") : t("alerts.editorEdit")}
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-3">
            <p className="text-sm text-muted" role="status">
              {ruleSentence(draft)}
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-metric">{t("alerts.metric")}</Label>
                <Select
                  value={draft.metric_id}
                  onValueChange={(metricId) => setDraft({ ...draft, metric_id: metricId })}
                >
                  <SelectTrigger id="alert-metric" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {ALERT_METRIC_OPTIONS.map((option) => (
                      <SelectItem key={option.id} value={option.id}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-entity">{t("alerts.entity")}</Label>
                <input
                  id="alert-entity"
                  type="text"
                  value={draft.entity_filter ?? ""}
                  placeholder={t("alerts.entityAny")}
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      entity_filter: event.target.value === "" ? null : event.target.value,
                    })
                  }
                  className={inputClass}
                />
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-comparator">{t("alerts.comparator")}</Label>
                <Select
                  value={draft.comparator}
                  onValueChange={(value) =>
                    setDraft({ ...draft, comparator: value as AlertComparator })
                  }
                >
                  <SelectTrigger id="alert-comparator" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="above">{t("alerts.comparator.above")}</SelectItem>
                    <SelectItem value="below">{t("alerts.comparator.below")}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-threshold">{t("alerts.threshold")}</Label>
                <input
                  id="alert-threshold"
                  type="number"
                  value={Number.isFinite(draft.threshold) ? draft.threshold : 0}
                  onChange={(event) =>
                    setDraft({ ...draft, threshold: Number(event.target.value) })
                  }
                  className={inputClass}
                />
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-hold">{t("alerts.holdSeconds")}</Label>
                <input
                  id="alert-hold"
                  type="number"
                  min={0}
                  max={3600}
                  value={draft.hold_duration_ms / 1_000}
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      hold_duration_ms: Math.max(0, Number(event.target.value)) * 1_000,
                    })
                  }
                  className={inputClass}
                />
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-hysteresis">{t("alerts.hysteresis")}</Label>
                <input
                  id="alert-hysteresis"
                  type="number"
                  min={0}
                  value={draft.hysteresis_delta}
                  onChange={(event) =>
                    setDraft({ ...draft, hysteresis_delta: Math.max(0, Number(event.target.value)) })
                  }
                  className={inputClass}
                />
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-cooldown">{t("alerts.cooldownMinutes")}</Label>
                <input
                  id="alert-cooldown"
                  type="number"
                  min={0}
                  max={1440}
                  value={draft.cooldown_ms / 60_000}
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      cooldown_ms: Math.max(0, Number(event.target.value)) * 60_000,
                    })
                  }
                  className={inputClass}
                />
              </div>
              <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                <Label htmlFor="alert-message">{t("alerts.message")}</Label>
                <input
                  id="alert-message"
                  type="text"
                  value={draft.notification_message ?? ""}
                  placeholder="{value} on {entity}"
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      notification_message: event.target.value === "" ? null : event.target.value,
                    })
                  }
                  className={inputClass}
                />
              </div>
            </div>

            {/* Optional local action — only editable while the master gate
                is on; the per-rule approval is the second key. */}
            <Separator />
            <p className="text-xs text-muted">
              {t("alerts.actionIntro")}
            </p>
            {settings?.alerts.actions_enabled ? (
              <div className="grid gap-3">
                <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                  <Label htmlFor="alert-action-program">{t("alerts.actionProgram")}</Label>
                  <input
                    id="alert-action-program"
                    type="text"
                    value={draft.action?.program ?? ""}
                    placeholder="/usr/local/bin/my-alert-script"
                    onChange={(event) => {
                      const program = event.target.value;
                      setDraft({
                        ...draft,
                        action:
                          program === ""
                            ? null
                            : {
                                program,
                                timeout_secs: draft.action?.timeout_secs ?? 10,
                                approved: draft.action?.approved ?? false,
                              },
                      });
                    }}
                    className={inputClass}
                  />
                </div>
                {draft.action ? (
                  <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                    <Label htmlFor="alert-action-timeout">{t("alerts.actionTimeout")}</Label>
                    <input
                      id="alert-action-timeout"
                      type="number"
                      min={1}
                      max={60}
                      value={draft.action.timeout_secs}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          action: {
                            ...draft.action!,
                            timeout_secs: Math.max(1, Number(event.target.value)),
                          },
                        })
                      }
                      className={inputClass}
                    />
                  </div>
                ) : null}
                {draft.action ? (
                  <div className="grid items-center gap-2 sm:grid-cols-[8rem_1fr]">
                    <Label htmlFor="alert-action-approve">{t("alerts.actionApprove")}</Label>
                    <div className="flex items-center gap-3">
                      <Switch
                        id="alert-action-approve"
                        checked={draft.action.approved}
                        onCheckedChange={(approved) =>
                          setDraft({
                            ...draft,
                            action: { ...draft.action!, approved },
                          })
                        }
                      />
                      <span className="text-xs text-muted">{t("alerts.actionApproveHint")}</span>
                    </div>
                  </div>
                ) : null}
              </div>
            ) : (
              <p className="text-xs text-warning">{t("alerts.actionGateOff")}</p>
            )}

            {probe ? (
              <p className="text-xs text-muted" role="status">
                {t("alerts.probePrefix")} {probe.matched_entity ?? "—"} ={" "}
                {probe.value === null ? "—" : probe.value} ·{" "}
                {probe.crossing_now ? t("alerts.probeCrossing") : t("alerts.probeIdle")}{" "}
                {probe.explanation}
              </p>
            ) : null}
            {draftIsNew && draft.id !== "" ? (
              <p className="text-xs text-critical">{t("alerts.editorIdError")}</p>
            ) : null}

            <div className="flex gap-2">
              <Button size="sm" onClick={() => void saveDraft()}>
                {t("alerts.save")}
              </Button>
              <Button size="sm" variant="secondary" onClick={() => void testDraft()}>
                {t("alerts.test")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  setDraft(null);
                  setProbe(null);
                }}
              >
                {t("common.cancel")}
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : null}

      {/* Delivery: notifications + quiet hours */}
      <Card>
        <CardHeader>
          <CardTitle>{t("alerts.delivery")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="alerts-notifications">{t("alerts.notificationsEnabled")}</Label>
            <Switch
              id="alerts-notifications"
              checked={settings?.alerts.notifications_enabled ?? false}
              disabled={!settings}
              onCheckedChange={(checked) => patchAlerts("notifications_enabled", checked)}
            />
          </div>
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="alerts-quiet">{t("alerts.quietHours")}</Label>
            <div className="flex items-center gap-3">
              <Switch
                id="alerts-quiet"
                checked={settings?.alerts.quiet_hours_enabled ?? false}
                disabled={!settings}
                onCheckedChange={(checked) => patchAlerts("quiet_hours_enabled", checked)}
              />
              {settings?.alerts.quiet_hours_enabled ? (
                <>
                  <Select
                    value={String(settings.alerts.quiet_start_minute)}
                    onValueChange={(value) => patchAlerts("quiet_start_minute", Number(value))}
                  >
                    <SelectTrigger className="w-24" aria-label={t("alerts.quietStart")}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {MINUTES_OF_DAY.map((minute) => (
                        <SelectItem key={minute} value={String(minute)}>
                          {formatMinuteOfDay(minute)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <span aria-hidden>–</span>
                  <Select
                    value={String(settings.alerts.quiet_end_minute)}
                    onValueChange={(value) => patchAlerts("quiet_end_minute", Number(value))}
                  >
                    <SelectTrigger className="w-24" aria-label={t("alerts.quietEnd")}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {MINUTES_OF_DAY.map((minute) => (
                        <SelectItem key={minute} value={String(minute)}>
                          {formatMinuteOfDay(minute)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </>
              ) : null}
            </div>
          </div>
          <p className="text-xs text-muted">{t("alerts.quietHint")}</p>
          <Separator />
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="alerts-test-notification">{t("alerts.notificationState")}</Label>
            <div className="flex items-center gap-3">
              <Button
                id="alerts-test-notification"
                variant="secondary"
                size="sm"
                onClick={() => {
                  setTestNotif(null);
                  ipc
                    .sendTestNotification()
                    .then(() => setTestNotif("ok"))
                    .catch(() => setTestNotif("failed"));
                }}
              >
                {t("alerts.sendTest")}
              </Button>
              {testNotif === "ok" ? <Badge variant="neutral">{t("alerts.testSent")}</Badge> : null}
              {testNotif === "failed" ? (
                <Badge variant="critical">{t("alerts.testFailed")}</Badge>
              ) : null}
            </div>
          </div>
          <p className="text-xs text-muted">
            {notifState
              ? t(notifState.detail_key as MessageKey)
              : t("status.connecting")}
          </p>
        </CardContent>
      </Card>

      {/* Local action master gate */}
      <Card>
        <CardHeader>
          <CardTitle>{t("alerts.actionsTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3">
          <p className="text-xs text-warning">{t("alerts.actionsWarning")}</p>
          <div className="grid items-center gap-2 sm:grid-cols-[10rem_1fr]">
            <Label htmlFor="alerts-actions-enabled">{t("alerts.actionsEnabled")}</Label>
            <Switch
              id="alerts-actions-enabled"
              checked={settings?.alerts.actions_enabled ?? false}
              disabled={!settings}
              onCheckedChange={(checked) => patchAlerts("actions_enabled", checked)}
            />
          </div>
          <p className="text-xs text-muted">{t("alerts.actionsScope")}</p>
        </CardContent>
      </Card>

      {/* Audit event log */}
      <Card>
        <CardHeader>
          <CardTitle>{t("alerts.events")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-2">
          {events === null ? (
            <p role="status" className="text-sm text-muted">
              {t("status.connecting")}
            </p>
          ) : events.length === 0 ? (
            <p className="text-sm text-muted">{t("alerts.noEvents")}</p>
          ) : (
            events.map((event) => (
              <div key={event.id} className="flex items-center gap-3 text-sm">
                <Badge variant={event.state === "firing" ? "warning" : "neutral"}>
                  {event.state === "firing"
                    ? t("alerts.eventFiring")
                    : t("alerts.eventResolved")}
                </Badge>
                <span className="flex-1">{ruleLabel(event.rule_id)}</span>
                <span className="numeric text-xs text-muted">
                  {new Date(event.triggered_at_utc_ms).toLocaleString()}
                </span>
              </div>
            ))
          )}
        </CardContent>
      </Card>
    </div>
  );
}
