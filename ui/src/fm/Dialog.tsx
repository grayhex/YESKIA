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
  const container = useRef<HTMLDivElement>(null);
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    first.current?.focus();
    return () => {
      if (previous?.isConnected) previous.focus();
    };
  }, []);
  return (
    <div className="dialog-backdrop">
      <div
        className="dialog"
        ref={container}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onKeyDown={(e) => {
          if (e.key === "Tab") {
            const controls = container.current?.querySelectorAll<HTMLElement>(
              'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]',
            );
            const start = controls?.[0];
            const end = controls?.[controls.length - 1];
            if (e.shiftKey && document.activeElement === start) {
              e.preventDefault();
              end?.focus();
            } else if (!e.shiftKey && document.activeElement === end) {
              e.preventDefault();
              start?.focus();
            }
          }
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
