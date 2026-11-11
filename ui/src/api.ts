import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type {
  AudioPreview,
  BackupInfo,
  BitmapPreview,
  Device,
  DoctorOptions,
  DoctorReport,
  ErrorInfo,
  ExportedReport,
  HostEntry,
  HostJobRequest,
  HostJobResult,
  HostRoot,
  InspectRecommendation,
  InspectReport,
  InspectTarget,
  JournalEntry,
  LibraryItem,
  LibraryQuery,
  MediaInfo,
  PackageSet,
  Provenance,
  OperationRequest,
  OperationResult,
  Progress,
  Settings,
  SystemInfo,
  ThemeReport,
  ThemeSource,
  ToolInfo,
  UpdateCheck,
} from "./types";
import { MockBackend } from "./mockBackend";

/** Строго типизированный контракт команд Tauri. */
export interface Backend {
  /** true — данные не от Rust-ядра, а из UI-заглушки (браузер/тесты). */
  readonly isUiMock: boolean;
  agentCancelConnect(id: string): Promise<void>;
  agentSetLab(id: string, armed: boolean, acknowledge: boolean): Promise<void>;
  agentPackageInfo(path: string): Promise<import("./types").AgentPackage>;
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
  cancelPersonalRead(): Promise<void>;
  inspectCertificate(path: string): Promise<import("./types").CertInfo>;
  readPersonalData(deviceId: string, domain: string): Promise<Record<string, unknown>[]>;
  savePersonalSnapshot(path: string, data: string): Promise<void>;
  installPhoneCertificate(
    deviceId: string,
    path: string,
    fingerprint: string,
    confirm: boolean,
  ): Promise<void>;
  openLatestRelease(): Promise<void>;
  exportDoctorReport(options: DoctorOptions, path: string): Promise<ExportedReport>;
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
  /** Обновление из файла: приложение ничего не скачивает. */
  updateCheck(installer: string, manifest: string, signature: string | null): Promise<UpdateCheck>;
  /** Причины, по которым обновление сейчас откладывается (операции с телефоном, задания записи). */
  updateBlockers(): Promise<string[]>;
  updateLaunch(
    installer: string,
    manifest: string,
    signature: string | null,
    confirm: boolean,
    acceptUnsigned: boolean,
  ): Promise<void>;
  /** Выбор любого локального файла (аудио, видео, картинка, тема) нативным диалогом. */
  pickMediaFile(): Promise<string | null>;
  /** FFmpeg: версия и лицензия; без инструмента — ошибка TOOL_MISSING. */
  mediaToolInfo(): Promise<ToolInfo>;
  mediaProbe(path: string): Promise<MediaInfo>;
  mediaWaveform(path: string, buckets: number): Promise<number[]>;
  mediaPreview(path: string): Promise<AudioPreview>;
  themeInspect(source: ThemeSource, model: string | null): Promise<ThemeReport>;
  themePreview(source: ThemeSource, limit: number): Promise<BitmapPreview[]>;
  /** Выбор файлов пакетов (SIS/SISX/JAR/JAD) нативным диалогом. */
  pickPackageFiles(): Promise<string[]>;
  /** Библиотека пакетов: локальная, офлайн. */
  libraryList(query: LibraryQuery): Promise<LibraryItem[]>;
  libraryImport(paths: string[], provenance: Provenance): Promise<LibraryItem[]>;
  libraryRemove(id: string): Promise<void>;
  libraryInspect(id: string, target: InspectTarget, at?: number): Promise<InspectReport>;
  librarySets(): Promise<PackageSet[]>;
  libraryCreateSet(name: string): Promise<PackageSet>;
  libraryDeleteSet(id: number): Promise<void>;
  librarySetAdd(set: number, pkg: string): Promise<void>;
  librarySetRemove(set: number, pkg: string): Promise<void>;
  packageDiagnose(message: string): Promise<InspectRecommendation>;
  /** «Передать на телефон»: запись файла пакета (не установка). Возвращает id операции. */
  startPackageTransfer(deviceId: string, pkg: string, dir: string): Promise<string>;
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
  agentCancelConnect = (id: string) => invoke<void>("agent_cancel_connect", { id });
  agentSetLab = (id: string, armed: boolean, acknowledge: boolean) =>
    invoke<void>("agent_set_lab", { id, armed, acknowledge });
  agentPackageInfo = (path: string) =>
    invoke<import("./types").AgentPackage>("agent_package_info", { path });
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
  cancelPersonalRead = () => invoke<void>("cancel_personal_read");
  inspectCertificate = (path: string) =>
    invoke<import("./types").CertInfo>("inspect_certificate", { path });
  readPersonalData = (deviceId: string, domain: string) =>
    invoke<Record<string, unknown>[]>("read_personal_data", { deviceId, domain });
  installPhoneCertificate = (
    deviceId: string,
    path: string,
    fingerprint: string,
    confirm: boolean,
  ) => invoke<void>("install_phone_certificate", { deviceId, path, fingerprint, confirm });
  savePersonalSnapshot = (path: string, data: string) =>
    invoke<void>("save_personal_snapshot", { path, data });
  openLatestRelease = () => invoke<void>("open_latest_release");
  exportDoctorReport = (options: DoctorOptions, path: string) =>
    invoke<ExportedReport>("export_doctor_report", { options, path });
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
  updateCheck = (installer: string, manifest: string, signature: string | null) =>
    invoke<UpdateCheck>("update_check", { installer, manifest, signature });
  updateBlockers = () => invoke<string[]>("update_blockers");
  updateLaunch = (
    installer: string,
    manifest: string,
    signature: string | null,
    confirm: boolean,
    acceptUnsigned: boolean,
  ) =>
    invoke<void>("update_launch", {
      installer,
      manifest,
      signature,
      confirm,
      acceptUnsigned,
    });
  pickMediaFile = async () => {
    const r = await openDialog({ directory: false, multiple: false });
    return typeof r === "string" ? r : null;
  };
  mediaToolInfo = () => invoke<ToolInfo>("media_tool_info");
  mediaProbe = (path: string) => invoke<MediaInfo>("media_probe", { path });
  mediaWaveform = (path: string, buckets: number) =>
    invoke<number[]>("media_waveform", { path, buckets });
  mediaPreview = (path: string) => invoke<AudioPreview>("media_preview", { path });
  themeInspect = (source: ThemeSource, model: string | null) =>
    invoke<ThemeReport>("theme_inspect", { source, model });
  themePreview = (source: ThemeSource, limit: number) =>
    invoke<BitmapPreview[]>("theme_preview", { source, limit });
  pickPackageFiles = async () => {
    const r = await openDialog({
      directory: false,
      multiple: true,
      filters: [{ name: "Symbian / Java ME", extensions: ["sis", "sisx", "jar", "jad"] }],
    });
    return Array.isArray(r) ? r : typeof r === "string" ? [r] : [];
  };
  libraryList = (query: LibraryQuery) => invoke<LibraryItem[]>("library_list", { query });
  libraryImport = (paths: string[], provenance: Provenance) =>
    invoke<LibraryItem[]>("library_import", { paths, provenance });
  libraryRemove = (id: string) => invoke<void>("library_remove", { id });
  libraryInspect = (id: string, target: InspectTarget, at?: number) =>
    invoke<InspectReport>("library_inspect", { id, target, at: at ?? null });
  librarySets = () => invoke<PackageSet[]>("library_sets");
  libraryCreateSet = (name: string) => invoke<PackageSet>("library_create_set", { name });
  libraryDeleteSet = (id: number) => invoke<void>("library_delete_set", { id });
  librarySetAdd = (set: number, pkg: string) =>
    invoke<void>("library_set_add", { set, package: pkg });
  librarySetRemove = (set: number, pkg: string) =>
    invoke<void>("library_set_remove", { set, package: pkg });
  packageDiagnose = (message: string) =>
    invoke<InspectRecommendation>("package_diagnose", { message });
  startPackageTransfer = (deviceId: string, pkg: string, dir: string) =>
    invoke<string>("start_package_transfer", { id: deviceId, package: pkg, dir });
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
