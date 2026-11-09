import { DeviceCard } from "../components/DeviceCard";
import { Panel, StateMessage } from "../components/Ui";
import { useI18n, type MessageKey } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { CapabilityMatrix, Device, Operation, OperationRequest } from "../types";

interface OpButton {
  label: MessageKey;
  operation: Operation;
  /** Запись допускается только на mock-устройстве: на реальном — пока нет сценария (#5). */
  mockOnly?: boolean;
  request: () => OperationRequest;
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
    request: () => ({ type: "files.list", path: "E:/" }),
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
          {matrix.entries.map((e) => (
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
            onClick={() => model.run(device.id, o.request())}
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

export function ConnectionPage({ model, developer }: { model: AppModel; developer: boolean }) {
  const { t } = useI18n();
  const { devices, busy, result, active } = model;

  return (
    <>
      <h1>{t("conn.title")}</h1>
      <div className="btn-row">
        <button className="btn btn-secondary" disabled={busy} onClick={() => model.refresh()}>
          {t("conn.refresh")}
        </button>
      </div>

      {active && (
        <StateMessage
          kind="empty"
          title={t("conn.running", { op: t(`op.${active.request.type}` as MessageKey) })}
        >
          <p>
            {active.progress && active.progress.total > 0
              ? t("conn.progress", { done: active.progress.done, total: active.progress.total })
              : "…"}
          </p>
          <button className="btn btn-secondary" onClick={() => model.cancelActive()}>
            {t("conn.cancel")}
          </button>
        </StateMessage>
      )}

      {devices.length === 0 && (
        <StateMessage kind="empty" title={t("conn.empty")}>
          <p>{t("conn.emptyHint")}</p>
        </StateMessage>
      )}

      {devices.map((d) => {
        const connected = d.connection === "connected";
        return (
          <Panel key={d.id}>
            <DeviceCard device={d} detailed />
            <div className="btn-row">
              {connected ? (
                <button
                  className="btn btn-secondary"
                  disabled={busy && !active}
                  onClick={() => model.disconnect(d.id)}
                >
                  {t("conn.disconnect")}
                </button>
              ) : (
                <button className="btn" disabled={busy} onClick={() => model.connect(d.id)}>
                  {t("conn.connect")}
                </button>
              )}
            </div>
            {connected && <Actions device={d} model={model} />}
            {d.matrix && <Matrix matrix={d.matrix} developer={developer} />}
          </Panel>
        );
      })}

      {result && (
        <Panel title={t("conn.result")}>
          <p data-testid="outcome">
            {result.operation_id} · {result.operation} · {result.outcome}
            {result.origin === "mock" && " · MOCK"}
          </p>
          {result.uncertain && (
            <StateMessage kind="error" title={t("table.uncertain")}>
              {t("conn.uncertain")}
            </StateMessage>
          )}
          {result.error && (
            <p>
              {result.error.code}: {result.error.message}
              {result.error.action !== "none" && (
                <> — {t(`action.${result.error.action}` as MessageKey)}</>
              )}
            </p>
          )}
          {result.evidence.map((e, i) => (
            <p key={i}>
              <em>
                {t("conn.evidence")}: {e.summary}
              </em>
            </p>
          ))}
          {developer && <pre className="result">{JSON.stringify(result.data, null, 2)}</pre>}
        </Panel>
      )}
    </>
  );
}
