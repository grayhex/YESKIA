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
  | "HOST_ACCESS"
  | "BACKUP_INVALID"
  | "LIMIT_EXCEEDED"
  | "TOOL_MISSING"
  | "INSUFFICIENT_SPACE"
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
  | "packages.install"
  | "packages.transfer";

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
  transport: "mock" | "usb" | "mass_storage" | "mtp" | "pc_suite" | "bluetooth";
  connection: ConnectionState;
  serial: string | null; // уже замаскирован ядром
  /** Модель, подтверждённая идентификацией по протоколу на этом соединении. */
  identified_model: string | null;
  matrix: CapabilityMatrix | null;
}

export type OverwritePolicy = "never" | "replace";

export type OperationRequest =
  | { type: "identity.read" }
  | { type: "files.list"; path: string }
  | { type: "files.read"; path: string }
  | { type: "files.write"; path: string; data: number[]; overwrite: OverwritePolicy }
  | { type: "files.download"; path: string; dest: string; overwrite: OverwritePolicy }
  | { type: "files.upload"; source: string; path: string; overwrite: OverwritePolicy }
  | { type: "backup.create"; path: string; dest: string; label: string; exclude: string[] }
  | { type: "backup.restore"; backup: string; path: string; only: string | null; apply: boolean }
  | { type: "packages.transfer"; source: string; path: string; sha256: string }
  | { type: "contacts.read" }
  | { type: "sms.read" };

export type OperationData =
  | { type: "identity"; model: string; firmware: string | null }
  | { type: "entries"; entries: { name: string; is_dir: boolean; size: number }[] }
  | { type: "file_read"; bytes: number; sha256: string }
  | { type: "written"; bytes: number; sha256: string; verified: boolean; replaced: boolean }
  | { type: "downloaded"; bytes: number; sha256: string; verified: boolean; replaced: boolean }
  | {
      type: "backup_created";
      files: number;
      bytes: number;
      skipped: number;
      complete: boolean;
      verified: boolean;
    }
  | { type: "backup_plan"; counts: PlanCounts; items: PlanItem[]; truncated: boolean }
  | {
      type: "backup_applied";
      written: number;
      identical: number;
      conflicts: number;
      blocked: number;
      raced: number;
      bytes: number;
    }
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
  /** Папка хранилища резервных копий; `null` — по умолчанию в каталоге данных. */
  backup_root: string | null;
  /** Папка с ffmpeg и ffprobe; null — искать рядом с приложением и в PATH. */
  ffmpeg_dir: string | null;
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

export type DoctorState =
  | "driver_missing"
  | "driver_error"
  | "com_busy"
  | "com_present"
  | "wpd_only"
  | "mass_storage"
  | "unknown_mode";

export type ProtocolStatus = "not_probed" | "mtp_responding" | "mtp_failed";

export type FindingCode =
  | "no_usb_devices"
  | "multiple_devices"
  | "not_nokia_vendor"
  | "vendor_hint_only"
  | "driver_missing"
  | "driver_error"
  | "com_present"
  | "com_busy"
  | "wpd_only"
  | "mass_storage_only"
  | "unknown_mode"
  | "mtp_listing_ok"
  | "mtp_listing_failed"
  | "mtp_is_not_pc_suite"
  | "partial_data";

export interface DoctorDevice {
  alias: string;
  vid: string;
  pid: string;
  nokia_vendor_hint: boolean;
  identity_confirmed: boolean;
  state: DoctorState;
  protocol: ProtocolStatus;
  pc_suite: "not_proven";
  pim: "not_proven";
  driver: {
    class: string | null;
    service: string | null;
    provider: string | null;
    version: string | null;
    problem_code: number;
  };
  wpd: {
    associated_by_instance: boolean;
    model: string | null;
    firmware: string | null;
    storages: number | null;
    top_level_objects: number | null;
    error_code: number | null;
  } | null;
  com_ports: number;
  com_busy: number;
  volumes: number;
  findings: FindingCode[];
}

export interface DoctorReport {
  schema: number;
  generated_at_ms: number;
  platform: { os: string; build: string; arch: string };
  devices: DoctorDevice[];
  findings: FindingCode[];
  issues: { stage: string; code: number | null }[];
  excluded: string[];
}

export interface DoctorOptions {
  probe_com_busy: boolean;
}

export interface ExportedReport {
  path: string;
  report: DoctorReport;
}

export interface HostEntry {
  name: string;
  is_dir: boolean;
  size: number;
  modified_ms: number | null;
  hidden: boolean;
  lossy: boolean;
}

export interface HostRoot {
  label: string;
  path: string;
  kind: "drive" | "folder";
}

export interface BackupInfo {
  name: string;
  label: string;
  created_at_ms: number;
  total_files: number;
  total_bytes: number;
  complete: boolean;
  interrupted: boolean;
  source_kind: string;
  manifest_ok: boolean;
}

export type PlanStatus = "new" | "identical" | "conflict" | "blocked";

export interface PlanItem {
  path: string;
  size: number;
  status: PlanStatus;
  reason: string | null;
}

export interface PlanCounts {
  new: number;
  identical: number;
  conflict: number;
  blocked: number;
}

export interface VerifyReport {
  ok: boolean;
  manifest_ok: boolean;
  files_checked: number;
  missing: string[];
  corrupt: string[];
  extra: string[];
  unsafe_paths: string[];
}

export interface ApplyReport {
  written: number;
  identical: number;
  conflicts: number;
  blocked: number;
  raced: number;
  bytes: number;
}

export type HostJobRequest =
  | { type: "backup.create_local"; source: string; label: string; exclude: string[] }
  | { type: "backup.verify"; name: string }
  | {
      type: "backup.restore_local";
      name: string;
      target: string;
      only: string | null;
      apply: boolean;
    }
  | { type: "media.ringtone"; source: string; dest: string; options: RingtoneOptions }
  | { type: "media.prepare"; source: string; dest: string; preset: string; model: string };

export type HostJobData =
  | {
      type: "backup_created";
      name: string;
      files: number;
      bytes: number;
      skipped: number;
      complete: boolean;
      verified: boolean;
    }
  | { type: "backup_verified"; report: VerifyReport }
  | { type: "backup_plan"; counts: PlanCounts; items: PlanItem[]; truncated: boolean }
  | { type: "backup_applied"; report: ApplyReport }
  | { type: "media_generated"; result: Generated };

export interface HostJobResult {
  job_id: string;
  outcome: "success" | "failed" | "cancelled";
  data: HostJobData | null;
  error: ErrorInfo | null;
}

// ---------- библиотека пакетов (issue #8) ----------

export interface Provenance {
  source: string;
  url: string;
  license: string;
  /** «user-confirmed» — пользователь подтвердил право хранить и передавать файл; приложение это не проверяет. */
  rights: "unknown" | "user-confirmed";
  note: string;
}

export type PackageFormat = "sis" | "sisx" | "jar" | "jad";

export interface LibraryItem {
  /** SHA-256 содержимого (hex). */
  id: string;
  name: string;
  format: PackageFormat;
  size: number;
  added_ms: number;
  status: "ok" | "corrupt" | "unreadable";
  error: string | null;
  uid: string | null;
  title: string | null;
  version: string | null;
  vendor: string | null;
  signed: boolean | null;
  platform_uids: string[];
  provenance: Provenance;
  /** Заявленная пакетом платформа относительно выбранной модели; не проверка установки. */
  fit: "declared" | "not-declared" | "unknown" | null;
}

export interface LibraryQuery {
  text: string;
  format: PackageFormat | null;
  model: string | null;
  only_fitting: boolean;
  set: number | null;
}

export interface PackageSet {
  id: number;
  name: string;
  items: string[];
}

export interface InspectTarget {
  model: string;
  rm: string;
  firmware: string;
  os: string;
}

export interface InspectRecommendation {
  issue: string;
  advice: string;
}

export interface CertInfo {
  subject: string;
  issuer: string;
  serial: string;
  sha256: string;
  not_before: string;
  not_after: string;
  validity: "valid" | "expired" | "not-yet-valid";
  key_type: string;
  self_issued: boolean;
}

export interface InspectReport {
  schema: string;
  name: string;
  format: PackageFormat;
  sha256: string;
  size: number;
  sis: {
    uid3: number;
    package: { uid: number; names: string[]; vendor_unique_name: string; version: number[] | null };
    files: { target: string; length: number; hash_ok: boolean | null }[];
    signed: boolean;
    signature_count: number;
  } | null;
  midlet: { manifest: Record<string, string>; midp: string; cldc: string } | null;
  descriptor: { descriptor: Record<string, string>; paired: boolean } | null;
  integrity: {
    uid_checksum_ok: boolean;
    package_uid_matches_header: boolean;
    controller_crc_ok: boolean | null;
    data_crc_ok: boolean | null;
    payload_hashes_ok: boolean | null;
  } | null;
  signatures: {
    status: "unsigned" | "valid" | "invalid" | "unsupported";
    trust: string;
    revocation: string;
    phone_constraints: string;
    at: string;
    chains: { status: string; certificates: CertInfo[]; root_signature: string }[];
  } | null;
  target: InspectTarget;
  dependencies: {
    platforms: { uid: string; label: string; relation: string }[];
    platform_verdict: string;
    packages: { uid: string | null; name: string; status: string }[];
    verdict: string;
    imports: string;
  } | null;
  binaries: {
    target: string;
    cpu: string;
    kernel: string;
    capabilities: string[] | null;
    restricted_capabilities: string[] | null;
  }[];
  capabilities: string[];
  issues: string[];
  compatibility: "compatible" | "incompatible" | "unknown";
  compatibility_scope: string;
  emulator: string;
  installation: string;
  physical_device: string;
  limitations: string[];
  recommendations: InspectRecommendation[];
}

// ---------- медиа и темы (issue #9) ----------

export interface ToolInfo {
  ffmpeg_path: string;
  ffprobe_path: string;
  version: string;
  license: "lgpl" | "lgpl-v3" | "gpl" | "gpl-v3" | "nonfree" | "unknown";
  redistributable: boolean;
  encoders: string[];
}

export interface StreamInfo {
  codec: string | null;
  width: number | null;
  height: number | null;
  sample_rate: number | null;
  channels: number | null;
}

export interface MediaInfo {
  kind: "audio" | "video" | "image";
  duration: number;
  container: string | null;
  video: StreamInfo | null;
  audio: StreamInfo | null;
  size: number;
}

export interface RingtoneOptions {
  start: number;
  end: number;
  fade_in: number;
  fade_out: number;
  mono: boolean;
  normalize: boolean;
  lufs: number;
  bitrate: number;
  sample_rate: number;
}

/** Файл создан на компьютере и перечитан. Передача, установка и проверка на телефоне — отдельные шаги. */
export interface Generated {
  path: string;
  bytes: number;
  sha256: string;
  duration: number;
  description: string;
  hardware: "not-tested";
}

export interface AudioPreview {
  mime: string;
  base64: string;
}

export type ThemeSource = { kind: "path"; path: string } | { kind: "library"; id: string };

export interface ThemeCheck {
  id: string;
  status: "pass" | "fail" | "warn" | "unknown" | "skip";
  detail: string;
}

export interface ThemeReport {
  schema: string;
  name: string;
  bitmaps: { width: number; height: number; bpp: number; compression: number }[];
  icons: number;
  skn_pid1: number | null;
  package: {
    source: "pkg" | "sis" | "none";
    name: string;
    uid: number | null;
    vendor: string;
    platform_deps: number[];
    package_deps: number[];
    targets: string[];
  };
  signed: boolean | null;
  uid_class: string | null;
  checks: ThemeCheck[];
  structure: "consistent" | "broken";
  firmware_acceptance: "unknown";
  installation: "not-tested";
  physical_device: "not-tested";
  palette: { hex: string; share: number; contrast_on_white: number; contrast_on_black: number }[];
  limitations: string[];
}

export interface BitmapPreview {
  index: number;
  width: number;
  height: number;
  bpp: number;
  png_base64: string;
  masked: boolean;
}
