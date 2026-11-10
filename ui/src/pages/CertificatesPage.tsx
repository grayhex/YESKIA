import { useState } from "react";
import { ErrorMessage, Panel } from "../components/Ui";
import { Icon } from "../components/Icon";
import { Dialog } from "../fm/Dialog";
import type { AppModel } from "../useAppModel";
import type { CertInfo, ErrorInfo } from "../types";
import { toErrorInfo } from "../api";
import { useWorkspaceCopy, type WorkspaceKey } from "../workspaceCopy";
export function CertificatesPage({ model }: { model: AppModel }) {
  const { c, error: localizeError } = useWorkspaceCopy();
  const [cert, setCert] = useState<CertInfo | null>(null);
  const [path, setPath] = useState("");
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [sent, setSent] = useState(false);
  const device = model.devices.find((d) => d.connection === "connected");
  const inspect = async () => {
    setBusy(true);
    setError(null);
    try {
      const p = await model.backend.pickFile();
      if (p) {
        setCert(null);
        setPath("");
        setSent(false);
        setCert(await model.backend.inspectCertificate(p));
        setPath(p);
      }
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  const install = async () => {
    if (!cert || !device) return;
    setConfirm(false);
    setBusy(true);
    setError(null);
    setSent(false);
    try {
      await model.backend.installPhoneCertificate(device.id, path, cert.sha256, true);
      setSent(true);
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  const fieldValue = (key: string, v: unknown) =>
    typeof v === "boolean"
      ? c(v ? "yes" : "no")
      : key === "validity"
        ? c(`cert.${v}` as WorkspaceKey)
        : String(v);
  return (
    <div className="workspace-page">
      <h1>{c("cert.title")}</h1>
      <Panel title={c("cert.inspect")}>
        <p>{c("cert.intro")}</p>
        <div className="btn-row">
          <button className="btn" disabled={busy} onClick={() => void inspect()}>
            <Icon name="plus" />
            {c("cert.open")}
          </button>
          <button
            className="btn btn-secondary"
            disabled={busy || !cert || !device}
            onClick={() => setConfirm(true)}
          >
            <Icon name="shield" />
            {c("cert.install")}
          </button>
        </div>
        {!device && <p className="hint">{c("cert.connect")}</p>}
      </Panel>
      {busy && <p role="status">{c("cert.sending")}</p>}
      {error && <ErrorMessage error={localizeError(error)} />}
      {sent && (
        <p className="banner" role="status">
          {c("cert.sent")}
        </p>
      )}
      {cert && (
        <div className="workspace-scroll">
          <Panel title={cert.subject}>
            <dl className="facts">
              {Object.entries(cert)
                .filter(([key]) => key !== "extensions")
                .map(([key, v]) => (
                  <div className="record-field" key={key}>
                    <dt>{key === "sha256" ? "SHA-256" : c(`cert.${key}` as WorkspaceKey)}</dt>
                    <dd>{fieldValue(key, v)}</dd>
                  </div>
                ))}
            </dl>
            <p>{c("cert.trust")}</p>
          </Panel>
        </div>
      )}
      <details className="panel">
        <summary>{c("cert.help")}</summary>
        <p>{c("cert.advice")}</p>
      </details>
      {confirm && cert && device && (
        <Dialog
          title={c("cert.confirm")}
          onClose={() => setConfirm(false)}
          actions={[
            { label: c("cancel"), onClick: () => setConfirm(false) },
            { label: c("cert.send"), primary: true, onClick: () => void install() },
          ]}
        >
          <p>{device.display_name}</p>
          <p>{cert.subject}</p>
          <p className="mono">SHA-256: {cert.sha256}</p>
          <p>{c("cert.confirmText")}</p>
        </Dialog>
      )}
    </div>
  );
}
