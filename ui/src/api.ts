import { invoke } from "@tauri-apps/api/core";
import type {
  Device,
  DoctorOptions,
  DoctorReport,
  ErrorInfo,
  ExportedReport,
  JournalEntry,
  OperationRequest,
  OperationResult,
  Progress,
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
  /** Ставит операцию в очередь устройства; возвращает её id. */
  startOperation(id: string, request: OperationRequest): Promise<string>;
  /** Ждёт результата (очередь → выполнение). Ошибки операции приходят внутри результата. */
  awaitOperation(operationId: string): Promise<OperationResult>;
  operationProgress(operationId: string): Promise<Progress | null>;
  cancelOperation(operationId: string): Promise<boolean>;
  /** Read-only диагностика USB/PnP/WPD/COM; отчёт без IMEI/серийников/instance id. */
  runDoctor(options: DoctorOptions): Promise<DoctorReport>;
  exportDoctorReport(options: DoctorOptions): Promise<ExportedReport>;
  getSettings(): Promise<Settings>;
  saveSettings(settings: Settings): Promise<Settings>;
  recentOperations(limit: number): Promise<JournalEntry[]>;
}

/** Ошибки команд приходят как `ErrorInfo`; всё остальное приводим к INTERNAL. */
export function toErrorInfo(e: unknown): ErrorInfo {
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) {
    return e as ErrorInfo;
  }
  return { code: "INTERNAL", message: String(e), retryable: false, action: "none" };
}

export class TauriBackend implements Backend {
  readonly isUiMock = false;
  getSystemInfo = () => invoke<SystemInfo>("get_system_info");
  listDevices = () => invoke<Device[]>("list_devices");
  connectDevice = (id: string) => invoke<Device>("connect_device", { id });
  disconnectDevice = (id: string) => invoke<void>("disconnect_device", { id });
  startOperation = (id: string, request: OperationRequest) =>
    invoke<string>("start_operation", { id, request });
  awaitOperation = (operationId: string) =>
    invoke<OperationResult>("await_operation", { operationId });
  operationProgress = (operationId: string) =>
    invoke<Progress | null>("operation_progress", { operationId });
  cancelOperation = (operationId: string) => invoke<boolean>("cancel_operation", { operationId });
  runDoctor = (options: DoctorOptions) => invoke<DoctorReport>("run_doctor", { options });
  exportDoctorReport = (options: DoctorOptions) =>
    invoke<ExportedReport>("export_doctor_report", { options });
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
