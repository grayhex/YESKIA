import { useState } from "react";
import { toErrorInfo, type Backend } from "../api";
import { useI18n } from "../i18n";
import type { ErrorInfo } from "../types";
import { ErrorMessage, Panel } from "./Ui";

export function UpdatePanel({ backend, version }: { backend: Backend; version: string }) {
  const { t } = useI18n();
  const [error, setError] = useState<ErrorInfo | null>(null);
  return (
    <Panel title={t("up.title")}>
      <p>{t("up.currentVersion", { version })}</p>
      {error && <ErrorMessage error={error} />}
      <button
        className="btn"
        onClick={() => {
          setError(null);
          void backend.openLatestRelease().catch((e) => setError(toErrorInfo(e)));
        }}
      >
        {t("up.latest")}
      </button>
    </Panel>
  );
}
