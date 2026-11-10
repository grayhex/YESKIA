import type { Backend } from "./api";
import type {
  AudioPreview,
  BitmapPreview,
  CapabilityEntry,
  CapabilityMatrix,
  BackupInfo,
  Device,
  DoctorOptions,
  DoctorReport,
  ErrorCode,
  ExportedReport,
  Evidence,
  HostEntry,
  HostJobData,
  HostJobRequest,
  HostJobResult,
  HostRoot,
  InspectRecommendation,
  InspectReport,
  InspectTarget,
  LibraryItem,
  LibraryQuery,
  MediaInfo,
  PackageSet,
  PlanItem,
  Provenance,
  JournalEntry,
  Operation,
  OperationRequest,
  OperationResult,
  Progress,
  Settings,
  SystemInfo,
  ThemeReport,
  ThemeSource,
  ToolInfo,
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
  "packages.transfer",
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
function matrixOperation(type: string, apply = false): Operation {
  if (type === "backup.restore") return apply ? "files.write" : "files.read";
  if (type === "files.download") return "files.read";
  if (type === "files.upload") return "files.write";
  if (type === "backup.create") return "files.read";
  if (type === "backup.restore") return "files.read"; // с apply = true — files.write (см. ниже)
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
    backup_root: null,
    ffmpeg_dir: null,
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
  /** Резервные копии «хранилища»: список и содержимое (путь внутри копии → размер). */
  backups: BackupInfo[] = [];
  backupFiles: Record<string, Record<string, number>> = {};
  /** Копии, которые verify считает повреждёнными (тестовый хук). */
  corruptBackups = new Set<string>();
  /** FFmpeg «установлен» (тестовый хук); false → TOOL_MISSING. */
  ffmpegAvailable = true;
  nextMediaFile: string | null = null;
  /** Содержимое «файлов» компьютера для проверки: путь → описание. */
  mediaFiles: Record<string, MediaInfo> = {};
  /** Отчёты тем (тестовый хук): ключ — путь или id библиотеки. */
  themeSamples: Record<string, ThemeReport> = {};
  /** Библиотека пакетов (UI-заглушка): ключ — «SHA-256». */
  library = new Map<string, LibraryItem>();
  packageSets: PackageSet[] = [];
  private setSeq = 1;
  /** Ответ диалога выбора файлов пакетов и «содержимое» файлов на компьютере (тестовые хуки). */
  nextPackageFiles: string[] = [];
  packageSamples: Record<
    string,
    Partial<LibraryItem> & { size: number; platform_uids?: string[] }
  > = {};
  private jobs = new Map<string, { request: HostJobRequest; cancelled: boolean }>();
  private jobSeq = 1;

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
      const implemented =
        operation === "files.write" ||
        operation === "packages.transfer" ||
        PROBED.includes(operation);
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
    const op = matrixOperation(request.type, request.type === "backup.restore" && request.apply);
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
    const req = p.request;
    const op = matrixOperation(req.type, req.type === "backup.restore" && req.apply);
    const isWrite =
      req.type === "files.write" ||
      req.type === "files.upload" ||
      req.type === "packages.transfer" ||
      (req.type === "backup.restore" && req.apply);
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
    if (request.type === "backup.create") {
      const prefix = request.path.endsWith("/") ? request.path : `${request.path}/`;
      const files: Record<string, number> = {};
      for (const [p, size] of Object.entries(this.deviceFiles)) {
        if (p.startsWith(prefix)) files[p.slice(prefix.length)] = size;
      }
      this.newBackup(request.label, files, "device");
      return {
        data: {
          type: "backup_created",
          files: Object.keys(files).length,
          bytes: Object.values(files).reduce((a, b) => a + b, 0),
          skipped: 0,
          complete: true,
          verified: true,
        },
      };
    }
    if (request.type === "backup.restore") {
      if (!this.backupFiles[request.backup]) return fail("BACKUP_INVALID", "копия не найдена");
      const prefix = request.path.endsWith("/") ? request.path : `${request.path}/`;
      const plan = this.plan(
        request.backup,
        request.only,
        (rel) => this.deviceFiles[`${prefix}${rel}`],
      );
      if (!request.apply)
        return {
          data: { type: "backup_plan", counts: plan.counts, items: plan.items, truncated: false },
        };
      for (const it of plan.items.filter((i) => i.status === "new"))
        this.deviceFiles[`${prefix}${it.path}`] = it.size;
      return {
        data: {
          type: "backup_applied",
          written: plan.counts.new,
          identical: plan.counts.identical,
          conflicts: plan.counts.conflict,
          blocked: plan.counts.blocked,
          raced: 0,
          bytes: 0,
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
    if (request.type === "packages.transfer") {
      const item = this.library.get(request.sha256);
      if (!item || item.status !== "ok")
        return fail("INVALID_ARGUMENT", "пакет не прошёл проверку целостности");
      if (request.path in this.deviceFiles)
        return fail("ALREADY_EXISTS", "целевой файл уже существует");
      this.deviceFiles[request.path] = item.size;
      return {
        data: {
          type: "written",
          bytes: item.size,
          sha256: item.id,
          verified: true,
          replaced: false,
        },
      };
    }
    return null;
  }

  /** Тестовый хук: добавляет копию в «хранилище». */
  newBackupForTest(label: string, files: Record<string, number>): string {
    return this.newBackup(label, files, "folder");
  }

  private newBackup(label: string, files: Record<string, number>, kind: string): string {
    const name = `20261109-120000${label ? `-${label.replace(/[^\p{L}\p{N}_-]+/gu, "-")}` : ""}-${this.backups.length + 1}`;
    const total = Object.values(files).reduce((a, b) => a + b, 0);
    this.backups.unshift({
      name,
      label,
      created_at_ms: 1_700_000_000_000 + this.backups.length,
      total_files: Object.keys(files).length,
      total_bytes: total,
      complete: true,
      interrupted: false,
      source_kind: kind,
      manifest_ok: true,
    });
    this.backupFiles[name] = files;
    return name;
  }

  private plan(
    name: string,
    only: string | null,
    existing: (rel: string) => number | undefined,
  ): {
    items: PlanItem[];
    counts: { new: number; identical: number; conflict: number; blocked: number };
  } {
    const counts = { new: 0, identical: 0, conflict: 0, blocked: 0 };
    const items: PlanItem[] = [];
    for (const [path, size] of Object.entries(this.backupFiles[name] ?? {})) {
      if (only && path !== only && !path.startsWith(`${only}/`)) continue;
      const have = existing(path);
      const status = have === undefined ? "new" : have === size ? "identical" : "conflict";
      counts[status]++;
      items.push({
        path,
        size,
        status,
        reason: status === "conflict" ? "содержимое отличается" : null,
      });
    }
    return { items, counts };
  }

  async backupList() {
    return this.backups.map((b) => ({ ...b }));
  }

  async startHostJob(request: HostJobRequest) {
    if (request.type === "media.ringtone" || request.type === "media.prepare") {
      if (!this.ffmpegAvailable) throw err("TOOL_MISSING", "FFmpeg не найден");
    } else if (request.type !== "backup.create_local" && !this.backupFiles[request.name])
      throw err("UNSAFE_PATH", "недопустимое имя копии");
    const id = `job-${this.jobSeq++}`;
    this.jobs.set(id, { request, cancelled: false });
    return id;
  }

  async awaitHostJob(jobId: string): Promise<HostJobResult> {
    const job = this.jobs.get(jobId);
    if (!job) throw err("INVALID_ARGUMENT", "неизвестное задание");
    const started = Date.now();
    while (Date.now() - started < this.holdMs && !job.cancelled) {
      await new Promise((r) => setTimeout(r, 10));
    }
    this.jobs.delete(jobId);
    if (job.cancelled)
      return {
        job_id: jobId,
        outcome: "cancelled",
        data: null,
        error: {
          code: "OPERATION_CANCELLED",
          message: "операция отменена",
          retryable: true,
          action: "retry_manually",
        },
      };
    const r = job.request;
    let data: HostJobData;
    if (r.type === "media.ringtone" || r.type === "media.prepare") {
      const fail = (code: ErrorCode, message: string): HostJobResult => ({
        job_id: jobId,
        outcome: "failed",
        data: null,
        error: { code, message, retryable: false, action: "none" },
      });
      if (!(r.source in this.mediaFiles) && !(r.source in this.hostFiles))
        return fail("HOST_ACCESS", "файл недоступен");
      if (r.dest in this.hostFiles) return fail("ALREADY_EXISTS", "целевой файл уже существует");
      const duration = r.type === "media.ringtone" ? r.options.end - r.options.start : 0;
      this.hostFiles[r.dest] = 4096;
      data = {
        type: "media_generated",
        result: {
          path: r.dest,
          bytes: 4096,
          sha256: "c".repeat(64),
          duration,
          description: r.type === "media.ringtone" ? "MP3 · моно" : `пресет ${r.preset}`,
          hardware: "not-tested",
        },
      };
    } else if (r.type === "backup.create_local") {
      const prefix = r.source.endsWith(HOST_SEP) ? r.source : `${r.source}${HOST_SEP}`;
      const files: Record<string, number> = {};
      for (const [p, size] of Object.entries(this.hostFiles)) {
        if (p.startsWith(prefix)) files[p.slice(prefix.length).split(HOST_SEP).join("/")] = size;
      }
      const name = this.newBackup(r.label, files, "folder");
      data = {
        type: "backup_created",
        name,
        files: Object.keys(files).length,
        bytes: Object.values(files).reduce((a, b) => a + b, 0),
        skipped: 0,
        complete: true,
        verified: true,
      };
    } else if (r.type === "backup.verify") {
      const bad = this.corruptBackups.has(r.name);
      const first = Object.keys(this.backupFiles[r.name] ?? {})[0];
      data = {
        type: "backup_verified",
        report: {
          ok: !bad,
          manifest_ok: true,
          files_checked: Object.keys(this.backupFiles[r.name] ?? {}).length,
          missing: [],
          corrupt: bad && first ? [first] : [],
          extra: [],
          unsafe_paths: [],
        },
      };
    } else {
      const prefix = r.target.endsWith(HOST_SEP) ? r.target : `${r.target}${HOST_SEP}`;
      const hostPath = (rel: string) => `${prefix}${rel.split("/").join(HOST_SEP)}`;
      const plan = this.plan(r.name, r.only, (rel) => this.hostFiles[hostPath(rel)]);
      if (r.apply) {
        if (this.corruptBackups.has(r.name))
          return {
            job_id: jobId,
            outcome: "failed",
            data: null,
            error: {
              code: "BACKUP_INVALID",
              message: "копия не прошла проверку; восстановление отменено",
              retryable: false,
              action: "none",
            },
          };
        let written = 0;
        for (const it of plan.items.filter((i) => i.status === "new")) {
          this.hostFiles[hostPath(it.path)] = it.size;
          written++;
        }
        data = {
          type: "backup_applied",
          report: {
            written,
            identical: plan.counts.identical,
            conflicts: plan.counts.conflict,
            blocked: plan.counts.blocked,
            raced: 0,
            bytes: 0,
          },
        };
      } else {
        data = { type: "backup_plan", counts: plan.counts, items: plan.items, truncated: false };
      }
    }
    return { job_id: jobId, outcome: "success", data, error: null };
  }

  async hostJobProgress(jobId: string): Promise<Progress | null> {
    return this.jobs.has(jobId) ? { done: 1, total: 4 } : null;
  }

  async cancelHostJob(jobId: string) {
    const j = this.jobs.get(jobId);
    if (!j) return false;
    j.cancelled = true;
    return true;
  }

  savedReports: { name: string; json: string }[] = [];
  async pickMediaFile() {
    return this.nextMediaFile;
  }
  async mediaToolInfo(): Promise<ToolInfo> {
    if (!this.ffmpegAvailable) throw err("TOOL_MISSING", "FFmpeg не найден");
    return {
      ffmpeg_path: "C:\\Tools\\ffmpeg\\ffmpeg.exe",
      ffprobe_path: "C:\\Tools\\ffmpeg\\ffprobe.exe",
      version: "6.1-ui-mock",
      license: "lgpl",
      redistributable: true,
      encoders: ["libmp3lame", "aac"],
    };
  }
  async mediaProbe(path: string): Promise<MediaInfo> {
    if (!this.ffmpegAvailable) throw err("TOOL_MISSING", "FFmpeg не найден");
    const m = this.mediaFiles[path];
    if (!m)
      throw err("INVALID_ARGUMENT", "файл не удалось прочитать как аудио, видео или картинку");
    return m;
  }
  async mediaWaveform(_path: string, buckets: number) {
    return Array.from({ length: buckets }, (_, i) => Math.abs(Math.sin(i / 7)) * 0.8);
  }
  async mediaPreview(): Promise<AudioPreview> {
    return { mime: "audio/mpeg", base64: "AAAA" };
  }
  async themeInspect(source: ThemeSource): Promise<ThemeReport> {
    const key = source.kind === "path" ? source.path : source.id;
    const r = this.themeSamples[key];
    if (!r) throw err("INVALID_ARGUMENT", "в папке нет файла .skn");
    return r;
  }
  async themePreview(_source: ThemeSource, limit: number): Promise<BitmapPreview[]> {
    const px =
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    return Array.from({ length: Math.min(limit, 3) }, (_, index) => ({
      index,
      width: 240,
      height: 320,
      bpp: 16,
      png_base64: px,
      masked: index === 1,
    }));
  }
  async pickPackageFiles() {
    return [...this.nextPackageFiles];
  }
  async libraryList(q: LibraryQuery) {
    const text = q.text.trim().toLowerCase();
    const members = q.set === null ? null : this.packageSets.find((x) => x.id === q.set)?.items;
    return [...this.library.values()]
      .map((it) => ({ ...it, fit: this.fitFor(it, q.model) }))
      .filter(
        (it) =>
          (!q.format || it.format === q.format) &&
          (!members || members.includes(it.id)) &&
          (!q.only_fitting || it.fit === "declared") &&
          (!text ||
            `${it.name} ${it.title ?? ""} ${it.vendor ?? ""} ${it.uid ?? ""}`
              .toLowerCase()
              .includes(text)),
      )
      .sort((a, b) => b.added_ms - a.added_ms);
  }
  private fitFor(it: LibraryItem, model: string | null): LibraryItem["fit"] {
    if (!model) return null;
    if (model === "nokia-7610" || it.platform_uids.length === 0) return "unknown";
    const known: Record<string, string[]> = {
      "nokia-e52": ["0x101f7961", "0x102032be", "0x102752ae"],
      "nokia-n95": ["0x101f7961", "0x102032be"],
      "nokia-n97-mini": ["0x1028315f"],
    };
    return it.platform_uids.some((u) => (known[model] ?? []).includes(u))
      ? "declared"
      : "not-declared";
  }
  async libraryImport(paths: string[], provenance: Provenance) {
    const out: LibraryItem[] = [];
    for (const path of paths) {
      const sample = this.packageSamples[path];
      const size = sample?.size ?? this.hostFiles[path];
      if (size === undefined) throw err("HOST_ACCESS", "файл недоступен");
      const name = path.split(/[\\/]/).pop()!;
      const ext = name.split(".").pop()!.toLowerCase();
      if (!["sis", "sisx", "jar", "jad"].includes(ext))
        throw err("INVALID_ARGUMENT", "поддерживаются файлы .sis, .sisx, .jar, .jad");
      const id = (sample?.id ?? `${name}:${size}`)
        .split("")
        .reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7)
        .toString(16)
        .padStart(8, "0")
        .repeat(8);
      const item = {
        id,
        name,
        format: ext as LibraryItem["format"],
        added_ms: 1_700_000_000_000 + this.library.size,
        status: "ok",
        error: null,
        uid: null,
        title: name.replace(/\.[^.]+$/, ""),
        version: null,
        vendor: null,
        signed: ext === "sis" || ext === "sisx" ? false : null,
        platform_uids: [],
        provenance: { ...provenance, rights: provenance.rights },
        fit: null,
        ...sample,
        size,
      } as LibraryItem;
      this.library.set(id, item);
      out.push(item);
    }
    return out;
  }
  async libraryRemove(id: string) {
    this.library.delete(id);
    for (const s of this.packageSets) s.items = s.items.filter((x) => x !== id);
  }
  async libraryInspect(id: string, target: InspectTarget): Promise<InspectReport> {
    const it = this.library.get(id);
    if (!it) throw err("INVALID_ARGUMENT", "пакета нет в библиотеке");
    const sis = it.format === "sis" || it.format === "sisx";
    const issues = [
      ...(sis && it.signed === false ? ["unsigned-package"] : []),
      ...(it.status === "corrupt" ? ["corrupt-package"] : []),
      ...(sis ? ["platform-undeclared"] : []),
    ];
    return {
      schema: "yeskia.inspector/1",
      name: it.name,
      format: it.format,
      sha256: it.id,
      size: it.size,
      sis: sis
        ? {
            uid3: 0,
            package: {
              uid: 0,
              names: [it.title ?? it.name],
              vendor_unique_name: it.vendor ?? "",
              version: [1, 0, 0],
            },
            files: [],
            signed: !!it.signed,
            signature_count: it.signed ? 1 : 0,
          }
        : null,
      midlet: null,
      descriptor: null,
      integrity: sis
        ? {
            uid_checksum_ok: true,
            package_uid_matches_header: true,
            controller_crc_ok: it.status !== "corrupt",
            data_crc_ok: it.status !== "corrupt",
            payload_hashes_ok: it.status !== "corrupt",
          }
        : null,
      signatures: sis
        ? {
            status: it.signed ? "valid" : "unsigned",
            trust: "unknown",
            revocation: "not-checked",
            phone_constraints: "not-verified",
            at: "2026-11-10T00:00:00+00:00",
            chains: [],
          }
        : null,
      target,
      dependencies: sis
        ? {
            platforms: [],
            platform_verdict: "unknown",
            packages: [],
            verdict: "no-declared-dependencies",
            imports: "not-decoded",
          }
        : null,
      binaries: [],
      capabilities: [],
      issues,
      compatibility: it.status === "corrupt" ? "incompatible" : "unknown",
      compatibility_scope:
        "только статический анализ; установка, доверие сертификатам телефона и запуск не установлены",
      emulator: "not-tested",
      installation: "not-tested",
      physical_device: "not-tested",
      limitations: ["UI-заглушка: отчёт не от Rust-ядра."],
      recommendations: issues.map((issue) => ({ issue, advice: `Совет для ${issue}` })),
    };
  }
  async librarySets() {
    return this.packageSets.map((s) => ({ ...s, items: [...s.items] }));
  }
  async libraryCreateSet(name: string) {
    const n = name.trim();
    if (!n) throw err("INVALID_ARGUMENT", "название набора не может быть пустым");
    if (this.packageSets.some((s) => s.name === n))
      throw err("ALREADY_EXISTS", "набор с таким названием уже есть");
    const set = { id: this.setSeq++, name: n, items: [] as string[] };
    this.packageSets.push(set);
    return { ...set };
  }
  async libraryDeleteSet(id: number) {
    this.packageSets = this.packageSets.filter((s) => s.id !== id);
  }
  async librarySetAdd(set: number, pkg: string) {
    const s = this.packageSets.find((x) => x.id === set);
    if (!s || !this.library.has(pkg))
      throw err("INVALID_ARGUMENT", "набора или пакета не существует");
    if (!s.items.includes(pkg)) s.items.push(pkg);
  }
  async librarySetRemove(set: number, pkg: string) {
    const s = this.packageSets.find((x) => x.id === set);
    if (s) s.items = s.items.filter((x) => x !== pkg);
  }
  async packageDiagnose(message: string): Promise<InspectRecommendation> {
    const m = message.toLowerCase();
    const issue = m.includes("expired") ? "expired-certificate" : "unknown";
    return { issue, advice: `Совет для ${issue}` };
  }
  async startPackageTransfer(deviceId: string, pkg: string, dir: string) {
    const it = this.library.get(pkg);
    if (!it) throw err("INVALID_ARGUMENT", "пакета нет в библиотеке");
    if (it.status !== "ok")
      throw err("INVALID_ARGUMENT", "передавать можно только пакеты, прошедшие проверку");
    return this.startOperation(deviceId, {
      type: "packages.transfer",
      source: `C:\\staging\\${it.name}`,
      path: `${dir.replace(/\/+$/, "")}/${it.name}`,
      sha256: it.id,
    });
  }
  async saveReport(name: string, json: string) {
    this.savedReports.push({ name, json });
    return `C:\\Users\\user\\AppData\\Roaming\\YESKIA\\reports\\${name}.json`;
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
