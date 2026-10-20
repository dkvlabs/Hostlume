/**
 * The single typed IPC adapter (architecture.md §3: lib/ipc, one adapter).
 * Every payload is runtime-validated before leaving this module; callers
 * can trust the types.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isAlertEventRow,
  isAlertRule,
  isCsvExportResult,
  isRecordingSession,
  isCapabilityReport,
  isGpuInfoReport,
  isDiagnosticsReport,
  isHistoryResponse,
  isHostLiveState,
  isHostLiveStateList,
  isNotificationStateInfo,
  isPairingWindowInfo,
  isProcessActionResult,
  isProcessPage,
  isRemoteSetup,
  isRemoteStatus,
  isRemoveHostResult,
  isRuleProbe,
  isSettings,
  isSystemSnapshot,
  isTelemetryEventPayload,
} from "./guards";
import type {
  AlertEventRow,
  AlertRule,
  CsvExportResult,
  CsvExportSpec,
  HostLiveState,
  PairingWindowInfo,
  RecordingSession,
  RecordingStart,
  RemoteMode,
  RemoteSetup,
  RemoteStatus,
  RemoveHostResult,
  CapabilityReport,
  GpuInfoReport,
  CommandErrorPayload,
  DiagnosticsReport,
  HistoryResponse,
  HistoryQuerySpec,
  NotificationStateInfo,
  ProcessActionRequest,
  ProcessActionResult,
  ProcessPage,
  RuleProbe,
  ScanChildren,
  ScanStatus,
  Settings,
  SettingsPatch,
  SystemSnapshot,
  TelemetryEventPayload,
  TrashPathResult,
} from "./types";

/** Error raised when Rust returns a structured CommandError payload. */
export class IpcCommandError extends Error {
  readonly code: string;
  readonly messageKey: string;
  readonly reason?: string;

  constructor(payload: CommandErrorPayload) {
    super(`${payload.code}${payload.reason ? `: ${payload.reason}` : ""}`);
    this.name = "IpcCommandError";
    this.code = payload.code;
    this.messageKey = payload.messageKey;
    this.reason = payload.reason;
  }
}

async function invokeValidated<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  guard: (value: unknown) => value is T
): Promise<T> {
  const raw: unknown = await invoke(command, args);
  if (!guard(raw)) {
    throw new Error(`${command}: response failed runtime validation`);
  }
  return raw;
}

function toIpcError(err: unknown): unknown {
  if (
    err !== null &&
    typeof err === "object" &&
    "code" in err &&
    "messageKey" in err
  ) {
    return new IpcCommandError(err as CommandErrorPayload);
  }
  return err;
}

export const ipc = {
  async getSnapshot(): Promise<SystemSnapshot> {
    try {
      return await invokeValidated("get_snapshot", undefined, isSystemSnapshot);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getCapabilities(): Promise<CapabilityReport> {
    try {
      return await invokeValidated("get_capabilities", undefined, isCapabilityReport);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async reprobeCapabilities(): Promise<CapabilityReport> {
    try {
      return await invokeValidated("reprobe_capabilities", undefined, isCapabilityReport);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getGpuInfo(): Promise<GpuInfoReport> {
    try {
      return await invokeValidated("get_gpu_info", undefined, isGpuInfoReport);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getSettings(): Promise<Settings> {
    try {
      return await invokeValidated("get_settings", undefined, isSettings);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async updateSettings(patch: SettingsPatch): Promise<Settings> {
    try {
      return await invokeValidated("update_settings", { patch }, isSettings);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async resetSettings(): Promise<Settings> {
    try {
      return await invokeValidated("reset_settings", undefined, isSettings);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getDiagnostics(): Promise<DiagnosticsReport> {
    try {
      return await invokeValidated("get_diagnostics", undefined, isDiagnosticsReport);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getProcessPage(): Promise<ProcessPage> {
    try {
      return await invokeValidated("get_process_page", undefined, isProcessPage);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async requestProcessAction(request: ProcessActionRequest): Promise<ProcessActionResult> {
    try {
      return await invokeValidated(
        "request_process_action",
        { request },
        isProcessActionResult
      );
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getHistory(spec: HistoryQuerySpec): Promise<HistoryResponse> {
    try {
      return await invokeValidated("get_history", { ...spec }, isHistoryResponse);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async clearHistory(confirmationId: string): Promise<void> {
    await invoke("clear_history", { confirmationId });
  },

  // Storage analyzer (FX-008). Scan payloads validated structurally below.

  async startScan(root: string): Promise<ScanStatus> {
    const raw: unknown = await invoke("start_scan", { root });
    return scanStatusFrom(raw);
  },

  async cancelScan(scanId: string): Promise<boolean> {
    return (await invoke("cancel_scan", { scanId })) as boolean;
  },

  async getScanStatus(scanId: string): Promise<ScanStatus | null> {
    const raw: unknown = await invoke("get_scan_status", { scanId });
    return raw === null ? null : scanStatusFrom(raw);
  },

  async getScanChildren(
    scanId: string,
    nodeIndex: number,
    offset: number,
    limit: number
  ): Promise<ScanChildren> {
    const raw: unknown = await invoke("get_scan_children", {
      scanId,
      nodeIndex,
      offset,
      limit,
    });
    return scanChildrenFrom(raw);
  },

  async moveToTrash(paths: string[], confirmationId: string): Promise<TrashPathResult[]> {
    const raw: unknown = await invoke("move_to_trash", { paths, confirmationId });
    if (!Array.isArray(raw)) {
      throw new Error("move_to_trash: response failed runtime validation");
    }
    return raw.map((item) => {
      if (
        typeof item === "object" &&
        item !== null &&
        typeof (item as TrashPathResult).path === "string" &&
        typeof (item as TrashPathResult).ok === "boolean"
      ) {
        return item as TrashPathResult;
      }
      throw new Error("move_to_trash: item failed runtime validation");
    });
  },

  async openInManager(path: string): Promise<void> {
    await invoke("open_in_manager", { path });
  },

  async lookupExternalIp(): Promise<string> {
    return (await invoke("lookup_external_ip")) as string;
  },

  // ---- Alerts and approved local actions (FX-017, phase 07) ----------------

  async getAlertRules(): Promise<AlertRule[]> {
    const raw: unknown = await invoke("get_alert_rules");
    if (!Array.isArray(raw)) {
      throw new Error("get_alert_rules: response failed runtime validation");
    }
    return raw.map((rule) => {
      if (!isAlertRule(rule)) {
        throw new Error("get_alert_rules: rule failed runtime validation");
      }
      return rule;
    });
  },

  async createAlertRule(rule: AlertRule): Promise<AlertRule> {
    try {
      return await invokeValidated("create_alert_rule", { rule }, isAlertRule);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async updateAlertRule(rule: AlertRule): Promise<AlertRule> {
    try {
      return await invokeValidated("update_alert_rule", { rule }, isAlertRule);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async deleteAlertRule(ruleId: string): Promise<boolean> {
    return (await invoke("delete_alert_rule", { ruleId })) as boolean;
  },

  async getAlertEvents(limit?: number): Promise<AlertEventRow[]> {
    const raw: unknown = await invoke("get_alert_events", { limit });
    if (!Array.isArray(raw)) {
      throw new Error("get_alert_events: response failed runtime validation");
    }
    return raw.map((row) => {
      if (!isAlertEventRow(row)) {
        throw new Error("get_alert_events: row failed runtime validation");
      }
      return row;
    });
  },

  async getNotificationState(): Promise<NotificationStateInfo> {
    try {
      return await invokeValidated("get_notification_state", undefined, isNotificationStateInfo);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async sendTestNotification(): Promise<void> {
    await invoke("send_test_notification");
  },

  async testAlertRule(rule: AlertRule): Promise<RuleProbe> {
    try {
      return await invokeValidated("test_alert_rule", { rule }, isRuleProbe);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  // ---- Recording + CSV export (FX-018, phase 08) ---------------------------

  async startRecording(request: RecordingStart): Promise<RecordingSession> {
    try {
      return await invokeValidated("start_recording", { request }, isRecordingSession);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async stopRecording(sessionId: string): Promise<void> {
    await invoke("stop_recording", { sessionId });
  },

  async listRecordings(limit?: number): Promise<RecordingSession[]> {
    const raw: unknown = await invoke("list_recordings", { limit });
    if (!Array.isArray(raw)) {
      throw new Error("list_recordings: response failed runtime validation");
    }
    return raw.map((row) => {
      if (!isRecordingSession(row)) {
        throw new Error("list_recordings: session failed runtime validation");
      }
      return row;
    });
  },

  async exportCsv(spec: CsvExportSpec): Promise<CsvExportResult> {
    try {
      return await invokeValidated("export_csv", { spec }, isCsvExportResult);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async exportRecordingCsv(sessionId: string): Promise<CsvExportResult> {
    try {
      return await invokeValidated("export_recording_csv", { sessionId }, isCsvExportResult);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  // ---- Remote server (FX-019, phase 08) ------------------------------------

  async getRemoteStatus(): Promise<RemoteStatus> {
    try {
      return await invokeValidated("get_remote_status", undefined, isRemoteStatus);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async configureRemote(mode: RemoteMode, port: number): Promise<RemoteSetup> {
    try {
      return await invokeValidated("configure_remote", { mode, port }, isRemoteSetup);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async rotateRemoteToken(): Promise<RemoteSetup> {
    try {
      return await invokeValidated("remote_rotate_token", undefined, isRemoteSetup);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async revokeRemoteToken(): Promise<RemoteStatus> {
    try {
      return await invokeValidated("remote_revoke_token", undefined, isRemoteStatus);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  // ---- phase 10: agent-side pairing + console-side hosts ----------------------

  async openPairingWindow(): Promise<PairingWindowInfo> {
    try {
      return await invokeValidated("remote_open_pairing_window", undefined, isPairingWindowInfo);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async closePairingWindow(): Promise<RemoteStatus> {
    try {
      return await invokeValidated("remote_close_pairing_window", undefined, isRemoteStatus);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async revokeRemoteDevice(deviceId: string): Promise<RemoteStatus> {
    try {
      return await invokeValidated(
        "remote_revoke_device",
        { deviceId },
        isRemoteStatus
      );
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async getHostsState(): Promise<HostLiveState[]> {
    try {
      return await invokeValidated("get_hosts_state", undefined, isHostLiveStateList);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  /** The pairing code never leaves this call; the token never reaches the
   * WebView at all (it is stored by Rust in the restricted credential file). */
  async pairHost(endpoint: string, code: string, label: string): Promise<HostLiveState> {
    try {
      return await invokeValidated("pair_host", { endpoint, code, label }, isHostLiveState);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async removeHost(hostId: string, purgeHistory: boolean): Promise<RemoveHostResult> {
    try {
      return await invokeValidated(
        "remove_host",
        { hostId, purgeHistory },
        isRemoveHostResult
      );
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async renameHost(hostId: string, label: string): Promise<HostLiveState> {
    try {
      return await invokeValidated("rename_host", { hostId, label }, isHostLiveState);
    } catch (err) {
      throw toIpcError(err);
    }
  },

  async setHostRecording(hostId: string, enabled: boolean): Promise<HostLiveState> {
    try {
      return await invokeValidated(
        "set_host_recording",
        { hostId, enabled },
        isHostLiveState
      );
    } catch (err) {
      throw toIpcError(err);
    }
  },
};

/** Subscribe to paired-host poll updates (payload: `{ host_id }`). */
export async function onHostsUpdated(
  callback: (hostId: string) => void
): Promise<() => void> {
  const unlisten = await listen<unknown>("hosts-updated", (event) => {
    const payload = event.payload;
    if (
      typeof payload === "object" &&
      payload !== null &&
      typeof (payload as { host_id?: unknown }).host_id === "string"
    ) {
      callback((payload as { host_id: string }).host_id);
    }
  });
  return unlisten;
}

function scanStatusFrom(raw: unknown): ScanStatus {
  if (
    typeof raw === "object" &&
    raw !== null &&
    typeof (raw as ScanStatus).scanId === "string" &&
    typeof (raw as ScanStatus).root === "string" &&
    typeof (raw as ScanStatus).progress === "object"
  ) {
    return raw as ScanStatus;
  }
  throw new Error("start_scan: response failed runtime validation");
}

function scanChildrenFrom(raw: unknown): ScanChildren {
  if (
    typeof raw === "object" &&
    raw !== null &&
    Array.isArray((raw as ScanChildren).children) &&
    typeof (raw as ScanChildren).total_children === "number"
  ) {
    return raw as ScanChildren;
  }
  throw new Error("get_scan_children: response failed runtime validation");
}

/** Subscribe to scheduler tick notifications. Returns an unlisten function. */
export async function onTelemetry(
  callback: (payload: TelemetryEventPayload) => void
): Promise<() => void> {
  const unlisten = await listen<unknown>("telemetry://snapshot", (event) => {
    if (isTelemetryEventPayload(event.payload)) {
      callback(event.payload);
    }
  });
  return unlisten;
}
