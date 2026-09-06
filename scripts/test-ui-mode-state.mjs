import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';

// Execute the production presentation store with controlled persistence.
function harness() {
  const requests = [], errors = [];
  const backend = { mode: 'pro' };
  const networkCalls = [];
  const gui = { isCaptureEnabled: false, prepareLiteCapture: async () => { networkCalls.push('handoff'); } };
  const dependencies = {
    '$app/environment': { browser: false },
    './core': {
      updateAppConfig: patch => new Promise((resolve, reject) => {
        requests.push({ mode: patch.ui.uiMode, reject, finish() { backend.mode = patch.ui.uiMode; resolve(); } });
      }),
      getGuiInteractionSurfaceSnapshot: async () => ({ navigation: [{ key: 'overview', visible: true }], actions: [], features: [] }),
    },
    './gui-state.svelte': { guiState: gui },
    './toast.svelte': { error: message => errors.push(message) },
    './onboarding': {},
  };
  const source = readFileSync(new URL('../src/lib/services/store.svelte.ts', import.meta.url), 'utf8');
  const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } });
  const exports = {};
  new Function('require', 'exports', '$state', 'console', outputText)(name => {
    assert.ok(name in dependencies, `unexpected dependency: ${name}`); return dependencies[name];
  }, exports, value => value, { time() {}, timeEnd() {}, error() {}, warn() {} });
  exports.store.uiMode = 'pro';
  return { store: exports.store, requests, errors, backend, gui, networkCalls };
}
const flush = () => new Promise(resolve => setImmediate(resolve));

test('rapid mode changes update presentation immediately and persist in selection order', async () => {
  const { store, requests, backend } = harness();
  await store.switchUIMode('lite'); await flush();
  await store.switchUIMode('pro'); await flush();
  assert.equal(store.uiMode, 'pro'); assert.equal(store.isSwitchingUiMode, false);
  assert.equal(requests.length, 1, 'a delayed older preference must not overwrite the latest selection');
  requests[0].finish(); await flush();
  assert.equal(requests.length, 2); assert.equal(requests[1].mode, 'pro');
  requests[1].finish(); await flush();
  assert.equal(backend.mode, 'pro'); assert.equal(store.uiMode, 'pro');
});

test('an older save failure does not stop the latest preference or replace its feedback', async () => {
  const { store, requests, backend, errors } = harness();
  await store.switchUIMode('lite'); await flush();
  await store.switchUIMode('pro'); await flush();
  requests[0].reject(new Error('old request failed')); await flush();
  assert.equal(requests.length, 2);
  requests[1].finish(); await flush();
  assert.equal(backend.mode, 'pro'); assert.equal(store.uiMode, 'pro');
  assert.deepEqual(errors, []);
});

test('latest save failure preserves the selected layout and reports lack of persistence', async () => {
  const { store, requests, errors } = harness();
  await store.switchUIMode('lite'); await flush();
  requests[0].reject(new Error('disk unavailable')); await flush();
  assert.equal(store.uiMode, 'lite'); assert.equal(store.isSwitchingUiMode, false);
  assert.equal(errors.length, 1); assert.match(errors[0], /保存设置失败/);
});

test('changing presentation preserves an active partial capture session without submitting network operations', async () => {
  const { store, gui, requests, networkCalls } = harness();
  gui.isCaptureEnabled = true;
  await store.switchUIMode('lite'); await flush();
  requests[0].finish(); await flush();
  assert.equal(store.uiMode, 'lite'); assert.equal(gui.isCaptureEnabled, true);
  assert.deepEqual(networkCalls, []);
  await store.switchUIMode('pro'); await flush();
  requests[1].finish(); await flush();
  assert.deepEqual(networkCalls, []);
});
