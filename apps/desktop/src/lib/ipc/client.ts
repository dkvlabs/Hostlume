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
  isSettings,
  isSystemSnapshot,
  isTelemetryEventPayload,
} from "./guards";
import type {
  CapabilityReport,
  CommandErrorPayload,
  DiagnosticsReport,
  Settings,
  SettingsPatch,
  SystemSnapshot,
  TelemetryEventPayload,
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
};

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
