/**
 * AlertsSection (FX-017): renders persisted rules with sentence previews,
 * the audit event log, the honest notification-state report, and keeps the
 * local-action editor locked while the master gate is off. IPC is mocked
 * (test-only); the mocked payloads mirror the Rust serde shapes.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AlertRule, Settings } from "@/lib/ipc/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AlertsSection } from "./AlertsSection";
import { useSettingsStore } from "@/stores/settingsStore";

const rule: AlertRule = {
  id: "rule-1",
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

const settingsFixture: Settings = {
  schema_version: 1,
  appearance: "system",
  compact_mode: false,
  polling_base_seconds: 3,
  close_to_tray: false,
  locale: "en",
  overview_cards: [{ id: "cpu", visible: true }],
  history: { raw_hours: 24, rollup_1m_days: 30, rollup_1h_days: 365 },
  rate_unit: "bytes",
  external_lookup: { enabled: false, provider: "ipify", timeout_secs: 5 },
  temperature_unit: "celsius",
  tray: { enabled: true, show_cpu: true, show_memory: true, show_network: true, show_title: true },
  launch_at_login: false,
  alerts: {
    notifications_enabled: true,
    actions_enabled: false,
    quiet_hours_enabled: false,
    quiet_start_minute: 1320,
    quiet_end_minute: 480,
  },
};

const ipcMocks = vi.hoisted(() => ({
  getAlertRules: vi.fn(),
  updateAlertRule: vi.fn(async (rule: AlertRule) => rule),
  createAlertRule: vi.fn(async (rule: AlertRule) => ({ ...rule, id: "new-1" })),
  deleteAlertRule: vi.fn(async () => true),
  getAlertEvents: vi.fn(async () => [
    {
      id: "rule-1-1000",
      rule_id: "rule-1",
      state: "resolved",
      triggered_at_utc_ms: 1_000,
      resolved_at_utc_ms: 61_000,
    },
  ]),
  getNotificationState: vi.fn(async () => ({
    platform: "macos",
    state: "os_managed",
    detail_key: "alerts.notificationStateMacos",
  })),
  sendTestNotification: vi.fn(async () => undefined),
  testAlertRule: vi.fn(async () => ({
    matched_entity: "host",
    availability: "available",
    value: 95,
    crossing_now: true,
    explanation: "Value 95 crosses the threshold now.",
  })),
}));

vi.mock("@/lib/ipc/client", () => ({ ipc: ipcMocks }));

describe("AlertsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    ipcMocks.getAlertRules.mockReset();
    ipcMocks.getAlertRules.mockImplementation(async () => [rule]);
    useSettingsStore.setState({ settings: settingsFixture, status: "ready", error: null });
  });

  it("renders rules as sentence previews and the audit event log", async () => {
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    expect(
      await screen.findByText(
        "Notify when CPU total usage rises above 90 % and stays there for 1 min. Re-alerts after 5 min."
      )
    ).toBeTruthy();
    expect(await screen.findByText("Recent alert events")).toBeTruthy();
    expect(await screen.findByText("Resolved")).toBeTruthy();
    expect(ipcMocks.getAlertEvents).toHaveBeenCalledWith(20);
  });

  it("renders the empty state when no rules exist", async () => {
    ipcMocks.getAlertRules.mockImplementation(async () => []);
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    expect(await screen.findByText(/No rules yet/)).toBeTruthy();
  });

  it("keeps the action editor locked while the master gate is off", async () => {
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    await userEvent.click(await screen.findByText("Add rule"));
    expect(await screen.findByText("New rule")).toBeTruthy();
    expect(
      screen.getByText(/Enable local actions \(below\) before configuring an action/)
    ).toBeTruthy();
    expect(screen.queryByText("Program path")).toBeNull();
  });

  it("offers the action fields once the master gate is on", async () => {
    useSettingsStore.setState({
      settings: {
        ...settingsFixture,
        alerts: { ...settingsFixture.alerts, actions_enabled: true },
      },
    });
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    await userEvent.click(await screen.findByText("Add rule"));
    expect(await screen.findByText("Program path")).toBeTruthy();
    await userEvent.type(screen.getByLabelText("Program path"), "/usr/local/bin/notify-wrap");
    expect(screen.getByLabelText("Approve action")).toBeTruthy();
  });

  it("saves a new rule with an empty id and refreshes", async () => {
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    await userEvent.click(await screen.findByText("Add rule"));
    await userEvent.click(await screen.findByText("Save rule"));
    await waitFor(() => {
      expect(ipcMocks.createAlertRule).toHaveBeenCalled();
    });
    const sent = ipcMocks.createAlertRule.mock.calls[0]?.[0] as AlertRule;
    expect(sent.id).toBe("");
    expect(sent.metric_id).toBe("cpu.total.usage");
  });

  it("toggles a rule through updateAlertRule", async () => {
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    const toggle = await screen.findByRole("switch", { name: "Enabled" });
    await userEvent.click(toggle);
    await waitFor(() => {
      expect(ipcMocks.updateAlertRule).toHaveBeenCalled();
    });
    expect(ipcMocks.updateAlertRule.mock.calls[0]?.[0]).toMatchObject({
      id: "rule-1",
      enabled: false,
    });
  });

  it("runs the fixture probe without persisting anything", async () => {
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    await userEvent.click(await screen.findByText("Add rule"));
    await userEvent.click(await screen.findByText("Test current values"));
    expect(await screen.findByText(/Probe: host = 95/)).toBeTruthy();
    expect(ipcMocks.testAlertRule).toHaveBeenCalled();
    expect(ipcMocks.createAlertRule).not.toHaveBeenCalled();
  });

  it("reports the honest macOS notification state", async () => {
    render(
      <TooltipProvider>
        <AlertsSection />
      </TooltipProvider>
    );
    expect(
      await screen.findByText(/macOS manages notification permission itself/)
    ).toBeTruthy();
  });
});
