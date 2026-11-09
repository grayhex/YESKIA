import { useCallback, useEffect, useRef, useState } from "react";
import { toErrorInfo, type Backend } from "./api";
import type {
  Device,
  ErrorInfo,
  JournalEntry,
  OperationRequest,
  OperationResult,
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
  const busyRef = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setDevices(await backend.listDevices());
      setJournal(await backend.recentOperations(50));
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
    isUiMock: backend.isUiMock,
    refresh: () => guarded(async () => undefined),
    connect: (id: string) => guarded(async () => void (await backend.connectDevice(id))),
    disconnect: (id: string) => guarded(() => backend.disconnectDevice(id)),
    run: (id: string, request: OperationRequest) =>
      guarded(async () => setResult(await backend.runOperation(id, request))),
    updateSettings: (patch: Partial<Settings>) =>
      guarded(async () => {
        if (settings) setSettings(await backend.saveSettings({ ...settings, ...patch }));
      }),
  };
}

export type AppModel = ReturnType<typeof useAppModel>;
