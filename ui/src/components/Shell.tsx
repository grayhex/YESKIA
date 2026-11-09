import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { useI18n, type Lang, type MessageKey } from "../i18n";

export type PageId = "home" | "connection" | "journal" | "settings";

export interface NavEntry {
  id: PageId;
  label: MessageKey;
}

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
