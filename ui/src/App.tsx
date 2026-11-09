import { useCallback, useEffect, useState } from "react";
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

const REQUESTS: { label: string; request: OperationRequest }[] = [
  { label: "Идентификация", request: { type: "identify" } },
  { label: "Список файлов", request: { type: "list_directory", path: "E:/" } },
  { label: "Тестовая запись", request: { type: "write_file", path: "E:/yeskia.txt", size: 12 } },
];

export function App({ backend }: { backend: Backend }) {
  const [system, setSystem] = useState<SystemInfo | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [journal, setJournal] = useState<JournalEntry[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setDevices(await backend.listDevices());
      setJournal(await backend.recentOperations(10));
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

  async function guarded(fn: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
      await refresh();
    }
  }

  const toggleMock = (value: boolean) =>
    guarded(async () => {
      if (settings)
        setSettings(await backend.saveSettings({ ...settings, use_mock_device: value }));
    });

  return (
    <main>
      <h1>YESKIA</h1>
      {backend.isUiMock && (
        <p role="alert" className="banner">
          UI-заглушка: данные не от Rust-ядра (запуск вне Tauri).
        </p>
      )}

      <section aria-label="Система">
        <h2>Система</h2>
        {system ? (
          <dl>
            <dt>Версия</dt>
            <dd>{system.app_version}</dd>
            <dt>ОС</dt>
            <dd data-testid="os">
              {system.os} / {system.arch}
            </dd>
            <dt>Данные</dt>
            <dd>{system.data_dir}</dd>
            <dt>Сеть</dt>
            <dd>{system.network_required ? "требуется" : "не требуется"}</dd>
          </dl>
        ) : (
          <p>Загрузка…</p>
        )}
      </section>

      <section aria-label="Настройки">
        <label>
          <input
            type="checkbox"
            checked={settings?.use_mock_device ?? false}
            disabled={!settings || busy}
            onChange={(e) => toggleMock(e.target.checked)}
          />{" "}
          Показывать mock-устройство
        </label>
      </section>

      <section aria-label="Устройства">
        <h2>Устройства</h2>
        <button onClick={() => guarded(async () => void 0)} disabled={busy}>
          Обновить
        </button>
        {devices.length === 0 && <p>Устройства не найдены.</p>}
        <ul>
          {devices.map((d) => (
            <li key={d.id}>
              <strong>{d.display_name}</strong>{" "}
              {d.origin === "mock" && <span className="tag">MOCK</span>}{" "}
              <span>{d.connection === "connected" ? "подключено" : "отключено"}</span>
              {d.serial && <span> · s/n {d.serial}</span>}
              <ul>
                {d.capabilities.map((c) => (
                  <li key={c.id}>
                    {c.id}: {c.status}
                  </li>
                ))}
              </ul>
              {d.connection === "connected" ? (
                <>
                  {REQUESTS.map((r) => (
                    <button
                      key={r.label}
                      disabled={busy}
                      onClick={() =>
                        guarded(async () => setResult(await backend.runOperation(d.id, r.request)))
                      }
                    >
                      {r.label}
                    </button>
                  ))}
                  <button
                    disabled={busy}
                    onClick={() => guarded(() => backend.disconnectDevice(d.id))}
                  >
                    Отключить
                  </button>
                </>
              ) : (
                <button
                  disabled={busy}
                  onClick={() => guarded(async () => void (await backend.connectDevice(d.id)))}
                >
                  Подключить
                </button>
              )}
            </li>
          ))}
        </ul>
      </section>

      {error && (
        <p role="alert" className="error" data-testid="error">
          {error.code}: {error.message}
          {error.retryable ? " (можно повторить вручную)" : ""}
        </p>
      )}

      {result && (
        <section aria-label="Результат">
          <h2>Последняя операция</h2>
          <p data-testid="outcome">
            {result.operation_id} · {result.operation} · {result.outcome}
            {result.origin === "mock" && " · MOCK"}
          </p>
          {result.error && (
            <p>
              {result.error.code}: {result.error.message}
            </p>
          )}
          {result.evidence.map((e, i) => (
            <p key={i} className="evidence">
              {e.summary}
            </p>
          ))}
          <pre>{JSON.stringify(result.data, null, 2)}</pre>
        </section>
      )}

      <section aria-label="Журнал">
        <h2>Журнал операций</h2>
        <ul>
          {journal.map((j) => (
            <li key={j.operation_id}>
              {j.operation_id} {j.operation} — {j.state}
              {j.error_code ? ` (${j.error_code})` : ""}
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
