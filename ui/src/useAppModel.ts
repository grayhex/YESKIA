import { useCallback, useEffect, useRef, useState } from "react";
import { toErrorInfo, type Backend } from "./api";
import type {
  Device,
  DoctorOptions,
  DoctorReport,
  ErrorInfo,
  JournalEntry,
  OperationRequest,
  OperationResult,
  Progress,
  Settings,
  SystemInfo,
} from "./types";

const POLL_MS = 3000;

export function useAppModel(backend: Backend) {
  const [system, setSystem] = useState<SystemInfo | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [journal, setJournal] = useState<JournalEntry[]>([]);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [active, setActive] = useState<{
    operationId: string;
    request: OperationRequest;
    progress: Progress | null;
  } | null>(null);
  const [doctorReport, setDoctorReport] = useState<DoctorReport | null>(null);
  const [doctorRunning, setDoctorRunning] = useState(false);
  const [exportedPath, setExportedPath] = useState<string | null>(null);
  const busyRef = useRef(false);
  const sessionStartMs = useRef(Date.now());

  const refresh = useCallback(async () => {
    try {
      setDevices(await backend.listDevices());
      setJournal(await backend.recentOperations(200));
    } catch (e) {
      setError(toErrorInfo(e));
    }
  }, [backend]);

  useEffect(() => {
    backend
      .getSystemInfo()
      .then(setSystem)
      .catch((e) => setError(toErrorInfo(e)));
    backend
      .getSettings()
      .then(setSettings)
      .catch((e) => setError(toErrorInfo(e)));
    void refresh();
  }, [backend, refresh]);

  // Периодический опрос: так замечаем вытащенный кабель.
  useEffect(() => {
    const id = setInterval(() => {
      if (!busyRef.current) void refresh();
    }, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const guarded = useCallback(
    async (fn: () => Promise<void>) => {
      busyRef.current = true;
      setBusy(true);
      setError(null);
      try {
        await fn();
      } catch (e) {
        setError(toErrorInfo(e));
      } finally {
        busyRef.current = false;
        setBusy(false);
        await refresh();
      }
    },
    [refresh],
  );

  return {
    system,
    devices,
    settings,
    journal,
    result,
    error,
    busy,
    /** Операции текущего запуска приложения (журнал в SQLite хранит и прошлые). */
    sessionJournal: journal.filter((j) => j.started_at_ms >= sessionStartMs.current),
    doctorReport,
    doctorRunning,
    exportedPath,
    runDoctor: async (options: DoctorOptions) => {
      setDoctorRunning(true);
      setError(null);
      setExportedPath(null);
      try {
        setDoctorReport(await backend.runDoctor(options));
      } catch (e) {
        setError(toErrorInfo(e));
      } finally {
        setDoctorRunning(false);
      }
    },
    exportReport: async (options: DoctorOptions) => {
      setDoctorRunning(true);
      setError(null);
      try {
        const path = await backend.pickSavePath(`doctor-${Date.now()}.json`);
        if (!path) return;
        const r = await backend.exportDoctorReport(options, path);
        setDoctorReport(r.report);
        setExportedPath(r.path);
      } catch (e) {
        setError(toErrorInfo(e));
      } finally {
        setDoctorRunning(false);
      }
    },
    backend,
    isUiMock: backend.isUiMock,
    refresh: () => guarded(async () => undefined),
    connect: (id: string) => guarded(async () => void (await backend.connectDevice(id))),
    disconnect: (id: string) => guarded(() => backend.disconnectDevice(id)),
    /** Операция в работе: id для отмены и последний прогресс. */
    active,
    run: (id: string, request: OperationRequest) =>
      guarded(async () => {
        const operationId = await backend.startOperation(id, request);
        setActive({ operationId, request, progress: null });
        const poll = setInterval(() => {
          backend
            .operationProgress(operationId)
            .then((progress) =>
              setActive((a) => (a && a.operationId === operationId ? { ...a, progress } : a)),
            )
            .catch(() => undefined);
        }, 150);
        try {
          setResult(await backend.awaitOperation(operationId));
        } finally {
          clearInterval(poll);
          setActive(null);
        }
      }),
    cancelActive: () =>
      active ? backend.cancelOperation(active.operationId) : Promise.resolve(false),
    updateSettings: (patch: Partial<Settings>) =>
      guarded(async () => {
        if (settings) setSettings(await backend.saveSettings({ ...settings, ...patch }));
      }),
  };
}

export type AppModel = ReturnType<typeof useAppModel>;
