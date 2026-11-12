import { DeviceCard } from "../components/DeviceCard";
import { AccentIcon } from "../components/Visual";
import { Icon } from "../components/Icon";
import { Metric, Panel, StateMessage } from "../components/Ui";
import { useI18n, type MessageKey } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { PageId } from "../components/Shell";
import type { Operation } from "../types";

export function HomePage({ model, go }: { model: AppModel; go: (p: PageId) => void }) {
  const { t, lang } = useI18n();
  const { devices, sessionJournal } = model;
  const selected = devices.find((d) => d.connection === "connected") ?? devices[0];
  const isMock = selected?.origin === "mock";
  const matrix = selected?.matrix ?? null;
  const available = matrix?.entries.filter((e) => e.availability === "available").length ?? 0;
  const has = (op: Operation) =>
    selected?.connection === "connected" &&
    matrix?.entries.some((e) => e.operation === op && e.availability === "available");
  const actions = [
    ...(has("files.list") ? [{ page: "files" as const, icon: "folder" as const }] : []),
    ...(has("files.list") && has("files.read")
      ? [{ page: "backups" as const, icon: "archive" as const }]
      : []),
    // Local tools remain useful without a phone; these tiles never start a transfer.
    { page: "packages" as const, icon: "apps" as const },
    { page: "studio" as const, icon: "music" as const },
  ];
  const ok = sessionJournal.filter((j) => j.state === "success").length;
  const bad = sessionJournal.filter(
    (j) => j.state === "failed" || j.state === "interrupted",
  ).length;
  const recent = [...sessionJournal].sort((a, b) => b.started_at_ms - a.started_at_ms).slice(0, 4);
  return (
    <>
      <div className="page-heading home-heading">
        <h1>{t("home.title")}</h1>
        <div className="btn-row">
          {model.system && model.settings?.developer_mode && (
            <button className="btn btn-secondary" onClick={() => go("settings")}>
              {t("home.system")}
            </button>
          )}
          <button className="btn btn-secondary" onClick={() => go("preparation")}>
            {lang === "ru" ? "Подготовка E72" : "Prepare E72"}
          </button>
          <button className="btn btn-secondary" onClick={() => go("connection")}>
            <Icon name="usb" />
            {t("nav.connection")}
          </button>
          {has("files.list") && (
            <button className="btn" onClick={() => go("files")}>
              <Icon name="folder" />
              {t("home.openFiles")}
            </button>
          )}
        </div>
      </div>
      <div className="home-dashboard">
        <section className="device-hero" aria-label={t("home.metric.connection")}>
          <svg className="hero-orbits" viewBox="0 0 400 400" fill="none" aria-hidden="true">
            <circle cx="220" cy="200" r="150" />
            <circle cx="220" cy="200" r="195" />
            <path d="M20 340 370 25" />
          </svg>
          <p className="eyebrow">{t("home.eyebrow")}</p>
          {selected ? (
            <>
              <DeviceCard device={selected} />
              <p className="device-hero-description">{t("home.deviceText")}</p>
              <p className="device-hero-transport">
                {t("dev.transport")}: {selected.transport}
              </p>
              <div className="btn-row">
                <button
                  className="btn"
                  onClick={() => go(has("files.list") ? "files" : "connection")}
                >
                  <Icon name={has("files.list") ? "folder" : "usb"} />
                  {has("files.list") ? t("wel.s4.open") : t("home.goConnection")}
                </button>
              </div>
            </>
          ) : (
            <StateMessage kind="empty" title={t("home.noDeviceTitle")}>
              <p>{t("home.noDeviceText")}</p>
              <button className="btn" onClick={() => go("connection")}>
                {t("home.goConnection")}
              </button>
            </StateMessage>
          )}
        </section>
        <Panel title={t("home.metric.capabilities")} icon="shield" tone="blue">
          {matrix ? (
            <ul className="capability-list">
              {(
                ["files.list", "files.read", "files.write", "contacts.read", "sms.read"] as const
              ).map((op) => {
                const entry = matrix.entries.find((e) => e.operation === op);
                return (
                  <li key={op} data-availability={entry?.availability ?? "unknown"}>
                    <AccentIcon
                      name={
                        op.startsWith("files")
                          ? "folder"
                          : op === "contacts.read"
                            ? "contacts"
                            : "message"
                      }
                      tone={op.startsWith("files") ? "blue" : "amber"}
                    />
                    <strong>{t(`op.${op}`)}</strong>
                    <span>{entry ? t(`avail.${entry.availability}` as MessageKey) : "—"}</span>
                  </li>
                );
              })}
            </ul>
          ) : (
            <StateMessage kind="empty" title={t(selected ? "home.detected" : "home.waiting")} />
          )}
          {isMock && <span className="tag tag-mock">{t("home.metric.mockNote")}</span>}
        </Panel>
        <dl className="metrics home-metrics" aria-label={t("home.metrics")}>
          <Metric label={t("home.metric.devices")} value={devices.length} icon="phone" />
          <Metric
            label={t("home.metric.capabilities")}
            icon="shield"
            tone="blue"
            value={
              matrix
                ? t("home.metric.capsValue", { n: available, total: matrix.entries.length })
                : "—"
            }
          />
          <Metric
            label={t("home.metric.ops")}
            value={t("home.metric.opsValue", { ok, bad })}
            icon="activity"
            tone="green"
          />
        </dl>
        <section className="home-tools">
          <h2>{t("home.tools")}</h2>
          {actions.length ? (
            <div className="quick-actions">
              {actions.map(({ page, icon }) => (
                <button key={page} onClick={() => go(page)}>
                  <AccentIcon
                    name={icon}
                    tone={
                      page === "backups"
                        ? "green"
                        : page === "packages"
                          ? "purple"
                          : page === "studio"
                            ? "amber"
                            : "blue"
                    }
                  />
                  <strong>{t(`nav.${page}`)}</strong>
                  <span>{t(`quick.${page}`)}</span>
                  <Icon name="chevron" />
                </button>
              ))}
            </div>
          ) : (
            <StateMessage kind="empty" title={t("home.noActions")} />
          )}
        </section>
        <Panel
          title={t(model.isUiMock || isMock ? "home.sessionMock" : "home.activity")}
          icon="activity"
          tone="green"
        >
          {recent.length ? (
            <ol className="activity-list">
              {recent.map((j) => (
                <li key={j.operation_id} data-state={j.state}>
                  <strong>{t(`op.${j.operation}` as MessageKey)}</strong>
                  <span>
                    {t(`home.state.${j.state}`)} ·{" "}
                    <time dateTime={new Date(j.started_at_ms).toISOString()}>
                      {new Date(j.started_at_ms).toLocaleTimeString(lang)}
                    </time>
                  </span>
                  {j.error_code && <span>{j.error_code}</span>}
                  {j.uncertain && <span className="error-inline">{t("fm.uncertain")}</span>}
                </li>
              ))}
            </ol>
          ) : (
            <StateMessage kind="empty" title={t("journal.empty")} />
          )}
        </Panel>
      </div>
    </>
  );
}
