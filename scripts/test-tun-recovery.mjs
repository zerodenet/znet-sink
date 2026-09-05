import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
import { createLatestRequestGate } from '../src/lib/services/latest-request-gate.js';

// Execute production service/store methods with controlled IPC responses.
// Svelte's state primitive is inert here; rendering remains covered by check/build.
function loadService(file, dependencies) {
  const source = readFileSync(new URL(`../src/lib/services/${file}`, import.meta.url), 'utf8');
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  });
  const exports = {};
  new Function('require', 'exports', '$state', outputText)((name) => {
    assert.ok(name in dependencies, `unexpected dependency: ${name}`);
    return dependencies[name];
  }, exports, (value) => value);
  return exports;
}

function tunService({ status, stop, start = () => {}, profile = {} }) {
  const config = { tun: { enabled: true } };
  const calls = [];
  const service = loadService('tun.ts', {
    '@tauri-apps/api/core': {
      async invoke(command) {
        if (command === 'proxy_config_list') return [{ active: true, content: profile }];
        if (command === 'gui_tun_status') return status();
        if (command === 'gui_tun_enable') { calls.push('start'); return start(); }
        if (command === 'gui_tun_disable') { calls.push('stop'); return stop(); }
        throw new Error(`unexpected command: ${command}`);
      },
    },
    './core': {
      async getAppConfig() { return config; },
      async getGuiConnectionStatus() { return { coreAvailable: true }; },
      async getGuiCoreHealth() { return { healthy: true }; },
      async updateAppConfig(patch) { calls.push(`save:${patch.tun.enabled}`); Object.assign(config.tun, patch.tun); },
    },
    '$lib/services/kernel-capabilities': {},
  });
  return { service, config, calls };
}

const snapshot = (enabled, desiredEnabled = enabled) => ({ enabled, desiredEnabled, supported: true });

test('unreachable runtime: explicit OFF persists before stop and survives failure', async () => {
  const failure = { code: 'refused', message: 'unreachable' };
  const { service, config, calls } = tunService({
    status: () => Promise.reject(failure), stop: () => Promise.reject(failure),
  });
  await assert.rejects(service.disableGuiTun(), (error) => error === failure);
  assert.equal(config.tun.enabled, false);
  assert.deepEqual(calls, ['save:false', 'stop']);
});

test('lost stop reply succeeds only after a stopped runtime is observed', async () => {
  let stopped = false;
  const { service, calls } = tunService({
    status: () => snapshot(!stopped),
    stop: () => { stopped = true; throw { code: 'timeout' }; },
  });
  assert.equal((await service.disableGuiTun()).enabled, false);
  assert.deepEqual(calls, ['save:false', 'stop']);
});

test('successful stop acknowledgement with running status remains an error', async () => {
  const { service, config } = tunService({ status: () => snapshot(true), stop: () => snapshot(true) });
  await assert.rejects(service.disableGuiTun(), { code: 'tun_stop_unconfirmed' });
  assert.equal(config.tun.enabled, false);
});

test('profile-owned TUN is never overwritten by cancelling app intent', async () => {
  for (const tun of [null, { name: 'profile-tun' }]) {
    const { service, config, calls } = tunService({ profile: { runtime: { tun } } });
    await assert.rejects(service.disableGuiTun(), { code: 'tun_managed_by_profile' });
    assert.equal(config.tun.enabled, true);
    assert.deepEqual(calls, []);
  }
});

function storeHarness({ status, desired = true, restart = async () => ({}) }) {
  const notifications = [];
  const { guiState } = loadService('gui-state.svelte.ts', {
    './core': { getAppConfig: async () => ({ tun: { enabled: desired } }), restartCoreProcess: restart, trayUpdateStatus: async () => {} },
    './tun': { getGuiTunStatus: status },
    './toast.svelte': Object.fromEntries(['error', 'success', 'warning'].map((level) => [level, (message) => notifications.push({ level, message })])),
    './telemetry': { tracedOperation: async (_area, _operation, operation) => operation() },
    './latest-request-gate.js': { createLatestRequestGate },
    './node-state-reconcile': {},
  });
  guiState.isInitializing = false;
  guiState.connection = { processState: 'running', coreAvailable: true };
  guiState.refreshRuntimeState = async () => {};
  guiState.refreshSelfTest = async () => {};
  return { state: guiState, notifications };
}

test('query failure keeps last observation, exposes unknown and allows cancelling saved ON', async () => {
  const { state } = storeHarness({ status: async () => { throw { message: 'pipe unavailable' }; } });
  const previous = snapshot(false, true);
  state.tunStatus = previous;
  await state.refreshTunStatus();
  assert.equal(state.tunStatus, previous);
  assert.equal(state.tunStatusError, 'pipe unavailable');
  assert.equal(state.isTunSwitchOn, true);
  assert.equal(state.canDisableTun, true);
  let disabled = false;
  state.disableTun = async () => { disabled = true; };
  state.enableTun = () => assert.fail('must not enable on unknown state');
  await state.toggleTun();
  assert.equal(disabled, true);
});

test('old query cannot overwrite a newer confirmed snapshot', async () => {
  let resolveOld;
  let queries = 0;
  const { state } = storeHarness({ status: () => ++queries === 1
    ? new Promise((resolve) => { resolveOld = resolve; }) : Promise.resolve(snapshot(true)) });
  const old = state.refreshTunStatus();
  await state.refreshTunStatus();
  resolveOld(snapshot(false));
  await old;
  assert.equal(state.isTunEnabled, true);
});

test('restart discards in-flight old status and warns about partial restoration', async () => {
  let resolveOld;
  const { state, notifications } = storeHarness({
    status: () => new Promise((resolve) => { resolveOld = resolve; }),
    restart: async () => ({ tunRestoreError: { message: 'restore timeout' } }),
  });
  const old = state.refreshTunStatus();
  await state.restartCore();
  resolveOld(snapshot(false));
  await old;
  assert.equal(state.tunStatus, null);
  assert.ok(state.tunStatusError);
  assert.equal(notifications.length, 1);
  assert.equal(notifications[0].level, 'warning');
  assert.match(notifications[0].message, /restore timeout/);
});

test('restart and TUN mutation guards prevent overlapping user actions', () => {
  const { state } = storeHarness({ status: async () => snapshot(true) });
  state.tunStatus = snapshot(true);
  state.isSwitchingTun = true;
  assert.equal(state.canRestartCore, false);
  state.isSwitchingTun = false;
  state.isStoppingCore = true;
  assert.equal(state.canDisableTun, false);
  assert.equal(state.canEnableTun, false);
});

for (const profileOwned of [false, true]) {
  test(`enabled but unhealthy TUN is never a successful enable (profile owned: ${profileOwned})`, async () => {
    const { service } = tunService({
      status: () => ({ ...snapshot(true), healthy: false }),
      profile: profileOwned ? { runtime: { tun: {} } } : {},
    });
    await assert.rejects(service.enableGuiTun(), {
      code: profileOwned ? 'tun_profile_runtime_inactive' : 'tun_start_unconfirmed',
    });
  });
}

test('healthy enabled TUN can confirm an explicit enable', async () => {
  const { service, calls } = tunService({ status: () => ({ ...snapshot(true), healthy: true }) });
  assert.equal((await service.enableGuiTun()).healthy, true);
  assert.deepEqual(calls, ['save:true']);
});
