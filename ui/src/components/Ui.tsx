import { AccentIcon, useVisualTheme, type AccentTone } from "./Visual";
import type { IconName } from "./Icon";
import { useWorkspaceCopy } from "../workspaceCopy";
import type { ReactNode } from "react";
import { useI18n } from "../i18n";
import type { ConnectionState, ErrorInfo } from "../types";

export function Panel({
  title,
  children,
  icon,
  tone,
}: {
  title?: string;
  children: ReactNode;
  icon?: IconName;
  tone?: AccentTone;
}) {
  const theme = useVisualTheme();
  return (
    <section className="panel" aria-label={title}>
      {title && (
        <h2>
          <AccentIcon name={icon ?? theme.icon} tone={tone ?? theme.tone} />
          {title}
        </h2>
      )}
      {children}
    </section>
  );
}

export function MockTag({ origin }: { origin: "mock" | "real" }) {
  const { t } = useI18n();
  return origin === "mock" ? <span className="tag tag-mock">{t("tag.mock")}</span> : null;
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
      <AccentIcon
        name={kind === "error" ? "alert" : "info"}
        tone={kind === "error" ? "red" : "neutral"}
      />
      <h3>{title}</h3>
      {children}
    </div>
  );
}

export function ErrorMessage({
  error: received,
  developer = false,
}: {
  error: ErrorInfo;
  developer?: boolean;
}) {
  const { error: localize } = useWorkspaceCopy();
  const error = developer ? received : localize(received);
  const { t } = useI18n();
  return (
    <StateMessage kind="error" title={t("state.error")}>
      <span data-testid="error">
        {developer ? `${error.code}: ` : ""}
        {error.message}
        {error.retryable ? ` (${t("conn.retryHint")})` : ""}
      </span>
    </StateMessage>
  );
}

export function Metric({
  label,
  value,
  note,
  icon,
  tone = "blue",
}: {
  label: string;
  value: ReactNode;
  note?: ReactNode;
  icon?: IconName;
  tone?: AccentTone;
}) {
  return (
    <div className="metric">
      <dt>
        {label}
        {icon && <AccentIcon name={icon} tone={tone} />}
      </dt>
      <dd>
        {value} {note}
      </dd>
    </div>
  );
}
