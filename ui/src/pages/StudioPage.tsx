import { Icon } from "../components/Icon";
import { useCallback, useEffect, useRef, useState } from "react";
import { toErrorInfo } from "../api";
import { Dialog } from "../fm/Dialog";
import { formatSize } from "../fm/paths";
import { useI18n, type MessageKey } from "../i18n";
import { ErrorMessage, Panel } from "../components/Ui";
import { PRESETS, RINGTONE_PRESETS, SCREENS } from "../mediaPresets";
import { PHONES } from "../phones/registry";
import type { AppModel } from "../useAppModel";
import type {
  AudioPreview,
  BitmapPreview,
  ErrorInfo,
  Generated,
  LibraryItem,
  MediaInfo,
  Progress,
  RingtoneOptions,
  ThemeReport,
  ThemeSource,
  ToolInfo,
} from "../types";

const BITRATES = [32, 48, 64, 96, 128, 160, 192, 256, 320];
const RATES = [8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000];

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
const stem = (p: string) => baseName(p).replace(/\.[^.]+$/, "");
const num = (v: string) => (v.trim() === "" ? NaN : Number(v));
const sec = (n: number) => String(Math.round(n * 10) / 10);

interface Done {
  title: string;
  outcome: "success" | "failed" | "cancelled";
  generated: Generated | null;
  error: ErrorInfo | null;
}

export function StudioPage({ model }: { model: AppModel }) {
  const { t } = useI18n();
  const { backend } = model;
  const device = model.devices.find((d) => d.connection === "connected") ?? null;
  const writeEntry = device?.matrix?.entries.find((e) => e.operation === "files.write");
  const canWrite = !!device && !!writeEntry && writeEntry.availability !== "unsupported";
  const deviceRoot = device?.origin === "mock" ? "E:/" : "/";

  const [error, setError] = useState<ErrorInfo | null>(null);
  const [tool, setTool] = useState<ToolInfo | null>(null);
  const [toolError, setToolError] = useState<ErrorInfo | null>(null);

  // источник
  const [file, setFile] = useState<string | null>(null);
  const [info, setInfo] = useState<MediaInfo | null>(null);
  const [peaks, setPeaks] = useState<number[]>([]);
  const [modelId, setModelId] = useState("");

  // рингтон
  const [start, setStart] = useState("0");
  const [end, setEnd] = useState("30");
  const [opt, setOpt] = useState({
    fadeIn: "0.1",
    fadeOut: "0.3",
    mono: true,
    normalize: true,
    lufs: "-16",
    bitrate: 128,
    rate: 44100,
  });
  const [presetId, setPresetId] = useState("");

  // выполнение
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  const [done, setDone] = useState<Done | null>(null);
  const [audio, setAudio] = useState<AudioPreview | null>(null);
  const [transferred, setTransferred] = useState(false);
  const [sendDlg, setSendDlg] = useState(false);
  const [sendPath, setSendPath] = useState("");

  // темы
  const [themeSource, setThemeSource] = useState<ThemeSource | null>(null);
  const [theme, setTheme] = useState<ThemeReport | null>(null);
  const [bitmaps, setBitmaps] = useState<BitmapPreview[]>([]);
  const [library, setLibrary] = useState<LibraryItem[]>([]);

  const refreshTool = useCallback(async () => {
    try {
      setTool(await backend.mediaToolInfo());
      setToolError(null);
    } catch (e) {
      setTool(null);
      setToolError(toErrorInfo(e));
    }
  }, [backend]);
  useEffect(() => void refreshTool(), [refreshTool, model.settings?.ffmpeg_dir]);

  useEffect(() => {
    backend
      .libraryList({ text: "", format: null, model: null, only_fitting: false, set: null })
      .then((l) => setLibrary(l.filter((i) => i.format === "sis" || i.format === "sisx")))
      .catch(() => undefined);
  }, [backend]);

  const guard = async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(toErrorInfo(e));
      return undefined;
    }
  };

  const chooseFile = () =>
    guard(async () => {
      const p = await backend.pickMediaFile();
      if (!p) return;
      setFile(p);
      setInfo(null);
      setPeaks([]);
      setDone(null);
      setAudio(null);
      setTransferred(false);
      const i = await backend.mediaProbe(p);
      setInfo(i);
      if (i.audio) {
        const len = Math.min(30, i.duration);
        setStart("0");
        setEnd(sec(len));
        setPeaks(await backend.mediaWaveform(p, 300));
      }
    });

  function applyRingtonePreset(id: "ringtone" | "notification") {
    const p = RINGTONE_PRESETS[id];
    setEnd(sec(Math.min(Number(start) + p.length, info?.duration ?? p.length)));
    setOpt((o) => ({ ...o, bitrate: p.bitrate, mono: p.mono }));
  }

  async function runJob(
    title: string,
    request: Parameters<typeof backend.startHostJob>[0],
  ): Promise<void> {
    setError(null);
    setDone(null);
    setAudio(null);
    setTransferred(false);
    setRunning(title);
    setProgress(null);
    let timer: ReturnType<typeof setInterval> | undefined;
    try {
      const id = await backend.startHostJob(request);
      cancelRef.current = () => void backend.cancelHostJob(id);
      timer = setInterval(
        () =>
          backend
            .hostJobProgress(id)
            .then((p) => p && setProgress(p))
            .catch(() => undefined),
        150,
      );
      const r = await backend.awaitHostJob(id);
      setDone({
        title,
        outcome: r.outcome,
        generated: r.data?.type === "media_generated" ? r.data.result : null,
        error: r.error,
      });
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      if (timer) clearInterval(timer);
      cancelRef.current = null;
      setRunning(null);
      setProgress(null);
    }
  }

  const ringtone = () =>
    guard(async () => {
      if (!file) return;
      const dest = await backend.pickSavePath(`${stem(file)}-ringtone.mp3`);
      if (!dest) return;
      const options: RingtoneOptions = {
        start: num(start),
        end: num(end),
        fade_in: num(opt.fadeIn) || 0,
        fade_out: num(opt.fadeOut) || 0,
        mono: opt.mono,
        normalize: opt.normalize,
        lufs: num(opt.lufs),
        bitrate: opt.bitrate,
        sample_rate: opt.rate,
      };
      await runJob(t("st.ringtone"), { type: "media.ringtone", source: file, dest, options });
    });

  const preset = PRESETS.find((p) => p.id === presetId) ?? null;
  const prepare = () =>
    guard(async () => {
      if (!file || !preset) return;
      const dest = await backend.pickSavePath(`${stem(file)}-${preset.id}.${preset.ext}`);
      if (!dest) return;
      await runJob(t("st.prepare"), {
        type: "media.prepare",
        source: file,
        dest,
        preset: preset.id,
        model: modelId,
      });
    });

  const listen = () =>
    guard(async () => {
      if (done?.generated) setAudio(await backend.mediaPreview(done.generated.path));
    });

  async function sendFile(g: Generated, path: string) {
    if (!device) return;
    setError(null);
    setRunning(t("st.transfer"));
    try {
      const id = await backend.startOperation(device.id, {
        type: "files.upload",
        source: g.path,
        path,
        overwrite: "never",
      });
      cancelRef.current = () => void backend.cancelOperation(id);
      const r = await backend.awaitOperation(id);
      if (r.outcome === "success") setTransferred(true);
      else if (r.error) setError(r.error);
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      cancelRef.current = null;
      setRunning(null);
    }
  }

  const inspectTheme = (source: ThemeSource) =>
    guard(async () => {
      setThemeSource(source);
      setTheme(null);
      setBitmaps([]);
      const r = await backend.themeInspect(source, modelId || null);
      setTheme(r);
      setBitmaps(await backend.themePreview(source, 12));
    });

  // изменение модели пересчитывает проверки темы
  useEffect(() => {
    if (!themeSource) return;
    backend
      .themeInspect(themeSource, modelId || null)
      .then(setTheme)
      .catch(() => undefined);
  }, [backend, themeSource, modelId]);

  const audioOk = !!info?.audio;
  const kindPresets = PRESETS.filter((p) => info && p.kind === info.kind);
  const screen = modelId ? SCREENS[modelId] : undefined;
  const transferHint = !device
    ? t("st.transferNoDevice")
    : !canWrite
      ? t("st.transferUnsupported")
      : null;

  return (
    <div className="compact-tools">
      <h1>{t("st.title")}</h1>
      <p role="note" className="banner banner-warn">
        {t("st.scope")}
      </p>
      {error && <ErrorMessage error={error} />}

      <Panel title={t("st.tools")}>
        {tool ? (
          <>
            <p data-testid="st-tool">
              {t("st.toolVersion", { version: tool.version, license: tool.license })}
            </p>
            <p className="mono">{t("st.toolPath", { path: tool.ffmpeg_path })}</p>
            {!tool.redistributable && <p role="alert">{t("st.toolNonfree")}</p>}
          </>
        ) : (
          <p role="alert" data-testid="st-tool-missing">
            {t("st.toolMissing")}
            {toolError ? ` (${toolError.code})` : ""}
          </p>
        )}
        <p className="hint" style={{ marginLeft: 0 }}>
          {t("st.toolExternal")}
        </p>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            onClick={() =>
              void guard(async () => {
                const p = await backend.pickFolder();
                if (p) await model.updateSettings({ ffmpeg_dir: p });
              })
            }
          >
            {t("st.toolPick")}
          </button>
          <button className="btn btn-secondary" onClick={() => void refreshTool()}>
            {t("st.toolRecheck")}
          </button>
        </div>
      </Panel>

      <Panel title={t("st.source")}>
        <div className="btn-row">
          <button className="btn" disabled={!!running} onClick={() => void chooseFile()}>
            <Icon name="folder" />
            {t("st.pickFile")}
          </button>
          <span className="mono">{file ?? t("st.noFile")}</span>
        </div>
        {info && (
          <p data-testid="st-info">
            {t(`st.info.${info.kind}` as MessageKey, {
              duration: sec(info.duration),
              container: info.container ?? "?",
              width: (info.video?.width ?? 0) as number,
              height: (info.video?.height ?? 0) as number,
            })}
          </p>
        )}
        <label className="field">
          {t("st.model")}{" "}
          <select
            value={modelId}
            aria-label={t("st.model")}
            onChange={(e) => setModelId(e.target.value)}
          >
            <option value="">{t("st.noModel")}</option>
            {PHONES.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      </Panel>

      {info && info.kind !== "image" && (
        <Panel title={t("st.ringtone")}>
          {!audioOk ? (
            <p>{t("st.ringtoneNeedsAudio")}</p>
          ) : (
            <>
              <Waveform
                peaks={peaks}
                duration={info.duration}
                start={num(start)}
                end={num(end)}
                label={t("st.waveform")}
              />
              <div className="btn-row">
                <button
                  className="btn btn-secondary"
                  onClick={() => applyRingtonePreset("ringtone")}
                >
                  {t("st.preset.ringtone")}
                </button>
                <button
                  className="btn btn-secondary"
                  onClick={() => applyRingtonePreset("notification")}
                >
                  {t("st.preset.notification")}
                </button>
              </div>
              <div className="btn-row">
                <NumberField label={t("st.start")} value={start} onChange={setStart} />
                <NumberField label={t("st.end")} value={end} onChange={setEnd} />
                <NumberField
                  label={t("st.fadeIn")}
                  value={opt.fadeIn}
                  onChange={(v) => setOpt({ ...opt, fadeIn: v })}
                />
                <NumberField
                  label={t("st.fadeOut")}
                  value={opt.fadeOut}
                  onChange={(v) => setOpt({ ...opt, fadeOut: v })}
                />
                <NumberField
                  label={t("st.lufs")}
                  value={opt.lufs}
                  onChange={(v) => setOpt({ ...opt, lufs: v })}
                />
              </div>
              <div className="btn-row">
                <label>
                  <input
                    type="checkbox"
                    checked={opt.mono}
                    onChange={(e) => setOpt({ ...opt, mono: e.target.checked })}
                  />{" "}
                  {t("st.mono")}
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={opt.normalize}
                    onChange={(e) => setOpt({ ...opt, normalize: e.target.checked })}
                  />{" "}
                  {t("st.normalize")}
                </label>
                <label className="field">
                  {t("st.bitrate")}{" "}
                  <select
                    value={opt.bitrate}
                    aria-label={t("st.bitrate")}
                    onChange={(e) => setOpt({ ...opt, bitrate: Number(e.target.value) })}
                  >
                    {BITRATES.map((b) => (
                      <option key={b}>{b}</option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  {t("st.sampleRate")}{" "}
                  <select
                    value={opt.rate}
                    aria-label={t("st.sampleRate")}
                    onChange={(e) => setOpt({ ...opt, rate: Number(e.target.value) })}
                  >
                    {RATES.map((b) => (
                      <option key={b}>{b}</option>
                    ))}
                  </select>
                </label>
              </div>
              <div className="btn-row">
                <button
                  className="btn"
                  disabled={!!running || !tool}
                  onClick={() => void ringtone()}
                >
                  <Icon name="music" />
                  {t("st.ringtoneCreate")}
                </button>
              </div>
            </>
          )}
        </Panel>
      )}

      {info && (
        <Panel title={t("st.prepare")}>
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("st.presetBasis")}
          </p>
          <label className="field">
            {t("st.presetLabel")}{" "}
            <select
              value={presetId}
              aria-label={t("st.presetLabel")}
              onChange={(e) => setPresetId(e.target.value)}
            >
              <option value="" />
              {kindPresets.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.title}
                </option>
              ))}
            </select>
          </label>
          {screen && (
            <p data-testid="st-screen">{t("st.screen", { w: screen[0], h: screen[1] })}</p>
          )}
          {preset?.kind === "image" && !modelId && <p>{t("st.prepareNeedsModel")}</p>}
          <div className="btn-row">
            <button
              className="btn"
              disabled={!!running || !tool || !preset || (preset.kind === "image" && !modelId)}
              onClick={() => void prepare()}
            >
              <Icon name="image" />
              {t("st.prepareCreate")}
            </button>
          </div>
        </Panel>
      )}

      {running && (
        <p role="status">
          {running}… {progress ? `${progress.done}/${progress.total}` : ""}{" "}
          <button className="btn btn-secondary" onClick={() => cancelRef.current?.()}>
            {t("st.stop")}
          </button>
        </p>
      )}

      {done && (
        <Panel title={t("st.result")}>
          <p data-testid="st-outcome">
            {done.title} · {t(`st.outcome.${done.outcome}` as MessageKey)}
          </p>
          {done.error && (
            <p>
              {done.error.code}: {done.error.message}
            </p>
          )}
          {done.generated && (
            <>
              <p className="mono">{t("st.generated", { path: done.generated.path })}</p>
              <p>
                {t("st.generatedInfo", {
                  description: done.generated.description,
                  size: formatSize(done.generated.bytes),
                  sha: done.generated.sha256.slice(0, 16),
                })}
              </p>
              {done.generated.path.toLowerCase().match(/\.(mp3|m4a|aac|wav)$/) && (
                <>
                  <div className="btn-row">
                    <button className="btn btn-secondary" onClick={() => void listen()}>
                      {t("st.listen")}
                    </button>
                  </div>
                  {audio && (
                    <audio
                      controls
                      data-testid="st-audio"
                      src={`data:${audio.mime};base64,${audio.base64}`}
                    />
                  )}
                </>
              )}
              <h3>{t("st.stages")}</h3>
              <ul data-testid="st-stages">
                <li>
                  {t("st.stage.generated")}: {t("st.stage.generatedDone")}
                </li>
                <li>
                  {t("st.stage.transferred")}:{" "}
                  {transferred ? t("st.stage.transferredYes") : t("st.stage.transferredNo")}
                </li>
                <li>
                  {t("st.stage.installed")}: {t("st.stage.installedNote")}
                </li>
                <li>
                  {t("st.stage.verified")}: {t("st.stage.verifiedNote")}
                </li>
              </ul>
              <div className="btn-row">
                <button
                  className="btn btn-secondary"
                  disabled={!canWrite || !!running || transferred}
                  title={transferHint ?? undefined}
                  onClick={() => {
                    setSendPath(`${deviceRoot}${baseName(done.generated!.path)}`);
                    setSendDlg(true);
                  }}
                >
                  {t("st.transfer")}
                </button>
              </div>
              {transferHint && <p className="hint">{transferHint}</p>}
            </>
          )}
        </Panel>
      )}

      <Panel title={t("st.themes")}>
        <p className="hint" style={{ marginLeft: 0 }}>
          {t("st.themeScope")}
        </p>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            onClick={() =>
              void guard(async () => {
                const p = await backend.pickMediaFile();
                if (p) await inspectTheme({ kind: "path", path: p });
              })
            }
          >
            {t("st.themeFile")}
          </button>
          <button
            className="btn btn-secondary"
            onClick={() =>
              void guard(async () => {
                const p = await backend.pickFolder();
                if (p) await inspectTheme({ kind: "path", path: p });
              })
            }
          >
            {t("st.themeFolder")}
          </button>
          <label className="field">
            {t("st.themeLibrary")}{" "}
            <select
              aria-label={t("st.themeLibrary")}
              value={themeSource?.kind === "library" ? themeSource.id : ""}
              onChange={(e) =>
                e.target.value && void inspectTheme({ kind: "library", id: e.target.value })
              }
            >
              <option value="">{t("st.themeNone")}</option>
              {library.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.title ?? i.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        {theme && <ThemeView report={theme} bitmaps={bitmaps} />}
      </Panel>

      {sendDlg && done?.generated && (
        <Dialog
          title={t("st.transferTitle")}
          onClose={() => setSendDlg(false)}
          actions={[
            { label: t("fm.consent.cancel"), onClick: () => setSendDlg(false) },
            {
              label: t("st.transferOk"),
              primary: true,
              onClick: () => {
                setSendDlg(false);
                void sendFile(done.generated!, sendPath);
              },
            },
          ]}
        >
          <p>
            {t("st.transferText", {
              name: baseName(done.generated.path),
              device: device?.display_name ?? "",
            })}
          </p>
          <label className="field">
            {t("st.transferDir")}{" "}
            <input
              value={sendPath}
              aria-label={t("st.transferDir")}
              onChange={(e) => setSendPath(e.target.value)}
            />
          </label>
        </Dialog>
      )}
    </div>
  );
}

function NumberField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <label className="field">
      {label}{" "}
      <input
        value={value}
        inputMode="decimal"
        aria-label={label}
        style={{ width: "6em" }}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

function Waveform({
  peaks,
  duration,
  start,
  end,
  label,
}: {
  peaks: number[];
  duration: number;
  start: number;
  end: number;
  label: string;
}) {
  const w = 600;
  const h = 80;
  const ok = Number.isFinite(start) && Number.isFinite(end) && end > start && duration > 0;
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${w} ${h}`}
      style={{ width: "100%", maxWidth: 640, background: "var(--panel, #fff)" }}
    >
      {peaks.map((p, i) => {
        const x = (i / peaks.length) * w;
        const bar = Math.max(1, p * h);
        return (
          <rect
            key={i}
            x={x}
            y={(h - bar) / 2}
            width={Math.max(1, w / peaks.length - 0.5)}
            height={bar}
            fill="currentColor"
            opacity={0.55}
          />
        );
      })}
      {ok && (
        <rect
          data-testid="st-region"
          x={(Math.max(0, start) / duration) * w}
          y={0}
          width={((Math.min(end, duration) - Math.max(0, start)) / duration) * w}
          height={h}
          fill="rgba(67,132,204,0.25)"
          stroke="rgb(67,132,204)"
        />
      )}
    </svg>
  );
}

function ThemeView({ report: r, bitmaps }: { report: ThemeReport; bitmaps: BitmapPreview[] }) {
  const { t } = useI18n();
  return (
    <>
      <h3>
        {r.package.name || r.name}
        {r.package.uid !== null ? ` · 0x${r.package.uid.toString(16).padStart(8, "0")}` : ""}
      </h3>
      <p data-testid="st-theme-structure">{t(`st.themeStructure.${r.structure}` as MessageKey)}</p>
      <p>{t("st.themeUnknown")}</p>
      <p className="hint" style={{ marginLeft: 0 }}>
        {r.bitmaps.length} · {r.icons}
      </p>
      <h4>{t("st.themeChecks")}</h4>
      <ul data-testid="st-theme-checks">
        {r.checks.map((c) => (
          <li key={c.id}>
            <strong>{t(`st.check.${c.status}` as MessageKey)}</strong> · {c.id}: {c.detail}
          </li>
        ))}
      </ul>
      {r.palette.length > 0 && (
        <>
          <h4>{t("st.themePalette")}</h4>
          <ul>
            {r.palette.map((c) => (
              <li key={c.hex}>
                <span
                  aria-hidden="true"
                  style={{
                    display: "inline-block",
                    width: "1em",
                    height: "1em",
                    background: c.hex,
                    border: "1px solid #888",
                    marginRight: "0.5em",
                  }}
                />
                {c.hex} · {t("st.contrast", { w: c.contrast_on_white, b: c.contrast_on_black })}
              </li>
            ))}
          </ul>
        </>
      )}
      {bitmaps.length > 0 && (
        <>
          <h4>{t("st.themePreview")}</h4>
          <p className="hint" style={{ marginLeft: 0 }}>
            {t("st.themePreviewNote")}
          </p>
          <div className="btn-row" style={{ alignItems: "flex-start" }}>
            {bitmaps.map((b) => (
              <figure key={b.index} style={{ margin: 0 }}>
                <img
                  alt={`#${b.index} ${b.width}×${b.height}`}
                  src={`data:image/png;base64,${b.png_base64}`}
                  style={{ maxWidth: 120, maxHeight: 160, imageRendering: "pixelated" }}
                />
                <figcaption className="hint" style={{ marginLeft: 0 }}>
                  {b.width}×{b.height}
                  {b.masked ? ` · ${t("st.themeMasked")}` : ""}
                </figcaption>
              </figure>
            ))}
          </div>
        </>
      )}
      <h4>{t("st.limits")}</h4>
      <ul>
        {r.limitations.map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
    </>
  );
}
