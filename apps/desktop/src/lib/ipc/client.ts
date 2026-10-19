/**
 * The single typed IPC adapter (architecture.md §3: lib/ipc, one adapter).
 * Every payload is runtime-validated before leaving this module; callers
 * can trust the types.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isCapabilityReport,
  isDiagnosticsReport,
  isHistoryResponse,
  isProcessActionResult,
  isProcessPage,
  isSettings,
  isSystemSnapshot,
  isTelemetryEventPayload,
} from "./guards";
import type {
  CapabilityReport,
  CommandErrorPayload,
  DiagnosticsReport,
  HistoryResponse,
  HistoryQuerySpec,
  ProcessActionRequest,
  ProcessActionResult,
  ProcessPage,
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
};

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
