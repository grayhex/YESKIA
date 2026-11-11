import { useCallback, useEffect, useRef, useState } from "react";
import { useI18n } from "../i18n";
import type { AppModel } from "../useAppModel";
import { selectDeviceForOperation } from "../deviceRouting";
import { ErrorMessage, Panel } from "../components/Ui";
import { Dialog } from "../fm/Dialog";
import { toErrorInfo } from "../api";
import type { ErrorInfo } from "../types";
import {
  emptyContact,
  type Book,
  type Contact,
  type ContactValue,
  type CsvPreview,
  type Duplicate,
  type Imported,
  type ImportOptions,
  type PhoneSnapshot,
  type Versioned,
} from "../workbench";
export function ContactsPage({ model }: { model: AppModel }) {
  const { lang } = useI18n();
  const l = (ru: string, en: string) => (lang === "ru" ? ru : en);
  const api = model.backend.workbench;
  const [local, setLocal] = useState<Versioned<Book>>({ revision: 0, value: { contacts: [] } });
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorInfo | null>(null);
  const [message, setMessage] = useState("");
  const [tab, setTab] = useState<"local" | "phone">("local");
  const [phone, setPhone] = useState<PhoneSnapshot | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState("name");
  const [selected, setSelected] = useState<string[]>([]);
  const [draft, setDraft] = useState<Contact | null>(null);
  const [dirty, setDirty] = useState(false);
  const [group, setGroup] = useState("");
  const [format, setFormat] = useState("json");
  const [duplicates, setDuplicates] = useState<Duplicate[]>([]);
  const [confirm, setConfirm] = useState<{
    title: string;
    text: string;
    run: () => Promise<void>;
  } | null>(null);
  const [merge, setMerge] = useState<Contact | null>(null);
  const [path, setPath] = useState("");
  const [options, setOptions] = useState<ImportOptions>({
    format: "auto",
    encoding: "auto",
    delimiter: null,
    mapping: {},
  });
  const [csv, setCsv] = useState<CsvPreview | null>(null);
  const [preview, setPreview] = useState<Imported | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const device = selectDeviceForOperation(model.devices, "contacts.read");
  const ticket = useRef(0);
  const reading = useRef(false);
  const invalidateRead = useCallback(() => {
    ticket.current++;
  }, []);
  useEffect(() => {
    let active = true;
    void api
      .call({ kind: "contacts_load" })
      .then((v) => {
        if (active) {
          setLocal(v);
          setReady(true);
        }
      })
      .catch((e) => {
        if (active) setError(toErrorInfo(e));
      });
    return () => {
      active = false;
    };
  }, [api]);
  useEffect(() => {
    setPhone(null);
    return () => {
      invalidateRead();
      if (reading.current) void model.backend.cancelPersonalRead().catch(() => undefined);
    };
  }, [device?.id, model.backend, invalidateRead]);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setMessage("");
    try {
      await fn();
    } catch (e) {
      setError(toErrorInfo(e));
    } finally {
      setBusy(false);
    }
  };
  const save = async (book: Book) => {
    const next = await api.call({ kind: "contacts_save", revision: local.revision, book });
    setLocal(next);
    setSelected([]);
    setDraft(null);
    setDirty(false);
    setDuplicates([]);
  };
  const read = () =>
    run(async () => {
      if (!device) return;
      const n = ++ticket.current;
      reading.current = true;
      try {
        const snapshot = await api.phoneRead(device.id);
        if (n === ticket.current) {
          setPhone(snapshot);
          setTab("phone");
          setSelected([]);
          setDraft(null);
          setDirty(false);
        }
      } finally {
        reading.current = false;
      }
    });
  const book = tab === "local" ? local.value : (phone?.book ?? { contacts: [] });
  const filtered = book.contacts
    .filter((c) => JSON.stringify(c).toLocaleLowerCase().includes(query.toLocaleLowerCase()))
    .sort((a, b) =>
      (sort === "family" ? a.family : a.name).localeCompare(
        sort === "family" ? b.family : b.name,
        lang,
      ),
    );
  const select = (c: Contact) => {
    const apply = () => {
      setDraft(structuredClone(c));
      setDirty(false);
    };
    if (dirty)
      setConfirm({
        title: l("Несохранённые изменения", "Unsaved changes"),
        text: l("Отменить изменения в открытой карточке?", "Discard changes in the open contact?"),
        run: async () => apply(),
      });
    else apply();
  };
  const edit = <K extends keyof Contact>(key: K, value: Contact[K]) => {
    setDraft(draft ? { ...draft, [key]: value } : null);
    setDirty(true);
  };
  const copyToLocal = () =>
    run(async () => {
      const copies = book.contacts
        .filter((c) => selected.includes(c.id))
        .map((c) => ({
          ...structuredClone(c),
          id: emptyContact().id,
          source: { ...c.source, original_id: c.id },
          status: "local",
        }));
      await save({ contacts: [...local.value.contacts, ...copies] });
      setTab("local");
    });
  const previewImport = () =>
    run(async () => {
      const result = await api.call({ kind: "contacts_import", path, options });
      setPreview(result);
    });
  const choose = () =>
    run(async () => {
      const p = await model.backend.pickFile();
      if (!p) return;
      setPath(p);
      setPreview(null);
      setCsv(null);
      const next = { ...options, mapping: {} };
      setOptions(next);
      setImportOpen(true);
      if (/\.csv$/i.test(p)) {
        const c = await api.call({ kind: "contacts_csv_preview", path: p, options: next });
        setCsv(c);
        setOptions({ ...next, mapping: c.mapping });
      }
    });
  const changeOptions = (next: ImportOptions) => {
    setOptions(next);
    setPreview(null);
  };
  const exportBook = () =>
    run(async () => {
      const p = await model.backend.pickSavePath(`contacts.${format === "vcf" ? "vcf" : format}`);
      if (!p) return;
      await api.call({ kind: "contacts_export", path: p, book, format });
      setMessage(l("Сохранено: ", "Saved: ") + p);
    });
  const valueEditor = (
    key: "phones" | "emails" | "addresses" | "websites",
    ru: string,
    en: string,
  ) =>
    draft && (
      <fieldset className="contact-values">
        <legend>{l(ru, en)}</legend>
        {draft[key].map((v, i) => (
          <div className="contact-value" key={i}>
            <input
              aria-label={`${l(ru, en)} ${i + 1}`}
              value={v.value}
              onChange={(e) => {
                const values = [...draft[key]];
                values[i] = {
                  ...v,
                  value: e.target.value,
                  components: key === "addresses" ? [] : v.components,
                };
                edit(key, values);
              }}
            />
            <input
              aria-label={l("Тип", "Label") + ` ${en} ${i + 1}`}
              placeholder={l("Дом / работа", "Home / work")}
              value={v.label}
              onChange={(e) => {
                const values = [...draft[key]];
                values[i] = {
                  ...v,
                  label: e.target.value,
                  params: v.params.filter((p) => !p.toUpperCase().startsWith("TYPE=")),
                };
                if (e.target.value) values[i].params.push(`TYPE=${e.target.value}`);
                edit(key, values);
              }}
            />
            <button
              className="btn btn-secondary"
              aria-label={l("Удалить поле", "Remove field")}
              onClick={() =>
                edit(
                  key,
                  draft[key].filter((_, n) => n !== i),
                )
              }
            >
              ×
            </button>
          </div>
        ))}
        <button
          className="btn btn-secondary"
          onClick={() =>
            edit(key, [
              ...draft[key],
              { value: "", label: "", params: [], components: [] } satisfies ContactValue,
            ])
          }
        >
          {l("Добавить поле", "Add field")}
        </button>
      </fieldset>
    );
  return (
    <div className="workspace-page contacts-page">
      <div className="page-heading">
        <h1>{l("Контакты", "Contacts")}</h1>
        <span className="tag">{book.contacts.length}</span>
      </div>
      <p className="hint">
        {l(
          "Локальная книга хранится на этом компьютере. Снимок телефона читается отдельно. Запись на телефон пока не поддерживается.",
          "The local book stays on this computer. Read phone snapshots separately. Writing contacts to the phone is not yet supported.",
        )}
      </p>
      <div className="btn-row">
        <button
          className={`btn ${tab === "local" ? "" : "btn-secondary"}`}
          disabled={busy || dirty}
          onClick={() => {
            setTab("local");
            setDraft(null);
            setSelected([]);
          }}
        >
          {l("Локальная книга", "Local book")}
        </button>
        <button
          className={`btn ${tab === "phone" ? "" : "btn-secondary"}`}
          disabled={busy || dirty || !phone}
          onClick={() => {
            setTab("phone");
            setDraft(null);
            setSelected([]);
          }}
        >
          {l("Снимок телефона", "Phone snapshot")}
        </button>
        <button
          className="btn btn-secondary"
          disabled={!device || busy || dirty}
          onClick={() => void read()}
        >
          {l("Прочитать телефон", "Read phone")}
        </button>
        {busy && reading.current && (
          <button
            className="btn btn-secondary"
            onClick={() => void model.backend.cancelPersonalRead()}
          >
            {l("Отмена", "Cancel")}
          </button>
        )}
      </div>
      {error && <ErrorMessage error={error} />} {message && <p role="status">{message}</p>}
      {phone && tab === "phone" && (
        <Panel title={l("Снимок: только чтение", "Read-only snapshot")}>
          <p>
            {phone.source} · {new Date(phone.read_at_ms).toLocaleString(lang)}
          </p>
          <p className="mono">SHA-256 {phone.hash}</p>
          <p>
            {l("Изменения с прошлого чтения", "Changes since previous read")}:{" "}
            {phone.changes.length}
          </p>
          <details>
            <summary>{l("Различия по полям", "Field differences")}</summary>
            {phone.changes.map((c, i) => (
              <p key={i}>
                {phone.book.contacts.find((v) => v.id === c.id)?.name ??
                  phone.previous.contacts.find((v) => v.id === c.id)?.name}
                : {c.kind} · {c.fields.join(", ")}
              </p>
            ))}
          </details>
          <button
            className="btn"
            disabled={!selected.length || busy}
            onClick={() => void copyToLocal()}
          >
            {l("Скопировать выбранные в локальную книгу", "Copy selected to local book")}
          </button>
        </Panel>
      )}
      <div className="btn-row">
        <input
          aria-label={l("Поиск записей", "Search records")}
          placeholder={l("Имя, телефон, почта, группа", "Name, phone, email, group")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <select
          aria-label={l("Сортировка", "Sort")}
          value={sort}
          onChange={(e) => setSort(e.target.value)}
        >
          <option value="name">{l("По имени", "By name")}</option>
          <option value="family">{l("По фамилии", "By family name")}</option>
        </select>
        <select
          aria-label={l("Формат экспорта", "Export format")}
          value={format}
          onChange={(e) => setFormat(e.target.value)}
        >
          {["json", "csv", "vcf"].map((f) => (
            <option key={f} value={f}>
              {f.toUpperCase()}
            </option>
          ))}
        </select>
        <button
          className="btn btn-secondary"
          disabled={busy || !book.contacts.length}
          onClick={() => void exportBook()}
        >
          {l("Экспорт ", "Export ") + format.toUpperCase()}
        </button>
      </div>
      {tab === "local" && (
        <div className="btn-row">
          <button
            className="btn"
            disabled={!ready || busy || dirty}
            onClick={() => {
              setDraft(emptyContact());
              setDirty(true);
            }}
          >
            {l("Новый контакт", "New contact")}
          </button>
          <button
            className="btn btn-secondary"
            disabled={busy || dirty}
            onClick={() => void choose()}
          >
            {l("Импорт VCF / CSV / JSON", "Import VCF / CSV / JSON")}
          </button>
          <button
            className="btn btn-secondary"
            disabled={busy || dirty || local.revision < 1}
            onClick={() =>
              void run(async () => {
                setLocal(await api.call({ kind: "contacts_undo", revision: local.revision }));
                setDraft(null);
                setSelected([]);
              })
            }
          >
            {l("Отменить последнее изменение", "Undo last change")}
          </button>
          <button
            className="btn btn-secondary"
            disabled={busy}
            onClick={() =>
              void run(async () =>
                setDuplicates(await api.call({ kind: "contacts_duplicates", book: local.value })),
              )
            }
          >
            {l("Найти дубли", "Find duplicates")}
          </button>
        </div>
      )}
      {duplicates.length > 0 && (
        <Panel
          title={l(
            "Предложения дублей — без автообъединения",
            "Duplicate suggestions — no automatic merge",
          )}
        >
          {duplicates.slice(0, 100).map((d, i) => (
            <button
              className="btn btn-secondary"
              key={i}
              onClick={() => {
                setSelected(d.ids);
                setDraft(structuredClone(local.value.contacts.find((c) => c.id === d.ids[0])!));
              }}
            >
              {d.ids.map((id) => local.value.contacts.find((c) => c.id === id)?.name).join(" · ")} (
              {d.reasons.join(", ")})
            </button>
          ))}
        </Panel>
      )}
      {selected.length > 0 && tab === "local" && (
        <div className="btn-row">
          <span>
            {l("Выбрано", "Selected")}: {selected.length}
          </span>
          <input
            aria-label={l("Группа", "Group")}
            value={group}
            onChange={(e) => setGroup(e.target.value)}
          />
          <button
            className="btn btn-secondary"
            disabled={busy || !group.trim() || dirty}
            onClick={() =>
              void run(async () =>
                save({
                  contacts: local.value.contacts.map((c) =>
                    selected.includes(c.id)
                      ? { ...c, groups: Array.from(new Set([...c.groups, group.trim()])) }
                      : c,
                  ),
                }),
              )
            }
          >
            {l("Добавить в группу", "Add to group")}
          </button>
          <button
            className="btn btn-secondary"
            disabled={busy || dirty || selected.length < 2 || selected.length > 200}
            onClick={() =>
              void run(async () =>
                setMerge(
                  await api.call({
                    kind: "contacts_merge",
                    book: local.value,
                    ids: selected,
                    primary: selected.includes(draft?.id ?? "") ? draft!.id : selected[0],
                  }),
                ),
              )
            }
          >
            {l("Предпросмотр объединения", "Preview merge")}
          </button>
          <button
            className="btn btn-danger"
            disabled={busy || dirty}
            onClick={() =>
              setConfirm({
                title: l("Удалить выбранные локально?", "Delete selected locally?"),
                text: l(
                  "Телефон не изменится. Доступна отмена последнего изменения.",
                  "The phone stays unchanged. You can undo the last local change.",
                ),
                run: async () =>
                  save({ contacts: local.value.contacts.filter((c) => !selected.includes(c.id)) }),
              })
            }
          >
            {l("Удалить", "Delete")}
          </button>
        </div>
      )}
      <div className="contact-grid">
        <Panel
          title={
            tab === "local"
              ? l("Локальная книга", "Local book")
              : l("Снимок телефона", "Phone snapshot")
          }
        >
          <div
            className="record-list"
            role="listbox"
            aria-label={l("Контакты", "Contacts")}
            aria-multiselectable="true"
          >
            {filtered.map((c) => (
              <div className="contact-list-row" key={c.id}>
                <input
                  type="checkbox"
                  aria-label={l("Выбрать ", "Select ") + c.name}
                  checked={selected.includes(c.id)}
                  onChange={(e) =>
                    setSelected(
                      e.target.checked ? [...selected, c.id] : selected.filter((id) => id !== c.id),
                    )
                  }
                />
                <button
                  role="option"
                  aria-selected={draft?.id === c.id}
                  className={`record-row ${draft?.id === c.id ? "selected" : ""}`}
                  onClick={() => select(c)}
                >
                  <strong>{c.name || l("Без имени", "Unnamed")}</strong>
                  <small>{c.phones.map((v) => v.value).join(" · ")}</small>
                  <small>{c.groups.join(", ")}</small>
                </button>
              </div>
            ))}
            {!filtered.length && (
              <p>
                {l(
                  "Нет записей. Создайте контакт или импортируйте файл.",
                  "No records. Create a contact or import a file.",
                )}
              </p>
            )}
          </div>
        </Panel>
        <Panel title={l("Карточка контакта", "Contact editor")}>
          {!draft ? (
            <p>{l("Выберите контакт слева.", "Select a contact on the left.")}</p>
          ) : (
            <fieldset className="contact-editor" disabled={tab === "phone" || busy}>
              <div className="form-grid">
                {(
                  [
                    ["name", "Отображаемое имя", "Display name"],
                    ["given", "Имя", "Given name"],
                    ["family", "Фамилия", "Family name"],
                    ["middle", "Отчество", "Middle name"],
                    ["prefix", "Обращение", "Prefix"],
                    ["suffix", "Суффикс", "Suffix"],
                    ["title", "Должность", "Job title"],
                    ["birthday", "День рождения", "Birthday"],
                  ] as const
                ).map(([key, ru, en]) => (
                  <label key={key}>
                    {l(ru, en)}
                    <input value={draft[key]} onChange={(e) => edit(key, e.target.value)} />
                  </label>
                ))}
                <label>
                  {l(
                    "Организация / подразделения (по строкам)",
                    "Organisation / units (one per line)",
                  )}
                  <textarea
                    value={draft.organisation.join("\n")}
                    onChange={(e) => edit("organisation", e.target.value.split("\n"))}
                  />
                </label>
                <label>
                  {l("Группы (по строкам)", "Groups (one per line)")}
                  <textarea
                    value={draft.groups.join("\n")}
                    onChange={(e) => edit("groups", e.target.value.split("\n").filter(Boolean))}
                  />
                </label>
              </div>
              {valueEditor("phones", "Телефоны", "Phones")}
              {valueEditor("emails", "Почта", "Emails")}
              {valueEditor("addresses", "Адреса", "Addresses")}
              {valueEditor("websites", "Сайты", "Websites")}
              <label>
                {l("Заметка", "Note")}
                <textarea value={draft.note} onChange={(e) => edit("note", e.target.value)} />
              </label>
              <details>
                <summary>
                  {l("Дополнительные поля и исходные данные", "Extra fields and source data")}
                </summary>
                <p className="hint">
                  {l(
                    "Неизвестные поля сохраняются при экспорте. Фото и внешние ссылки не загружаются.",
                    "Unknown fields survive export. Photos and external URLs are never fetched.",
                  )}
                </p>
                {draft.unknown.map((v, i) => (
                  <div className="contact-value" key={i}>
                    <input
                      aria-label={l("Имя свойства", "Property name")}
                      value={v.head}
                      onChange={(e) =>
                        edit(
                          "unknown",
                          draft.unknown.map((p, n) =>
                            n === i ? { ...p, head: e.target.value } : p,
                          ),
                        )
                      }
                    />
                    <textarea
                      aria-label={l("Значение свойства", "Property value")}
                      value={v.value}
                      onChange={(e) =>
                        edit(
                          "unknown",
                          draft.unknown.map((p, n) =>
                            n === i ? { ...p, value: e.target.value } : p,
                          ),
                        )
                      }
                    />
                  </div>
                ))}
                <button
                  className="btn btn-secondary"
                  onClick={() => edit("unknown", [...draft.unknown, { head: "X-", value: "" }])}
                >
                  {l("Добавить свойство", "Add property")}
                </button>
                <dl>
                  {Object.entries(draft.csv_extra).map(([k, v]) => (
                    <div key={k}>
                      <dt>{k}</dt>
                      <dd>{v}</dd>
                    </div>
                  ))}
                </dl>
                <p>
                  {draft.source.kind} {draft.status}
                </p>
              </details>
              <div className="btn-row">
                <button
                  className="btn"
                  disabled={!dirty}
                  onClick={() =>
                    void run(async () =>
                      save({
                        contacts: local.value.contacts.some((c) => c.id === draft.id)
                          ? local.value.contacts.map((c) =>
                              c.id === draft.id ? { ...draft, status: "local" } : c,
                            )
                          : [...local.value.contacts, draft],
                      }),
                    )
                  }
                >
                  {l("Сохранить локально", "Save locally")}
                </button>
                <button
                  className="btn btn-secondary"
                  onClick={() => {
                    setDraft({
                      ...structuredClone(draft),
                      id: emptyContact().id,
                      name: draft.name + l(" (копия)", " (copy)"),
                      source: { ...draft.source, original_id: draft.id },
                      status: "local",
                    });
                    setDirty(true);
                  }}
                >
                  {l("Дублировать", "Duplicate")}
                </button>
                <button
                  className="btn btn-secondary"
                  onClick={() => {
                    setDraft(null);
                    setDirty(false);
                  }}
                >
                  {l("Отменить правки", "Discard edits")}
                </button>
              </div>
            </fieldset>
          )}
        </Panel>
      </div>
      {confirm && (
        <Dialog
          title={confirm.title}
          onClose={() => setConfirm(null)}
          actions={[
            { label: l("Отмена", "Cancel"), onClick: () => setConfirm(null) },
            {
              label: l("Подтвердить", "Confirm"),
              primary: true,
              onClick: () => {
                const action = confirm.run;
                setConfirm(null);
                void run(action);
              },
            },
          ]}
        >
          <p>{confirm.text}</p>
        </Dialog>
      )}
      {merge && (
        <Dialog
          title={l("Предпросмотр объединения", "Merge preview")}
          onClose={() => setMerge(null)}
          actions={[
            { label: l("Отмена", "Cancel"), onClick: () => setMerge(null) },
            {
              label: l("Объединить локально", "Merge locally"),
              primary: true,
              onClick: () => {
                const value = merge;
                setMerge(null);
                void run(async () =>
                  save({
                    contacts: [
                      ...local.value.contacts.filter((c) => !selected.includes(c.id)),
                      value,
                    ],
                  }),
                );
              },
            },
          ]}
        >
          <p>
            {l("Основная карточка", "Primary record")}: {merge.name}
          </p>
          <p>
            {l(
              "Конфликты сохраняются в X-YESKIA-MERGED-* полях. Телефон не изменится.",
              "Conflicting scalar values stay in X-YESKIA-MERGED-* fields. The phone stays unchanged.",
            )}
          </p>
          <pre className="contact-preview">{JSON.stringify(merge, null, 2)}</pre>
        </Dialog>
      )}
      {importOpen && (
        <Dialog
          title={l("Импорт: сопоставление и предпросмотр", "Import: mapping and preview")}
          onClose={() => setImportOpen(false)}
          actions={[
            { label: l("Отмена", "Cancel"), onClick: () => setImportOpen(false) },
            ...(!busy
              ? [{ label: l("Предпросмотр", "Preview"), onClick: () => void previewImport() }]
              : []),
            ...(preview && !busy
              ? [
                  {
                    label: l("Добавить в локальную книгу", "Add to local book"),
                    primary: true,
                    onClick: () => {
                      const data = preview.book.contacts.map((c) => ({
                        ...c,
                        id: emptyContact().id,
                        status: "local",
                      }));
                      void run(async () => {
                        await save({ contacts: [...local.value.contacts, ...data] });
                        setImportOpen(false);
                      });
                    },
                  },
                ]
              : []),
          ]}
        >
          <p className="path">{path}</p>
          <div className="btn-row">
            <label>
              {l("Формат", "Format")}
              <select
                value={options.format}
                onChange={(e) => changeOptions({ ...options, format: e.target.value })}
              >
                {["auto", "vcf", "csv", "json"].map((v) => (
                  <option key={v}>{v}</option>
                ))}
              </select>
            </label>
            <label>
              {l("Кодировка", "Encoding")}
              <select
                value={options.encoding}
                onChange={(e) => changeOptions({ ...options, encoding: e.target.value })}
              >
                {["auto", "utf-8", "utf-16le", "utf-16be", "windows-1251"].map((v) => (
                  <option key={v}>{v}</option>
                ))}
              </select>
            </label>
            <label>
              {l("Разделитель CSV", "CSV delimiter")}
              <select
                value={options.delimiter ?? ""}
                onChange={(e) => changeOptions({ ...options, delimiter: e.target.value || null })}
              >
                <option value="">auto</option>
                {[",", ";", "\t"].map((v) => (
                  <option value={v} key={v}>
                    {v === "\t" ? "Tab" : v}
                  </option>
                ))}
              </select>
            </label>
            <button
              className="btn btn-secondary"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const v = await api.call({ kind: "contacts_csv_preview", path, options });
                  setCsv(v);
                  changeOptions({ ...options, mapping: v.mapping });
                })
              }
            >
              {l("Проверить колонки CSV", "Review CSV columns")}
            </button>
          </div>
          {csv && (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{l("Колонка", "Column")}</th>
                    <th>{l("Поле", "Field")}</th>
                    <th>{l("Пример", "Sample")}</th>
                  </tr>
                </thead>
                <tbody>
                  {csv.columns.map((name, i) => (
                    <tr key={i}>
                      <td>{name}</td>
                      <td>
                        <select
                          aria-label={name}
                          value={options.mapping[i] ?? "extra"}
                          onChange={(e) =>
                            changeOptions({
                              ...options,
                              mapping: { ...options.mapping, [i]: e.target.value },
                            })
                          }
                        >
                          {[
                            "name",
                            "given",
                            "family",
                            "middle",
                            "prefix",
                            "suffix",
                            "phone",
                            "email",
                            "address",
                            "company",
                            "title",
                            "note",
                            "birthday",
                            "website",
                            "groups",
                            "preserve",
                          ].map((v) => (
                            <option key={v}>{v}</option>
                          ))}
                        </select>
                      </td>
                      <td>{csv.sample[0]?.[i]}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {preview && (
            <div>
              <p>
                {l("Контактов", "Contacts")}: {preview.book.contacts.length} · {preview.encoding}
              </p>
              {preview.warnings.map((w, i) => (
                <p key={i}>{w}</p>
              ))}
              <div className="contact-preview">
                {preview.book.contacts.slice(0, 30).map((c) => (
                  <p key={c.id}>
                    {c.name} · {c.phones.map((v) => v.value).join(" · ")} ·{" "}
                    {c.emails.map((v) => v.value).join(" · ")}
                  </p>
                ))}
              </div>
            </div>
          )}
          {error && <ErrorMessage error={error} />}
        </Dialog>
      )}
    </div>
  );
}
