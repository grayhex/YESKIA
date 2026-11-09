import { useState } from "react";
import { Panel, StateMessage } from "../components/Ui";
import { useI18n, type MessageKey } from "../i18n";
import type { AppModel } from "../useAppModel";
import type { DoctorDevice } from "../types";

function DeviceReport({ d, developer }: { d: DoctorDevice; developer: boolean }) {
  const { t } = useI18n();
  return (
    <Panel>
      <h3>
        {d.alias} · {d.vid}:{d.pid}
      </h3>
      <p>
        <span
          className={`tag ${d.state === "wpd_only" ? "tag-state-connected" : "tag-state-disconnected"}`}
        >
          {t(`dstate.${d.state}` as MessageKey)}
        </span>
      </p>
      <dl className="facts">
        <dt>{t("doctor.vendor")}</dt>
        <dd>{d.nokia_vendor_hint ? t("doctor.vendorNokia") : t("doctor.vendorOther")}</dd>
        <dt>{t("doctor.protocol")}</dt>
        <dd>{t(`proto.${d.protocol}` as MessageKey)}</dd>
        {d.wpd && d.wpd.storages !== null && (
          <>
            <dt>{t("doctor.wpd")}</dt>
            <dd>
              {t("doctor.wpdValue", {
                storages: d.wpd.storages,
                objects: d.wpd.top_level_objects ?? 0,
              })}
            </dd>
          </>
        )}
        <dt>{t("doctor.com.count")}</dt>
        <dd>
          {d.com_ports}
          {d.com_busy > 0 ? ` (${t("dstate.com_busy")}: ${d.com_busy})` : ""}
        </dd>
        <dt>{t("doctor.pcSuite")}</dt>
        <dd>{t("doctor.notProven")}</dd>
        <dt>{t("doctor.pim")}</dt>
        <dd>{t("doctor.notProven")}</dd>
        {developer && (
          <>
            <dt>{t("doctor.driver")}</dt>
            <dd className="mono">
              {[d.driver.class, d.driver.service, d.driver.provider, d.driver.version]
                .filter(Boolean)
                .join(" / ") || "—"}
              {d.driver.problem_code !== 0 &&
                ` · ${t("doctor.problemCode", { code: d.driver.problem_code })}`}
            </dd>
            <dt>{t("doctor.volumes")}</dt>
            <dd>{d.volumes}</dd>
          </>
        )}
      </dl>
      <h3>{t("doctor.findings")}</h3>
      <ul>
        {d.findings.map((f) => (
          <li key={f}>{t(`finding.${f}` as MessageKey)}</li>
        ))}
      </ul>
    </Panel>
  );
}

export function DoctorPage({ model, developer }: { model: AppModel; developer: boolean }) {
  const { t } = useI18n();
  const [comBusy, setComBusy] = useState(false);
  const { doctorReport: r, doctorRunning: running, exportedPath } = model;
  const options = { probe_com_busy: comBusy };

  return (
    <>
      <h1>{t("doctor.title")}</h1>
      <p>{t("doctor.intro")}</p>
      <Panel>
        <label className="field">
          <input
            type="checkbox"
            checked={comBusy}
            disabled={running}
            onChange={(e) => setComBusy(e.target.checked)}
          />{" "}
          {t("doctor.com")}
          <span className="hint">{t("doctor.comHint")}</span>
        </label>
        <div className="btn-row">
          <button className="btn" disabled={running} onClick={() => model.runDoctor(options)}>
            {t("doctor.run")}
          </button>
          <button
            className="btn btn-secondary"
            disabled={running}
            onClick={() => model.exportReport(options)}
          >
            {t("doctor.export")}
          </button>
        </div>
        {running && <p role="status">{t("doctor.running")}</p>}
        {exportedPath && (
          <p role="status" className="mono">
            {t("doctor.saved", { path: exportedPath })}
          </p>
        )}
      </Panel>

      {!r && !running && <StateMessage kind="empty" title={t("doctor.none")} />}

      {r && (
        <>
          <p>
            {t("doctor.platform")}: {r.platform.os} {r.platform.build} / {r.platform.arch}
          </p>
          {r.devices.length === 0 && (
            <StateMessage kind="empty" title={t("doctor.noDevices")}>
              {r.findings.includes("no_usb_devices") && <p>{t("finding.no_usb_devices")}</p>}
            </StateMessage>
          )}
          {r.findings.includes("multiple_devices") && <p>{t("finding.multiple_devices")}</p>}
          {r.devices.map((d) => (
            <DeviceReport key={d.alias} d={d} developer={developer} />
          ))}
          {r.issues.length > 0 && (
            <StateMessage kind="error" title={t("finding.partial_data")}>
              {t("doctor.issues", {
                stages: r.issues
                  .map(
                    (i) =>
                      `${i.stage}${i.code !== null ? ` (0x${i.code.toString(16).toUpperCase()})` : ""}`,
                  )
                  .join(", "),
              })}
            </StateMessage>
          )}
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("doctor.excluded", { list: r.excluded.join(", ") })}
          </p>
        </>
      )}
    </>
  );
}
