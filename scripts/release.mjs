#!/usr/bin/env node
// Вспомогательные шаги релиза без внешних зависимостей: уведомления о лицензиях, SBOM (CycloneDX 1.5),
// контрольные суммы, манифест обновления, заметки. Ничего не публикует и не подписывает.
//
//   node scripts/release.mjs notices   --out THIRD_PARTY_NOTICES.md
//   node scripts/release.mjs sbom      --out sbom.cdx.json
//   node scripts/release.mjs checksums --out SHA256SUMS.txt file1 file2 ...
//   node scripts/release.mjs manifest  --installer X.exe --version 0.2.0 [--notes "..."] --out update.json
//   node scripts/release.mjs keygen    --out private-update-key.pem        (печатает публичный ключ для TRUSTED_KEYS)
//   node scripts/release.mjs sign      --key private-update-key.pem --manifest update.json --out update.sig
//   node scripts/release.mjs notes     --version 0.2.0 --out RELEASE_NOTES.md [--since <ref>]

import { execFileSync } from "node:child_process";
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign as edSign } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const TARGET = "x86_64-pc-windows-msvc";
const MAX_LICENSE_FILE = 64 * 1024;

export const sha256File = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");

/** `sha512-<base64>` (npm integrity) → hex; иначе null. */
export function integrityToHex(integrity) {
  const m = /^sha(256|384|512)-(.+)$/.exec(integrity ?? "");
  if (!m) return null;
  return { alg: `SHA-${m[1]}`, hex: Buffer.from(m[2], "base64").toString("hex") };
}

export const cargoPurl = (name, version) => `pkg:cargo/${encodeURIComponent(name)}@${version}`;

export function npmPurl(name, version) {
  const [scope, rest] = name.startsWith("@") ? name.slice(1).split("/") : [null, name];
  return scope
    ? `pkg:npm/%40${encodeURIComponent(scope)}/${encodeURIComponent(rest)}@${version}`
    : `pkg:npm/${encodeURIComponent(rest)}@${version}`;
}

/** Строка `licenses` CycloneDX: SPDX-выражение или имя. */
export function licenseEntry(license) {
  if (!license) return [];
  return /\b(OR|AND|WITH)\b|\(|\)/.test(license)
    ? [{ expression: license }]
    : [{ license: { id: license } }];
}

/** Ed25519: приватный ключ (PEM) и публичный (32 байта, hex). Приватный ключ хранится вне репозитория. */
export function newUpdateKey() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  return { pem: privateKey.export({ format: "pem", type: "pkcs8" }), publicHex: raw.toString("hex") };
}

export function signManifest(pem, manifestBytes) {
  return edSign(null, manifestBytes, createPrivateKey(pem)).toString("hex");
}

export function publicHexOf(pem) {
  return createPublicKey(createPrivateKey(pem)).export({ format: "der", type: "spki" }).subarray(-32).toString("hex");
}

export const checksumsText = (entries) =>
  entries.map(([hash, name]) => `${hash}  ${name}`).join("\n") + "\n";

export function updateManifest({ file, version, size, sha256, notes = "" }) {
  if (!/^\d{1,5}\.\d{1,5}\.\d{1,5}$/.test(version)) throw new Error("версия должна быть вида 1.2.3");
  return { version, file, sha256, size, notes };
}

function cargoPackages() {
  const meta = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET,
        "--manifest-path", join(ROOT, "src-tauri", "Cargo.toml")],
      { cwd: ROOT, maxBuffer: 1 << 28 },
    ).toString(),
  );
  // нормальные (не dev/build) зависимости, достижимые от приложения
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const root = meta.resolve.root ?? meta.workspace_members.find((id) => byId.get(id).name === "yeskia-app");
  const seen = new Set();
  const stack = [root];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const d of nodes.get(id)?.deps ?? []) {
      if (d.dep_kinds.some((k) => k.kind === null)) stack.push(d.pkg);
    }
  }
  const lock = readFileSync(join(ROOT, "Cargo.lock"), "utf8");
  const sums = new Map();
  for (const block of lock.split("[[package]]").slice(1)) {
    const name = /name = "(.+)"/.exec(block)?.[1];
    const ver = /version = "(.+)"/.exec(block)?.[1];
    const sum = /checksum = "([0-9a-f]{64})"/.exec(block)?.[1];
    if (name && ver && sum) sums.set(`${name}@${ver}`, sum);
  }
  return [...seen]
    .map((id) => byId.get(id))
    .filter((p) => p)
    .map((p) => ({
      ecosystem: "cargo",
      name: p.name,
      version: p.version,
      license: p.license ?? null,
      repository: p.repository ?? null,
      source: p.source ?? null,
      dir: dirname(p.manifest_path),
      hash: sums.get(`${p.name}@${p.version}`) ?? null,
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

function npmPackages() {
  const lock = JSON.parse(readFileSync(join(ROOT, "package-lock.json"), "utf8"));
  const out = [];
  for (const [path, p] of Object.entries(lock.packages)) {
    if (!path.includes("node_modules/") || p.dev || p.devOptional || p.link) continue;
    const name = path.slice(path.lastIndexOf("node_modules/") + "node_modules/".length);
    const ih = integrityToHex(p.integrity);
    out.push({
      ecosystem: "npm",
      name,
      version: p.version,
      license: p.license ?? null,
      repository: p.resolved ?? null,
      dir: join(ROOT, path),
      hash: ih && ih.alg === "SHA-512" ? ih.hex : null,
      hashAlg: ih?.alg,
    });
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

function licenseTexts(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => /^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)([-._].*)?$/i.test(f))
    .map((f) => join(dir, f))
    .filter((f) => statSync(f).isFile() && statSync(f).size <= MAX_LICENSE_FILE)
    .map((f) => ({ file: basename(f), text: readFileSync(f, "utf8").trim() }));
}

export function renderNotices(packages, { appVersion }) {
  const lines = [
    `# Уведомления о лицензиях третьих сторон — YESKIA ${appVersion}`,
    "",
    "Приложение включает перечисленные компоненты. Список создан автоматически из `Cargo.lock` и",
    "`package-lock.json` (только компоненты, попадающие в сборку, без инструментов разработки).",
    "",
    "FFmpeg **не входит** в сборку: это внешний инструмент пользователя, его лицензия определяется по его",
    "сборке и показывается в приложении. Лицензия самого YESKIA указывается владельцем репозитория.",
    "",
    "| Компонент | Версия | Экосистема | Лицензия |",
    "| --- | --- | --- | --- |",
    ...packages.map((p) => `| ${p.name} | ${p.version} | ${p.ecosystem} | ${p.license ?? "**не указана**"} |`),
    "",
    "## Тексты лицензий",
    "",
  ];
  for (const p of packages) {
    const texts = licenseTexts(p.dir);
    if (!texts.length) continue;
    lines.push(`### ${p.name} ${p.version} (${p.license ?? "лицензия не указана в метаданных"})`, "");
    for (const t of texts) lines.push(`<details><summary>${t.file}</summary>`, "", "```text", t.text, "```", "", "</details>", "");
  }
  return lines.join("\n");
}

export function renderSbom(packages, { appVersion, timestamp }) {
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    version: 1,
    metadata: {
      timestamp,
      component: { type: "application", name: "YESKIA", version: appVersion, "bom-ref": "yeskia" },
      properties: [{ name: "yeskia:note", value: "SBOM из lock-файлов; не заменяет юридическую проверку лицензий" }],
    },
    components: packages.map((p) => ({
      type: "library",
      "bom-ref": `${p.ecosystem}:${p.name}@${p.version}`,
      name: p.name,
      version: p.version,
      purl: p.ecosystem === "cargo" ? cargoPurl(p.name, p.version) : npmPurl(p.name, p.version),
      licenses: licenseEntry(p.license),
      ...(p.hash ? { hashes: [{ alg: p.hashAlg ?? "SHA-256", content: p.hash }] } : {}),
    })),
  };
}

function git(args) {
  try {
    return execFileSync("git", args, { cwd: ROOT, stdio: ["ignore", "pipe", "ignore"] }).toString().trim();
  } catch {
    return "";
  }
}

export function renderNotes({ version, subjects, sha }) {
  return [
    `# YESKIA ${version}`,
    "",
    "## Важно",
    "",
    "- Сборка **не подписана** сертификатом издателя: Windows SmartScreen может предупредить о неизвестном издателе.",
    "- Проверка на реальных телефонах не выполнялась автоматически: статусы возможностей — в `docs/STATUS.md`.",
    "  Отметка `verified-on-device` ставится только вручную по отчёту проверки на устройстве.",
    "- Приложение работает офлайн, ничего не скачивает и не обновляется само.",
    "",
    "## Изменения",
    "",
    ...(subjects.length ? subjects.map((s) => `- ${s}`) : ["- (нет коммитов в диапазоне)"]),
    "",
    `Коммит: ${sha || "неизвестен"}`,
    "",
  ].join("\n");
}

function arg(args, name, def = undefined) {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : def;
}

function main(argv) {
  const [cmd, ...rest] = argv;
  const appVersion = JSON.parse(readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8")).version;
  const out = arg(rest, "out");
  if (!out && cmd) throw new Error("--out обязателен");
  switch (cmd) {
    case "notices":
      writeFileSync(out, renderNotices([...cargoPackages(), ...npmPackages()], { appVersion }));
      break;
    case "sbom":
      writeFileSync(
        out,
        JSON.stringify(
          renderSbom([...cargoPackages(), ...npmPackages()], { appVersion, timestamp: new Date().toISOString() }),
          null,
          2,
        ),
      );
      break;
    case "checksums": {
      const files = rest.filter((a, i) => !a.startsWith("--") && rest[i - 1] !== "--out");
      writeFileSync(out, checksumsText(files.map((f) => [sha256File(f), basename(f)])));
      break;
    }
    case "manifest": {
      const installer = arg(rest, "installer");
      const m = updateManifest({
        file: basename(installer),
        version: arg(rest, "version"),
        size: statSync(installer).size,
        sha256: sha256File(installer),
        notes: arg(rest, "notes", ""),
      });
      writeFileSync(out, JSON.stringify(m));
      break;
    }
    case "keygen": {
      const k = newUpdateKey();
      writeFileSync(out, k.pem, { mode: 0o600 });
      const bytes = k.publicHex.match(/../g).map((h) => `0x${h}`);
      console.log(`Публичный ключ (hex): ${k.publicHex}`);
      console.log(`Для TRUSTED_KEYS в crates/yeskia-core/src/update.rs:\n[${bytes.join(", ")}]`);
      console.log(`Приватный ключ записан в ${out}: храните ВНЕ репозитория и не публикуйте.`);
      break;
    }
    case "sign": {
      const sig = signManifest(readFileSync(arg(rest, "key"), "utf8"), readFileSync(arg(rest, "manifest")));
      writeFileSync(out, sig);
      break;
    }
    case "notes": {
      const since = arg(rest, "since") || git(["describe", "--tags", "--abbrev=0"]);
      const range = since ? [`${since}..HEAD`] : ["-n", "50"];
      const subjects = git(["log", "--no-merges", "--format=%s", ...range]).split("\n").filter(Boolean);
      writeFileSync(out, renderNotes({ version: arg(rest, "version", appVersion), subjects, sha: git(["rev-parse", "HEAD"]) }));
      break;
    }
    default:
      throw new Error("команды: notices, sbom, checksums, manifest, keygen, sign, notes");
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`release.mjs: ${e.message}`);
    process.exit(1);
  }
}
