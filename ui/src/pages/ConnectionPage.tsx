import { useState } from "react";
import { Icon } from "../components/Icon";
import { ConnectionTag } from "../components/Ui";
import { DeviceCard } from "../components/DeviceCard";
import { StateMessage } from "../components/Ui";
import { useI18n, type MessageKey } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { CapabilityMatrix, Device, Operation, OperationRequest } from "../types";

interface OpButton {
  label: MessageKey;
  operation: Operation;
  /** Запись допускается только на mock-устройстве: на реальном — пока нет сценария (#5). */
  mockOnly?: boolean;
  request: (device: Device) => OperationRequest;
}

const OPS: OpButton[] = [
  {
    label: "conn.op.identify",
    operation: "identity.read",
    request: () => ({ type: "identity.read" }),
  },
  {
    label: "conn.op.list",
    operation: "files.list",
    request: (device) => ({ type: "files.list", path: device.origin === "mock" ? "E:/" : "/" }),
  },
  {
    label: "conn.op.write",
    operation: "files.write",
    mockOnly: true,
    request: () => ({
      type: "files.write",
      path: `E:/yeskia-test-${Date.now()}.txt`,
      data: Array.from(new TextEncoder().encode("yeskia")),
      overwrite: "never",
    }),
  },
];

function Matrix({ matrix, developer }: { matrix: CapabilityMatrix; developer: boolean }) {
  const { t } = useI18n();
  return (
    <>
      <h3>{t("conn.matrix")}</h3>
      <p className="hint" style={{ marginLeft: 0 }}>
        {t("conn.matrixHint")}
      </p>
      <table className="data">
        <caption>
          {t("conn.matrix")}
          {developer && (
            <>
              {" · "}
              {t("conn.fingerprint")}: <span className="mono">{matrix.device_fingerprint}</span>
            </>
          )}
        </caption>
        <thead>
          <tr>
            <th scope="col">{t("table.capability")}</th>
            <th scope="col">{t("table.status")}</th>
            {developer && (
              <>
                <th scope="col">{t("table.maturity")}</th>
                <th scope="col">{t("table.evidence")}</th>
                <th scope="col">{t("table.detail")}</th>
              </>
            )}
          </tr>
        </thead>
        <tbody>
          {matrix.entries
            .filter(
              (e) =>
                developer ||
                ![
                  "agent.stop",
                  "files.delete",
                  "process.exec",
                  "device.reset",
                  "system.read",
                ].includes(e.operation),
            )
            .map((e) => (
              <tr key={e.operation}>
                <td>
                  {t(`op.${e.operation}` as MessageKey)}
                  {developer && <span className="mono"> ({e.operation})</span>}
                </td>
                <td>{t(`avail.${e.availability}` as MessageKey)}</td>
                {developer && (
                  <>
                    <td>{e.maturity}</td>
                    <td>
                      {e.evidence.length === 0
                        ? "—"
                        : e.evidence.map((ev) => `${ev.source}: ${ev.summary}`).join("; ")}
                    </td>
                    <td>{e.detail ?? "—"}</td>
                  </>
                )}
              </tr>
            ))}
        </tbody>
      </table>
    </>
  );
}

function Actions({ device, model }: { device: Device; model: AppModel }) {
  const { t } = useI18n();
  const matrix = device.matrix;
  if (!matrix) return null;
  const usable = OPS.filter(
    (o) =>
      matrix.entries.some((e) => e.operation === o.operation && e.availability !== "unsupported") &&
      (!o.mockOnly || device.origin === "mock"),
  );
  const unavailable = OPS.filter(
    (o) =>
      !o.mockOnly &&
      matrix.entries.some((e) => e.operation === o.operation && e.availability === "unsupported"),
  );
  return (
    <>
      <h3>{t("conn.actions")}</h3>
      <div className="btn-row">
        {usable.map((o) => (
          <button
            key={o.label}
            className="btn"
            disabled={model.busy}
            onClick={() => model.run(device.id, o.request(device))}
          >
            {t(o.label)}
          </button>
        ))}
      </div>
      {unavailable.length > 0 && (
        <StateMessage kind="unavailable" title={t("conn.unavailable")}>
          {unavailable.map((o) => t(`op.${o.operation}` as MessageKey)).join(", ")}
        </StateMessage>
      )}
    </>
  );
}

export function ConnectionPage({
  model,
  developer,
  go,
}: {
  model: AppModel;
  developer: boolean;
  go: (page: "doctor" | "files") => void;
}) {
  const { t, lang } = useI18n();
  const l = (ru: string, en: string) => (lang === "ru" ? ru : en);
  const { devices, busy, result, active } = model;
  const [filter, setFilter] = useState<"auto" | "bluetooth" | "usb">("auto");
  const [selected, setSelected] = useState("");
  const visible = devices.filter(
    (d) =>
      filter === "auto" ||
      (filter === "bluetooth"
        ? d.transport === "bluetooth"
        : d.transport !== "bluetooth" && d.origin !== "mock"),
  );
  const device =
    visible.find((d) => d.id === selected) ??
    visible.find((d) => d.connection === "connected") ??
    visible[0];
  const bluetooth = devices.filter((d) => d.transport === "bluetooth").length;
  const usb = devices.filter((d) => d.transport !== "bluetooth" && d.origin !== "mock").length;
  return (
    <div className="connection-layout">
      <h1 className="sr-only">{t("conn.title")}</h1>
      <div className="transport-guides" aria-label={l("Способы подключения", "Connection methods")}>
        <section className="transport-guide">
          <Icon name="bluetooth" />
          <div>
            <h2>
              Bluetooth <span className="tag">{bluetooth}</span>
            </h2>
            <p>
              {l(
                "Для файлов: сопрягите Nokia в Windows, запустите rshelld на телефоне и нажмите «Подключить» ниже.",
                "For files: pair your Nokia in Windows, start rshelld on the phone and choose Connect below.",
              )}
            </p>
          </div>
        </section>
        <section className="transport-guide">
          <Icon name="usb" />
          <div>
            <h2>
              USB <span className="tag">{usb}</span>
            </h2>
            <p>
              {l(
                "Для PIM выберите PC Suite на телефоне. MTP и накопитель дают доступ к файлам. Затем нажмите «Подключить» ниже.",
                "For PIM, select PC Suite on the phone. MTP and mass storage provide files. Then choose Connect below.",
              )}
            </p>
          </div>
        </section>
      </div>
      <div className="connection-toolbar">
        <div
          className="segmented"
          role="group"
          aria-label={l("Тип подключения", "Connection type")}
        >
          {(
            [
              ["auto", l("Автоматически", "Automatic")],
              ["bluetooth", "Bluetooth"],
              ["usb", "USB"],
            ] as const
          ).map(([value, label]) => (
            <button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>
              {label}
            </button>
          ))}
        </div>
        <span className="hint">
          {l(
            "Обнаружение — автоматически. Соединение — по вашему запросу.",
            "Detection is automatic. Connection starts only on your request.",
          )}
        </span>
        <button className="btn btn-secondary" disabled={busy} onClick={() => void model.refresh()}>
          <Icon name="refresh" />
          {t("conn.refresh")}
        </button>
        <button className="btn btn-secondary" onClick={() => go("doctor")}>
          {t("wel.driver.doctor")}
        </button>
      </div>
      {active && (
        <div className="connection-progress" role="status">
          <span>
            {t("conn.running", { op: t(`op.${active.request.type}` as MessageKey) })}{" "}
            {active.progress && active.progress.total > 0
              ? t("conn.progress", { done: active.progress.done, total: active.progress.total })
              : "…"}
          </span>
          <button className="btn btn-secondary" onClick={() => void model.cancelActive()}>
            {t("conn.cancel")}
          </button>
        </div>
      )}
      {!visible.length ? (
        <StateMessage kind="empty" title={t("conn.empty")}>
          <p>
            {filter === "bluetooth"
              ? l(
                  "Показываются только сопряжённые устройства класса Phone. Аудиоустройства и неизвестный класс не подключаются. Проверьте Bluetooth Nokia в Windows.",
                  "Only paired Phone-class devices are shown. Audio devices and unknown classes are never connected. Check the Nokia Bluetooth pairing in Windows.",
                )
              : t("conn.emptyHint")}
          </p>
        </StateMessage>
      ) : (
        <div className="connection-split">
          <section
            className="panel connection-devices"
            aria-label={l("Обнаруженные телефоны", "Detected phones")}
          >
            <h2>
              {l("Телефоны", "Phones")} <span className="tag">{visible.length}</span>
            </h2>
            <div className="connection-device-list">
              {visible.map((d) => (
                <button
                  className={`record-row ${device?.id === d.id ? "selected" : ""}`}
                  key={d.id}
                  aria-pressed={device?.id === d.id}
                  onClick={() => setSelected(d.id)}
                >
                  <strong>{d.display_name.replace(/\s*\(MOCK\)/, "")}</strong>
                  <small>
                    <Icon
                      name={
                        d.transport === "bluetooth"
                          ? "bluetooth"
                          : d.origin === "mock"
                            ? "phone"
                            : "usb"
                      }
                    />
                    {d.origin === "mock" ? `MOCK · ${d.transport}` : d.transport}
                  </small>
                  <ConnectionTag state={d.connection} />
                </button>
              ))}
            </div>
          </section>
          {device && (
            <section
              className="panel connection-detail"
              aria-label={l("Выбранный телефон", "Selected phone")}
            >
              <div className="connection-device-header">
                <DeviceCard device={device} />
                <span className="tag transport-indicator">
                  <Icon
                    name={
                      device.transport === "bluetooth"
                        ? "bluetooth"
                        : device.origin === "mock"
                          ? "phone"
                          : "usb"
                    }
                  />
                  {device.origin === "mock" ? `MOCK · ${device.transport}` : device.transport}
                </span>
              </div>
              <div className="btn-row">
                {device.connection === "connected" ? (
                  <button
                    className="btn btn-secondary"
                    disabled={busy && !active}
                    onClick={() => void model.disconnect(device.id)}
                  >
                    {t("conn.disconnect")}
                  </button>
                ) : (
                  <button
                    className="btn"
                    disabled={busy}
                    onClick={() => void model.connect(device.id)}
                  >
                    <Icon name={device.transport === "bluetooth" ? "bluetooth" : "usb"} />
                    {t("conn.connect")}
                  </button>
                )}
                {device.connection === "connected" &&
                  device.matrix?.entries.some(
                    (e) => e.operation === "files.list" && e.availability !== "unsupported",
                  ) && (
                    <button className="btn" onClick={() => go("files")}>
                      {t("wel.s4.open")}
                    </button>
                  )}
              </div>
              {device.connection === "connected" && <Actions device={device} model={model} />}
              {device.matrix && (
                <details className="connection-matrix">
                  <summary>{t("conn.matrix")}</summary>
                  {developer && (
                    <dl className="facts">
                      <dt>{t("dev.origin")}</dt>
                      <dd>{device.origin}</dd>
                      <dt>{t("dev.transport")}</dt>
                      <dd>{device.transport}</dd>
                    </dl>
                  )}
                  <Matrix matrix={device.matrix} developer={developer} />
                </details>
              )}
            </section>
          )}
        </div>
      )}
      {result && (
        <section className="panel connection-result" aria-label={t("conn.result")}>
          <p data-testid="outcome">
            {t(`op.${result.operation}` as MessageKey)} ·{" "}
            {t(`result.${result.outcome}` as MessageKey)}
            {result.origin === "mock" && " · MOCK"}
          </p>
          {result.uncertain && <p role="alert">{t("conn.uncertain")}</p>}
          {result.error && (
            <p role="alert">
              {result.error.message}
              {result.error.action !== "none" && (
                <> — {t(`action.${result.error.action}` as MessageKey)}</>
              )}
            </p>
          )}
          {developer && (
            <details>
              <summary>{l("Подробности результата", "Result details")}</summary>
              <p>
                {result.operation_id} · {result.operation} · {result.outcome}
              </p>
              <pre className="result">{JSON.stringify(result.data, null, 2)}</pre>
              {result.evidence.map((e, i) => (
                <p key={i}>
                  {e.source}: {e.summary}
                </p>
              ))}
            </details>
          )}
        </section>
      )}
    </div>
  );
}
