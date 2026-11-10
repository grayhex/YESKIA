import { useWorkspaceCopy } from "../workspaceCopy";
import { useState } from "react";
import apps from "../catalog/apps.json";
import { PackagesPage } from "./PackagesPage";
import type { AppModel } from "../useAppModel";
export function StorePage({ model }: { model: AppModel }) {
  const { c, lang } = useWorkspaceCopy();
  const [local, setLocal] = useState(false);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const filtered = apps.filter(
    (a) =>
      (!category || a.category === category) &&
      `${a.name} ${lang === "en" ? a.description_en || a.description : a.description} ${a.developer}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  return (
    <div className="workspace-page">
      <div className="page-heading">
        <h1>{c("store.title")}</h1>
        <div className="btn-row">
          <button
            className="btn btn-secondary"
            aria-pressed={!local}
            onClick={() => setLocal(false)}
          >
            {c("store.catalog")}
          </button>
          <button className="btn btn-secondary" aria-pressed={local} onClick={() => setLocal(true)}>
            {c("store.local")}
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
              aria-label={c("store.search")}
              placeholder={c("store.find")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <select
              aria-label={c("store.category")}
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            >
              <option value="">{c("store.all")}</option>
              {[...new Set(apps.map((a) => a.category))].map((c) => (
                <option key={c} value={c ?? ""}>
                  {c}
                </option>
              ))}
            </select>
            <span>
              {filtered.length} {c("store.count")}
            </span>
          </div>
          <p className="hint">{c("store.preview")}</p>
          <div className="catalog-grid workspace-scroll">
            {filtered.map((a) => (
              <article className="panel catalog-card" key={a.id}>
                <span className="app-avatar" aria-hidden="true">
                  {a.name?.[0]}
                </span>
                <h2>{a.name}</h2>
                <small>{a.developer}</small>
                <p>{lang === "en" ? a.description_en || a.description : a.description}</p>
                <div className="btn-row">
                  <span className="tag">{a.platforms?.join(" · ")}</span>
                  <button
                    className="btn btn-secondary"
                    aria-expanded={selected === a.id}
                    onClick={() => setSelected(selected === a.id ? null : a.id)}
                  >
                    {c("store.details")}
                  </button>
                </div>
                {selected === a.id && (
                  <p>
                    {a.version} · {c("store.unavailable")}
                  </p>
                )}
              </article>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
