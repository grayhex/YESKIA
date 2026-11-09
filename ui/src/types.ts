// Зеркало DTO из yeskia-core (serde). Любое изменение — синхронно с Rust.

export type ErrorCode =
  | "DEVICE_NOT_FOUND"
  | "DEVICE_DISCONNECTED"
  | "ALREADY_CONNECTED"
  | "NOT_CONNECTED"
  | "OPERATION_CANCELLED"
  | "OPERATION_FAILED"
  | "OPERATION_IN_PROGRESS"
  | "CAPABILITY_UNSUPPORTED"
  | "TRANSPORT_ERROR"
  | "STORAGE_ERROR"
  | "INVALID_ARGUMENT"
  | "INTERNAL";

export interface ErrorInfo {
  code: ErrorCode;
  message: string;
  retryable: boolean;
}

export type SupportStatus =
  "unsupported" | "implemented" | "tested-on-mock" | "tested-on-host" | "verified-on-device";

export type CapabilityId =
  | "identify"
  | "file_browse"
  | "file_read"
  | "file_write"
  | "backup"
  | "pim_read"
  | "package_install";

export interface Evidence {
  kind: "mock_test" | "host_test" | "device_run" | "trace";
  summary: string;
  reference: string | null;
}

export interface Capability {
  id: CapabilityId;
  status: SupportStatus;
  evidence: Evidence[];
}

export type ConnectionState = "disconnected" | "connecting" | "connected" | "disconnecting";

export interface Device {
  id: string;
  display_name: string;
  origin: "mock" | "real";
  transport: "mock" | "mass_storage" | "mtp" | "pc_suite" | "bluetooth";
  connection: ConnectionState;
  capabilities: Capability[];
  serial: string | null; // уже замаскирован ядром
}

export type OperationRequest =
  | { type: "identify" }
  | { type: "list_directory"; path: string }
  | { type: "write_file"; path: string; size: number };

export type OperationData =
  | { type: "identity"; model: string; firmware: string | null }
  | { type: "entries"; entries: { name: string; is_dir: boolean; size: number }[] }
  | { type: "written"; bytes: number };

export interface OperationResult {
  operation_id: string;
  device_id: string;
  operation: string;
  origin: "mock" | "real";
  outcome: "success" | "failed" | "cancelled";
  data: OperationData | null;
  error: ErrorInfo | null;
  evidence: Evidence[];
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
  use_mock_device: boolean;
  log_to_file: boolean;
}

export interface JournalEntry {
  operation_id: string;
  device_ref: string;
  operation: string;
  state: "running" | "success" | "failed" | "cancelled" | "interrupted";
  error_code: string | null;
  started_at_ms: number;
  finished_at_ms: number | null;
}
