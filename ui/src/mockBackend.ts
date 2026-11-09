import type { Backend } from "./api";
import type {
  CapabilityEntry,
  CapabilityMatrix,
  Device,
  DoctorOptions,
  DoctorReport,
  ErrorCode,
  ExportedReport,
  Evidence,
  HostEntry,
  HostRoot,
  JournalEntry,
  Operation,
  OperationRequest,
  OperationResult,
  Progress,
  Settings,
  SystemInfo,
} from "./types";

const err = (code: ErrorCode, message: string) => ({
  code,
  message,
  retryable: false,
  action: "none" as const,
});

const OPERATIONS: Operation[] = [
  "identity.read",
  "files.list",
  "files.read",
  "files.write",
  "contacts.read",
  "contacts.write",
  "sms.read",
  "sms.send",
  "packages.install",
];
const PROBED: Operation[] = ["identity.read", "files.list", "files.read"];
const FP = "mockfingerprint01";

function evidence(source: string, summary: string): Evidence {
  return {
    kind: "mock_test",
    source,
    summary,
    checked_at_ms: Date.now(),
    device_fingerprint: FP,
    reference: null,
  };
}

/** Операция матрицы для запроса (скачивание — это `files.read`, загрузка — `files.write`). */
function matrixOperation(type: string): Operation {
  if (type === "files.download") return "files.read";
  if (type === "files.upload") return "files.write";
  return type as Operation;
}

const HOST_SEP = "\\";
const hostParent = (p: string) => p.slice(0, Math.max(p.lastIndexOf(HOST_SEP), 0)) || p;

interface Pending {
  request: OperationRequest;
  cancelled: boolean;
}

/**
 * UI-заглушка для браузера и воспроизводимых тестов. Не имеет отношения к Rust-ядру;
 * `isUiMock = true` выводится в интерфейсе явным баннером. Повторяет контракт ядра:
 * матрица строится при подключении, запись — «не проверено» до первой записи.
 */
export class MockBackend implements Backend {
  readonly isUiMock = true;
  private connected = false;
  private matrix: CapabilityMatrix | null = null;
  private settings: Settings = {
    language: "ru",
    developer_mode: false,
    use_mock_device: true,
    log_to_file: false,
  };
  private journal: JournalEntry[] = [];
  private seq = 1;
  private pending = new Map<string, Pending>();
  /** Тестовый хук: следующая запись оборвётся (исход неизвестен). */
  failNextWrite = false;
  /** Тестовый хук: операция «выполняется» столько миллисекунд (для отмены/прогресса). */
  holdMs = 0;
  /** Файлы «телефона» (путь устройства → размер) и «компьютера» (путь Windows → размер). */
  deviceFiles: Record<string, number> = {
    "E:/readme.txt": 12,
    "E:/Images/cat.jpg": 4,
    "E:/Images/Фото/снимок.jpg": 2048,
  };
  hostFiles: Record<string, number> = {
    "C:\\Users\\user\\Downloads\\old.txt": 5,
    "C:\\Users\\user\\Documents\\отчёт.docx": 1024,
  };
  hostDirs = new Set<string>([
    "C:\\",
    "C:\\Users",
    "C:\\Users\\user",
    "C:\\Users\\user\\Downloads",
    "C:\\Users\\user\\Documents",
  ]);
  /** Ответы диалогов (тестовый хук); `null` — пользователь отменил. */
  nextFolder: string | null = "C:\\Users\\user\\Documents";
  nextFile: string | null = "C:\\Users\\user\\Documents\\отчёт.docx";
  nextSavePath: string | null = null;
  revealed: string[] = [];

  private device(): Device {
    return {
      id: "mock-n95",
      display_name: "Nokia N95 (MOCK)",
      origin: "mock",
      transport: "mock",
      connection: this.connected ? "connected" : "disconnected",
      serial: "***09",
      identified_model: this.connected ? "Nokia N95 (MOCK)" : null,
      matrix: this.connected ? this.matrix : null,
    };
  }

  private buildMatrix(): CapabilityMatrix {
    const entries: CapabilityEntry[] = OPERATIONS.map((operation) => {
      const implemented = operation === "files.write" || PROBED.includes(operation);
      return {
        operation,
        availability: !implemented
          ? "unsupported"
          : PROBED.includes(operation)
            ? "available"
            : "not-tested",
        maturity: implemented ? "tested-on-mock" : "unsupported",
        detail: null,
        evidence: PROBED.includes(operation)
          ? [evidence("probe", "read-only probe: Available")]
          : [],
      };
    });
    return { device_fingerprint: FP, probed_at_ms: Date.now(), entries };
  }

  async getSystemInfo(): Promise<SystemInfo> {
    return {
      app_version: "0.1.0",
      os: "ui-mock",
      arch: "n/a",
      is_windows: false,
      logical_cpus: 1,
      data_dir: "(ui mock)",
      network_required: false,
    };
  }
  async listDevices() {
    return this.settings.use_mock_device ? [this.device()] : [];
  }
  async connectDevice(id: string) {
    if (id !== "mock-n95" || !this.settings.use_mock_device)
      throw err("DEVICE_NOT_FOUND", "устройство не найдено");
    if (this.connected) throw err("ALREADY_CONNECTED", "устройство уже подключено");
    this.connected = true;
    this.matrix = this.buildMatrix();
    return this.device();
  }
  async disconnectDevice() {
    if (!this.connected) throw err("NOT_CONNECTED", "устройство не подключено");
    this.connected = false;
    this.matrix = null;
    for (const p of this.pending.values()) p.cancelled = true;
  }

  async startOperation(_id: string, request: OperationRequest): Promise<string> {
    if (!this.connected || !this.matrix) throw err("NOT_CONNECTED", "устройство не подключено");
    const op = matrixOperation(request.type);
    const entry = this.matrix.entries.find((e) => e.operation === op);
    if (!entry || entry.availability === "unsupported")
      throw err("CAPABILITY_UNSUPPORTED", `операция не поддерживается: ${op}`);
    const operation_id = `op-${this.seq++}`;
    this.pending.set(operation_id, { request, cancelled: false });
    return operation_id;
  }

  async awaitOperation(operationId: string): Promise<OperationResult> {
    const p = this.pending.get(operationId);
    if (!p) throw err("INVALID_ARGUMENT", "неизвестная операция");
    const started = Date.now();
    while (Date.now() - started < this.holdMs && !p.cancelled) {
      await new Promise((r) => setTimeout(r, 10));
    }
    this.pending.delete(operationId);
    const op = matrixOperation(p.request.type);
    const isWrite = p.request.type === "files.write" || p.request.type === "files.upload";
    const cancelled = p.cancelled;
    // Сбой записи имитируем до применения эффектов: на «устройстве» ничего не появляется.
    const fail = isWrite && this.failNextWrite && !cancelled;
    const fileOp = !cancelled && !fail ? this.fileOperation(p.request) : null;
    if (isWrite) this.failNextWrite = false;
    const ok = !cancelled && !fail && !fileOp?.error;

    if (ok && isWrite && this.matrix) {
      const e = this.matrix.entries.find((x) => x.operation === op)!;
      e.availability = "available";
      e.evidence.push(evidence(`operation:${op}`, "операция выполнена на этом соединении"));
    }
    if (fail && this.matrix) {
      const e = this.matrix.entries.find((x) => x.operation === op)!;
      e.availability = "error";
      e.detail = "mock: обрыв записи";
    }
    const uncertain = isWrite && !ok && !fileOp?.error;
    const result: OperationResult = {
      operation_id: operationId,
      device_id: "mock-n95",
      operation: op,
      origin: "mock",
      outcome: cancelled ? "cancelled" : fail || fileOp?.error ? "failed" : "success",
      data: !ok
        ? null
        : fileOp?.data
          ? fileOp.data
          : p.request.type === "identity.read"
            ? { type: "identity", model: "Nokia N95 (MOCK)", firmware: "mock-1.0" }
            : p.request.type === "files.list"
              ? { type: "entries", entries: [{ name: "Images", is_dir: true, size: 0 }] }
              : p.request.type === "files.write"
                ? {
                    type: "written",
                    bytes: p.request.data.length,
                    sha256: "0".repeat(64),
                    verified: true,
                    replaced: false,
                  }
                : { type: "file_read", bytes: 0, sha256: "0".repeat(64) },
      error: cancelled
        ? {
            code: "OPERATION_CANCELLED",
            message: "операция отменена",
            retryable: true,
            action: uncertain ? "verify_device_contents" : "retry_manually",
          }
        : fileOp?.error
          ? fileOp.error
          : fail
            ? {
                code: "TRANSPORT_ERROR",
                message: "mock: обрыв записи",
                retryable: true,
                action: "verify_device_contents",
              }
            : null,
      uncertain,
      duration_ms: Date.now() - started,
      evidence: [
        evidence(
          `operation:${op}`,
          "Выполнено на mock-устройстве; не подтверждает работу на реальном телефоне",
        ),
      ],
    };
    this.journal.unshift({
      operation_id: operationId,
      device_ref: "dev-mock",
      operation: op,
      state:
        result.outcome === "success"
          ? "success"
          : result.outcome === "cancelled"
            ? "cancelled"
            : "failed",
      error_code: result.error?.code ?? null,
      uncertain,
      started_at_ms: started,
      finished_at_ms: Date.now(),
    });
    return result;
  }

  /** Эффекты файловых операций на деревьях заглушки (листинг, скачивание, загрузка). */
  private fileOperation(
    request: OperationRequest,
  ): { data?: OperationResult["data"]; error?: OperationResult["error"] } | null {
    const fail = (code: ErrorCode, message: string) => ({
      error: {
        code,
        message,
        retryable: code !== "ALREADY_EXISTS",
        action: "choose_another_path" as const,
      },
    });
    if (request.type === "files.list") {
      const prefix = request.path.endsWith("/") ? request.path : `${request.path}/`;
      const seen = new Map<string, { is_dir: boolean; size: number }>();
      for (const [p, size] of Object.entries(this.deviceFiles)) {
        if (!p.startsWith(prefix)) continue;
        const rest = p.slice(prefix.length);
        const slash = rest.indexOf("/");
        if (slash < 0) seen.set(rest, { is_dir: false, size });
        else seen.set(rest.slice(0, slash), { is_dir: true, size: 0 });
      }
      return {
        data: {
          type: "entries",
          entries: [...seen.entries()]
            .map(([name, v]) => ({ name, ...v }))
            .sort(
              (a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name, "ru"),
            ),
        },
      };
    }
    if (request.type === "files.download") {
      const size = this.deviceFiles[request.path];
      if (size === undefined) return fail("OPERATION_FAILED", "файл на устройстве не найден");
      if (!this.hostDirs.has(hostParent(request.dest)))
        return fail("HOST_ACCESS", "папка назначения не найдена");
      const exists = request.dest in this.hostFiles;
      if (exists && request.overwrite === "never")
        return fail("ALREADY_EXISTS", "целевой файл уже существует");
      this.hostFiles[request.dest] = size;
      return {
        data: {
          type: "downloaded",
          bytes: size,
          sha256: "a".repeat(64),
          verified: true,
          replaced: exists,
        },
      };
    }
    if (request.type === "files.upload") {
      const size = this.hostFiles[request.source];
      if (size === undefined) return fail("HOST_ACCESS", "исходный файл не найден");
      const exists = request.path in this.deviceFiles;
      if (exists && request.overwrite === "never")
        return fail("ALREADY_EXISTS", "целевой файл уже существует");
      this.deviceFiles[request.path] = size;
      return {
        data: {
          type: "written",
          bytes: size,
          sha256: "b".repeat(64),
          verified: true,
          replaced: exists,
        },
      };
    }
    return null;
  }

  async hostRoots(): Promise<HostRoot[]> {
    return [
      { label: "C:", path: "C:\\", kind: "drive" },
      { label: "Загрузки", path: "C:\\Users\\user\\Downloads", kind: "folder" },
      { label: "Документы", path: "C:\\Users\\user\\Documents", kind: "folder" },
    ];
  }
  async hostListDir(path: string): Promise<HostEntry[]> {
    if (!this.hostDirs.has(path)) throw err("HOST_ACCESS", "папка не найдена");
    const prefix = path.endsWith(HOST_SEP) ? path : `${path}${HOST_SEP}`;
    const out = new Map<string, HostEntry>();
    const entry = (name: string, is_dir: boolean, size: number): HostEntry => ({
      name,
      is_dir,
      size,
      modified_ms: 1_700_000_000_000,
      hidden: false,
      lossy: false,
    });
    for (const d of this.hostDirs) {
      if (d !== path && d.startsWith(prefix) && !d.slice(prefix.length).includes(HOST_SEP))
        out.set(d.slice(prefix.length), entry(d.slice(prefix.length), true, 0));
    }
    for (const [f, size] of Object.entries(this.hostFiles)) {
      if (f.startsWith(prefix) && !f.slice(prefix.length).includes(HOST_SEP))
        out.set(f.slice(prefix.length), entry(f.slice(prefix.length), false, size));
    }
    return [...out.values()];
  }
  async hostCreateDir(path: string) {
    if (path in this.hostFiles) throw err("ALREADY_EXISTS", "целевой файл уже существует");
    if (!this.hostDirs.has(hostParent(path))) throw err("HOST_ACCESS", "папка не найдена");
    this.hostDirs.add(path);
  }
  async revealInExplorer(path: string) {
    this.revealed.push(path);
  }
  async pickFolder() {
    return this.nextFolder;
  }
  async pickFile() {
    return this.nextFile;
  }
  async pickSavePath(defaultName: string) {
    return this.nextSavePath ?? `C:\\Users\\user\\Downloads\\${defaultName}`;
  }

  async operationProgress(operationId: string): Promise<Progress | null> {
    return this.pending.has(operationId) ? { done: 2, total: 5 } : null;
  }
  async cancelOperation(operationId: string) {
    const p = this.pending.get(operationId);
    if (!p) return false;
    p.cancelled = true;
    return true;
  }
  async runDoctor(options: DoctorOptions): Promise<DoctorReport> {
    // UI-заглушка: вымышленная топология, не результат реальной диагностики.
    return {
      schema: 1,
      generated_at_ms: Date.now(),
      platform: { os: "ui-mock", build: "0", arch: "n/a" },
      devices: [
        {
          alias: "device-1",
          vid: "0421",
          pid: "0223",
          nokia_vendor_hint: true,
          identity_confirmed: false,
          state: options.probe_com_busy ? "com_busy" : "wpd_only",
          protocol: "mtp_responding",
          pc_suite: "not_proven",
          pim: "not_proven",
          driver: {
            class: "USB",
            service: "WinUSB",
            provider: null,
            version: null,
            problem_code: 0,
          },
          wpd: {
            associated_by_instance: true,
            model: null,
            firmware: null,
            storages: 1,
            top_level_objects: 3,
            error_code: null,
          },
          com_ports: options.probe_com_busy ? 1 : 0,
          com_busy: options.probe_com_busy ? 1 : 0,
          volumes: 0,
          findings: ["vendor_hint_only", "wpd_only", "mtp_listing_ok", "mtp_is_not_pc_suite"],
        },
      ],
      findings: [],
      issues: [],
      excluded: ["IMEI", "serial numbers", "PnP instance IDs", "MAC addresses"],
    };
  }
  async exportDoctorReport(options: DoctorOptions): Promise<ExportedReport> {
    return { path: "(ui mock)/doctor.json", report: await this.runDoctor(options) };
  }
  async getSettings() {
    return this.settings;
  }
  async saveSettings(s: Settings) {
    this.settings = s;
    if (!s.use_mock_device) {
      this.connected = false;
      this.matrix = null;
    }
    return s;
  }
  async recentOperations(limit: number) {
    return this.journal.slice(0, limit);
  }
}
