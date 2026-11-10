import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { useI18n, type Lang, type MessageKey } from "../i18n";

export type PageId =
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

const iconPaths: Record<PageId, string> = {
  home: "M3 10 12 3l9 7v11h-6v-7H9v7H3Z",
  connection: "M8 3h8v18H8ZM10 6h4M11 18h2",
  contacts: "M5 3h14v18H5ZM9 7h6M8 16c0-4 8-4 8 0M12 9v2",
  sms: "M3 5h18v14H3ZM3 5l9 8 9-8",
  calendar: "M3 5h18v16H3ZM7 2v6M17 2v6M3 10h18",
  certificates: "M12 2 3 6v6c0 6 9 10 9 10s9-4 9-10V6ZM8 12l3 3 5-6",
  files: "M3 7h7l2 2h9v11H3ZM3 7V4h7l2 3",
  packages: "m3 7 9-4 9 4v10l-9 4-9-4ZM3 7l9 4 9-4M12 11v10",
  studio: "M9 18V5l11-2v13M9 7l11-2M9 18c0 3-6 4-6 1s6-4 6-1ZM20 16c0 3-6 4-6 1s6-4 6-1Z",
  backups: "M4 5h16v5H4ZM6 10v10h12V10M10 14h4",
  doctor: "M3 12h4l3-8 4 16 3-8h4",
  journal: "M5 3h14v18H5ZM8 7h8M8 12h8M8 17h5",
  settings: "M4 7h16M4 17h16M9 4v6M15 14v6",
};

export function SideNav({
  items,
  current,
  onSelect,
}: {
  items: NavEntry[];
  current: PageId;
  onSelect: (id: PageId) => void;
}) {
  const { t } = useI18n();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  function onKeyDown(e: KeyboardEvent, index: number) {
    const last = items.length - 1;
    let next: number | null = null;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") next = index === last ? 0 : index + 1;
    else if (e.key === "ArrowUp" || e.key === "ArrowLeft") next = index === 0 ? last : index - 1;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = last;
    if (next !== null) {
      e.preventDefault();
      refs.current[next]?.focus();
    }
  }

  return (
    <nav className="side-nav" aria-label={t("navLabel")}>
      <ul>
        {items.map((item, i) => (
          <li key={item.id}>
            <button
              ref={(el) => {
                refs.current[i] = el;
              }}
              className="nav-item"
              aria-current={item.id === current ? "page" : undefined}
              onClick={() => onSelect(item.id)}
              onKeyDown={(e) => onKeyDown(e, i)}
            >
              <svg
                className="nav-icon"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.7"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d={iconPaths[item.id]} />
              </svg>
              {t(item.label)}
            </button>
          </li>
        ))}
      </ul>
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
        <span className="brand-name">{t("appName")}</span>
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
