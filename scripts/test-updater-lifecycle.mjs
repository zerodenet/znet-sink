import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { createContext, SourceTextModule, SyntheticModule } from 'node:vm';
import { test } from 'node:test';

// Exercise the actual service with deterministic native API boundaries. Runes
// are identity values here: these tests concern resource ownership, not rendering.
const source = stripTypeScriptTypes(readFileSync('src/lib/services/updater.svelte.ts', 'utf8'));
const release = { tagName: 'v2.0.0', version: '2.0.0', notes: '' };
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
async function fixture(overrides = {}) {
  const calls = { check: 0, close: 0, download: 0, install: 0, select: 0, relaunch: 0 };
  const update = {
    version: '2.0.0', currentVersion: '1.0.0',
    close: async () => { calls.close++; },
    download: async () => { calls.download++; await overrides.download?.(); },
    install: async () => { calls.install++; await overrides.install?.(); },
    downloadAndInstall: async () => { calls.install++; await overrides.install?.(); },
  };
  const context = createContext({ $state: (value) => value, Error, Date, setTimeout, clearTimeout, setInterval, clearInterval });
  const modules = {
    '@tauri-apps/plugin-updater': {
      check: async () => { calls.check++; return overrides.check ? overrides.check() : update; },
      Update: class { constructor() { return update; } },
    },
    '@tauri-apps/api/app': { getVersion: async () => '1.0.0' },
     '@tauri-apps/api/core': {
      Channel: class {},
      invoke: async (command, args) => {
        if (command === 'app_download_update') { calls.download++; await overrides.download?.(args.onEvent); return; }
        if (command === 'app_install_update') { calls.install++; await overrides.install?.(); return; }
        calls.select++; return overrides.select ? overrides.select() : {};
      },
    },
    '@tauri-apps/plugin-process': { relaunch: async () => { calls.relaunch++; await overrides.relaunch?.(); } },
    '$lib/services/toast.svelte': { info() {}, warning() {} },
    '$lib/services/core': { appendLog: async () => {} },
    '$lib/services/telemetry': { tracedOperation: (_category, _name, operation) => operation() },
    '$lib/services/app-update-policy': { shouldShowProminentUpdate: () => true },
    '$lib/services/release-check-policy': { RELEASE_CHECK_INTERVAL_MS: 3600000 },
  };
  const module = new SourceTextModule(source, { context });
  await module.link((name) => {
    const exports = modules[name];
    assert.ok(exports, `unexpected dependency ${name}`);
    return new SyntheticModule(Object.keys(exports), function () {
      for (const [name, value] of Object.entries(exports)) this.setExport(name, value);
    }, { context });
  });
  await module.evaluate();
  return { updater: module.namespace.updater, calls };
}

test('download retains its native resource against check, select, dismiss and duplicate download', async () => {
  const gate = deferred();
  const { updater, calls } = await fixture({ download: () => gate.promise });
  await updater.checkForUpdate();
  const pending = updater.downloadUpdate();
  assert.equal(await updater.checkForUpdate(), false);
  assert.equal(await updater.selectRelease(release), false);
  assert.equal(await updater.downloadAndInstall(), false);
  assert.equal(await updater.downloadUpdate(), false);
  updater.dismissUpdate();
  assert.equal(calls.close, 0);
  assert.equal(calls.check, 1);
  assert.equal(calls.download, 1);
  gate.resolve();
  assert.equal(await pending, true);
  assert.equal(await updater.checkForUpdate(), false);
  assert.equal(updater.readyToInstall, true);
});

test('install excludes every competing transition until relaunch finishes', async () => {
  const gate = deferred();
  const { updater, calls } = await fixture({ install: () => gate.promise });
  await updater.checkForUpdate();
  await updater.downloadUpdate();
  const pending = updater.installUpdate();
  assert.equal(updater.installing, true);
  assert.equal(await updater.installUpdate(), false);
  assert.equal(await updater.checkForUpdate(), false);
  assert.equal(await updater.selectRelease(release), false);
  updater.dismissUpdate();
  assert.equal(calls.close, 0);
  gate.resolve();
  assert.equal(await pending, true);
  assert.equal(calls.install, 1);
  assert.equal(calls.relaunch, 1);
});

test('relaunch failure preserves installed state and retries only relaunch', async () => {
  let fail = true;
  const { updater, calls } = await fixture({ relaunch: () => { if (fail) throw new Error('restart unavailable'); } });
  await updater.checkForUpdate();
  await updater.downloadUpdate();
  assert.equal(await updater.installUpdate(), false);
  assert.equal(updater.status, 'restart-required');
  assert.equal(updater.readyToInstall, false);
  assert.equal(await updater.installUpdate(), false);
  assert.equal(await updater.downloadAndInstall(), false);
  fail = false;
  assert.equal(await updater.restartApp(), true);
  assert.equal(calls.install, 1);
  assert.equal(calls.relaunch, 2);
});

test('immediate installation also preserves installed state after restart failure', async () => {
  const { updater, calls } = await fixture({ relaunch: () => { throw new Error('restart unavailable'); } });
  assert.equal(await updater.downloadAndInstall(), false);
  assert.equal(updater.restartRequired, true);
  assert.equal(updater.status, 'restart-required');
  assert.equal(await updater.downloadAndInstall(), false);
  assert.equal(calls.install, 1);
});

for (const outcome of ['missing', 'error']) {
  test(`selection ${outcome} clears a previous downloaded release`, async () => {
    const { updater, calls } = await fixture({ select: () => { if (outcome === 'error') throw new Error('missing field version'); return null; } });
    await updater.checkForUpdate();
    await updater.downloadUpdate();
    assert.equal(await updater.selectRelease(release), false);
    assert.equal(updater.updateAvailable, false);
    assert.equal(updater.latestVersion, null);
    assert.equal(updater.readyToInstall, false);
    assert.equal(await updater.installUpdate(), false);
    assert.equal(calls.close, 1);
    assert.equal(calls.install, 0);
  });
}

test('unusable manifest is a visible check failure, never an up-to-date result', async () => {
  const { updater } = await fixture({ check: () => { throw new Error('missing field version'); } });
  assert.equal(await updater.checkForUpdate(), false);
  assert.equal(updater.status, 'error');
  assert.match(updater.lastError, /无法确认/);
});

test('resumed progress is absolute and full-download fallback resets it', async () => {
  const { updater } = await fixture({ download: (channel) => {
    channel.onmessage({ bytesDownloaded: 400, bytesTotal: 1000, state: 'downloading', attempt: 1 });
    assert.equal(updater.downloaded, 400);
    channel.onmessage({ bytesDownloaded: 400, bytesTotal: 1000, state: 'retrying', attempt: 1 });
    assert.match(updater.downloadLabel, /重试/);
    channel.onmessage({ bytesDownloaded: 0, bytesTotal: 2000, state: 'downloading', attempt: 2 });
    assert.equal(updater.downloaded, 0);
    assert.equal(updater.total, 2000);
  } });
  await updater.checkForUpdate();
  assert.equal(await updater.downloadUpdate(), true);
});

test('download interruption keeps selected release available for resume and never installs', async () => {
  let interrupted = true;
  const { updater, calls } = await fixture({ download: () => { if (interrupted) throw new Error('下载中断，已保留进度'); } });
  await updater.checkForUpdate();
  assert.equal(await updater.downloadUpdate(), false);
  assert.equal(updater.updateAvailable, true);
  assert.equal(updater.readyToInstall, false);
  assert.equal(calls.install, 0);
  interrupted = false;
  assert.equal(await updater.downloadUpdate(), true);
  assert.equal(calls.check, 1);
});
