import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AndroidUpdateController, validAndroidUpdate } from '../src/lib/features/mobile/android-update.ts';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import { compareAppVersions } from '../src/lib/services/app-update-policy.ts';
const browser = {};
new Function('exports', 'require', ts.transpileModule(readFileSync(new URL('../src/lib/features/mobile/release-browser.ts', import.meta.url), 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText)(browser, () => ({ compareAppVersions }));
const { validReleaseCatalog } = browser;

const id = 'e2695293-5580-4eec-9437-a1e6b2621111';
const snapshot = (state = 'ready') => ({ state, operationId: id, reason: null, packageName: 'org.zerodenet.znetsink.debug', installedVersion: '1.0.0', installedVersionCode: '1', targetVersion: '1.1.0', targetVersionCode: '2', downloadedBytes: '100', sizeBytes: '100', assetName: 'client.apk', sha256: 'a'.repeat(64), installPermission: 'allowed', confirmationAvailable: false, installerStatus: null });
test('unknown installation reply blocks replay until authoritative read', async () => {
  let calls = 0;
  const gateway = { status: async () => snapshot(), action: async () => { calls++; throw Error('lost response'); }, download: async () => { throw Error('unexpected'); } };
  const owner = new AndroidUpdateController(gateway, () => {});
  await owner.read(); await owner.action('install');
  assert.equal(owner.state.phase, 'unknown');
  await owner.action('install'); assert.equal(calls, 1);
  gateway.status = async () => snapshot('awaiting-user');
  await owner.read(); assert.equal(owner.state.snapshot.state, 'awaiting-user'); assert.equal(calls, 1);
});
test('confirmation is bound to the reviewed operation and never follows a changed target', async () => {
  let calls = 0;
  const owner = new AndroidUpdateController({ status: async () => snapshot(), action: async () => { calls++; return snapshot(); }, download: async () => snapshot() }, () => {});
  await owner.read(); await owner.action('install', 'f2695293-5580-4eec-9437-a1e6b2621111');
  assert.equal(calls, 0); assert.match(owner.state.error, /目标已变化/);
});
test('late status cannot overwrite an installation reply', async () => {
  let done;
  const owner = new AndroidUpdateController({ status: async () => new Promise(resolve => done = resolve), action: async () => snapshot('installing'), download: async () => snapshot() }, () => {});
  const reading = owner.read(); owner.destroy(); done(snapshot()); await reading;
  assert.equal(owner.state.snapshot, null);
});
test('unsafe counters and status are rejected', () => {
  assert.equal(validAndroidUpdate(snapshot()), true);
  assert.equal(validAndroidUpdate({ ...snapshot(), downloadedBytes: 9007199254740992 }), false);
  assert.equal(validAndroidUpdate({ ...snapshot(), state: 'success' }), false);
  assert.equal(validAndroidUpdate({ ...snapshot(), reason: 'https://token=secret' }), false);
});
test('Android installer catalog keeps digest and project URL bound to selected tag', () => {
  const release = { tagName: 'v1.1.0', version: '1.1.0', channel: 'stable', kernelChannel: null, publishedAt: null, notes: null, notesTruncated: false, releaseUrl: 'https://github.com/zerodenet/znet-sink/releases/tag/v1.1.0', androidAssets: [{ assetId: '1', name: 'client.apk', sizeBytes: '100', sha256: 'a'.repeat(64), url: 'https://github.com/zerodenet/znet-sink/releases/download/v1.1.0/client.apk' }] };
  const catalog = { source: 'client', channel: 'stable', platform: 'android', requestId: 'request', responseStatus: 200, reusedValidatedCache: false, checkedAtUnixMs: '1', catalogLimit: 30, targetReason: 'system confirmation', targetStatus: 'android-system-installer', releases: [release] };
  assert.equal(validReleaseCatalog(catalog), true);
  assert.equal(validReleaseCatalog({ ...catalog, platform: 'ios' }), false);
  assert.equal(validReleaseCatalog({ ...catalog, releases: [{ ...release, androidAssets: [{ ...release.androidAssets[0], sha256: '' }] }] }), false);
});
