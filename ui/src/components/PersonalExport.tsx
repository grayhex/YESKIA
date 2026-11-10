import { useState } from "react";
import { toErrorInfo, type Backend } from "../api";
import { useI18n } from "../i18n";
import type { ErrorInfo } from "../types";
import { ErrorMessage, Panel } from "./Ui";

export function PersonalExport({ backend }: { backend: Backend }) {
  const { t } = useI18n();
  const [executable, setExecutable] = useState<string | null>(null);
  const [config, setConfig] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  const run = async (kind: "contacts" | "calendar") => {
    if (!executable || !config) return;
    setError(null);
    setSaved(null);
    setBusy(true);
    try {
      const destination = await backend.pickSavePath(
        kind === "contacts" ? "contacts.vcf" : "calendar.ics",
      );
      if (!destination) return;
      await backend.exportPersonalData(executable, config, destination, kind);
      setSaved(destination);
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Panel title={t("personal.title")}>
      <p>{t("personal.intro")}</p>
      <div className="btn-row">
        <button
          className="btn btn-secondary"
          disabled={busy}
          onClick={() =>
            void backend
              .pickFile()
              .then((p) => {
                if (p) setExecutable(p);
              })
              .catch((e) => setError(toErrorInfo(e)))
          }
        >
          {t("personal.program")}
        </button>
        {executable && <span>{executable}</span>}
      </div>
      <div className="btn-row">
        <button
          className="btn btn-secondary"
          disabled={busy}
          onClick={() =>
            void backend
              .pickFile()
              .then((p) => {
                if (p) setConfig(p);
              })
              .catch((e) => setError(toErrorInfo(e)))
          }
        >
          {t("personal.config")}
        </button>
        {config && <span>{config}</span>}
      </div>
      <div className="btn-row">
        <button
          className="btn"
          disabled={busy || !executable || !config}
          onClick={() => void run("contacts")}
        >
          <svg
            className="button-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <path d="M12 3v12m-4-4 4 4 4-4M4 17v3h16v-3" />
          </svg>
          {t("personal.contacts")}
        </button>
        <button
          className="btn"
          disabled={busy || !executable || !config}
          onClick={() => void run("calendar")}
        >
          <svg
            className="button-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <path d="M12 3v12m-4-4 4 4 4-4M4 17v3h16v-3" />
          </svg>
          {t("personal.calendar")}
        </button>
      </div>
      {busy && <p role="status">{t("personal.busy")}</p>}
      {error && <ErrorMessage error={error} />}
      {saved && <p role="status">{t("personal.done", { path: saved })}</p>}
    </Panel>
  );
}
