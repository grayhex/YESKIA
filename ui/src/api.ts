import { invoke } from "@tauri-apps/api/core";
import type {
  Device,
  ErrorInfo,
  JournalEntry,
  OperationRequest,
  OperationResult,
  Settings,
  SystemInfo,
} from "./types";
import { MockBackend } from "./mockBackend";

/** Строго типизированный контракт команд Tauri. */
export interface Backend {
  /** true — данные не от Rust-ядра, а из UI-заглушки (браузер/тесты). */
  readonly isUiMock: boolean;
  getSystemInfo(): Promise<SystemInfo>;
  listDevices(): Promise<Device[]>;
  connectDevice(id: string): Promise<Device>;
  disconnectDevice(id: string): Promise<void>;
  runOperation(id: string, request: OperationRequest): Promise<OperationResult>;
  cancelOperation(operationId: string): Promise<boolean>;
  getSettings(): Promise<Settings>;
  saveSettings(settings: Settings): Promise<Settings>;
  recentOperations(limit: number): Promise<JournalEntry[]>;
}

/** Ошибки команд приходят как `ErrorInfo`; всё остальное приводим к INTERNAL. */
export function toErrorInfo(e: unknown): ErrorInfo {
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) {
    return e as ErrorInfo;
  }
  return { code: "INTERNAL", message: String(e), retryable: false };
}

export class TauriBackend implements Backend {
  readonly isUiMock = false;
  getSystemInfo = () => invoke<SystemInfo>("get_system_info");
  listDevices = () => invoke<Device[]>("list_devices");
  connectDevice = (id: string) => invoke<Device>("connect_device", { id });
  disconnectDevice = (id: string) => invoke<void>("disconnect_device", { id });
  runOperation = (id: string, request: OperationRequest) =>
    invoke<OperationResult>("run_operation", { id, request });
  cancelOperation = (operationId: string) => invoke<boolean>("cancel_operation", { operationId });
  getSettings = () => invoke<Settings>("get_settings");
  saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });
  recentOperations = (limit: number) => invoke<JournalEntry[]>("recent_operations", { limit });
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function createBackend(): Backend {
  return isTauri() ? new TauriBackend() : new MockBackend();
}
