import { DeviceCard } from "../components/DeviceCard";
import { Metric, Panel, StateMessage } from "../components/Ui";
import { useI18n } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { PageId } from "../components/Shell";

export function HomePage({ model, go }: { model: AppModel; go: (p: PageId) => void }) {
  const { t } = useI18n();
  const { devices, sessionJournal, system } = model;
  const selected = devices.find((d) => d.connection === "connected") ?? devices[0];
  const isMock = selected?.origin === "mock";
  const matrix = selected?.matrix ?? null;
  const available = matrix?.entries.filter((e) => e.availability === "available").length ?? 0;
  const ok = sessionJournal.filter((j) => j.state === "success").length;
  const bad = sessionJournal.filter(
    (j) => j.state === "failed" || j.state === "interrupted",
  ).length;
  const note = isMock ? <span className="tag tag-mock">{t("home.metric.mockNote")}</span> : null;

  return (
    <>
      <h1>{t("home.title")}</h1>
      <Panel>
        {selected ? (
          <DeviceCard device={selected} />
        ) : (
          <StateMessage kind="empty" title={t("home.noDeviceTitle")}>
            <p>{t("home.noDeviceText")}</p>
            <button className="btn" onClick={() => go("connection")}>
              {t("home.goConnection")}
            </button>
          </StateMessage>
        )}
      </Panel>

      <Panel title={t("home.metrics")}>
        <dl className="metrics">
          <Metric label={t("home.metric.devices")} value={devices.length} />
          <Metric
            label={t("home.metric.connection")}
            value={selected ? t(`conn.${selected.connection}`) : "—"}
          />
          <Metric
            label={t("home.metric.capabilities")}
            value={
              matrix
                ? t("home.metric.capsValue", { n: available, total: matrix.entries.length })
                : "—"
            }
            note={note}
          />
          <Metric label={t("home.metric.ops")} value={t("home.metric.opsValue", { ok, bad })} />
        </dl>
      </Panel>

      {system && (
        <Panel title={t("home.system")}>
          <dl className="facts">
            <dt>{t("settings.platform")}</dt>
            <dd data-testid="os">
              {system.os} / {system.arch}
            </dd>
            <dt>{t("settings.network")}</dt>
            <dd>{system.network_required ? t("settings.networkYes") : t("settings.networkNo")}</dd>
          </dl>
        </Panel>
      )}
    </>
  );
}
