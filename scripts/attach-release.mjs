#!/usr/bin/env node
// Only attach a byte-verified artifact from matching source to an existing release.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const requireValue = (ok, message) => { if (!ok) throw new Error(message); };
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export function verifyCandidate({ run, artifact, release, tag, sourceTree, targetTree, config }) {
  const version = /^v?(\d+\.\d+\.\d+)$/.exec(tag)?.[1];
  requireValue(version && release.tag_name === tag, 'Existing release tag must be major.minor.patch');
  requireValue(run.status === 'completed' && run.conclusion === 'success', 'Source workflow must succeed');
  requireValue(['.github/workflows/ci.yml', '.github/workflows/release.yml'].includes(run.path), 'Unrecognized source workflow');
  requireValue(artifact.name === 'YESKIA-Windows-Installer' && !artifact.expired, 'Missing or expired installer artifact');
  requireValue(artifact.workflow_run?.id === run.id, 'Artifact belongs to another run');
  requireValue(/^sha256:[a-f0-9]{64}$/.test(artifact.digest), 'Artifact must have a SHA-256 digest');
  requireValue(sourceTree && sourceTree === targetTree, 'Release source differs from the tested source');
  requireValue(config.version === version && config.mainBinaryName === 'yeskia-app' && config.productName === 'YESKIA', 'Release app identity/version mismatch');
  return `YESKIA_${version}_x64-setup.exe`;
}
export function verifyArchive(bytes, digest) {
  requireValue(`sha256:${sha256(bytes)}` === digest, 'Downloaded archive digest mismatch');
}
export function existingAssetMatches(assets, name, size, digest) {
  const same = assets.filter(a => a.name === name);
  requireValue(same.length <= 1, 'Ambiguous release asset');
  if (!same.length) return false;
  requireValue(same[0].size === size && same[0].digest === `sha256:${digest}`, 'Existing asset differs; refusing to overwrite');
  return true;
}
export function releaseUnchanged(before, after) {
  for (const k of ['id', 'tag_name', 'target_commitish', 'draft', 'prerelease'])
    requireValue(before[k] === after[k], `Release changed during transfer: ${k}`);
}

function main() {
  const repo = process.env.GITHUB_REPOSITORY, runId = process.env.SOURCE_RUN_ID, tag = process.env.RELEASE_TAG;
  requireValue(/^[\w.-]+\/[\w.-]+$/.test(repo ?? '') && /^\d+$/.test(runId ?? '') && /^v?\d+\.\d+\.\d+$/.test(tag ?? ''), 'Invalid repository, run or tag');
  const gh = (...args) => {
    try { return execFileSync('gh', args, { maxBuffer: 64 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] }); }
    catch (e) { throw new Error(`GitHub ${args[0]} failed (exit ${e.status}); check workflow access.`, { cause: undefined }); }
  };
  const api = path => JSON.parse(gh('api', `repos/${repo}/${path}`).toString());
  const releasePath = `releases/tags/${tag}`;
  const release = api(releasePath); // Existing only: no create/edit/publish calls.
  const run = api(`actions/runs/${runId}`);
  requireValue(run.repository?.full_name === repo && run.head_repository?.full_name === repo, 'Source must be from this repository');
  const artifacts = api(`actions/runs/${runId}/artifacts?per_page=100`).artifacts.filter(a => a.name === 'YESKIA-Windows-Installer');
  requireValue(artifacts.length === 1, 'Expected exactly one installer artifact');
  const artifact = artifacts[0];
  const source = api(`git/commits/${run.head_sha}`);
  // Published releases resolve their real tag, not the possibly stale target_commitish.
  let ref;
  try { ref = api(`git/ref/tags/${tag}`).object; } catch (e) { if (!release.draft) throw e; }
  for (let i = 0; ref?.type === 'tag' && i < 5; i++) ref = api(`git/tags/${ref.sha}`).object;
  requireValue(!ref || ref.type === 'commit', 'Tag must resolve to a commit');
  const target = api(`commits/${ref?.sha ?? encodeURIComponent(release.target_commitish)}`);
  const configFile = api(`contents/src-tauri/tauri.conf.json?ref=${run.head_sha}`);
  const config = JSON.parse(Buffer.from(configFile.content, 'base64').toString());
  const name = verifyCandidate({ run, artifact, release, tag, sourceTree: source.tree.sha, targetTree: target.commit.tree.sha, config });
  const dir = mkdtempSync(join(tmpdir(), 'yeskia-attach-'));
  try {
    const archiveIndex = process.argv.indexOf('--archive');
    requireValue(archiveIndex < 0 || process.argv.includes('--verify-only'), 'Local archives are only allowed for read-only verification');
    const archive = archiveIndex >= 0 ? readFileSync(process.argv[archiveIndex + 1]) : gh('api', `repos/${repo}/actions/artifacts/${artifact.id}/zip`);
    verifyArchive(archive, artifact.digest);
    const zip = join(dir, 'artifact.zip'), exe = join(dir, name);
    writeFileSync(zip, archive);
    execFileSync('python3', ['-c', 'import sys,zipfile\nwith zipfile.ZipFile(sys.argv[1]) as z:\n if z.namelist()!=[sys.argv[3]]: raise ValueError("Expected one installer, no paths")\n z.extractall(sys.argv[2])', zip, dir, name]);
    const bytes = readFileSync(exe), digest = sha256(bytes);
    requireValue(bytes.subarray(0, 2).toString() === 'MZ', 'Installer is not a Windows executable');
    const fresh = api(releasePath);
    releaseUnchanged(release, fresh);
    if (process.argv.includes('--verify-only')) {
      console.log(JSON.stringify({ name, size: bytes.length, sha256: digest, sourceTree: source.tree.sha, target: target.sha, draft: fresh.draft }));
      return;
    }
    if (!existingAssetMatches(fresh.assets, name, bytes.length, digest)) gh('release', 'upload', tag, exe, '--repo', repo);
    const attached = api(releasePath);
    releaseUnchanged(release, attached);
    requireValue(existingAssetMatches(attached.assets, name, bytes.length, digest), 'Uploaded asset was not verified');
    const message = `${name}\nSHA-256: ${digest}\nSource: ${run.html_url}\nRelease: ${attached.html_url}\nPublication state preserved.\n`;
    console.log(message);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, message);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
