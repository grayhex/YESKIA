import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type {
  BackupInfo,
  Device,
  DoctorOptions,
  DoctorReport,
  ErrorInfo,
  ExportedReport,
  HostEntry,
  HostJobRequest,
  HostJobResult,
  HostRoot,
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
  /** Диски и быстрые папки компьютера. */
  hostRoots(): Promise<HostRoot[]>;
  /** Содержимое папки на компьютере (только чтение). */
  hostListDir(path: string): Promise<HostEntry[]>;
  hostCreateDir(path: string): Promise<void>;
  /** «Показать в проводнике». */
  revealInExplorer(path: string): Promise<void>;
  /** Нативные диалоги Windows: выбор папки / файла / пути сохранения. `null` — отмена. */
  pickFolder(): Promise<string | null>;
  pickFile(): Promise<string | null>;
  pickSavePath(defaultName: string): Promise<string | null>;
  /** Резервные копии в хранилище. */
  backupList(): Promise<BackupInfo[]>;
  /** Задания без устройства: копия папки компьютера, проверка, восстановление в папку. */
  startHostJob(request: HostJobRequest): Promise<string>;
  awaitHostJob(jobId: string): Promise<HostJobResult>;
  hostJobProgress(jobId: string): Promise<Progress | null>;
  cancelHostJob(jobId: string): Promise<boolean>;
  /** Сохраняет JSON-отчёт в каталог данных приложения; возвращает путь. */
  saveReport(name: string, json: string): Promise<string>;
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
  hostRoots = () => invoke<HostRoot[]>("host_roots");
  hostListDir = (path: string) => invoke<HostEntry[]>("host_list_dir", { path });
  hostCreateDir = (path: string) => invoke<void>("host_create_dir", { path });
  revealInExplorer = (path: string) => invoke<void>("reveal_in_explorer", { path });
  pickFolder = async () => {
    const r = await openDialog({ directory: true, multiple: false });
    return typeof r === "string" ? r : null;
  };
  pickFile = async () => {
    const r = await openDialog({ directory: false, multiple: false });
    return typeof r === "string" ? r : null;
  };
  pickSavePath = async (defaultName: string) =>
    (await saveDialog({ defaultPath: defaultName })) ?? null;
  backupList = () => invoke<BackupInfo[]>("backup_list");
  startHostJob = (request: HostJobRequest) => invoke<string>("start_host_job", { request });
  awaitHostJob = (jobId: string) => invoke<HostJobResult>("await_host_job", { jobId });
  hostJobProgress = (jobId: string) => invoke<Progress | null>("host_job_progress", { jobId });
  cancelHostJob = (jobId: string) => invoke<boolean>("cancel_host_job", { jobId });
  saveReport = (name: string, json: string) => invoke<string>("save_report", { name, json });
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
