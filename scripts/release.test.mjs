import assert from "node:assert/strict";
import { verify, createPublicKey } from "node:crypto";
import { test } from "node:test";
import {
  checksumsText,
  cargoPurl,
  integrityToHex,
  licenseEntry,
  newUpdateKey,
  publicHexOf,
  signManifest,
  npmPurl,
  renderNotes,
  renderNotices,
  renderSbom,
  updateManifest,
} from "./release.mjs";

test("integrity npm переводится в hex", () => {
  const r = integrityToHex("sha512-" + Buffer.from("abc").toString("base64"));
  assert.deepEqual(r, { alg: "SHA-512", hex: Buffer.from("abc").toString("hex") });
  assert.equal(integrityToHex("md5-xxx"), null);
  assert.equal(integrityToHex(undefined), null);
});

test("purl для cargo и npm (в том числе scoped)", () => {
  assert.equal(cargoPurl("serde", "1.0.1"), "pkg:cargo/serde@1.0.1");
  assert.equal(npmPurl("react", "18.3.1"), "pkg:npm/react@18.3.1");
  assert.equal(npmPurl("@tauri-apps/api", "2.0.0"), "pkg:npm/%40tauri-apps/api@2.0.0");
});

test("SPDX-выражения и одиночные лицензии", () => {
  assert.deepEqual(licenseEntry("MIT"), [{ license: { id: "MIT" } }]);
  assert.deepEqual(licenseEntry("MIT OR Apache-2.0"), [{ expression: "MIT OR Apache-2.0" }]);
  assert.deepEqual(licenseEntry(null), []);
});

test("контрольные суммы в формате sha256sum", () => {
  assert.equal(checksumsText([["ab", "x.exe"], ["cd", "y.zip"]]), "ab  x.exe\ncd  y.zip\n");
});

test("манифест обновления: строгая версия", () => {
  const m = updateManifest({ file: "a.exe", version: "0.2.0", size: 5, sha256: "f".repeat(64), notes: "n" });
  assert.deepEqual(Object.keys(m).sort(), ["file", "notes", "sha256", "size", "version"]);
  for (const bad of ["1", "1.2", "1.2.3-beta", "a.b.c", ""]) {
    assert.throws(() => updateManifest({ file: "a", version: bad, size: 1, sha256: "x" }), /версия/);
  }
});

test("SBOM содержит приложение, компоненты и хеши; уведомления называют FFmpeg внешним", () => {
  const pk = [
    { ecosystem: "cargo", name: "serde", version: "1.0.0", license: "MIT OR Apache-2.0", hash: "a".repeat(64), dir: "/нет" },
    { ecosystem: "npm", name: "react", version: "18.3.1", license: "MIT", hash: "b".repeat(128), hashAlg: "SHA-512", dir: "/нет" },
    { ecosystem: "npm", name: "nolic", version: "1.0.0", license: null, dir: "/нет" },
  ];
  const bom = renderSbom(pk, { appVersion: "0.1.0", timestamp: "2026-11-10T00:00:00Z" });
  assert.equal(bom.bomFormat, "CycloneDX");
  assert.equal(bom.specVersion, "1.5");
  assert.equal(bom.metadata.component.name, "YESKIA");
  assert.equal(bom.components.length, 3);
  assert.equal(bom.components[0].hashes[0].alg, "SHA-256");
  assert.equal(bom.components[1].hashes[0].alg, "SHA-512");
  assert.equal(bom.components[2].licenses.length, 0);
  const n = renderNotices(pk, { appVersion: "0.1.0" });
  assert.match(n, /FFmpeg \*\*не входит\*\*/);
  assert.match(n, /\*\*не указана\*\*/);
});

test("заметки релиза честно предупреждают о подписи и устройстве", () => {
  const n = renderNotes({ version: "0.2.0", subjects: ["feat: a", "fix: b"], sha: "abc" });
  assert.match(n, /не подписана/);
  assert.match(n, /verified-on-device/);
  assert.match(n, /- feat: a/);
});

test("ключ обновлений: подпись Ed25519 проверяется публичным ключом, подмена — нет", () => {
  const k = newUpdateKey();
  assert.equal(k.publicHex.length, 64);
  assert.equal(publicHexOf(k.pem), k.publicHex);
  const manifest = Buffer.from('{"version":"0.2.0"}');
  const sig = signManifest(k.pem, manifest);
  assert.equal(sig.length, 128);
  const spkiPrefix = Buffer.from("302a300506032b6570032100", "hex");
  const pub = createPublicKey({ key: Buffer.concat([spkiPrefix, Buffer.from(k.publicHex, "hex")]), format: "der", type: "spki" });
  assert.equal(verify(null, manifest, pub, Buffer.from(sig, "hex")), true);
  assert.equal(verify(null, Buffer.from('{"version":"9.9.9"}'), pub, Buffer.from(sig, "hex")), false);
});

test("packaging requires the fresh selected x64 application and emits only one setup.exe", { skip: process.platform === "win32" }, async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, copyFileSync, chmodSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join, resolve } = await import("node:path");
  const { execFileSync } = await import("node:child_process");
  const root=mkdtempSync(join(tmpdir(),"yeskia-package-"));
  const shellQuote=s=>"'"+s.replaceAll("'","'\\''")+"'";
  const config=JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json",import.meta.url)));
  const version=config.version;const rel="target/x86_64-pc-windows-msvc/release";
  try {
    for(const d of ["scripts","src-tauri","ui","bin","dist-release",`${rel}/bundle/nsis`])mkdirSync(join(root,d),{recursive:true});
    for(const file of ["scripts/package.sh","scripts/installer.conf.json","scripts/release-check.mjs","src-tauri/tauri.conf.json","Cargo.toml","package.json","package-lock.json","ui/package.json"])copyFileSync(new URL(`../${file}`,import.meta.url),join(root,file));
    const bytes=Buffer.alloc(256);bytes.write("MZ");bytes.writeUInt32LE(64,60);bytes.write("PE\0\0",64);bytes.writeUInt16LE(0x8664,68);bytes.writeUInt16LE(0x20b,88);bytes.writeUInt16LE(2,156);writeFileSync(join(root,"expected.exe"),bytes);
    writeFileSync(join(root,"dist-release/portable.zip"),"stale");
    writeFileSync(join(root,`${rel}/bundle/nsis/YESKIA_0.0.1_x64-setup.exe`),"stale installer");
    writeFileSync(join(root,`${rel}/yeskia-app.exe`),"stale application");
    writeFileSync(join(root,"bin/node"),`#!/bin/sh\nif [ "$1" = scripts/release.mjs ]; then printf 'test licenses' > build-support/THIRD_PARTY_NOTICES.md; else exec ${shellQuote(process.execPath)} "$@"; fi\n`);
    const metadata={packages:[{name:"yeskia-app",version,targets:[{name:"yeskia-app",kind:["bin"],src_path:"/repo/src-tauri/src/main.rs"}]}]};
    writeFileSync(join(root,"bin/cargo"),`#!/bin/sh\nprintf '%s' ${shellQuote(JSON.stringify(metadata))}\n`);
    writeFileSync(join(root,"bin/npm"),`#!/bin/sh\n[ "$*" = "run tauri build -- --ci --target x86_64-pc-windows-msvc --bundles nsis --config scripts/installer.conf.json -- --locked --bin yeskia-app" ] || exit 2\ntest -s build-support/THIRD_PARTY_NOTICES.md || exit 3\ncp expected.exe ${rel}/yeskia-app.exe\nprintf 'fresh installer' > ${rel}/bundle/nsis/YESKIA_${version}_x64-setup.exe\n`);
    for(const f of ["node","npm","cargo"])chmodSync(join(root,"bin",f),0o755);
    const env={...process.env,PATH:join(root,"bin")+":"+process.env.PATH};
    execFileSync("bash",["scripts/package.sh"],{cwd:root,env});
    assert.deepEqual(readdirSync(join(root,"dist-release")),[`YESKIA_${version}_x64-setup.exe`]);
    assert.equal(readFileSync(join(root,`dist-release/YESKIA_${version}_x64-setup.exe`),"utf8"),"fresh installer");
    assert(!readdirSync(join(root,`${rel}/bundle/nsis`)).includes("YESKIA_0.0.1_x64-setup.exe"));
    assert.throws(()=>execFileSync("bash",["scripts/package.sh","9.9.9"],{cwd:root,env,stdio:"pipe"}));
    // A successful tool exit without a freshly built app must not reuse the old executable.
    writeFileSync(join(root,"bin/npm"),'#!/bin/sh\nexit 0\n');
    assert.throws(()=>execFileSync("bash",["scripts/package.sh"],{cwd:root,env,stdio:"pipe"}));
    assert.deepEqual(readdirSync(join(root,"dist-release")),[]);
    const overlay=JSON.parse(readFileSync(resolve(root,"scripts/installer.conf.json")));
    assert.equal(overlay.bundle.resources["../build-support/THIRD_PARTY_NOTICES.md"],"THIRD_PARTY_NOTICES.md");
  }finally{rmSync(root,{recursive:true,force:true});}
});
