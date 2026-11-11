import { selectDeviceForOperation } from "../deviceRouting";
import { useEffect, useRef, useState } from "react";
import type { AppModel } from "../useAppModel";
import { toErrorInfo } from "../api";
import { ErrorMessage, Panel } from "../components/Ui";
import type { ErrorInfo } from "../types";
import { useI18n } from "../i18n";

import { Icon } from "../components/Icon";
import { useWorkspaceCopy, type WorkspaceKey } from "../workspaceCopy";
const fields = new Set([
  "name",
  "given",
  "family",
  "phones",
  "emails",
  "number",
  "text",
  "date",
  "state",
  "direction",
  "summary",
  "description",
  "location",
  "start",
  "end",
  "due",
  "status",
  "priority",
  "recurrence",
  "part",
  "parts",
]);

export type PersonalDomain = "contacts" | "sms" | "calendar" | "tasks";
export function PersonalPage({ model, domain }: { model: AppModel; domain: PersonalDomain }) {
  const { c, error: localizeError } = useWorkspaceCopy();
  const { t } = useI18n();
  const device = selectDeviceForOperation(
    model.devices,
    domain === "sms" ? "sms.read" : "contacts.read",
  );
  const value = (v: unknown): string =>
    Array.isArray(v)
      ? v.join(", ")
      : v == null
        ? ""
        : typeof v === "boolean"
          ? c(v ? "yes" : "no")
          : v === "incoming" ||
              v === "outgoing" ||
              v === "unread" ||
              v === "read" ||
              v === "unsent" ||
              v === "sent"
            ? c(`field.${v}`)
            : String(v);
  const request = useRef(0);
  const reading = useRef(false);
  const [rows, setRows] = useState<Record<string, unknown>[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [saved, setSaved] = useState("");
  const [tasks, setTasks] = useState(false);
  const actual = domain === "calendar" && tasks ? "tasks" : domain;
  const title = domain === "tasks" ? c("personal.tasks") : t(`nav.${domain}`);
  const filtered = rows.filter((r) =>
    Object.values(r).map(value).join(" ").toLocaleLowerCase().includes(query.toLocaleLowerCase()),
  );
  const row = filtered[selected];
  useEffect(() => {
    setRows([]);
    setLoaded(false);
    setSelected(0);
    setSaved("");
    setBusy(false);
    return () => {
      request.current += 1;
      if (reading.current) void model.backend.cancelPersonalRead().catch(() => undefined);
    };
  }, [model.backend, device?.id, actual]);
  const read = async () => {
    if (!device) return;
    const ticket = ++request.current;
    reading.current = true;
    setBusy(true);
    setError(null);
    setRows([]);
    setLoaded(false);
    setSaved("");
    try {
      const records = await model.backend.readPersonalData(device.id, actual);
      if (ticket !== request.current) return;
      setRows(records);
      setSelected(0);
      setLoaded(true);
    } catch (e) {
      if (ticket === request.current) setError(toErrorInfo(e));
    } finally {
      reading.current = false;
      if (ticket === request.current) setBusy(false);
    }
  };
  const exportRows = async () => {
    try {
      const path = await model.backend.pickSavePath(`${actual}.json`);
      if (path) {
        await model.backend.savePersonalSnapshot(
          path,
          JSON.stringify({ domain: actual, records: rows }, null, 2),
        );
        setSaved(path);
      }
    } catch (e) {
      setError(toErrorInfo(e));
    }
  };
  return (
    <div className="workspace-page">
      <div className="page-heading">
        <h1>{title}</h1>
        <span className="tag">{rows.length}</span>
      </div>
      <p className="hint">{c(device ? "personal.ready" : "personal.connect")}</p>
      <div className="btn-row">
        {domain === "calendar" && (
          <select
            aria-label={c("personal.calendarSection")}
            disabled={busy}
            value={tasks ? "tasks" : "calendar"}
            onChange={(e) => {
              setTasks(e.target.value === "tasks");
              setRows([]);
              setLoaded(false);
            }}
          >
            <option value="calendar">{c("personal.events")}</option>
            <option value="tasks">{c("personal.tasks")}</option>
          </select>
        )}
        <button className="btn" disabled={busy || !device} onClick={() => void read()}>
          <Icon name="refresh" /> {c(busy ? "personal.reading" : "personal.read")}
        </button>
        {busy && (
          <button
            className="btn btn-secondary"
            onClick={() =>
              void model.backend.cancelPersonalRead().catch((e) => setError(toErrorInfo(e)))
            }
          >
            {c("personal.cancel")}
          </button>
        )}
        <input
          aria-label={c("personal.search")}
          placeholder={c("search")}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
          }}
        />
        <button
          className="btn btn-secondary"
          disabled={busy || !loaded}
          onClick={() => void exportRows()}
        >
          <Icon name="download" /> {c("personal.export")}
        </button>
      </div>
      {error && <ErrorMessage error={localizeError(error)} />}
      {saved && (
        <p role="status">
          {c("saved")}: {saved}
        </p>
      )}
      <div className="personal-grid">
        <Panel title={title}>
          <div className="record-list" role="listbox" aria-label={title}>
            {filtered.map((r, i) => (
              <button
                role="option"
                aria-selected={i === selected}
                className={`record-row ${i === selected ? "selected" : ""}`}
                key={String(r.native_id ?? i)}
                onClick={() => setSelected(i)}
              >
                <strong>{value(r.name || r.summary || r.number) || c("personal.noName")}</strong>
                <small>{value(r.phones || r.start || r.date)}</small>
                {domain === "sms" && <span>{value(r.text)}</span>}
              </button>
            ))}
            {!filtered.length && (
              <p role="status">
                {busy ? c("personal.reading") : loaded ? c("personal.empty") : c("personal.start")}
              </p>
            )}
          </div>
        </Panel>
        <Panel title={c("personal.details")}>
          {row ? (
            <dl className="facts">
              {Object.entries(row)
                .filter(([key, v]) => fields.has(key) && value(v))
                .map(([key, v]) => (
                  <div className="record-field" key={key}>
                    <dt>{c(`field.${key}` as WorkspaceKey)}</dt>
                    <dd>{value(v)}</dd>
                  </div>
                ))}
            </dl>
          ) : (
            <p>{c("personal.select")}</p>
          )}
        </Panel>
      </div>
    </div>
  );
}
