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
