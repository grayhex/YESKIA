// Пути двух миров: компьютер (Windows, `\`) и устройство (`/`, диск `E:/` или корень `/`).

/** Разделитель по виду пути (на Windows — `\`). */
function sep(base: string): string {
  return base.includes("\\") || /^[A-Za-z]:$/.test(base) ? "\\" : "/";
}

export function joinHost(base: string, name: string): string {
  const s = sep(base);
  return base.endsWith(s) ? `${base}${name}` : `${base}${s}${name}`;
}

/** Родитель папки на компьютере; у корня диска родителя нет (`null`). */
export function parentHost(path: string): string | null {
  const s = sep(path);
  const trimmed = path.length > 1 && path.endsWith(s) ? path.slice(0, -1) : path;
  const i = trimmed.lastIndexOf(s);
  if (i < 0) return null;
  if (s === "\\" && /^[A-Za-z]:$/.test(trimmed.slice(0, i))) {
    return trimmed.length === i + 1 ? null : `${trimmed.slice(0, i)}\\`;
  }
  if (i === 0) return trimmed === "/" ? null : "/";
  return trimmed.slice(0, i);
}

export function joinDevice(base: string, name: string): string {
  // Несколько томов Mass Storage: запись корня «F:» — это корень диска `F:/`, а не папка `/F:`.
  if (base === "/" && /^[A-Za-z]:$/.test(name)) return `${name}/`;
  return base.endsWith("/") ? `${base}${name}` : `${base}/${name}`;
}

/**
 * Родитель на устройстве: `E:/a/b` → `E:/a`, `E:/a` → `E:/`, `/a` → `/`, корень → `null`.
 * `root` — корень панели: при `/` (несколько томов) родитель корня диска `F:/` — это `/`.
 */
export function parentDevice(path: string, root = "/"): string | null {
  if (path === root) return null;
  if (path === "/") return null;
  if (/^[A-Za-z]:\/?$/.test(path)) return root === "/" ? "/" : null;
  const t = path.endsWith("/") ? path.slice(0, -1) : path;
  const i = t.lastIndexOf("/");
  if (i < 0) return null;
  const head = t.slice(0, i);
  if (head === "") return "/";
  if (/^[A-Za-z]:$/.test(head)) return `${head}/`;
  return head;
}

export function splitName(name: string): { stem: string; ext: string } {
  const i = name.lastIndexOf(".");
  return i > 0 ? { stem: name.slice(0, i), ext: name.slice(i) } : { stem: name, ext: "" };
}

/** `фото.jpg` → `фото (1).jpg`, если имя занято (без учёта регистра, как в Windows). */
export function uniqueName(name: string, taken: Iterable<string>): string {
  const set = new Set([...taken].map((n) => n.toLowerCase()));
  if (!set.has(name.toLowerCase())) return name;
  const { stem, ext } = splitName(name);
  for (let n = 1; n < 10_000; n++) {
    const candidate = `${stem} (${n})${ext}`;
    if (!set.has(candidate.toLowerCase())) return candidate;
  }
  return `${stem} (${Date.now()})${ext}`;
}

export function formatSize(bytes: number, lang: "ru" | "en" = "ru"): string {
  if (bytes < 1024) return `${bytes} ${lang === "ru" ? "Б" : "B"}`;
  const units = lang === "ru" ? ["КБ", "МБ", "ГБ", "ТБ"] : ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

/** Заменяет последнее имя в пути (`/` или `\`): `C:\a\b.txt`, `b (1).txt` → `C:\a\b (1).txt`. */
export function withName(path: string, name: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i < 0 ? name : `${path.slice(0, i + 1)}${name}`;
}

export function baseName(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i < 0 ? path : path.slice(i + 1);
}

/** Ancestors derived from the actual pane path, bounded by the transport root. */
export function pathBreadcrumbs(path: string, side: "host" | "device", root = "/"): string[] {
  if (!path) return [];
  const result = [path];
  let parent = side === "host" ? parentHost(path) : parentDevice(path, root);
  while (parent && !result.includes(parent)) {
    result.push(parent);
    parent = side === "host" ? parentHost(parent) : parentDevice(parent, root);
  }
  return result.reverse();
}
