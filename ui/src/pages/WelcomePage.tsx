import { PhoneHero } from "../components/PhoneHero";
import { useState } from "react";
import { Panel, StateMessage } from "../components/Ui";
import { useI18n, type MessageKey } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { PageId } from "../components/Shell";

/** Первый запуск: Welcome → найти Nokia → возможности → файловый менеджер. Ничего не устанавливается. */
export function WelcomePage({
  model,
  go,
  onFinish,
}: {
  model: AppModel;
  go: (p: PageId) => void;
  onFinish: () => void;
}) {
  const { t } = useI18n();
  const [step, setStep] = useState(0);
  const [searched, setSearched] = useState(false);
  const { devices, busy } = model;
  const connected = devices.find((d) => d.connection === "connected") ?? null;
  const filesEntry = connected?.matrix?.entries.find((e) => e.operation === "files.list");
  const canFiles = !!filesEntry && filesEntry.availability !== "unsupported";

  const finish = (to?: PageId) => {
    onFinish();
    if (to) go(to);
  };

  return (
    <>
      <h1>{t("wel.title")}</h1>
      <p className="hint" style={{ marginLeft: 0 }} data-testid="wel-step">
        {t("wel.step", { n: step + 1, total: 4 })}
      </p>

      {step === 0 && <PhoneHero />}
      {step === 0 && (
        <Panel title={t("wel.s1.title")}>
          <p>{t("wel.s1.text")}</p>
          <ul>
            {(["wel.s1.p1", "wel.s1.p2", "wel.s1.p3", "wel.s1.p4"] as MessageKey[]).map((k) => (
              <li key={k}>{t(k)}</li>
            ))}
          </ul>
        </Panel>
      )}

      {step === 1 && (
        <Panel title={t("wel.s2.title")}>
          <div className="btn-row">
            <button
              className="btn"
              disabled={busy}
              onClick={() => {
                setSearched(true);
                void model.refresh();
              }}
            >
              {t("wel.s2.search")}
            </button>
          </div>
          {searched && devices.length === 0 && (
            <StateMessage kind="empty" title={t("wel.s2.noneTitle")}>
              <p>{t("wel.s2.noneText")}</p>
            </StateMessage>
          )}
          {devices.length > 0 && (
            <ul data-testid="wel-devices">
              {devices.map((d) => (
                <li key={d.id}>
                  {d.display_name} · {d.transport}
                  {d.connection === "connected" ? (
                    <> · {t("conn.connected")}</>
                  ) : (
                    <>
                      {" "}
                      <button
                        className="btn btn-secondary"
                        disabled={busy}
                        aria-label={`${t("wel.s2.connect")}: ${d.display_name}`}
                        onClick={() => void model.connect(d.id)}
                      >
                        {t("wel.s2.connect")}
                      </button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}
          <details>
            <summary>{t("wel.driver.title")}</summary>
            <ol>
              {(
                ["wel.driver.1", "wel.driver.2", "wel.driver.3", "wel.driver.4"] as MessageKey[]
              ).map((k) => (
                <li key={k}>{t(k)}</li>
              ))}
            </ol>
            <div className="btn-row">
              <button className="btn btn-secondary" onClick={() => finish("doctor")}>
                {t("wel.driver.doctor")}
              </button>
            </div>
          </details>
        </Panel>
      )}

      {step === 2 && (
        <Panel title={t("wel.s3.title")}>
          {connected?.matrix ? (
            <>
              <p>{t("wel.s3.text")}</p>
              <table className="data" data-testid="wel-caps">
                <thead>
                  <tr>
                    <th scope="col">{t("table.operation")}</th>
                    <th scope="col">{t("wel.s3.availability")}</th>
                  </tr>
                </thead>
                <tbody>
                  {connected.matrix.entries.map((e) => (
                    <tr key={e.operation}>
                      <td>{t(`op.${e.operation}` as MessageKey)}</td>
                      <td>{t(`wel.av.${e.availability}` as MessageKey)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          ) : (
            <StateMessage kind="empty" title={t("wel.s3.noneTitle")}>
              <p>{t("wel.s3.noneText")}</p>
            </StateMessage>
          )}
        </Panel>
      )}

      {step === 3 && (
        <Panel title={t("wel.s4.title")}>
          <p>{canFiles ? t("wel.s4.text") : t("wel.s4.noFiles")}</p>
          <div className="btn-row">
            <button className="btn" disabled={!canFiles} onClick={() => finish("files")}>
              {t("wel.s4.open")}
            </button>
            <button className="btn btn-secondary" onClick={() => finish()}>
              {t("wel.s4.home")}
            </button>
          </div>
        </Panel>
      )}

      <div className="btn-row">
        <button
          className="btn btn-secondary"
          disabled={step === 0}
          onClick={() => setStep((s) => s - 1)}
        >
          {t("wel.back")}
        </button>
        {step < 3 && (
          <button className="btn" onClick={() => setStep((s) => s + 1)}>
            {step === 0 ? t("wel.start") : t("wel.next")}
          </button>
        )}
        <button className="btn btn-secondary" onClick={() => finish()}>
          {t("wel.skip")}
        </button>
      </div>
    </>
  );
}
