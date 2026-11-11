import { selectDeviceForOperation } from "../deviceRouting";
import { Icon } from "../components/Icon";
import { useCallback, useEffect, useRef, useState } from "react";
import { toErrorInfo } from "../api";
import { Dialog } from "../fm/Dialog";
import { formatSize } from "../fm/paths";
import { useI18n, type MessageKey } from "../i18n";
import { ErrorMessage, Panel, StateMessage } from "../components/Ui";
import type { AppModel } from "../useAppModel";
import type {
  ApplyReport,
  BackupInfo,
  ErrorInfo,
  HostJobData,
  HostJobRequest,
  OperationRequest,
  PlanCounts,
  PlanItem,
  Progress,
  VerifyReport,
} from "../types";

/** Единый вид результата для заданий на компьютере и операций устройства. */
type Data =
  | {
      kind: "created";
      files: number;
      bytes: number;
      skipped: number;
      complete: boolean;
      verified: boolean;
    }
  | { kind: "verified"; report: VerifyReport }
  | { kind: "plan"; counts: PlanCounts; items: PlanItem[]; truncated: boolean }
  | { kind: "applied"; report: ApplyReport };

interface Outcome {
  outcome: "success" | "failed" | "cancelled";
  data: Data | null;
  error: ErrorInfo | null;
  uncertain: boolean;
}

function fromHost(d: HostJobData | null): Data | null {
  if (!d) return null;
  switch (d.type) {
    case "backup_created":
      return { kind: "created", ...d };
    case "backup_verified":
      return { kind: "verified", report: d.report };
    case "backup_plan":
      return { kind: "plan", counts: d.counts, items: d.items, truncated: d.truncated };
    case "backup_applied":
      return { kind: "applied", report: d.report };
    case "media_generated":
      return null;
  }
}

function fromDevice(d: import("../types").OperationData | null): Data | null {
  if (!d) return null;
  if (d.type === "backup_created") return { kind: "created", ...d };
  if (d.type === "backup_plan")
    return { kind: "plan", counts: d.counts, items: d.items, truncated: d.truncated };
  if (d.type === "backup_applied") {
    const { type: _t, ...report } = d;
    void _t;
    return { kind: "applied", report };
  }
  return null;
}

const splitList = (s: string) =>
  s
    .split(/[\n,;]/)
    .map((x) => x.trim())
    .filter(Boolean);

export function BackupsPage({ model }: { model: AppModel }) {
  const { t, lang } = useI18n();
  const { backend, settings } = model;
  const device = selectDeviceForOperation(model.devices, "files.read") ?? null;
  const entry = (op: string) => device?.matrix?.entries.find((e) => e.operation === op);
  const deviceRead =
    !!device && !!entry("files.list") && entry("files.list")!.availability !== "unsupported";
  const deviceWrite =
    !!device && !!entry("files.write") && entry("files.write")!.availability !== "unsupported";
  const deviceRoot = device?.origin === "mock" ? "E:/" : "/";

  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // создание
  const [srcMode, setSrcMode] = useState<"local" | "device">("local");
  const [localSrc, setLocalSrc] = useState("");
  const [devPath, setDevPath] = useState(deviceRoot);
  const [label, setLabel] = useState("");
  const [exclude, setExclude] = useState("");

  // восстановление
  const [selected, setSelected] = useState<string | null>(null);
  const [tgtMode, setTgtMode] = useState<"local" | "device">("local");
  const [localTgt, setLocalTgt] = useState("");
  const [devTgt, setDevTgt] = useState(deviceRoot);
  const [only, setOnly] = useState("");
  const [confirmApply, setConfirmApply] = useState(false);

  // выполнение
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  const [result, setResult] = useState<{ title: MessageKey; out: Outcome; name?: string } | null>(
    null,
  );
  const [lastPlan, setLastPlan] = useState<{ name: string; counts: PlanCounts } | null>(null);
  const [reportPath, setReportPath] = useState<string | null>(null);

  useEffect(() => setDevPath(deviceRoot), [deviceRoot]);
  useEffect(() => setDevTgt(deviceRoot), [deviceRoot]);

  const refresh = useCallback(async () => {
    try {
      setBackups(await backend.backupList());
    } catch (e) {
      setError(toErrorInfo(e));
    }
  }, [backend]);
  useEffect(() => void refresh(), [refresh, settings?.backup_root]);

  async function execute(
    title: MessageKey,
    runner: (
      poll: (fn: () => Promise<Progress | null>) => void,
      setCancel: (c: () => void) => void,
    ) => Promise<Outcome>,
  ) {
    setError(null);
    setNotice(null);
    setReportPath(null);
    setRunning(title);
    setProgress(null);
    let timer: ReturnType<typeof setInterval> | undefined;
    try {
      const out = await runner(
        (fn) => {
          timer = setInterval(
            () =>
              fn()
                .then((p) => p && setProgress(p))
                .catch(() => undefined),
            150,
          );
        },
        (c) => {
          cancelRef.current = c;
        },
      );
      return out;
    } catch (e) {
      setError(toErrorInfo(e));
      return null;
    } finally {
      if (timer) clearInterval(timer);
      cancelRef.current = null;
      setRunning(null);
      setProgress(null);
      void refresh();
    }
  }

  const hostJob = (title: MessageKey, req: HostJobRequest, name?: string) =>
    execute(title, async (poll, setCancel) => {
      const id = await backend.startHostJob(req);
      setCancel(() => void backend.cancelHostJob(id));
      poll(() => backend.hostJobProgress(id));
      const r = await backend.awaitHostJob(id);
      return { outcome: r.outcome, data: fromHost(r.data), error: r.error, uncertain: false };
    }).then((out) => {
      if (out) {
        const created = out.data?.kind === "created" ? undefined : name;
        setResult({ title, out, name: created });
      }
      return out;
    });

  const deviceJob = (title: MessageKey, req: OperationRequest, name?: string) =>
    execute(title, async (poll, setCancel) => {
      if (!device) throw new Error("устройство не подключено");
      const id = await backend.startOperation(device.id, req);
      setCancel(() => void backend.cancelOperation(id));
      poll(() => backend.operationProgress(id));
      const r = await backend.awaitOperation(id);
      return {
        outcome: r.outcome,
        data: fromDevice(r.data),
        error: r.error,
        uncertain: r.uncertain,
      };
    }).then((out) => {
      if (out) setResult({ title, out, name });
      return out;
    });

  const create = () => {
    const ex = splitList(exclude);
    if (srcMode === "local") {
      if (!localSrc) return setNotice(t("bk.pickSource"));
      return void hostJob("bk.create", {
        type: "backup.create_local",
        source: localSrc,
        label,
        exclude: ex,
      });
    }
    return void deviceJob("bk.create", {
      type: "backup.create",
      path: devPath,
      dest: "",
      label,
      exclude: ex,
    });
  };

  const verify = (name: string) => void hostJob("bk.verify", { type: "backup.verify", name }, name);

  const restore = async (apply: boolean) => {
    if (!selected) return;
    const o = only.trim() || null;
    if (tgtMode === "local") {
      if (!localTgt) return setNotice(t("bk.pickTarget"));
      const out = await hostJob(
        "bk.restore",
        { type: "backup.restore_local", name: selected, target: localTgt, only: o, apply },
        selected,
      );
      if (out?.data?.kind === "plan") setLastPlan({ name: selected, counts: out.data.counts });
      if (out?.data?.kind === "applied") setLastPlan(null);
    } else {
      const out = await deviceJob(
        "bk.restore",
        { type: "backup.restore", backup: selected, path: devTgt, only: o, apply },
        selected,
      );
      if (out?.data?.kind === "plan") setLastPlan({ name: selected, counts: out.data.counts });
      if (out?.data?.kind === "applied") setLastPlan(null);
    }
  };

  const exportReport = async () => {
    if (!result) return;
    const json = JSON.stringify({ schema: "yeskia.backup.report/1", ...result }, null, 2);
    try {
      setReportPath(await backend.saveReport(`backup-report-${Date.now()}`, json));
    } catch (e) {
      setError(toErrorInfo(e));
    }
  };

  const status = (b: BackupInfo) =>
    !b.manifest_ok
      ? t("bk.status.badManifest")
      : b.interrupted
        ? t("bk.status.interrupted")
        : b.complete
          ? t("bk.status.complete")
          : t("bk.status.incomplete");

  const planFor = lastPlan && lastPlan.name === selected ? lastPlan.counts : null;
  const canApply =
    !!planFor && planFor.new > 0 && (tgtMode === "local" || (deviceWrite && !!device)) && !running;

  return (
    <div className="compact-tools backups-workspace">
      <h1>{t("bk.title")}</h1>
      <p role="note" className="banner banner-warn">
        {t("bk.scope")}
      </p>
      {error && <ErrorMessage error={error} />}
      {notice && (
        <p role="status" className="banner">
          {notice}
        </p>
      )}

      <Panel title={t("bk.storage")}>
        <p className="mono">{settings?.backup_root ?? t("bk.storageDefault")}</p>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            disabled={!settings || !!running}
            onClick={async () => {
              const p = await backend.pickFolder();
              if (p) void model.updateSettings({ backup_root: p });
            }}
          >
            {t("bk.storageChoose")}
          </button>
        </div>
      </Panel>

      <Panel title={t("bk.createTitle")}>
        <div role="radiogroup" aria-label={t("bk.source")} className="btn-row">
          <label>
            <input
              type="radio"
              name="src"
              checked={srcMode === "local"}
              onChange={() => setSrcMode("local")}
            />{" "}
            {t("bk.source.local")}
          </label>
          <label>
            <input
              type="radio"
              name="src"
              checked={srcMode === "device"}
              disabled={!deviceRead}
              onChange={() => setSrcMode("device")}
            />{" "}
            {t("bk.source.device")}
          </label>
        </div>
        {!deviceRead && (
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("bk.deviceUnavailable")}
          </p>
        )}
        {srcMode === "local" ? (
          <div className="btn-row">
            <button
              className="btn btn-secondary"
              disabled={!!running}
              onClick={async () => setLocalSrc((await backend.pickFolder()) ?? localSrc)}
            >
              {t("bk.pickFolder")}
            </button>
            <span className="mono">{localSrc || "—"}</span>
          </div>
        ) : (
          <label className="field">
            {t("bk.devicePath")}{" "}
            <input
              value={devPath}
              onChange={(e) => setDevPath(e.target.value)}
              aria-label={t("bk.devicePath")}
            />
          </label>
        )}
        <label className="field">
          {t("bk.label")}{" "}
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            aria-label={t("bk.label")}
          />
        </label>
        <label className="field">
          {t("bk.exclude")}
          <textarea
            value={exclude}
            onChange={(e) => setExclude(e.target.value)}
            aria-label={t("bk.exclude")}
            rows={2}
            style={{ display: "block", width: "100%" }}
          />
          <span className="hint">{t("bk.excludeHint")}</span>
        </label>
        <div className="btn-row">
          <button className="btn" disabled={!!running} onClick={create}>
            <Icon name="archive" />
            {t("bk.create")}
          </button>
        </div>
      </Panel>

      {running && (
        <StateMessage kind="empty" title={t("bk.running")}>
          <p>
            {progress && progress.total > 0
              ? t("bk.progress", { done: progress.done, total: progress.total })
              : "…"}
          </p>
          <button className="btn btn-secondary" onClick={() => cancelRef.current?.()}>
            {t("bk.stop")}
          </button>
        </StateMessage>
      )}

      <Panel title={t("bk.listTitle")}>
        {backups.length === 0 ? (
          <StateMessage kind="empty" title={t("bk.empty")} />
        ) : (
          <table className="data">
            <thead>
              <tr>
                <th scope="col">{t("bk.col.name")}</th>
                <th scope="col">{t("bk.col.date")}</th>
                <th scope="col">{t("bk.col.files")}</th>
                <th scope="col">{t("bk.col.size")}</th>
                <th scope="col">{t("bk.col.status")}</th>
                <th scope="col" />
              </tr>
            </thead>
            <tbody>
              {backups.map((b) => (
                <tr key={b.name} aria-selected={selected === b.name}>
                  <td>
                    {b.label || b.name}
                    <div className="mono hint" style={{ marginLeft: 0 }}>
                      {b.name}
                    </div>
                  </td>
                  <td>{b.created_at_ms ? new Date(b.created_at_ms).toLocaleString(lang) : "—"}</td>
                  <td>{b.total_files}</td>
                  <td>{formatSize(b.total_bytes)}</td>
                  <td>{status(b)}</td>
                  <td>
                    <button
                      className="btn btn-secondary"
                      disabled={!!running}
                      onClick={() => verify(b.name)}
                    >
                      {t("bk.verify")}
                    </button>{" "}
                    <button
                      className="btn btn-secondary"
                      disabled={!!running}
                      onClick={() => {
                        setSelected(b.name);
                        setLastPlan(null);
                      }}
                    >
                      {t("bk.restoreOpen")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>

      {selected && (
        <Panel title={t("bk.restoreTitle", { name: selected })}>
          <p>{t("bk.restoreRules")}</p>
          <div role="radiogroup" aria-label={t("bk.target")} className="btn-row">
            <label>
              <input
                type="radio"
                name="tgt"
                checked={tgtMode === "local"}
                onChange={() => setTgtMode("local")}
              />{" "}
              {t("bk.target.local")}
            </label>
            <label>
              <input
                type="radio"
                name="tgt"
                checked={tgtMode === "device"}
                disabled={!deviceRead}
                onChange={() => setTgtMode("device")}
              />{" "}
              {t("bk.target.device")}
            </label>
          </div>
          {tgtMode === "local" ? (
            <div className="btn-row">
              <button
                className="btn btn-secondary"
                disabled={!!running}
                onClick={async () => setLocalTgt((await backend.pickFolder()) ?? localTgt)}
              >
                {t("bk.pickFolder")}
              </button>
              <span className="mono">{localTgt || "—"}</span>
            </div>
          ) : (
            <>
              <label className="field">
                {t("bk.devicePath")}{" "}
                <input
                  value={devTgt}
                  onChange={(e) => setDevTgt(e.target.value)}
                  aria-label={t("bk.targetPath")}
                />
              </label>
              {!deviceWrite && (
                <p className="hint" style={{ marginLeft: 0 }}>
                  {t("fm.writeUnavailable")}
                </p>
              )}
            </>
          )}
          <label className="field">
            {t("bk.only")}{" "}
            <input
              value={only}
              onChange={(e) => setOnly(e.target.value)}
              aria-label={t("bk.only")}
            />
          </label>
          <div className="btn-row">
            <button className="btn" disabled={!!running} onClick={() => void restore(false)}>
              {t("bk.plan")}
            </button>
            <button
              className="btn btn-secondary"
              disabled={!canApply}
              onClick={() => setConfirmApply(true)}
            >
              {t("bk.apply", { n: planFor?.new ?? 0 })}
            </button>
          </div>
        </Panel>
      )}

      {result && <ResultPanel result={result} onExport={exportReport} reportPath={reportPath} />}

      {confirmApply && (
        <Dialog
          title={t("bk.confirm.title")}
          onClose={() => setConfirmApply(false)}
          actions={[
            { label: t("fm.consent.cancel"), onClick: () => setConfirmApply(false) },
            {
              label: t("bk.confirm.ok"),
              primary: true,
              onClick: () => {
                setConfirmApply(false);
                void restore(true);
              },
            },
          ]}
        >
          <p>{t("bk.confirm.text", { n: planFor?.new ?? 0 })}</p>
        </Dialog>
      )}
    </div>
  );
}

function ResultPanel({
  result,
  onExport,
  reportPath,
}: {
  result: { title: MessageKey; out: Outcome };
  onExport: () => void;
  reportPath: string | null;
}) {
  const { t } = useI18n();
  const { out } = result;
  return (
    <Panel title={t("bk.result")}>
      <p data-testid="bk-outcome">
        {t(result.title)} · {t(`bk.outcome.${out.outcome}` as MessageKey)}
      </p>
      {out.error && (
        <p>
          {out.error.code}: {out.error.message}
        </p>
      )}
      {out.uncertain && (
        <StateMessage kind="error" title={t("table.uncertain")}>
          {t("conn.uncertain")}
        </StateMessage>
      )}
      {out.data?.kind === "created" && (
        <>
          <p>
            {t("bk.created", {
              files: out.data.files,
              size: formatSize(out.data.bytes),
              skipped: out.data.skipped,
            })}
          </p>
          <p>{out.data.complete ? t("bk.status.complete") : t("bk.status.incomplete")}</p>
          <p>{out.data.verified ? t("bk.verifiedOk") : t("bk.verifiedNo")}</p>
        </>
      )}
      {out.data?.kind === "verified" && <VerifyView r={out.data.report} />}
      {out.data?.kind === "plan" && (
        <>
          <p data-testid="bk-plan-counts">{t("bk.plan.counts", { ...out.data.counts })}</p>
          <table className="data">
            <thead>
              <tr>
                <th scope="col">{t("table.operation")}</th>
                <th scope="col">{t("table.status")}</th>
                <th scope="col">{t("bk.col.size")}</th>
              </tr>
            </thead>
            <tbody>
              {out.data.items.map((i) => (
                <tr key={i.path}>
                  <td className="mono">{i.path}</td>
                  <td>
                    {t(`bk.plan.${i.status}` as MessageKey)}
                    {i.reason ? ` — ${i.reason}` : ""}
                  </td>
                  <td>{formatSize(i.size)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {out.data.truncated && (
            <p className="hint" style={{ marginLeft: 0 }}>
              {t("bk.truncated")}
            </p>
          )}
        </>
      )}
      {out.data?.kind === "applied" && (
        <p data-testid="bk-applied">
          {t("bk.applied", {
            written: out.data.report.written,
            identical: out.data.report.identical,
            conflicts: out.data.report.conflicts,
          })}
        </p>
      )}
      <div className="btn-row">
        <button className="btn btn-secondary" onClick={onExport}>
          {t("bk.export")}
        </button>
      </div>
      {reportPath && (
        <p className="mono" role="status">
          {t("doctor.saved", { path: reportPath })}
        </p>
      )}
    </Panel>
  );
}

function VerifyView({ r }: { r: VerifyReport }) {
  const { t } = useI18n();
  const list = (title: MessageKey, items: string[]) =>
    items.length > 0 && (
      <>
        <h3>{t(title)}</h3>
        <ul>
          {items.map((p) => (
            <li key={p} className="mono">
              {p}
            </li>
          ))}
        </ul>
      </>
    );
  return (
    <>
      <p data-testid="bk-verify">
        {r.ok ? t("bk.verify.ok", { n: r.files_checked }) : t("bk.verify.bad")}
      </p>
      {!r.manifest_ok && <p>{t("bk.verify.manifest")}</p>}
      {list("bk.verify.corrupt", r.corrupt)}
      {list("bk.verify.missing", r.missing)}
      {list("bk.verify.extra", r.extra)}
      {list("bk.verify.unsafe", r.unsafe_paths)}
    </>
  );
}
