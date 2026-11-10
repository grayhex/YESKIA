import { useState } from "react";
import type { AppModel } from "../useAppModel";
import { toErrorInfo } from "../api";
import { ErrorMessage, Panel } from "../components/Ui";
import type { ErrorInfo } from "../types";
import { useI18n } from "../i18n";

export type PersonalDomain = "contacts" | "sms" | "calendar" | "tasks";
const titles = { contacts: "Контакты", sms: "Сообщения", calendar: "Календарь", tasks: "Задачи" };
const labels: Record<string, string> = {
  name: "Имя",
  given: "Имя",
  family: "Фамилия",
  phones: "Телефоны",
  emails: "Почта",
  number: "Номер",
  text: "Сообщение",
  date: "Дата",
  state: "Состояние",
  direction: "Направление",
  summary: "Название",
  description: "Описание",
  location: "Место",
  start: "Начало",
  end: "Окончание",
  due: "Срок",
  completed: "Завершено",
  priority: "Приоритет",
  recurrence: "Повторение",
  part: "Часть",
  parts: "Всего частей",
};
const value = (v: unknown): string =>
  Array.isArray(v)
    ? v.join(", ")
    : v == null
      ? ""
      : typeof v === "boolean"
        ? v
          ? "Да"
          : "Нет"
        : String(v);
export function PersonalPage({
  model,
  domain,
  profile,
  setProfile,
}: {
  model: AppModel;
  domain: PersonalDomain;
  profile: { python: string; config: string };
  setProfile: (p: { python: string; config: string }) => void;
}) {
  const { lang } = useI18n();
  const [rows, setRows] = useState<Record<string, unknown>[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [saved, setSaved] = useState("");
  const [tasks, setTasks] = useState(false);
  const actual = domain === "calendar" && tasks ? "tasks" : domain;
  const title =
    lang === "ru"
      ? titles[domain]
      : domain === "sms"
        ? "Messages"
        : domain[0].toUpperCase() + domain.slice(1);
  const filtered = rows.filter((r) =>
    Object.values(r).map(value).join(" ").toLocaleLowerCase().includes(query.toLocaleLowerCase()),
  );
  const row = filtered[selected];
  const read = async () => {
    setBusy(true);
    setError(null);
    setRows([]);
    setLoaded(false);
    setSaved("");
    try {
      setRows(await model.backend.readPersonalData(profile.python, profile.config, actual));
      setSelected(0);
      setLoaded(true);
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
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
      <details className="panel" open={!profile.python || !profile.config}>
        <summary>{lang === "ru" ? "Настройка чтения телефона" : "Phone connection setup"}</summary>
        <p>
          {lang === "ru"
            ? "Для чтения нужен Python 3 с python-gammu и профиль вашего телефона. Закройте другие программы, использующие его порт."
            : "Select Python 3 with python-gammu and your phone connection profile. Close other applications using its port."}
        </p>
        <div className="btn-row">
          {(["python", "config"] as const).map((key) => (
            <button
              className="btn btn-secondary"
              key={key}
              disabled={busy}
              onClick={async () => {
                try {
                  const p = await model.backend.pickFile();
                  if (p) setProfile({ ...profile, [key]: p });
                } catch (e) {
                  setError(toErrorInfo(e));
                }
              }}
            >
              {key === "python" ? "Python" : "Профиль"}
              {profile[key] ? ` · ${profile[key].split(/[\\/]/).pop()}` : "…"}
            </button>
          ))}
        </div>
      </details>
      <div className="btn-row">
        {domain === "calendar" && (
          <select
            aria-label="Раздел календаря"
            disabled={busy}
            value={tasks ? "tasks" : "calendar"}
            onChange={(e) => {
              setTasks(e.target.value === "tasks");
              setRows([]);
              setLoaded(false);
            }}
          >
            <option value="calendar">События</option>
            <option value="tasks">Задачи</option>
          </select>
        )}
        <button
          className="btn"
          disabled={busy || !profile.python || !profile.config}
          onClick={() => void read()}
        >
          ↻ {busy ? "Чтение…" : "Прочитать телефон"}
        </button>
        {busy && (
          <button
            className="btn btn-secondary"
            onClick={() =>
              void model.backend.cancelPersonalRead().catch((e) => setError(toErrorInfo(e)))
            }
          >
            Отменить чтение
          </button>
        )}
        <input
          aria-label="Поиск записей"
          placeholder="Поиск"
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
          ↓ Экспорт JSON
        </button>
      </div>
      {error && <ErrorMessage error={error} />}
      {saved && <p role="status">Сохранено: {saved}</p>}
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
                <strong>{value(r.name || r.summary || r.number) || "Без названия"}</strong>
                <small>{value(r.phones || r.start || r.date)}</small>
                {domain === "sms" && <span>{value(r.text)}</span>}
              </button>
            ))}
            {!filtered.length && (
              <p role="status">
                {busy
                  ? "Чтение…"
                  : loaded
                    ? "Записей не найдено"
                    : "Нажмите «Прочитать телефон», чтобы загрузить записи."}
              </p>
            )}
          </div>
        </Panel>
        <Panel title="Подробности">
          {row ? (
            <dl className="facts">
              {Object.entries(row)
                .filter(([key, v]) => labels[key] && value(v))
                .map(([key, v]) => (
                  <div className="record-field" key={key}>
                    <dt>{labels[key]}</dt>
                    <dd>{value(v)}</dd>
                  </div>
                ))}
            </dl>
          ) : (
            <p>Выберите запись из списка.</p>
          )}
        </Panel>
      </div>
    </div>
  );
}
