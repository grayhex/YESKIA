import { useCallback, useEffect, useRef, useState } from "react";
import { toErrorInfo } from "../api";
import { Dialog } from "../fm/Dialog";
import { FilePane, type PaneEntry } from "../fm/FilePane";
import { baseName, joinDevice, joinHost, parentDevice, parentHost } from "../fm/paths";
import { useTransfers, type PlanItem, type TransferItem } from "../fm/useTransfers";
import { useI18n, type MessageKey } from "../i18n";
import type { MenuItem } from "../fm/ContextMenu";
import { Panel, StateMessage } from "../components/Ui";
import type { PageId } from "../components/Shell";
import type { AppModel } from "../useAppModel";
import type { ErrorInfo, HostRoot } from "../types";

type Side = "host" | "device";
interface Clip {
  side: Side;
  dir: string;
  names: string[];
}

const MAX_PLAN = 5000;
const MAX_DEPTH = 16;

function usePaneData(load: (() => Promise<PaneEntry[]>) | null, deps: unknown[]) {
  const [entries, setEntries] = useState<PaneEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const seq = useRef(0);
  const reload = useCallback(async () => {
    if (!load) {
      setEntries([]);
      setError(null);
      return;
    }
    const mine = ++seq.current;
    setLoading(true);
    try {
      const e = await load();
      if (mine === seq.current) {
        setEntries(e);
        setError(null);
      }
    } catch (e) {
      if (mine === seq.current) {
        setEntries([]);
        setError(toErrorInfo(e));
      }
    } finally {
      if (mine === seq.current) setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  useEffect(() => {
    void reload();
  }, [reload]);
  return { entries, loading, error, reload };
}

export function FileManagerPage({ model, go }: { model: AppModel; go: (p: PageId) => void }) {
  const { t } = useI18n();
  const { backend } = model;
  const device = model.devices.find((d) => d.connection === "connected") ?? null;
  const files = device?.matrix?.entries.find((e) => e.operation === "files.list");
  const deviceReady = !!device && files !== undefined && files.availability !== "unsupported";
  const write = device?.matrix?.entries.find((e) => e.operation === "files.write");
  const canWrite = !!write && write.availability !== "unsupported";

  const [roots, setRoots] = useState<HostRoot[]>([]);
  const [hostPath, setHostPath] = useState<string | null>(null);
  const [devicePath, setDevicePath] = useState("/");
  const [hostSel, setHostSel] = useState<Set<string>>(new Set());
  const [devSel, setDevSel] = useState<Set<string>>(new Set());
  const [clip, setClip] = useState<Clip | null>(null);
  const [drag, setDrag] = useState<Side | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [consent, setConsent] = useState<(() => void) | null>(null);
  const consented = useRef<string | null>(null);

  // Корень устройства: у mock — диск E:/, у реальных — «/» (хранилища WPD или единственный том).
  const rootOfDevice = device?.origin === "mock" ? "E:/" : "/";
  useEffect(() => {
    setDevicePath(rootOfDevice);
    setDevSel(new Set());
  }, [device?.id, rootOfDevice]);

  useEffect(() => {
    backend
      .hostRoots()
      .then((r) => {
        setRoots(r);
        setHostPath((p) => p ?? r.find((x) => x.kind === "folder")?.path ?? r[0]?.path ?? null);
      })
      .catch(() => undefined);
  }, [backend]);

  const host = usePaneData(
    hostPath
      ? async () => (await backend.hostListDir(hostPath)).map((e) => ({ ...e, disabled: e.lossy }))
      : null,
    [backend, hostPath],
  );

  const listDevice = useCallback(
    async (path: string): Promise<PaneEntry[]> => {
      if (!device) return [];
      const opId = await backend.startOperation(device.id, { type: "files.list", path });
      const r = await backend.awaitOperation(opId);
      if (r.outcome !== "success" || r.data?.type !== "entries") {
        throw (
          r.error ??
          ({
            code: "OPERATION_FAILED",
            message: "не удалось прочитать папку",
            retryable: true,
            action: "retry_manually",
          } as ErrorInfo)
        );
      }
      return r.data.entries;
    },
    [backend, device],
  );
  const dev = usePaneData(deviceReady ? () => listDevice(devicePath) : null, [
    deviceReady,
    listDevice,
    devicePath,
  ]);

  const refreshBoth = useCallback(() => {
    void host.reload();
    void dev.reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host.reload, dev.reload]);

  const transfers = useTransfers(backend, device?.id ?? null, refreshBoth);

  /* ---------- планирование передач ---------- */

  async function planDownload(names: string[], destDir: string): Promise<PlanItem[]> {
    const plan: PlanItem[] = [];
    async function walk(devDir: string, hostDir: string, entries: PaneEntry[], depth: number) {
      for (const e of entries) {
        if (plan.length >= MAX_PLAN) return;
        const from = joinDevice(devDir, e.name);
        const to = joinHost(hostDir, e.name);
        if (!e.is_dir) {
          plan.push({ kind: "download", name: e.name, from, to });
        } else if (depth < MAX_DEPTH) {
          plan.push({ kind: "mkdir", name: e.name, from, to });
          await walk(from, to, await listDevice(from), depth + 1);
        }
      }
    }
    await walk(
      devicePath,
      destDir,
      dev.entries.filter((e) => names.includes(e.name)),
      0,
    );
    return plan;
  }

  async function download(names: string[], destDir: string | null) {
    if (!destDir) return;
    if (names.length === 0) return setNotice(t("fm.nothingSelected"));
    setNotice(null);
    try {
      const plan = await planDownload(names, destDir);
      await transfers.enqueue(plan);
    } catch (e) {
      setNotice(`${toErrorInfo(e).code}: ${toErrorInfo(e).message}`);
    }
  }

  async function downloadAs(name: string) {
    const entry = dev.entries.find((e) => e.name === name);
    if (!entry || entry.is_dir) return setNotice(t("fm.nothingSelected"));
    const dest = await backend.pickSavePath(name);
    if (!dest) return;
    await transfers.enqueue([
      { kind: "download", name: baseName(dest), from: joinDevice(devicePath, name), to: dest },
    ]);
  }

  function upload(sources: { name: string; path: string; is_dir: boolean }[]) {
    if (!canWrite) return setNotice(t("fm.writeUnavailable"));
    if (sources.length === 0) return setNotice(t("fm.nothingSelected"));
    if (sources.some((s) => s.is_dir)) return setNotice(t("fm.foldersUpload"));
    const plan: PlanItem[] = sources.map((s) => ({
      kind: "upload",
      name: s.name,
      from: s.path,
      to: joinDevice(devicePath, s.name),
    }));
    const go = () => void transfers.enqueue(plan);
    // запись на устройство — только после явного согласия (один раз на соединение)
    if (consented.current === device?.id) go();
    else
      setConsent(() => () => {
        consented.current = device?.id ?? null;
        go();
      });
  }

  const hostEntriesByName = (names: string[]) =>
    host.entries
      .filter((e) => names.includes(e.name))
      .map((e) => ({
        name: e.name,
        path: joinHost(hostPath ?? "", e.name),
        is_dir: e.is_dir,
      }));

  async function pickAndUpload() {
    const file = await backend.pickFile();
    if (file) upload([{ name: baseName(file), path: file, is_dir: false }]);
  }

  function paste(target: Side) {
    if (!clip) return setNotice(t("fm.nothingSelected"));
    if (clip.side === target) return setNotice(t("fm.pasteSameSide"));
    if (target === "host") void download(clip.names, hostPath);
    else
      upload(
        host.entries
          .filter((e) => clip.names.includes(e.name))
          .map((e) => ({ name: e.name, path: joinHost(clip.dir, e.name), is_dir: e.is_dir })),
      );
  }

  function copy(side: Side) {
    const names = [...(side === "host" ? hostSel : devSel)];
    if (names.length === 0) return;
    setClip({ side, dir: side === "host" ? (hostPath ?? "") : devicePath, names });
    setNotice(null);
  }

  /* ---------- меню ---------- */

  const hostMenu = (sel: PaneEntry[]): MenuItem[] => [
    {
      label: t("fm.upload"),
      disabled: !canWrite || !deviceReady || sel.length === 0,
      onSelect: () => upload(hostEntriesByName(sel.map((s) => s.name))),
    },
    {
      label: t("fm.reveal"),
      disabled: sel.length === 0,
      onSelect: () =>
        void backend
          .revealInExplorer(joinHost(hostPath ?? "", sel[0].name))
          .catch((e) => setNotice(toErrorInfo(e).message)),
    },
    { label: t("fm.copy"), disabled: sel.length === 0, onSelect: () => copy("host") },
    {
      label: t("fm.paste"),
      disabled: !clip || clip.side === "host",
      onSelect: () => paste("host"),
    },
    { label: t("fm.refresh"), onSelect: () => void host.reload() },
  ];
  const deviceMenu = (sel: PaneEntry[]): MenuItem[] => [
    {
      label: t("fm.download"),
      disabled: sel.length === 0,
      onSelect: () =>
        void download(
          sel.map((s) => s.name),
          hostPath,
        ),
    },
    {
      label: t("fm.downloadAs"),
      disabled: sel.length !== 1 || sel[0].is_dir,
      onSelect: () => void downloadAs(sel[0].name),
    },
    { label: t("fm.copy"), disabled: sel.length === 0, onSelect: () => copy("device") },
    {
      label: t("fm.paste"),
      disabled: !clip || clip.side === "device" || !canWrite,
      onSelect: () => paste("device"),
    },
    { label: t("fm.refresh"), onSelect: () => void dev.reload() },
  ];

  /* ---------- вид ---------- */

  const statusKey = (i: TransferItem) => `fm.status.${i.status}` as MessageKey;

  return (
    <>
      <h1>{t("fm.title")}</h1>
      <p>{t("fm.intro")}</p>
      <p className="hint" style={{ marginLeft: 0 }}>
        {t("fm.shortcuts")}
      </p>
      {notice && (
        <p role="status" className="banner">
          {notice}
        </p>
      )}

      <div className="fm-grid">
        <FilePane
          title={t("fm.host.title")}
          side="host"
          pathLabel={hostPath ?? ""}
          entries={host.entries}
          loading={host.loading}
          error={host.error}
          emptyText={t("fm.empty")}
          canGoUp={!!hostPath && parentHost(hostPath) !== null}
          selection={hostSel}
          onSelection={setHostSel}
          onOpen={(e) => {
            if (hostPath) setHostPath(joinHost(hostPath, e.name));
            setHostSel(new Set());
          }}
          onUp={() => {
            const p = hostPath && parentHost(hostPath);
            if (p) setHostPath(p);
            setHostSel(new Set());
          }}
          onRefresh={() => void host.reload()}
          onCopy={() => copy("host")}
          onPaste={() => paste("host")}
          menuItems={hostMenu}
          onDragStart={() => setDrag("host")}
          onDrop={() => {
            if (drag === "device") void download([...devSel], hostPath);
            setDrag(null);
          }}
          acceptDrop={drag === "device"}
          toolbar={
            <>
              <select
                aria-label={t("fm.host.roots")}
                value=""
                onChange={(e) => {
                  if (e.target.value) {
                    setHostPath(e.target.value);
                    setHostSel(new Set());
                  }
                }}
              >
                <option value="">{t("fm.host.roots")}</option>
                {roots.map((r) => (
                  <option key={r.path} value={r.path}>
                    {r.label}
                  </option>
                ))}
              </select>
              <button
                className="btn btn-secondary"
                onClick={async () => {
                  const p = await backend.pickFolder();
                  if (p) {
                    setHostPath(p);
                    setHostSel(new Set());
                  }
                }}
              >
                {t("fm.host.choose")}
              </button>
              <button
                className="btn btn-secondary"
                disabled={hostSel.size === 0}
                onClick={() =>
                  hostPath &&
                  void backend
                    .revealInExplorer(joinHost(hostPath, [...hostSel][0]))
                    .catch((e) => setNotice(toErrorInfo(e).message))
                }
              >
                {t("fm.reveal")}
              </button>
            </>
          }
        />

        {deviceReady ? (
          <FilePane
            title={t("fm.device.title")}
            side="device"
            pathLabel={devicePath}
            entries={dev.entries}
            loading={dev.loading}
            error={dev.error}
            emptyText={t("fm.empty")}
            canGoUp={parentDevice(devicePath) !== null}
            selection={devSel}
            onSelection={setDevSel}
            onOpen={(e) => {
              setDevicePath(joinDevice(devicePath, e.name));
              setDevSel(new Set());
            }}
            onUp={() => {
              const p = parentDevice(devicePath);
              if (p) setDevicePath(p);
              setDevSel(new Set());
            }}
            onRefresh={() => void dev.reload()}
            onCopy={() => copy("device")}
            onPaste={() => paste("device")}
            menuItems={deviceMenu}
            onDragStart={() => setDrag("device")}
            onDrop={() => {
              if (drag === "host") upload(hostEntriesByName([...hostSel]));
              setDrag(null);
            }}
            acceptDrop={drag === "host" && canWrite}
            toolbar={
              <>
                <button
                  className="btn"
                  disabled={devSel.size === 0 || transfers.busy}
                  onClick={() => void download([...devSel], hostPath)}
                >
                  {t("fm.download")}
                </button>
                <button
                  className="btn btn-secondary"
                  disabled={devSel.size !== 1 || transfers.busy}
                  onClick={() => void downloadAs([...devSel][0])}
                >
                  {t("fm.downloadAs")}
                </button>
                {canWrite && (
                  <>
                    <button
                      className="btn btn-secondary"
                      disabled={hostSel.size === 0 || transfers.busy}
                      onClick={() => upload(hostEntriesByName([...hostSel]))}
                    >
                      {t("fm.upload")}
                    </button>
                    <button
                      className="btn btn-secondary"
                      disabled={transfers.busy}
                      onClick={() => void pickAndUpload()}
                    >
                      {t("fm.uploadPick")}
                    </button>
                  </>
                )}
              </>
            }
          />
        ) : (
          <Panel title={t("fm.device.title")}>
            {!device ? (
              <StateMessage kind="empty" title={t("fm.device.none")}>
                <p>{t("fm.device.noneHint")}</p>
                <button className="btn" onClick={() => go("connection")}>
                  {t("home.goConnection")}
                </button>
              </StateMessage>
            ) : (
              <StateMessage kind="unavailable" title={t("fm.device.noFiles")}>
                <p>{t("fm.device.noFilesHint")}</p>
                <button className="btn" onClick={() => go("doctor")}>
                  {t("nav.doctor")}
                </button>
              </StateMessage>
            )}
          </Panel>
        )}
      </div>

      {deviceReady && !canWrite && (
        <p className="hint" style={{ marginLeft: 0 }}>
          {t("fm.writeUnavailable")}
        </p>
      )}

      {transfers.items.length > 0 && (
        <Panel title={t("fm.transfers")}>
          <ul className="transfer-list" aria-label={t("fm.transfers")}>
            {transfers.items.map((i) => (
              <li key={i.id} data-status={i.status}>
                <span className="mono">
                  {i.kind === "download" ? "↓" : i.kind === "upload" ? "↑" : "📁"}
                </span>{" "}
                {i.name} — {t(statusKey(i))}
                {i.status === "done" && i.kind !== "mkdir" && i.sha256 && (
                  <span>
                    {" "}
                    · {t("fm.verified")}
                    {i.replaced ? ` · ${t("fm.replaced")}` : ""}
                  </span>
                )}
                {i.status === "running" && i.progress && i.progress.total > 0 && (
                  <progress max={i.progress.total} value={i.progress.done} aria-label={i.name} />
                )}
                {i.error && (
                  <span className="error-inline">
                    {" "}
                    · {i.error.code}: {i.error.message}
                  </span>
                )}
                {i.uncertain && <strong> · {t("fm.uncertain")}</strong>}
              </li>
            ))}
          </ul>
          <div className="btn-row">
            <button
              className="btn btn-secondary"
              disabled={!transfers.busy}
              onClick={transfers.cancel}
            >
              {t("fm.cancel")}
            </button>
            <button
              className="btn btn-secondary"
              disabled={transfers.busy}
              onClick={transfers.clear}
            >
              {t("fm.clear")}
            </button>
          </div>
        </Panel>
      )}

      {transfers.conflict && (
        <ConflictDialog
          name={baseName(transfers.conflict.to)}
          onChoice={transfers.resolveConflict}
        />
      )}
      {consent && (
        <Dialog
          title={t("fm.consent.title")}
          onClose={() => setConsent(null)}
          actions={[
            {
              label: t("fm.consent.cancel"),
              onClick: () => setConsent(null),
            },
            {
              label: t("fm.consent.ok"),
              primary: true,
              onClick: () => {
                const run = consent;
                setConsent(null);
                run();
              },
            },
          ]}
        >
          <p>{t("fm.consent.text", { device: device?.display_name ?? "" })}</p>
        </Dialog>
      )}
    </>
  );
}

function ConflictDialog({
  name,
  onChoice,
}: {
  name: string;
  onChoice: (c: { action: "skip" | "rename" | "replace"; all: boolean }) => void;
}) {
  const { t } = useI18n();
  const [all, setAll] = useState(false);
  return (
    <Dialog
      title={t("fm.conflict.title")}
      onClose={() => onChoice({ action: "skip", all: false })}
      actions={[
        { label: t("fm.conflict.skip"), onClick: () => onChoice({ action: "skip", all }) },
        {
          label: t("fm.conflict.rename"),
          primary: true,
          onClick: () => onChoice({ action: "rename", all }),
        },
        { label: t("fm.conflict.replace"), onClick: () => onChoice({ action: "replace", all }) },
      ]}
    >
      <p>{t("fm.conflict.text", { name })}</p>
      <label>
        <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />{" "}
        {t("fm.conflict.all")}
      </label>
    </Dialog>
  );
}
