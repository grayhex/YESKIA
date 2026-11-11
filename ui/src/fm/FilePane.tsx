import { Icon, type IconName } from "../components/Icon";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { baseName, formatSize, pathBreadcrumbs } from "./paths";
import { useI18n } from "../i18n";
import { StateMessage } from "../components/Ui";
import type { ErrorInfo } from "../types";

export interface PaneEntry {
  name: string;
  is_dir: boolean;
  size: number;
  modified_ms?: number | null;
  hidden?: boolean;
  /** Показывается, но недоступно для операций (например, имя не в Unicode). */
  disabled?: boolean;
}

type SortKey = "name" | "size" | "modified" | "type";

interface Props {
  title: string;
  side: "host" | "device";
  pathLabel: string;
  rootPath?: string;
  onNavigate?: (path: string) => void;
  entries: PaneEntry[];
  loading: boolean;
  error: ErrorInfo | null;
  emptyText: string;
  canGoUp: boolean;
  selection: Set<string>;
  onSelection: (s: Set<string>) => void;
  onOpen: (e: PaneEntry) => void;
  onUp: () => void;
  onRefresh: () => void;
  onCopy: () => void;
  onPaste: () => void;
  menuItems: (selected: PaneEntry[]) => MenuItem[];
  onDragStart: () => void;
  onDrop: () => void;
  acceptDrop: boolean;
  toolbar?: ReactNode;
}

const ext = (n: string) => (n.includes(".") ? n.slice(n.lastIndexOf(".") + 1).toLowerCase() : "");

function fileIcon(entry: PaneEntry): IconName {
  if (entry.is_dir) return "folder";
  const extension = ext(entry.name);
  if (/^(jpg|jpeg|png|webp|gif|bmp)$/.test(extension)) return "image";
  if (/^(mp3|aac|wav|m4a|ogg|mid)$/.test(extension)) return "music";
  if (/^(sis|sisx|jar|jad)$/.test(extension)) return "apps";
  return "file";
}

export function FilePane(p: Props) {
  const { t, lang } = useI18n();
  const [sort, setSort] = useState<{ key: SortKey; dir: 1 | -1 }>({ key: "name", dir: 1 });
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [focus, setFocus] = useState(0);
  const [anchor, setAnchor] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const rowRefs = useRef<(HTMLTableRowElement | null)[]>([]);
  const wantFocus = useRef(false);

  const rows = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = p.entries.filter(
      (e) => (showHidden || !e.hidden) && (q === "" || e.name.toLowerCase().includes(q)),
    );
    const cmp = (a: PaneEntry, b: PaneEntry) => {
      let r = 0;
      if (sort.key === "size") r = a.size - b.size;
      else if (sort.key === "modified") r = (a.modified_ms ?? 0) - (b.modified_ms ?? 0);
      else if (sort.key === "type") r = ext(a.name).localeCompare(ext(b.name));
      if (r === 0) r = a.name.localeCompare(b.name, lang, { numeric: true, sensitivity: "base" });
      return r * sort.dir;
    };
    // папки всегда выше файлов
    return [...list].sort((a, b) => Number(b.is_dir) - Number(a.is_dir) || cmp(a, b));
  }, [p.entries, filter, showHidden, sort, lang]);

  useEffect(() => {
    setFocus((f) => Math.min(f, Math.max(rows.length - 1, 0)));
  }, [rows.length]);

  useEffect(() => {
    if (wantFocus.current) {
      rowRefs.current[focus]?.focus();
      wantFocus.current = false;
    }
  }, [focus, rows]);

  const crumbs = pathBreadcrumbs(p.pathLabel, p.side, p.rootPath);

  const selectedEntries = rows.filter((r) => p.selection.has(r.name));

  function select(i: number, mode: "one" | "toggle" | "range") {
    const e = rows[i];
    if (!e || e.disabled) return;
    if (mode === "one") {
      p.onSelection(new Set([e.name]));
      setAnchor(i);
    } else if (mode === "toggle") {
      const s = new Set(p.selection);
      if (s.has(e.name)) s.delete(e.name);
      else s.add(e.name);
      p.onSelection(s);
      setAnchor(i);
    } else {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
      p.onSelection(
        new Set(
          rows
            .slice(a, b + 1)
            .filter((r) => !r.disabled)
            .map((r) => r.name),
        ),
      );
    }
  }

  function move(i: number, e: React.KeyboardEvent) {
    const n = Math.max(0, Math.min(rows.length - 1, i));
    wantFocus.current = true;
    setFocus(n);
    if (e.ctrlKey || e.metaKey) return;
    select(n, e.shiftKey ? "range" : "one");
  }

  function onKeyDown(e: React.KeyboardEvent) {
    const key = e.key;
    const mod = e.ctrlKey || e.metaKey;
    const act = (fn: () => void) => {
      e.preventDefault();
      fn();
    };
    if (key === "ArrowDown") return act(() => move(focus + 1, e));
    if (key === "ArrowUp") return act(() => move(focus - 1, e));
    if (key === "Home") return act(() => move(0, e));
    if (key === "End") return act(() => move(rows.length - 1, e));
    if (key === " ") return act(() => select(focus, "toggle"));
    if (key === "Enter") {
      const r = rows[focus];
      if (r?.is_dir && !r.disabled) act(() => p.onOpen(r));
      return;
    }
    if (key === "Backspace") return act(() => p.canGoUp && p.onUp());
    if (key === "F5") return act(p.onRefresh);
    if (key === "Escape") return act(() => p.onSelection(new Set()));
    if (mod && key.toLowerCase() === "a")
      return act(() => p.onSelection(new Set(rows.filter((r) => !r.disabled).map((r) => r.name))));
    if (mod && key.toLowerCase() === "c") return act(p.onCopy);
    if (mod && key.toLowerCase() === "v") return act(p.onPaste);
    if (key === "ContextMenu" || (e.shiftKey && key === "F10")) {
      act(() => {
        const el = rowRefs.current[focus]?.getBoundingClientRect();
        if (rows[focus] && !p.selection.has(rows[focus].name)) select(focus, "one");
        setMenu({ x: (el?.left ?? 0) + 24, y: el?.bottom ?? 0 });
      });
    }
  }

  const header = (key: SortKey, label: string) => (
    <th
      scope="col"
      aria-sort={sort.key === key ? (sort.dir === 1 ? "ascending" : "descending") : "none"}
    >
      <button
        className="sort-btn"
        onClick={() => setSort((s) => ({ key, dir: s.key === key ? (s.dir === 1 ? -1 : 1) : 1 }))}
      >
        {label}
        {sort.key === key ? (sort.dir === 1 ? " ▲" : " ▼") : ""}
      </button>
    </th>
  );

  return (
    <section className="pane" aria-label={p.title}>
      <div className="pane-head">
        <h2>
          <Icon name={p.side === "host" ? "computer" : "phone"} />
          {p.title}
        </h2>
        <div
          className="pane-path"
          role="group"
          aria-label={t("fm.breadcrumbs", { side: p.title })}
          title={p.pathLabel}
        >
          {crumbs.map((path, i) => (
            <span key={path}>
              {i > 0 && <span aria-hidden="true"> › </span>}
              {i === crumbs.length - 1 || !p.onNavigate ? (
                <span aria-current={i === crumbs.length - 1 ? "location" : undefined}>
                  {baseName(path) || path}
                </span>
              ) : (
                <button onClick={() => p.onNavigate?.(path)}>{baseName(path) || path}</button>
              )}
            </span>
          ))}
        </div>
        <div className="btn-row">
          <button className="btn btn-secondary" disabled={!p.canGoUp} onClick={p.onUp}>
            <Icon name="up" />
            {t("fm.up")}
          </button>
          <button className="btn btn-secondary" onClick={p.onRefresh}>
            <Icon name="refresh" />
            {t("fm.refresh")}
          </button>
          {p.toolbar}
        </div>
        <div className="btn-row">
          <input
            className="pane-filter"
            type="search"
            aria-label={t("fm.filter")}
            placeholder={t("fm.filter")}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <label>
            <input
              type="checkbox"
              checked={showHidden}
              onChange={(e) => setShowHidden(e.target.checked)}
            />{" "}
            {t("fm.hidden")}
          </label>
        </div>
      </div>

      {/* Цель drag & drop всегда в DOM: перетаскивать можно и в пустую папку, и при пустом фильтре. */}
      <div
        className={`pane-body${p.acceptDrop ? " drop-ok" : ""}`}
        onDragOver={(e) => p.acceptDrop && e.preventDefault()}
        onDrop={(e) => {
          if (p.acceptDrop) {
            e.preventDefault();
            p.onDrop();
          }
        }}
      >
        {p.error && (
          <StateMessage kind="error" title={t("state.error")}>
            <span data-testid={`pane-error-${p.side}`}>
              {p.error.code}: {p.error.message}
            </span>
          </StateMessage>
        )}
        {!p.error && p.loading && <p role="status">{t("state.loading")}</p>}
        {!p.error && !p.loading && rows.length === 0 && (
          <StateMessage kind="empty" title={p.emptyText} />
        )}

        {rows.length > 0 && (
          <table
            className="data fm-table"
            role="grid"
            aria-label={p.title}
            aria-multiselectable="true"
            onKeyDown={onKeyDown}
          >
            <thead>
              <tr>
                {header("name", t("fm.col.name"))}
                {header("type", t("fm.col.type"))}
                {header("size", t("fm.col.size"))}
                {header("modified", t("fm.col.modified"))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr
                  key={r.name}
                  ref={(el) => {
                    rowRefs.current[i] = el;
                  }}
                  role="row"
                  aria-selected={p.selection.has(r.name)}
                  aria-disabled={r.disabled || undefined}
                  tabIndex={i === focus ? 0 : -1}
                  className={p.selection.has(r.name) ? "row-selected" : undefined}
                  draggable={!r.disabled}
                  onFocus={() => setFocus(i)}
                  onClick={(e) =>
                    select(i, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "one")
                  }
                  onDoubleClick={() => r.is_dir && !r.disabled && p.onOpen(r)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    if (!p.selection.has(r.name)) select(i, "one");
                    setMenu({ x: e.clientX, y: e.clientY });
                  }}
                  onDragStart={(e) => {
                    if (!p.selection.has(r.name)) select(i, "one");
                    e.dataTransfer.setData("text/plain", "yeskia");
                    e.dataTransfer.effectAllowed = "copy";
                    p.onDragStart();
                  }}
                >
                  <td>
                    <span className={`file-glyph file-glyph-${fileIcon(r)}`}>
                      <Icon name={fileIcon(r)} />
                    </span>
                    {r.name}
                  </td>
                  <td>
                    {r.is_dir
                      ? t("fm.type.folder")
                      : ext(r.name).toUpperCase() || t("fm.type.file")}
                  </td>
                  <td>{r.is_dir ? "" : formatSize(r.size, lang)}</td>
                  <td>{r.modified_ms ? new Date(r.modified_ms).toLocaleDateString(lang) : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      <p className="hint" style={{ marginLeft: 0 }}>
        {t("fm.selected", { n: selectedEntries.length })}
      </p>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={p.menuItems(selectedEntries)}
          onClose={() => setMenu(null)}
        />
      )}
    </section>
  );
}
