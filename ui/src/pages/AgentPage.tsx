import { useEffect, useRef, useState } from "react";
import { toErrorInfo } from "../api";
import { ErrorMessage, Panel, StateMessage } from "../components/Ui";
import { Dialog } from "../fm/Dialog";
import { useI18n } from "../i18n";
import type { AppModel } from "../useAppModel";
import type {
  AgentDocument,
  AgentQuery,
  AgentPackage,
  AgentReply,
  ErrorInfo,
  OperationRequest,
  OperationResult,
  Progress,
} from "../types";

type Pending = {
  id: string;
  stamp: number;
  name: string;
  request: OperationRequest;
  operation: string;
  package?: AgentPackage;
};
type History = {
  command: string;
  reply?: AgentReply;
  document?: AgentDocument;
  error?: string;
  uncertain: boolean;
};
const examples = [
  "pwd",
  "ls C:\\Data",
  "stat C:\\Data",
  "hal",
  "ps 0xE0AA00F2 1",
  "cenrep 0x101f8766 1",
  "find C:\\Data test",
  "grep C:\\Data hex:ff01",
  "cat C:\\Data\\_logs\\rshelld.txt",
  "exec C:\\sys\\bin\\test.exe",
  "rm C:\\Data\\test.txt",
  "reboot now",
  "quit",
];

export function AgentPage({ model }: { model: AppModel }) {
  const { lang } = useI18n();
  const c = (ru: string, en: string) => (lang === "ru" ? ru : en);
  const { backend } = model;
  const candidates = model.devices.filter((d) => d.id.startsWith("adbian-"));
  const [chosen, setChosen] = useState("");
  const device =
    candidates.find((d) => d.id === chosen) ??
    candidates.find((d) => d.connection === "connected") ??
    candidates[0];
  const connected = device?.connection === "connected";
  const id = connected ? device.id : null;
  const stamp = device?.matrix?.probed_at_ms ?? 0;
  const developer = model.settings?.developer_mode ?? false;
  const [labView, setLabView] = useState(false);
  const [ack, setAck] = useState(false);
  const [armed, setArmed] = useState(false);
  const [line, setLine] = useState("pwd");
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [notice, setNotice] = useState("");
  const [pending, setPending] = useState<Pending | null>(null);
  const [history, setHistory] = useState<History[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [busy, setBusy] = useState(false);
  const [staged, setStaged] = useState<AgentPackage | null>(null);
  const [installedOnPhone, setInstalledOnPhone] = useState(false);
  const epoch = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    epoch.current++;
    setArmed(false);
    setAck(false);
    setPending(null);
    setHistory([]);
    setNotice("");
    setStaged(null);
    setInstalledOnPhone(false);
    return () => {
      if (id) void backend.agentSetLab(id, false, false).catch(() => undefined);
    };
  }, [backend, id, stamp, developer, labView]);
  useEffect(() => {
    if (!developer) setLabView(false);
  }, [developer]);
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => {
      void backend
        .operationProgress(active)
        .then(setProgress)
        .catch(() => undefined);
    }, 200);
    return () => clearInterval(timer);
  }, [active, backend]);

  async function operation(target: string, request: OperationRequest): Promise<OperationResult> {
    const operationId = await backend.startOperation(target, request);
    if (mounted.current) {
      setActive(operationId);
      setProgress(null);
    }
    try {
      const result = await backend.awaitOperation(operationId);
      if (result.outcome !== "success")
        throw Object.assign(new Error(result.error?.message ?? result.outcome), result.error, {
          uncertain: result.uncertain,
          data: result.data,
        });
      return result;
    } finally {
      if (mounted.current) setActive(null);
    }
  }
  async function run(
    target: string,
    request: OperationRequest,
    label: string,
    expectedEpoch = epoch.current,
  ) {
    setBusy(true);
    setError(null);
    setNotice("");
    try {
      const result = await operation(target, request);
      if (!mounted.current || expectedEpoch !== epoch.current) return;
      const reply = result.data?.type === "agent_reply" ? result.data.reply : undefined;
      const document = result.data?.type === "agent_document" ? result.data.response : undefined;
      setHistory((h) => [...h.slice(-29), { command: label, reply, document, uncertain: false }]);
      if (request.type === "agent.ping")
        setNotice(
          c(
            "Агент отвечает. Версия базового ADBian неизвестна; установка и автозапуск не подтверждены.",
            "Agent responds. Base ADBian version is unknown; installation and autostart are unverified.",
          ),
        );
      await model.refresh();
    } catch (e) {
      if (!mounted.current || expectedEpoch !== epoch.current) return;
      const info = toErrorInfo(e);
      const remote =
        e && typeof e === "object" && "data" in e ? (e.data as OperationResult["data"]) : null;
      setError(info);
      const uncertain = !!(e && typeof e === "object" && "uncertain" in e && e.uncertain);
      setHistory((h) => [
        ...h.slice(-29),
        {
          command: label,
          reply: remote?.type === "agent_reply" ? remote.reply : undefined,
          document: remote?.type === "agent_document" ? remote.response : undefined,
          error: info.message,
          uncertain,
        },
      ]);
      if (uncertain)
        setNotice(
          c(
            "Исход операции неизвестен. Проверьте телефон перед ручным повтором.",
            "Outcome is uncertain. Check the phone before retrying manually.",
          ),
        );
      await model.refresh();
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function toggleLab() {
    if (!id) return;
    setError(null);
    try {
      await backend.agentSetLab(id, !armed, ack);
      setArmed(!armed);
    } catch (e) {
      setError(toErrorInfo(e));
    }
  }
  function submitCommand() {
    if (!id || !device || !armed || !line.trim()) return;
    const command = line.trim();
    const changing = ["rm", "exec", "reboot", "mkdir", "quit"].includes(command.split(/\s/)[0]);
    const request: OperationRequest = { type: "agent.command", line: command, confirm: changing };
    if (changing) setPending({ id, stamp, name: device.display_name, request, operation: command });
    else void run(id, request, command);
  }
  async function choosePackage() {
    if (!id || !device) return;
    setError(null);
    try {
      const source = (await backend.pickPackageFiles())[0];
      if (!source) return;
      const pkg = await backend.agentPackageInfo(source);
      setPending({
        id,
        stamp,
        name: device.display_name,
        operation: `Sideload → C:/Data/_app_install/${pkg.name}`,
        package: pkg,
        request: {
          type: "packages.transfer",
          source,
          path: `C:/Data/_app_install/${pkg.name}`,
          sha256: pkg.sha256,
        },
      });
    } catch (e) {
      setError(toErrorInfo(e));
    }
  }
  function queryHelper(query: AgentQuery) {
    if (!id || !device || !armed) return;
    setPending({
      id,
      stamp,
      name: device.display_name,
      operation: `YESKIA ${query.command}\nput C:/Data/_yeskia/request.json\nexec C:/sys/bin/yeskiaagent.exe`,
      request: { type: "agent.query", query, confirm: true },
    });
  }
  async function choosePut() {
    if (!id || !device || !armed) return;
    const source = await backend.pickFile();
    if (!source) return;
    const name = source.split(/[\\/]/).pop() ?? "file.bin";
    const path = `C:/Data/${name}`;
    setPending({
      id,
      stamp,
      name: device.display_name,
      operation: `put ${source} → ${path}`,
      request: { type: "agent.put", source, path, confirm: true },
    });
  }
  async function confirmPending() {
    const p = pending;
    setPending(null);
    if (!p || p.id !== id || p.stamp !== stamp) return;
    if (!p.package) {
      void run(p.id, p.request, p.operation);
      return;
    }
    const currentEpoch = epoch.current;
    setBusy(true);
    setError(null);
    try {
      await operation(p.id, { type: "files.mkdir", path: "C:/Data/_app_install" });
      await operation(p.id, p.request);
      if (currentEpoch === epoch.current) {
        setStaged(p.package);
        setInstalledOnPhone(false);
        setNotice(
          c(
            "Готово к установке на телефоне. Установка не запускалась.",
            "Ready to install on phone. Installation has not been started.",
          ),
        );
      }
    } catch (e) {
      setError(toErrorInfo(e));
      setNotice(
        c(
          "Передача не подтверждена. Проверьте файл на телефоне; автоматического повтора нет.",
          "Transfer is unverified. Check the phone file; no automatic retry.",
        ),
      );
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function exportHistory() {
    try {
      const path = await backend.pickSavePath("agent-session-private.json");
      if (path)
        await backend.savePersonalSnapshot(
          path,
          JSON.stringify({ schema: "yeskia.agent.private-session/1", history }, null, 2),
        );
    } catch (e) {
      setError(toErrorInfo(e));
    }
  }

  return (
    <div className="workspace-page agent-workspace">
      <div className="page-heading">
        <h1>{c("Агент на телефоне", "Phone agent")}</h1>
        {developer && (
          <div className="btn-row">
            <button
              className="btn btn-secondary"
              aria-pressed={!labView}
              onClick={() => setLabView(false)}
            >
              {c("Настройка агента", "Agent setup")}
            </button>
            <button
              className="btn btn-secondary"
              aria-pressed={labView}
              onClick={() => setLabView(true)}
            >
              Device Lab
            </button>
          </div>
        )}
      </div>
      <div className="workspace-scroll agent-scroll">
        <p>
          {c(
            "Bluetooth-подключение к ADBian на вашем S60. USB остаётся доступен для других сервисов.",
            "Connect to ADBian on your S60 over Bluetooth. USB remains available for other services.",
          )}
        </p>
        {error && <ErrorMessage error={error} developer={developer} />}
        {notice && (
          <p role="status" className="banner">
            {notice}
          </p>
        )}
        {(!developer || !labView) && (
          <div className="agent-grid">
            <Panel title={c("1. Подготовка телефона", "1. Prepare your phone")}>
              <ol className="agent-steps">
                <li>
                  {c(
                    "Соберите или получите проверенный SIS агента. Сохраните предыдущий пакет и доступные пользовательские файлы.",
                    "Build or obtain a verified agent SIS. Keep the previous package and accessible user files.",
                  )}
                </li>
                <li>
                  {c(
                    "Передайте SIS через доступный USB, карту памяти или локальную загрузку. Установите на экране телефона.",
                    "Copy the SIS using available USB, a memory card or a local download. Install on the phone screen.",
                  )}
                </li>
                <li>
                  {c(
                    "Сопрягите телефон в настройках Bluetooth Windows. Запустите rshelld на телефоне и проверьте подключение.",
                    "Pair in Windows Bluetooth settings. Start rshelld on the phone and check the connection.",
                  )}
                </li>
              </ol>
              <p className="hint">
                {c(
                  "На заводской прошивке подпись и привилегии могут блокировать агент. YESKIA не меняет прошивку и не устанавливает агент автоматически.",
                  "Stock firmware signing and capability restrictions can block the agent. YESKIA does not modify firmware or install the agent automatically.",
                )}
              </p>
            </Panel>
            <Panel title={c("2. Подключение и проверка", "2. Connect and check")}>
              {candidates.length === 0 ? (
                <StateMessage
                  kind="empty"
                  title={c("Сопряжённые устройства не найдены", "No paired devices found")}
                >
                  <p>
                    {c(
                      "Включите Bluetooth и выполните сопряжение в Windows. Этот канал доступен в Windows-приложении.",
                      "Enable Bluetooth and pair in Windows. This channel is available in the Windows desktop app.",
                    )}
                  </p>
                </StateMessage>
              ) : (
                <>
                  <label>
                    {c("Телефон / канал", "Phone / channel")}
                    <select
                      value={device?.id ?? ""}
                      disabled={busy}
                      onChange={(e) => setChosen(e.target.value)}
                    >
                      {candidates.map((d) => (
                        <option key={d.id} value={d.id}>
                          {d.display_name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p>
                    {connected
                      ? c("Подключено · ADBian", "Connected · ADBian")
                      : c(
                          "Наличие сопряжения ещё не подтверждает работу агента.",
                          "Pairing alone does not verify the agent service.",
                        )}
                  </p>
                  <div className="btn-row">
                    <button
                      className="btn"
                      disabled={busy || model.busy}
                      onClick={() =>
                        device &&
                        void (connected ? model.disconnect(device.id) : model.connect(device.id))
                      }
                    >
                      {connected ? c("Отключить", "Disconnect") : c("Подключить", "Connect")}
                    </button>
                    <button
                      className="btn btn-secondary"
                      disabled={!id || busy}
                      onClick={() => id && void run(id, { type: "agent.ping" }, "ping")}
                    >
                      {c("Проверить ответ", "Check response")}
                    </button>
                  </div>
                </>
              )}
              <p className="hint">
                {c(
                  "Версия и расширенные возможности не выводятся из имени телефона или ответа pong.",
                  "Version and extended capabilities are not inferred from the phone name or a pong response.",
                )}
              </p>
              <button
                className="btn btn-secondary"
                disabled={busy || model.busy}
                onClick={() => void model.refresh()}
              >
                {c("Обновить список", "Refresh devices")}
              </button>
            </Panel>
            {model.busy && device && !connected && (
              <button
                className="btn btn-secondary"
                onClick={() => void backend.agentCancelConnect(device.id)}
              >
                {c("Отменить подключение", "Cancel connection")}
              </button>
            )}
            <Panel title={c("3. Пакет и обновление", "3. Package and update")}>
              <p>
                {c(
                  "Sideload переносит SIS/JAR в C:\\Data\\_app_install\\ с проверкой SHA-256. Предел базового агента — 1 MiB на файл.",
                  "Sideload copies SIS/JAR to C:\\Data\\_app_install\\ with SHA-256 read-back. Base agent limit: 1 MiB per file.",
                )}
              </p>
              <button className="btn" disabled={!id || busy} onClick={() => void choosePackage()}>
                {c("Sideload SIS / JAR", "Sideload SIS / JAR")}
              </button>
              {staged && (
                <>
                  <p>
                    <strong>
                      {c("Готово к установке", "Ready to install")}: {staged.name}
                    </strong>
                  </p>
                  <code className="agent-hash">SHA-256 {staged.sha256}</code>
                  <label className="checkbox">
                    <input
                      type="checkbox"
                      checked={installedOnPhone}
                      onChange={(e) => setInstalledOnPhone(e.target.checked)}
                    />
                    {c(
                      "Я завершил установку на телефоне (мой отчёт)",
                      "I completed installation on the phone (self-reported)",
                    )}
                  </label>
                </>
              )}
              <p className="hint">
                {c(
                  "При обновлении остановите работающий daemon на телефоне, установите пакет и перезапустите телефон. Затем подключитесь снова. Один ping не подтверждает новую версию или автозапуск после reboot.",
                  "For an update, stop the running daemon on the phone, install and restart the phone. Reconnect afterwards. Ping alone does not verify the new version or autostart after reboot.",
                )}
              </p>
            </Panel>
            <Panel title={c("Если агент не запускается", "If the agent does not start")}>
              <p>
                {c(
                  "Нет SDP: проверьте daemon, сопряжение и Bluetooth. Ошибка подписи: проверьте совместимость пакета и права прошивки. Сбой обновления: используйте предыдущий SIS и независимый канал установки.",
                  "No SDP: check the daemon, pairing and Bluetooth. Signing error: check package compatibility and firmware capabilities. Failed update: use the previous SIS and an independent install channel.",
                )}
              </p>
              <p>
                {c(
                  "Логи rshelld: C:\\Data\\_logs\\. Они могут содержать личные пути и команды. Сохраняйте их локально.",
                  "rshelld logs: C:\\Data\\_logs\\. They can contain personal paths and commands. Keep them local.",
                )}
              </p>
            </Panel>
          </div>
        )}
        {developer && labView && (
          <Panel title="Device Lab / ADBian Console" tone="amber">
            <label>
              {c("Телефон / канал", "Phone / channel")}
              <select
                value={device?.id ?? ""}
                disabled={busy}
                onChange={(e) => setChosen(e.target.value)}
              >
                {candidates.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.display_name} · {d.connection}
                  </option>
                ))}
              </select>
            </label>
            <p>
              {c(
                "Команды исполняются на выбранном телефоне. Вывод хранится только в памяти этой страницы.",
                "Commands run on the selected phone. Output stays in this page's memory.",
              )}
            </p>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={ack}
                disabled={armed || !id}
                onChange={(e) => setAck(e.target.checked)}
              />
              {c(
                "Это моя экспериментальная E72. Принимаю риск потери данных и неработоспособности телефона.",
                "This is my experimental E72. I accept data loss and the risk of an unusable phone.",
              )}
            </label>
            <button
              className="btn btn-secondary"
              disabled={!id || busy || (!armed && !ack)}
              onClick={() => void toggleLab()}
            >
              {armed
                ? c("Выключить Lab", "Disarm Lab")
                : c("Включить Lab для этого телефона", "Arm Lab for this phone")}
            </button>
            <div className="btn-row">
              <button
                className="btn btn-secondary"
                disabled={!armed || busy}
                onClick={() => queryHelper({ command: "hello" })}
              >
                YESKIA hello
              </button>
              <button
                className="btn btn-secondary"
                disabled={!armed || busy}
                onClick={() => queryHelper({ command: "capabilities" })}
              >
                YESKIA capabilities
              </button>
              <button
                className="btn btn-secondary"
                disabled={!armed || busy}
                onClick={() => queryHelper({ command: "apps_list", offset: 0, limit: 32 })}
              >
                {c("Реестр приложений", "App registry")}
              </button>
            </div>
            <form
              className="agent-command"
              onSubmit={(e) => {
                e.preventDefault();
                submitCommand();
              }}
            >
              <label>
                {c("Команда на телефоне", "Phone command")}
                <input
                  value={line}
                  disabled={!armed || busy}
                  onChange={(e) => setLine(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <button className="btn" type="submit" disabled={!armed || busy || !line.trim()}>
                {c("Выполнить", "Run")}
              </button>
            </form>
            <div className="agent-examples">
              {examples.map((example) => (
                <button
                  key={example}
                  className="btn btn-secondary"
                  disabled={!armed || busy}
                  onClick={() => setLine(example)}
                >
                  {example.split(" ")[0] === "ps" ? "P&S" : example.split(" ")[0]}
                </button>
              ))}
            </div>
            <div className="btn-row">
              <button
                className="btn btn-secondary"
                disabled={!armed || busy}
                onClick={() => void choosePut().catch((e) => setError(toErrorInfo(e)))}
              >
                {c("Отправить файл (put)", "Send file (put)")}
              </button>
              <button
                className="btn btn-secondary"
                disabled={busy || history.length === 0}
                onClick={() => void exportHistory()}
              >
                {c("Экспорт личной сессии…", "Export private session…")}
              </button>
              <button className="btn btn-secondary" disabled={busy} onClick={() => setHistory([])}>
                {c("Очистить вывод", "Clear output")}
              </button>
            </div>
            <p className="hint">
              {c(
                "quit останавливает daemon. ps читает P&S, а не список процессов. reboot now — жёсткий сброс без сброса файловых буферов. put передаёт новый файл; замена требует отдельного решения. Для скачивания используйте «Файлы».",
                "quit stops the daemon. ps reads P&S, not a process list. reboot now is a hard reset without flushing files. put sends a new file; replacement requires a separate decision. Use Files for downloads.",
              )}
            </p>
            <div className="agent-history" aria-live="polite">
              {history.map((entry, i) => (
                <article key={i}>
                  <code>› {entry.command}</code>
                  {entry.document && (
                    <details open>
                      <summary>
                        YESKIA {entry.document.agent_version} · {entry.document.build}
                      </summary>
                      <pre>{JSON.stringify(entry.document.data, null, 2)}</pre>
                      {entry.document.data.type === "apps" &&
                        entry.document.data.next_offset !== null && (
                          <button
                            className="btn btn-secondary"
                            disabled={!armed || busy}
                            onClick={() =>
                              queryHelper({
                                command: "apps_list",
                                offset:
                                  entry.document!.data.type === "apps"
                                    ? entry.document!.data.next_offset!
                                    : 0,
                                limit: 32,
                              })
                            }
                          >
                            {c("Следующая страница", "Next page")}
                          </button>
                        )}
                    </details>
                  )}
                  {entry.error && <p role="alert">{entry.error}</p>}
                  {entry.uncertain && (
                    <p className="banner banner-warn">
                      {c("Исход неизвестен", "Outcome uncertain")}
                    </p>
                  )}
                  {entry.reply && (
                    <>
                      <pre>
                        {entry.reply.ok ? "+" : "-"} {entry.reply.text.slice(0, 16000)}
                      </pre>
                      {entry.reply.data.length > 0 && (
                        <details>
                          <summary>
                            {c("Данные ответа", "Response data")}: {entry.reply.data.length} B
                          </summary>
                          <pre>{decodeData(entry.reply.data)}</pre>
                        </details>
                      )}
                    </>
                  )}
                </article>
              ))}
            </div>
          </Panel>
        )}
        {active && (
          <div role="status" className="banner">
            <span>
              {c("Операция выполняется", "Operation in progress")}
              {progress && ` · ${progress.done} / ${progress.total} B`}
            </span>
            <button
              className="btn btn-secondary"
              onClick={() => void backend.cancelOperation(active)}
            >
              {c("Отменить", "Cancel")}
            </button>
          </div>
        )}
        {pending && (
          <Dialog
            title={c("Подтвердите операцию на телефоне", "Confirm phone operation")}
            onClose={() => setPending(null)}
            actions={[
              { label: c("Отмена", "Cancel"), onClick: () => setPending(null) },
              {
                label: c("Запустить на этом телефоне", "Run on this phone"),
                onClick: () => void confirmPending(),
                primary: true,
              },
            ]}
          >
            <p>
              <strong>{pending.name}</strong>
            </p>
            <pre>{pending.operation}</pre>
            {pending.package ? (
              <>
                <p>{pending.package.bytes} B</p>
                <code className="agent-hash">SHA-256 {pending.package.sha256}</code>
                <p>
                  {c(
                    "Только передача. Установку нужно завершить на телефоне.",
                    "Transfer only. Finish installation on the phone.",
                  )}
                </p>
              </>
            ) : (
              <p>
                {c(
                  "Операция может изменить систему, удалить данные или прервать работу телефона. Автоматического повтора не будет.",
                  "This operation can modify the system, delete data or interrupt the phone. It will not retry automatically.",
                )}
              </p>
            )}
            {pending.request.type === "agent.put" && (
              <label>
                {c("Путь назначения на телефоне", "Destination path on phone")}
                <input
                  value={pending.request.path}
                  onChange={(e) => {
                    const path = e.target.value;
                    setPending((p) =>
                      p && p.request.type === "agent.put"
                        ? {
                            ...p,
                            operation: `put ${p.request.source} → ${path}`,
                            request: { ...p.request, path },
                          }
                        : p,
                    );
                  }}
                />
              </label>
            )}
          </Dialog>
        )}
      </div>
    </div>
  );
}
function decodeData(data: number[]): string {
  const bytes = new Uint8Array(data.slice(0, 64 * 1024));
  try {
    if (bytes.some((b) => b < 32 && ![9, 10, 13].includes(b))) throw new Error("binary");
    return (
      new TextDecoder("utf-8", { fatal: true }).decode(bytes) +
      (bytes.length < data.length ? "\n…" : "")
    );
  } catch {
    return (
      data
        .slice(0, 256)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join(" ") + (data.length > 256 ? " …" : "")
    );
  }
}
