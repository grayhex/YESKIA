import { useEffect, useState } from "react";
import { toErrorInfo, type Backend } from "../api";
import { Dialog } from "../fm/Dialog";
import { formatSize } from "../fm/paths";
import { useI18n } from "../i18n";
import type { ErrorInfo, UpdateCheck } from "../types";
import { ErrorMessage, Panel } from "./Ui";

/** Обновление из файла: приложение ничего не скачивает и не обновляется само. */
export function UpdatePanel({ backend, version }: { backend: Backend; version: string }) {
  const { t } = useI18n();
  const [installer, setInstaller] = useState<string | null>(null);
  const [manifest, setManifest] = useState<string | null>(null);
  const [signature, setSignature] = useState<string | null>(null);
  const [check, setCheck] = useState<UpdateCheck | null>(null);
  const [blockers, setBlockers] = useState<string[]>([]);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [acceptUnsigned, setAcceptUnsigned] = useState(false);
  const [launched, setLaunched] = useState(false);

  useEffect(() => {
    if (!check) return;
    const poll = () =>
      backend
        .updateBlockers()
        .then(setBlockers)
        .catch(() => undefined);
    void poll();
    const id = setInterval(poll, 2000);
    return () => clearInterval(id);
  }, [backend, check]);

  const pick = async (set: (p: string | null) => void) => {
    setError(null);
    setCheck(null);
    setLaunched(false);
    const p = await backend.pickMediaFile();
    if (p) set(p);
  };

  const verify = async () => {
    setError(null);
    setLaunched(false);
    try {
      setCheck(await backend.updateCheck(installer!, manifest!, signature));
    } catch (e) {
      setCheck(null);
      setError(toErrorInfo(e));
    }
  };

  const launch = async () => {
    setConfirm(false);
    setError(null);
    try {
      await backend.updateLaunch(installer!, manifest!, signature, true, acceptUnsigned);
      setLaunched(true);
    } catch (e) {
      setError(toErrorInfo(e));
    }
  };

  const postponed = blockers.length > 0;
  return (
    <Panel title={t("up.title")}>
      <p role="note" className="banner">
        {t("up.scope", { version })}
      </p>
      {error && <ErrorMessage error={error} />}
      {(
        [
          ["up.installer", installer, setInstaller],
          ["up.manifest", manifest, setManifest],
          ["up.signature", signature, setSignature],
        ] as const
      ).map(([label, value, set]) => (
        <div className="btn-row" key={label}>
          <button className="btn btn-secondary" onClick={() => void pick(set)}>
            {t(label)}
          </button>
          <span className="mono">{value ?? "—"}</span>
        </div>
      ))}
      <div className="btn-row">
        <button className="btn" disabled={!installer || !manifest} onClick={() => void verify()}>
          {t("up.verify")}
        </button>
      </div>
      {check && (
        <>
          <p data-testid="up-result">
            {t("up.found", { version: check.version, current: check.current })} · {check.installer}{" "}
            · {formatSize(check.size)}
          </p>
          <p className="mono">SHA-256 {check.sha256}</p>
          <p data-testid="up-trust">
            {check.trust === "signed" ? t("up.trust.signed") : t("up.trust.unsigned")}
          </p>
          {check.notes && <p>{check.notes}</p>}
          <ul>
            {check.limitations.map((l) => (
              <li key={l}>{l}</li>
            ))}
          </ul>
          {postponed && (
            <p role="alert" data-testid="up-blocked">
              {t("up.blocked")}
            </p>
          )}
          <div className="btn-row">
            <button
              className="btn"
              disabled={postponed || launched}
              onClick={() => {
                setAcceptUnsigned(false);
                setConfirm(true);
              }}
            >
              {t("up.launch")}
            </button>
          </div>
          {launched && <p role="status">{t("up.launched")}</p>}
        </>
      )}
      {confirm && check && (
        <Dialog
          title={t("up.confirmTitle")}
          onClose={() => setConfirm(false)}
          actions={[
            { label: t("fm.consent.cancel"), onClick: () => setConfirm(false) },
            {
              label: t("up.confirmOk"),
              primary: true,
              onClick: () => {
                if (check.trust === "unsigned-source" && !acceptUnsigned) return;
                void launch();
              },
            },
          ]}
        >
          <p>{t("up.confirmText", { installer: check.installer, version: check.version })}</p>
          {check.trust === "unsigned-source" && (
            <label>
              <input
                type="checkbox"
                checked={acceptUnsigned}
                onChange={(e) => setAcceptUnsigned(e.target.checked)}
              />{" "}
              {t("up.acceptUnsigned")}
            </label>
          )}
        </Dialog>
      )}
    </Panel>
  );
}
