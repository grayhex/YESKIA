import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toErrorInfo } from "../api";
import { Dialog } from "../fm/Dialog";
import { formatSize } from "../fm/paths";
import { useI18n, type MessageKey } from "../i18n";
import { ErrorMessage, Panel } from "../components/Ui";
import { PHONES } from "../phones/registry";
import type { AppModel } from "../useAppModel";
import type {
  ErrorInfo,
  InspectRecommendation,
  InspectReport,
  LibraryItem,
  PackageFormat,
  PackageSet,
  Progress,
  Provenance,
} from "../types";

const FORMATS: PackageFormat[] = ["sis", "sisx", "jar", "jad"];

interface TransferResult {
  name: string;
  outcome: "success" | "failed" | "cancelled";
  bytes: number | null;
  error: ErrorInfo | null;
  uncertain: boolean;
}

export function PackagesPage({ model }: { model: AppModel }) {
  const { t } = useI18n();
  const { backend } = model;
  const device = model.devices.find((d) => d.connection === "connected") ?? null;
  const transferEntry = device?.matrix?.entries.find((e) => e.operation === "packages.transfer");
  const canTransfer = !!device && !!transferEntry && transferEntry.availability !== "unsupported";
  const deviceRoot = device?.origin === "mock" ? "E:/" : "/";

  const [items, setItems] = useState<LibraryItem[]>([]);
  const [sets, setSets] = useState<PackageSet[]>([]);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // фильтры
  const [text, setText] = useState("");
  const [format, setFormat] = useState<PackageFormat | "">("");
  const [modelId, setModelId] = useState("");
  const [onlyFit, setOnlyFit] = useState(false);
  const [setFilter, setSetFilter] = useState<number | null>(null);

  // происхождение для импорта
  const [prov, setProv] = useState<Provenance>({
    source: "",
    url: "",
    license: "",
    rights: "unknown",
    note: "",
  });
  const [newSet, setNewSet] = useState("");

  // отчёт, наборы, передача
  const [report, setReport] = useState<InspectReport | null>(null);
  const [reportPath, setReportPath] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [diagnosis, setDiagnosis] = useState<InspectRecommendation | null>(null);
  const [addTo, setAddTo] = useState<LibraryItem | null>(null);
  const [chosenSet, setChosenSet] = useState<number | "">("");
  const [sendItem, setSendItem] = useState<LibraryItem | null>(null);
  const [sendDir, setSendDir] = useState(deviceRoot);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  const [transfer, setTransfer] = useState<TransferResult | null>(null);

  useEffect(() => setSendDir(deviceRoot), [deviceRoot]);

  const refresh = useCallback(async () => {
    try {
      const [list, s] = await Promise.all([
        backend.libraryList({
          text,
          format: format || null,
          model: modelId || null,
          only_fitting: onlyFit && !!modelId,
          set: setFilter,
        }),
        backend.librarySets(),
      ]);
      setItems(list);
      setSets(s);
    } catch (e) {
      setError(toErrorInfo(e));
    }
  }, [backend, text, format, modelId, onlyFit, setFilter]);
  useEffect(() => void refresh(), [refresh]);

  const guard = async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    setError(null);
    setNotice(null);
    try {
      return await fn();
    } catch (e) {
      setError(toErrorInfo(e));
      return undefined;
    }
  };

  const importFiles = () =>
    guard(async () => {
      const paths = await backend.pickPackageFiles();
      if (!paths.length) return;
      const added = await backend.libraryImport(paths, prov);
      setNotice(t("pk.imported", { n: added.length }));
      await refresh();
    });

  const inspect = (it: LibraryItem) =>
    guard(async () => {
      setReportPath(null);
      setDiagnosis(null);
      setReport(await backend.libraryInspect(it.id, targetFor(modelId)));
    });

  const remove = (it: LibraryItem) =>
    guard(async () => {
      await backend.libraryRemove(it.id);
      if (report?.sha256 === it.id) setReport(null);
      await refresh();
    });

  const createSet = () =>
    guard(async () => {
      await backend.libraryCreateSet(newSet);
      setNewSet("");
      await refresh();
    });

  const exportReport = () =>
    guard(async () => {
      if (!report) return;
      const path = await backend.saveReport(
        `inspect-${report.sha256.slice(0, 12)}`,
        JSON.stringify(report, null, 2),
      );
      setReportPath(path);
    });

  const diagnose = () => guard(async () => setDiagnosis(await backend.packageDiagnose(message)));

  async function runTransfer(it: LibraryItem, dir: string) {
    if (!device) return;
    setError(null);
    setNotice(null);
    setTransfer(null);
    setRunning(true);
    setProgress(null);
    let timer: ReturnType<typeof setInterval> | undefined;
    try {
      const id = await backend.startPackageTransfer(device.id, it.id, dir);
      cancelRef.current = () => void backend.cancelOperation(id);
      timer = setInterval(
        () =>
          backend
            .operationProgress(id)
            .then((p) => p && setProgress(p))
            .catch(() => undefined),
        150,
      );
      const r = await backend.awaitOperation(id);
      setTransfer({
        name: it.name,
        outcome: r.outcome,
        bytes: r.data && r.data.type === "written" ? r.data.bytes : null,
        error: r.error,
        uncertain: r.uncertain,
      });
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      if (timer) clearInterval(timer);
      cancelRef.current = null;
      setRunning(false);
      setProgress(null);
    }
  }

  const selectedSet = useMemo(
    () => sets.find((s) => s.id === setFilter) ?? null,
    [sets, setFilter],
  );
  const transferHint = !device
    ? t("pk.transferNoDevice")
    : !canTransfer
      ? t("pk.transferUnsupported")
      : null;

  return (
    <>
      <h1>{t("pk.title")}</h1>
      <p role="note" className="banner banner-warn">
        {t("pk.scope")}
      </p>
      {error && <ErrorMessage error={error} />}
      {notice && (
        <p role="status" className="banner">
          {notice}
        </p>
      )}

      <Panel title={t("pk.import")}>
        <details>
          <summary>{t("pk.provenance")}</summary>
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("pk.provenanceHint")}
          </p>
          {(["source", "url", "license", "note"] as const).map((k) => (
            <label className="field" key={k}>
              {t(`pk.${k}`)}{" "}
              <input
                value={prov[k]}
                maxLength={300}
                aria-label={t(`pk.${k}`)}
                onChange={(e) => setProv({ ...prov, [k]: e.target.value })}
              />
            </label>
          ))}
          <label>
            <input
              type="checkbox"
              checked={prov.rights === "user-confirmed"}
              onChange={(e) =>
                setProv({ ...prov, rights: e.target.checked ? "user-confirmed" : "unknown" })
              }
            />{" "}
            {t("pk.rights")}
          </label>
        </details>
        <div className="btn-row">
          <button className="btn" onClick={() => void importFiles()}>
            {t("pk.import")}
          </button>
        </div>
      </Panel>

      <Panel title={t("pk.sets")}>
        <div className="btn-row">
          <label className="field">
            {t("pk.setName")}{" "}
            <input
              value={newSet}
              maxLength={300}
              aria-label={t("pk.setName")}
              onChange={(e) => setNewSet(e.target.value)}
            />
          </label>
          <button className="btn btn-secondary" onClick={() => void createSet()}>
            {t("pk.setCreate")}
          </button>
        </div>
        <ul>
          {sets.map((s) => (
            <li key={s.id}>
              {s.name} · {t("pk.setItems", { n: s.items.length })}{" "}
              <button
                className="btn btn-secondary"
                onClick={() =>
                  void guard(async () => {
                    await backend.libraryDeleteSet(s.id);
                    if (setFilter === s.id) setSetFilter(null);
                    await refresh();
                  })
                }
              >
                {t("pk.setDelete")} «{s.name}»
              </button>
            </li>
          ))}
        </ul>
      </Panel>

      <Panel>
        <div className="btn-row" role="search">
          <label className="field">
            {t("pk.search")}{" "}
            <input
              value={text}
              aria-label={t("pk.search")}
              onChange={(e) => setText(e.target.value)}
            />
          </label>
          <label className="field">
            {t("pk.format")}{" "}
            <select
              value={format}
              aria-label={t("pk.format")}
              onChange={(e) => setFormat(e.target.value as PackageFormat | "")}
            >
              <option value="">{t("pk.allFormats")}</option>
              {FORMATS.map((f) => (
                <option key={f} value={f}>
                  {f.toUpperCase()}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            {t("pk.model")}{" "}
            <select
              value={modelId}
              aria-label={t("pk.model")}
              onChange={(e) => setModelId(e.target.value)}
            >
              <option value="">{t("pk.noModel")}</option>
              {PHONES.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            {t("pk.sets")}{" "}
            <select
              value={setFilter ?? ""}
              aria-label={t("pk.sets")}
              onChange={(e) => setSetFilter(e.target.value === "" ? null : Number(e.target.value))}
            >
              <option value="">{t("pk.setAll")}</option>
              {sets.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <input
              type="checkbox"
              checked={onlyFit}
              disabled={!modelId}
              onChange={(e) => setOnlyFit(e.target.checked)}
            />{" "}
            {t("pk.onlyFit")}
          </label>
        </div>
        {modelId && (
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("pk.fitHint")}
          </p>
        )}
        {selectedSet && <p className="hint">{selectedSet.name}</p>}
        {items.length === 0 ? (
          <p>{t("pk.empty")}</p>
        ) : (
          <table className="data">
            <thead>
              <tr>
                <th scope="col">{t("pk.col.name")}</th>
                <th scope="col">{t("pk.col.version")}</th>
                <th scope="col">{t("pk.col.signed")}</th>
                <th scope="col">{t("pk.col.status")}</th>
                {modelId && <th scope="col">{t("pk.col.fit")}</th>}
                <th scope="col" />
              </tr>
            </thead>
            <tbody>
              {items.map((it) => (
                <tr key={it.id}>
                  <td>
                    {it.title ?? it.name}
                    <div className="mono hint" style={{ marginLeft: 0 }}>
                      {it.name} · {it.format.toUpperCase()} · {formatSize(it.size)}
                      {it.uid ? ` · ${it.uid}` : ""}
                    </div>
                    {it.provenance.rights === "user-confirmed" && (
                      <div className="hint" style={{ marginLeft: 0 }}>
                        {t("pk.rights")}
                      </div>
                    )}
                  </td>
                  <td>{it.version ?? "—"}</td>
                  <td>
                    {it.signed === null
                      ? t("pk.signed.na")
                      : it.signed
                        ? t("pk.signed.yes")
                        : t("pk.signed.no")}
                  </td>
                  <td title={it.error ?? undefined}>{t(`pk.status.${it.status}` as MessageKey)}</td>
                  {modelId && (
                    <td>{it.fit ? t(`pk.fit.${it.fit}` as MessageKey) : t("pk.signed.na")}</td>
                  )}
                  <td>
                    <button
                      className="btn btn-secondary"
                      aria-label={`${t("pk.inspect")}: ${it.name}`}
                      onClick={() => void inspect(it)}
                    >
                      {t("pk.inspect")}
                    </button>{" "}
                    <button
                      className="btn btn-secondary"
                      aria-label={`${t("pk.addToSet")} ${it.name}`}
                      onClick={() => {
                        setAddTo(it);
                        setChosenSet(sets[0]?.id ?? "");
                      }}
                    >
                      {t("pk.addToSet")}
                    </button>{" "}
                    <button
                      className="btn btn-secondary"
                      aria-label={`${t("pk.transfer")} ${it.name}`}
                      disabled={!canTransfer || it.status !== "ok" || running}
                      title={
                        transferHint ?? (it.status !== "ok" ? t("pk.transferBadStatus") : undefined)
                      }
                      onClick={() => setSendItem(it)}
                    >
                      {t("pk.transfer")}
                    </button>{" "}
                    <button
                      className="btn btn-secondary"
                      aria-label={`${t("pk.remove")} ${it.name}`}
                      onClick={() => void remove(it)}
                    >
                      {t("pk.remove")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {transferHint && <p className="hint">{transferHint}</p>}
      </Panel>

      {running && (
        <p role="status">
          {t("pk.running")}
          {progress ? ` ${progress.done}/${progress.total}` : ""}{" "}
          <button className="btn btn-secondary" onClick={() => cancelRef.current?.()}>
            {t("pk.stop")}
          </button>
        </p>
      )}
      {transfer && (
        <Panel title={t("pk.transferTitle")}>
          <p data-testid="pk-transfer">
            {transfer.name} · {t(`bk.outcome.${transfer.outcome}` as MessageKey)}
          </p>
          {transfer.outcome === "success" && (
            <p>{t("pk.transferDone", { size: formatSize(transfer.bytes ?? 0) })}</p>
          )}
          {transfer.error && (
            <p>
              {transfer.error.code}: {transfer.error.message}
            </p>
          )}
          {transfer.uncertain && <p role="alert">{transfer.error?.action}</p>}
        </Panel>
      )}

      {report && (
        <ReportView
          report={report}
          onExport={() => void exportReport()}
          reportPath={reportPath}
          message={message}
          setMessage={setMessage}
          onDiagnose={() => void diagnose()}
          diagnosis={diagnosis}
        />
      )}

      {addTo && (
        <Dialog
          title={t("pk.addToSetTitle")}
          onClose={() => setAddTo(null)}
          actions={[
            { label: t("fm.consent.cancel"), onClick: () => setAddTo(null) },
            {
              label: t("pk.addToSet"),
              primary: true,
              onClick: () => {
                const it = addTo;
                setAddTo(null);
                if (chosenSet !== "")
                  void guard(async () => {
                    await backend.librarySetAdd(chosenSet, it.id);
                    await refresh();
                  });
              },
            },
          ]}
        >
          {sets.length === 0 ? (
            <p>{t("pk.noSets")}</p>
          ) : (
            <label className="field">
              {t("pk.chooseSet")}{" "}
              <select
                value={chosenSet}
                aria-label={t("pk.chooseSet")}
                onChange={(e) => setChosenSet(Number(e.target.value))}
              >
                {sets.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </Dialog>
      )}

      {sendItem && (
        <Dialog
          title={t("pk.transferTitle")}
          onClose={() => setSendItem(null)}
          actions={[
            { label: t("fm.consent.cancel"), onClick: () => setSendItem(null) },
            {
              label: t("pk.transferOk"),
              primary: true,
              onClick: () => {
                const it = sendItem;
                setSendItem(null);
                void runTransfer(it, sendDir);
              },
            },
          ]}
        >
          <p>{t("pk.transferText", { name: sendItem.name, device: device?.display_name ?? "" })}</p>
          <label className="field">
            {t("pk.transferDir")}{" "}
            <input
              value={sendDir}
              aria-label={t("pk.transferDir")}
              onChange={(e) => setSendDir(e.target.value)}
            />
          </label>
        </Dialog>
      )}
    </>
  );
}

function targetFor(model: string) {
  return { model, rm: "", firmware: "", os: "" };
}

function ReportView({
  report: r,
  onExport,
  reportPath,
  message,
  setMessage,
  onDiagnose,
  diagnosis,
}: {
  report: InspectReport;
  onExport: () => void;
  reportPath: string | null;
  message: string;
  setMessage: (m: string) => void;
  onDiagnose: () => void;
  diagnosis: InspectRecommendation | null;
}) {
  const { t } = useI18n();
  const integrityOk =
    r.integrity &&
    r.integrity.uid_checksum_ok &&
    r.integrity.package_uid_matches_header &&
    r.integrity.controller_crc_ok !== false &&
    r.integrity.data_crc_ok !== false &&
    r.integrity.payload_hashes_ok !== false;
  return (
    <Panel title={`${t("pk.report")}: ${r.name}`}>
      <p className="mono">SHA-256: {r.sha256}</p>
      <p data-testid="pk-compat" role="status">
        {t(`pk.compat.${r.compatibility}` as MessageKey)}
      </p>
      <p className="hint" style={{ marginLeft: 0 }}>
        {r.compatibility_scope}. {t("pk.notTested")}
      </p>
      {r.integrity && (
        <>
          <h3>{t("pk.integrity")}</h3>
          <p>{integrityOk ? t("pk.integrity.ok") : t("pk.integrity.bad")}</p>
        </>
      )}
      {r.signatures && (
        <>
          <h3>{t("pk.sig")}</h3>
          <p data-testid="pk-sig">{t(`pk.sig.${r.signatures.status}` as MessageKey)}</p>
          {r.signatures.status !== "unsigned" && <p>{t("pk.sig.trust")}</p>}
          {r.signatures.chains.some((c) => c.certificates.length > 0) && (
            <>
              <h4>{t("pk.certs")}</h4>
              <ul>
                {r.signatures.chains.flatMap((c) =>
                  c.certificates.map((cert) => (
                    <li key={cert.sha256}>
                      {cert.subject} · {cert.not_before.slice(0, 10)} —{" "}
                      {cert.not_after.slice(0, 10)} · {t(`pk.cert.${cert.validity}` as MessageKey)}{" "}
                      · {cert.key_type}
                    </li>
                  )),
                )}
              </ul>
            </>
          )}
        </>
      )}
      {r.dependencies && (
        <>
          <h3>{t("pk.deps")}</h3>
          <p data-testid="pk-deps">{t(`pk.deps.${r.dependencies.verdict}` as MessageKey)}</p>
          <ul>
            {r.dependencies.packages.map((p) => (
              <li key={`${p.uid}-${p.name}`}>
                {p.name} {p.uid ?? ""} — {p.status}
              </li>
            ))}
          </ul>
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("pk.imports")}
          </p>
        </>
      )}
      {r.issues.length > 0 && (
        <>
          <h3>{t("pk.issues")}</h3>
          <ul data-testid="pk-issues">
            {r.recommendations.map((x) => (
              <li key={x.issue}>
                <strong>{x.issue}</strong>: {x.advice}
              </li>
            ))}
          </ul>
        </>
      )}
      <h3>{t("pk.limits")}</h3>
      <ul>
        {r.limitations.map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
      <div className="btn-row">
        <button className="btn btn-secondary" onClick={onExport}>
          {t("pk.export")}
        </button>
      </div>
      {reportPath && (
        <p role="status" className="mono">
          {t("pk.exported", { path: reportPath })}
        </p>
      )}
      <h3>{t("pk.diagnose")}</h3>
      <div className="btn-row">
        <label className="field">
          {t("pk.diagnoseHint")}{" "}
          <input
            value={message}
            maxLength={2000}
            aria-label={t("pk.diagnoseHint")}
            onChange={(e) => setMessage(e.target.value)}
          />
        </label>
        <button className="btn btn-secondary" onClick={onDiagnose}>
          {t("pk.diagnoseRun")}
        </button>
      </div>
      {diagnosis && (
        <p data-testid="pk-diagnosis">
          <strong>{diagnosis.issue}</strong>: {diagnosis.advice} <em>{t("pk.diagnoseNote")}</em>
        </p>
      )}
    </Panel>
  );
}
