import { Panel } from "../components/Ui";
import { UpdatePanel } from "../components/UpdatePanel";
import { useI18n } from "../i18n";
import type { AppModel } from "../useAppModel";

export function SettingsPage({ model }: { model: AppModel }) {
  const { t } = useI18n();
  const { settings, system, busy } = model;
  return (
    <>
      <h1>{t("settings.title")}</h1>
      <Panel>
        {settings ? (
          <>
            <label className="field">
              <input
                type="checkbox"
                checked={settings.use_mock_device}
                disabled={busy}
                onChange={(e) => model.updateSettings({ use_mock_device: e.target.checked })}
              />{" "}
              {t("settings.mockLabel")}
              <span className="hint">{t("settings.mockHint")}</span>
            </label>
            <label className="field">
              <input
                type="checkbox"
                checked={settings.log_to_file}
                disabled={busy}
                onChange={(e) => model.updateSettings({ log_to_file: e.target.checked })}
              />{" "}
              {t("settings.logLabel")}
              <span className="hint">{t("settings.logHint")}</span>
            </label>
          </>
        ) : (
          <p>{t("state.loading")}</p>
        )}
      </Panel>
      <Panel title={t("settings.about")}>
        <p>{t("settings.aboutText")}</p>
        {system && (
          <dl className="facts">
            <dt>{t("settings.version")}</dt>
            <dd>{system.app_version}</dd>
            <dt>{t("settings.dataDir")}</dt>
            <dd className="mono">{system.data_dir}</dd>
            <dt>{t("settings.mode")}</dt>
            <dd data-testid="settings-mode">
              {system.portable ? t("settings.modePortable") : t("settings.modeInstalled")}
            </dd>
          </dl>
        )}
        <p className="hint" style={{ marginLeft: 0 }}>
          {t("settings.uninstallNote")}
        </p>
      </Panel>
      {system && <UpdatePanel backend={model.backend} version={system.app_version} />}
    </>
  );
}
