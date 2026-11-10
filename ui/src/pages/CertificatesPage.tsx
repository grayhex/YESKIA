import { useState } from "react";
import { ErrorMessage, Panel } from "../components/Ui";
import type { AppModel } from "../useAppModel";
import type { CertInfo, ErrorInfo } from "../types";
import { toErrorInfo } from "../api";
export function CertificatesPage({ model }: { model: AppModel }) {
  const [cert, setCert] = useState<CertInfo | null>(null);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const inspect = async () => {
    setBusy(true);
    setError(null);
    try {
      const path = await model.backend.pickFile();
      if (path) {
        setCert(null);
        setCert(await model.backend.inspectCertificate(path));
      }
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="workspace-page">
      <h1>Сертификаты S60</h1>
      <Panel title="Проверка сертификата">
        <p>
          Откройте публичный сертификат CER, DER или PEM, чтобы проверить издателя, срок действия и
          отпечаток.
        </p>
        <button className="btn" disabled={busy} onClick={() => void inspect()}>
          ⊕ Открыть сертификат
        </button>
      </Panel>
      {error && <ErrorMessage error={error} />}
      {cert && (
        <Panel title={cert.subject}>
          <dl className="facts">
            {Object.entries(cert)
              .filter(([key]) => key !== "extensions")
              .map(([key, v]) => (
                <div className="record-field" key={key}>
                  <dt>
                    {{
                      issuer: "Издатель",
                      subject: "Владелец",
                      serial: "Серийный номер",
                      sha256: "SHA-256",
                      not_before: "Действует с",
                      not_after: "Действует до",
                      validity: "Срок действия",
                      key_type: "Ключ",
                      self_issued: "Самоизданный",
                    }[key] ?? key}
                  </dt>
                  <dd>{String(v)}</dd>
                </div>
              ))}
          </dl>
          <p>Доверие этого телефона к сертификату проверяется отдельно.</p>
        </Panel>
      )}
      <details className="panel">
        <summary>Ошибка сертификата при установке</summary>
        <p>
          Проверьте дату и время телефона и получите актуальный пакет у издателя. Сертификаты внутри
          SIS/SISX можно проверить в разделе «Приложения → Мои установщики».
        </p>
      </details>
    </div>
  );
}
