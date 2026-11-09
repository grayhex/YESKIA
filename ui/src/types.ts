// Зеркало DTO из yeskia-core (serde). Любое изменение — синхронно с Rust.

export type ErrorCode =
  | "DEVICE_NOT_FOUND"
  | "DEVICE_DISCONNECTED"
  | "ALREADY_CONNECTED"
  | "NOT_CONNECTED"
  | "OPERATION_CANCELLED"
  | "OPERATION_FAILED"
  | "QUEUE_FULL"
  | "CAPABILITY_UNSUPPORTED"
  | "TIMEOUT"
  | "FINGERPRINT_MISMATCH"
  | "ALREADY_EXISTS"
  | "UNSAFE_PATH"
  | "VERIFICATION_FAILED"
  | "TRANSPORT_ERROR"
  | "STORAGE_ERROR"
  | "INVALID_ARGUMENT"
  | "INTERNAL";

export type UserAction =
  | "none"
  | "reconnect"
  | "check_cable"
  | "choose_another_path"
  | "wait_and_retry"
  | "retry_manually"
  | "verify_device_contents";

export interface ErrorInfo {
  code: ErrorCode;
  message: string;
  retryable: boolean;
  action: UserAction;
}

export type SupportStatus =
  "unsupported" | "implemented" | "tested-on-mock" | "tested-on-host" | "verified-on-device";

export type Operation =
  | "identity.read"
  | "files.list"
  | "files.read"
  | "files.write"
  | "contacts.read"
  | "contacts.write"
  | "sms.read"
  | "sms.send"
  | "packages.install";

export type Availability = "available" | "not-tested" | "unsupported" | "error";

export interface Evidence {
  kind: "mock_test" | "host_test" | "device_run" | "trace";
  source: string;
  summary: string;
  checked_at_ms: number;
  device_fingerprint: string | null;
  reference: string | null;
}

export interface CapabilityEntry {
  operation: Operation;
  availability: Availability;
  maturity: SupportStatus;
  detail: string | null;
  evidence: Evidence[];
}

/** Матрица конкретного соединения; есть только у подключённого устройства. */
export interface CapabilityMatrix {
  device_fingerprint: string;
  probed_at_ms: number;
  entries: CapabilityEntry[];
}

export type ConnectionState = "disconnected" | "connecting" | "connected" | "disconnecting";

export interface Device {
  id: string;
  display_name: string;
  origin: "mock" | "real";
  transport: "mock" | "mass_storage" | "mtp" | "pc_suite" | "bluetooth";
  connection: ConnectionState;
  serial: string | null; // уже замаскирован ядром
  matrix: CapabilityMatrix | null;
}

export type OverwritePolicy = "never" | "replace";

export type OperationRequest =
  | { type: "identity.read" }
  | { type: "files.list"; path: string }
  | { type: "files.read"; path: string }
  | { type: "files.write"; path: string; data: number[]; overwrite: OverwritePolicy }
  | { type: "contacts.read" }
  | { type: "sms.read" };

export type OperationData =
  | { type: "identity"; model: string; firmware: string | null }
  | { type: "entries"; entries: { name: string; is_dir: boolean; size: number }[] }
  | { type: "file_read"; bytes: number; sha256: string }
  | { type: "written"; bytes: number; sha256: string; verified: boolean; replaced: boolean }
  | { type: "pim_count"; count: number };

export interface OperationResult {
  operation_id: string;
  device_id: string;
  operation: Operation;
  origin: "mock" | "real";
  outcome: "success" | "failed" | "cancelled";
  data: OperationData | null;
  error: ErrorInfo | null;
  /** Исход записи на устройстве неизвестен — сначала проверить содержимое. */
  uncertain: boolean;
  duration_ms: number;
  evidence: Evidence[];
}

export interface Progress {
  done: number;
  total: number;
}

export interface SystemInfo {
  app_version: string;
  os: string;
  arch: string;
  is_windows: boolean;
  logical_cpus: number;
  data_dir: string;
  network_required: boolean;
}

export interface Settings {
  language: string;
  developer_mode: boolean;
  use_mock_device: boolean;
  log_to_file: boolean;
}

export interface JournalEntry {
  operation_id: string;
  device_ref: string;
  operation: string;
  state: "running" | "success" | "failed" | "cancelled" | "interrupted";
  error_code: string | null;
  uncertain: boolean;
  started_at_ms: number;
  finished_at_ms: number | null;
}
