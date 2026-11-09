import { useEffect, useRef } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
}

/** Контекстное меню: Esc/клик снаружи закрывают, ↑/↓ перемещают фокус, Enter выбирает. */
export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLUListElement>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus();
    const away = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [onClose]);

  function onKeyDown(e: React.KeyboardEvent) {
    const buttons = [
      ...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not([disabled])") ?? []),
    ];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      buttons[(i + 1) % buttons.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      buttons[(i - 1 + buttons.length) % buttons.length]?.focus();
    }
  }

  return (
    <ul
      ref={ref}
      className="context-menu"
      role="menu"
      style={{ left: x, top: y }}
      onKeyDown={onKeyDown}
    >
      {items.map((it) => (
        <li key={it.label} role="none">
          <button
            role="menuitem"
            disabled={it.disabled}
            onClick={() => {
              onClose();
              it.onSelect();
            }}
          >
            {it.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
