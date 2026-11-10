import { useState } from "react";
import apps from "../catalog/apps.json";
import { PackagesPage } from "./PackagesPage";
import type { AppModel } from "../useAppModel";
export function StorePage({ model }: { model: AppModel }) {
  const [local, setLocal] = useState(false);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const filtered = apps.filter(
    (a) =>
      (!category || a.category === category) &&
      `${a.name} ${a.description} ${a.developer}`.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <div className="workspace-page">
      <div className="page-heading">
        <h1>Приложения</h1>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            aria-pressed={!local}
            onClick={() => setLocal(false)}
          >
            Каталог
          </button>
          <button className="btn btn-secondary" aria-pressed={local} onClick={() => setLocal(true)}>
            Мои установщики
          </button>
        </div>
      </div>
      {local ? (
        <div className="workspace-scroll">
          <PackagesPage model={model} />
        </div>
      ) : (
        <>
          <div className="btn-row">
            <input
              aria-label="Поиск приложений"
              placeholder="Найти приложение"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <select
              aria-label="Категория"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            >
              <option value="">Все категории</option>
              {[...new Set(apps.map((a) => a.category))].map((c) => (
                <option key={c} value={c ?? ""}>
                  {c}
                </option>
              ))}
            </select>
            <span>{filtered.length} приложений</span>
          </div>
          <p className="hint">
            Каталог s60modern · Предварительная версия. Загрузка и установка появятся после
            подключения сервиса каталога.
          </p>
          <div className="catalog-grid workspace-scroll">
            {filtered.map((a) => (
              <article className="panel catalog-card" key={a.id}>
                <span className="app-avatar" aria-hidden="true">
                  {a.name?.[0]}
                </span>
                <h2>{a.name}</h2>
                <small>{a.developer}</small>
                <p>{a.description}</p>
                <div className="btn-row">
                  <span className="tag">{a.platforms?.join(" · ")}</span>
                  <button
                    className="btn btn-secondary"
                    aria-expanded={selected === a.id}
                    onClick={() => setSelected(selected === a.id ? null : a.id)}
                  >
                    Подробнее
                  </button>
                </div>
                {selected === a.id && <p>{a.version} · Загрузка пока недоступна.</p>}
              </article>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
