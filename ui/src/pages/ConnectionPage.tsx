import { DeviceCard } from "../components/DeviceCard";
import { ErrorMessage, Panel, StateMessage } from "../components/Ui";
import { useI18n, type MessageKey } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { CapabilityId, Device, OperationRequest } from "../types";

const OPS: { label: MessageKey; cap: CapabilityId; request: OperationRequest }[] = [
  { label: "conn.op.identify", cap: "identify", request: { type: "identify" } },
  { label: "conn.op.list", cap: "file_browse", request: { type: "list_directory", path: "E:/" } },
  {
    label: "conn.op.write",
    cap: "file_write",
    request: { type: "write_file", path: "E:/yeskia.txt", size: 12 },
  },
];

function Capabilities({ device }: { device: Device }) {
  const { t } = useI18n();
  return (
    <table className="data">
      <caption>{t("table.capability")}</caption>
      <thead>
        <tr>
          <th scope="col">{t("table.capability")}</th>
          <th scope="col">{t("table.status")}</th>
          <th scope="col">{t("table.evidence")}</th>
        </tr>
      </thead>
      <tbody>
        {device.capabilities.map((c) => (
          <tr key={c.id}>
            <td className="mono">{c.id}</td>
            <td>{c.status}</td>
            <td>{c.evidence.map((e) => e.summary).join("; ") || "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function ConnectionPage({ model, developer }: { model: AppModel; developer: boolean }) {
  const { t } = useI18n();
  const { devices, busy, result, error } = model;

  return (
    <>
      <h1>{t("conn.title")}</h1>
      <div className="btn-row">
        <button className="btn btn-secondary" disabled={busy} onClick={() => model.refresh()}>
          {t("conn.refresh")}
        </button>
      </div>

      {error && <ErrorMessage error={error} />}

      {devices.length === 0 && (
        <StateMessage kind="empty" title={t("conn.empty")}>
          <p>{t("conn.emptyHint")}</p>
        </StateMessage>
      )}

      {devices.map((d) => {
        const connected = d.connection === "connected";
        const isAvailable = (cap: CapabilityId) =>
          d.capabilities.some((c) => c.id === cap && c.status !== "unsupported");
        const unavailable = OPS.filter((o) => !isAvailable(o.cap));
        return (
          <Panel key={d.id}>
            <DeviceCard device={d} detailed />
            <div className="btn-row">
              {connected ? (
                <button
                  className="btn btn-secondary"
                  disabled={busy}
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

            {connected && (
              <>
                <h3>{t("conn.actions")}</h3>
                <div className="btn-row">
                  {OPS.filter((o) =>
                    d.capabilities.find((c) => c.id === o.cap && c.status !== "unsupported"),
                  ).map((o) => (
                    <button
                      key={o.label}
                      className="btn"
                      disabled={busy}
                      onClick={() => model.run(d.id, o.request)}
                    >
                      {t(o.label)}
                    </button>
                  ))}
                </div>
                {unavailable.length > 0 && (
                  <StateMessage kind="unavailable" title={t("conn.unavailable")}>
                    {unavailable.map((o) => t(o.label)).join(", ")}
                  </StateMessage>
                )}
              </>
            )}
            {developer && <Capabilities device={d} />}
          </Panel>
        );
      })}

      {result && (
        <Panel title={t("conn.result")}>
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
