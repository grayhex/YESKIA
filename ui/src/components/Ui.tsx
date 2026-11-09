import type { ReactNode } from "react";
import { useI18n } from "../i18n";
import type { ConnectionState, ErrorInfo } from "../types";

export function Panel({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="panel" aria-label={title}>
      {title && <h2>{title}</h2>}
      {children}
    </section>
  );
}

export function MockTag({ origin }: { origin: "mock" | "real" }) {
  const { t } = useI18n();
  return origin === "mock" ? (
    <span className="tag tag-mock">{t("tag.mock")}</span>
  ) : (
    <span className="tag tag-real">{t("tag.real")}</span>
  );
}

export function ConnectionTag({ state }: { state: ConnectionState }) {
  const { t } = useI18n();
  const key = `conn.${state}` as const;
  return (
    <span className={`tag tag-state-${state === "connected" ? "connected" : "disconnected"}`}>
      {t(key)}
    </span>
  );
}

type StateKind = "empty" | "error" | "unavailable";

/** Единые пустое / ошибочное / недоступное состояния. */
export function StateMessage({
  kind,
  title,
  children,
}: {
  kind: StateKind;
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className={`state state-${kind}`} role={kind === "error" ? "alert" : "status"}>
      <h3>{title}</h3>
      {children}
    </div>
  );
}

export function ErrorMessage({ error }: { error: ErrorInfo }) {
  const { t } = useI18n();
  return (
    <StateMessage kind="error" title={t("state.error")}>
      <span data-testid="error">
        {error.code}: {error.message}
        {error.retryable ? ` (${t("conn.retryHint")})` : ""}
      </span>
    </StateMessage>
  );
}

export function Metric({
  label,
  value,
  note,
}: {
  label: string;
  value: ReactNode;
  note?: ReactNode;
}) {
  return (
    <div className="metric">
      <dt>{label}</dt>
      <dd>
        {value} {note}
      </dd>
    </div>
  );
}
