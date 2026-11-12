import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyCandidate, verifyArchive, existingAssetMatches, releaseUnchanged, sha256 } from './attach-release.mjs';
const candidate = () => ({
  run: { id: 123, status: 'completed', conclusion: 'success', path: '.github/workflows/ci.yml' },
  artifact: { name: 'YESKIA-Windows-Installer', expired: false, workflow_run: { id: 123 }, digest: `sha256:${'a'.repeat(64)}` },
  release: { tag_name: '0.5.0', draft: true }, tag: '0.5.0', sourceTree: 'abc', targetTree: 'abc',
  config: { version: '0.5.0', mainBinaryName: 'yeskia-app', productName: 'YESKIA' },
});
test('matching tested source can attach to existing draft or published release', () => {
  for (const draft of [true, false]) { const c = candidate(); c.release.draft = draft; assert.equal(verifyCandidate(c), 'YESKIA_0.5.0_x64-setup.exe'); }
});
test('unverified run, different source/version, expired or foreign artifact are rejected', () => {
  for (const mutate of [
    c => { c.run.conclusion = 'failure'; }, c => { c.run.status = 'in_progress'; },
    c => { c.run.path = 'other.yml'; }, c => { c.targetTree = 'other'; },
    c => { c.config.version = '0.4.0'; }, c => { c.config.mainBinaryName = 'yeskia-lab'; },
    c => { c.artifact.expired = true; }, c => { c.artifact.workflow_run.id = 124; },
    c => { c.artifact.digest = null; }, c => { c.artifact.name = 'other'; },
    c => { c.release.tag_name = 'other'; }, c => { c.tag = '../bad'; },
  ]) { const c = candidate(); mutate(c); assert.throws(() => verifyCandidate(c)); }
});
test('archive substitution and conflicting release assets fail without clobber', () => {
  const bytes = Buffer.from('MZ-test'), digest = sha256(bytes);
  verifyArchive(bytes, `sha256:${digest}`);
  assert.throws(() => verifyArchive(Buffer.from('changed'), `sha256:${digest}`));
  assert.equal(existingAssetMatches([], 'setup.exe', bytes.length, digest), false);
  const asset = { name: 'setup.exe', size: bytes.length, digest: `sha256:${digest}` };
  assert.equal(existingAssetMatches([asset], asset.name, bytes.length, digest), true);
  assert.throws(() => existingAssetMatches([{ ...asset, digest: 'different' }], asset.name, bytes.length, digest));
  assert.throws(() => existingAssetMatches([asset, asset], asset.name, bytes.length, digest));
});
test('publication or target changes during transfer are rejected', () => {
  const release = { id: 1, tag_name: '0.5.0', target_commitish: 'abc', draft: true, prerelease: true };
  releaseUnchanged(release, { ...release, assets: ['new'] });
  for (const key of ['id', 'tag_name', 'target_commitish', 'draft', 'prerelease']) assert.throws(() => releaseUnchanged(release, { ...release, [key]: 'changed' }));
});
