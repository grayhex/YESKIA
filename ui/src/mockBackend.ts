import type { Backend } from "./api";
import type {
  CapabilityEntry,
  CapabilityMatrix,
  Device,
  ErrorCode,
  Evidence,
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

  private device(): Device {
    return {
      id: "mock-n95",
      display_name: "Nokia N95 (MOCK)",
      origin: "mock",
      transport: "mock",
      connection: this.connected ? "connected" : "disconnected",
      serial: "***09",
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
    const op = request.type as Operation;
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
    const op = p.request.type as Operation;
    const isWrite = p.request.type === "files.write";
    const cancelled = p.cancelled;
    const fail = isWrite && this.failNextWrite && !cancelled;
    if (isWrite) this.failNextWrite = false;
    const ok = !cancelled && !fail;

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
    const uncertain = isWrite && !ok;
    const result: OperationResult = {
      operation_id: operationId,
      device_id: "mock-n95",
      operation: op,
      origin: "mock",
      outcome: cancelled ? "cancelled" : fail ? "failed" : "success",
      data: !ok
        ? null
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

  async operationProgress(operationId: string): Promise<Progress | null> {
    return this.pending.has(operationId) ? { done: 2, total: 5 } : null;
  }
  async cancelOperation(operationId: string) {
    const p = this.pending.get(operationId);
    if (!p) return false;
    p.cancelled = true;
    return true;
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
