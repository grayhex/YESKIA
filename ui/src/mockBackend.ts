import type { Backend } from "./api";
import type {
  Device,
  JournalEntry,
  OperationRequest,
  OperationResult,
  Settings,
  SystemInfo,
} from "./types";

const err = (code: import("./types").ErrorCode, message: string) => ({
  code,
  message,
  retryable: false,
});

/**
 * UI-заглушка для браузера и воспроизводимых тестов. Не имеет отношения к Rust-ядру;
 * `isUiMock = true` выводится в интерфейсе явным баннером.
 */
export class MockBackend implements Backend {
  readonly isUiMock = true;
  private connected = false;
  private settings: Settings = { language: "ru", use_mock_device: true, log_to_file: false };
  private journal: JournalEntry[] = [];
  private seq = 1;
  /** Тестовый хук: следующая запись завершится ошибкой. */
  failNextWrite = false;

  private device(): Device {
    return {
      id: "mock-n95",
      display_name: "Nokia N95 (MOCK)",
      origin: "mock",
      transport: "mock",
      connection: this.connected ? "connected" : "disconnected",
      serial: "***09",
      capabilities: [
        { id: "identify", status: "tested-on-mock", evidence: [] },
        { id: "file_browse", status: "tested-on-mock", evidence: [] },
        { id: "file_write", status: "tested-on-mock", evidence: [] },
        { id: "pim_read", status: "unsupported", evidence: [] },
      ],
    };
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
    return this.device();
  }
  async disconnectDevice() {
    if (!this.connected) throw err("NOT_CONNECTED", "устройство не подключено");
    this.connected = false;
  }
  async runOperation(id: string, request: OperationRequest): Promise<OperationResult> {
    if (!this.connected) throw err("NOT_CONNECTED", "устройство не подключено");
    const operation_id = `op-${this.seq++}`;
    const fail = request.type === "write_file" && this.failNextWrite;
    this.failNextWrite = false;
    const result: OperationResult = {
      operation_id,
      device_id: id,
      operation: request.type,
      origin: "mock",
      outcome: fail ? "failed" : "success",
      data: fail
        ? null
        : request.type === "identify"
          ? { type: "identity", model: "Nokia N95 (MOCK)", firmware: "mock-1.0" }
          : request.type === "list_directory"
            ? { type: "entries", entries: [{ name: "Images", is_dir: true, size: 0 }] }
            : { type: "written", bytes: request.size },
      error: fail
        ? { code: "OPERATION_FAILED", message: "mock: сбой записи", retryable: true }
        : null,
      evidence: [
        {
          kind: "mock_test",
          summary: "Выполнено на mock-устройстве; не подтверждает работу на реальном телефоне",
          reference: null,
        },
      ],
    };
    this.journal.unshift({
      operation_id,
      device_ref: "dev-mock",
      operation: request.type,
      state: fail ? "failed" : "success",
      error_code: fail ? "OPERATION_FAILED" : null,
      started_at_ms: Date.now(),
      finished_at_ms: Date.now(),
    });
    return result;
  }
  async cancelOperation() {
    return false;
  }
  async getSettings() {
    return this.settings;
  }
  async saveSettings(s: Settings) {
    this.settings = s;
    if (!s.use_mock_device) this.connected = false;
    return s;
  }
  async recentOperations(limit: number) {
    return this.journal.slice(0, limit);
  }
}
