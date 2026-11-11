import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import type { Device } from "../types";
import { useI18n, type Lang, type MessageKey } from "../i18n";

export type PageId =
  | "agent"
  | "home"
  | "connection"
  | "contacts"
  | "sms"
  | "calendar"
  | "certificates"
  | "files"
  | "packages"
  | "studio"
  | "backups"
  | "doctor"
  | "journal"
  | "settings";

export interface NavEntry {
  id: PageId;
  label: MessageKey;
}

const navIcons: Record<PageId, IconName> = {
  agent: "activity",
  home: "home",
  connection: "phone",
  contacts: "contacts",
  sms: "message",
  calendar: "calendar",
  certificates: "shield",
  files: "folder",
  packages: "apps",
  studio: "music",
  backups: "archive",
  doctor: "info",
  journal: "activity",
  settings: "settings",
};

export function SideNav({
  items,
  current,
  onSelect,
  device,
}: {
  device?: Device;
  items: NavEntry[];
  current: PageId;
  onSelect: (id: PageId) => void;
}) {
  const { t } = useI18n();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const navRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const group = navRef.current?.querySelector('[aria-current="page"]')?.closest("details");
    if (group) group.open = true;
  }, [current]);

  const groups: { label: MessageKey; ids: PageId[] }[] = [
    {
      label: "nav.group.workspace",
      ids: [
        "home",
        "connection",
        "agent",
        "files",
        "contacts",
        "sms",
        "calendar",
        "packages",
        "studio",
        "certificates",
        "backups",
      ],
    },
    { label: "nav.group.app", ids: ["journal", "doctor", "settings"] },
  ];

  function onKeyDown(e: KeyboardEvent, index: number) {
    const last = items.length - 1;
    let next: number | null = null;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") next = index === last ? 0 : index + 1;
    else if (e.key === "ArrowUp" || e.key === "ArrowLeft") next = index === 0 ? last : index - 1;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = last;
    if (next !== null) {
      e.preventDefault();
      const step = e.key === "ArrowUp" || e.key === "ArrowLeft" || e.key === "End" ? -1 : 1;
      for (let n = 0; n < items.length; n++) {
        if (refs.current[next]?.closest("details")?.open) break;
        next = (next + step + items.length) % items.length;
      }
      refs.current[next]?.focus();
    }
  }

  return (
    <nav ref={navRef} className="side-nav" aria-label={t("navLabel")}>
      <div className="sidebar-brand">
        <strong>YESKIA</strong>
        <span>{t("appTagline")}</span>
      </div>
      {groups.map((group) => (
        <details className="nav-group" open key={group.label}>
          <summary>{t(group.label)}</summary>
          <ul>
            {items
              .filter((item) => group.ids.includes(item.id))
              .map((item) => (
                <li key={item.id}>
                  <button
                    ref={(el) => {
                      refs.current[items.indexOf(item)] = el;
                    }}
                    className="nav-item"
                    aria-current={item.id === current ? "page" : undefined}
                    onClick={() => onSelect(item.id)}
                    onKeyDown={(e) => onKeyDown(e, items.indexOf(item))}
                  >
                    <Icon className="nav-icon" name={navIcons[item.id]} />
                    {t(item.label)}
                  </button>
                </li>
              ))}
          </ul>
        </details>
      ))}
      <div className="sidebar-device" data-state={device?.connection ?? "absent"}>
        <span className="connection-dot" aria-hidden="true" />
        <div>
          <strong>
            {t(
              device
                ? device.connection === "connected"
                  ? "sidebar.connected"
                  : "sidebar.detected"
                : "sidebar.empty",
            )}
          </strong>
          <small>
            {device
              ? `${device.display_name.replace(/\s*\(MOCK\)/, "")} · ${device.transport}`
              : t("status.offline")}
          </small>
        </div>
      </div>
    </nav>
  );
}

export function Header({
  developer,
  lang,
  onMode,
  onLang,
  disabled,
}: {
  developer: boolean;
  lang: Lang;
  onMode: (developer: boolean) => void;
  onLang: (lang: Lang) => void;
  disabled: boolean;
}) {
  const { t } = useI18n();
  return (
    <header className="app-header">
      <div className="brand">
        <span className="brand-wordmark">{t("appName")}</span>
        <span className="beta-badge">Beta</span>
        <span className="brand-tagline">{t("appTagline")}</span>
      </div>
      <div className="header-controls">
        <div role="group" aria-label={t("mode.label")} className="segmented">
          <button disabled={disabled} aria-pressed={!developer} onClick={() => onMode(false)}>
            {t("mode.user")}
          </button>
          <button disabled={disabled} aria-pressed={developer} onClick={() => onMode(true)}>
            {t("mode.developer")}
          </button>
        </div>
        <div role="group" aria-label={t("lang.label")} className="segmented">
          <button
            disabled={disabled}
            aria-pressed={lang === "ru"}
            onClick={() => onLang("ru")}
            lang="ru"
          >
            RU
          </button>
          <button
            disabled={disabled}
            aria-pressed={lang === "en"}
            onClick={() => onLang("en")}
            lang="en"
          >
            EN
          </button>
        </div>
      </div>
    </header>
  );
}

export function StatusBar({ children }: { children: ReactNode }) {
  return (
    <footer className="status-bar" role="status">
      {children}
    </footer>
  );
}
