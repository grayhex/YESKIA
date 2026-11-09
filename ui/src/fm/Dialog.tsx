import { useEffect, useRef, type ReactNode } from "react";

export interface DialogAction {
  label: string;
  onClick: () => void;
  primary?: boolean;
}

/** Модальное окно: фокус на первой кнопке, Esc вызывает `onClose` (безопасный отказ). */
export function Dialog({
  title,
  children,
  actions,
  onClose,
}: {
  title: string;
  children?: ReactNode;
  actions: DialogAction[];
  onClose: () => void;
}) {
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    first.current?.focus();
  }, []);
  return (
    <div className="dialog-backdrop">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onClose();
          }
        }}
      >
        <h2>{title}</h2>
        {children}
        <div className="btn-row">
          {actions.map((a, i) => (
            <button
              key={a.label}
              ref={i === 0 ? first : undefined}
              className={a.primary ? "btn" : "btn btn-secondary"}
              onClick={a.onClick}
            >
              {a.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
