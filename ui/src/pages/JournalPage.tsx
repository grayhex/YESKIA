import { Panel, StateMessage } from "../components/Ui";
import { useI18n } from "../i18n";
import type { AppModel } from "../useAppModel";

export function JournalPage({ model }: { model: AppModel }) {
  const { t, lang } = useI18n();
  const { journal } = model;
  return (
    <>
      <h1>{t("journal.title")}</h1>
      {journal.length === 0 ? (
        <StateMessage kind="empty" title={t("journal.empty")} />
      ) : (
        <Panel>
          <table className="data">
            <thead>
              <tr>
                <th scope="col">ID</th>
                <th scope="col">{t("table.operation")}</th>
                <th scope="col">{t("table.state")}</th>
                <th scope="col">{t("table.code")}</th>
                <th scope="col">{t("table.uncertain")}</th>
                <th scope="col">{t("table.time")}</th>
              </tr>
            </thead>
            <tbody>
              {journal.map((j) => (
                <tr key={j.operation_id}>
                  <td className="mono">{j.operation_id}</td>
                  <td>{j.operation}</td>
                  <td>{j.state}</td>
                  <td className="mono">{j.error_code ?? "—"}</td>
                  <td>{j.uncertain ? t("table.yes") : "—"}</td>
                  <td>{new Date(j.started_at_ms).toLocaleTimeString(lang)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>
      )}
    </>
  );
}
