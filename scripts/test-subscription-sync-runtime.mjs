import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';

function harness(reply, failure) {
  const calls = [], changes = [];
  let tunWrites = 0;
  const deps = {
    '@tauri-apps/api/core': { invoke: async (command, args) => { calls.push([command, args]); if (failure) throw failure; return reply; } },
    '$lib/features/configuration/profile-activation': { ProfileActivation: class {} },
    './core-events.svelte': { coreEvents: { stop: async () => {}, start: async () => {} } },
    './proxy-config-signal.svelte': { proxyConfigSignal: { markChanged: value => changes.push(['profiles', value]) } },
    './rule-set-signal.svelte': { ruleSetSignal: { markChanged: () => changes.push(['rules']) } },
    './tun': { reconcileGuiTunRuntime: async () => { tunWrites++; } },
  };
  const { outputText } = ts.transpileModule(readFileSync(new URL('../src/lib/services/config.ts', import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  });
  const exports = {};
  new Function('require', 'exports', outputText)(name => {
    assert.ok(name in deps, `unexpected dependency: ${name}`); return deps[name];
  }, exports);
  return { service: exports, calls, changes, tunWrites: () => tunWrites };
}

for (const target of ['active-profile', 'another-profile']) {
  test(`subscription sync retains ${target} association without issuing another capture operation`, async () => {
    const reply = { id: 'subscription', targetProxyConfigId: target };
    const h = harness(reply);
    assert.equal(await h.service.syncSubscription('subscription'), reply);
    assert.deepEqual(h.calls, [['subscription_sync', { id: 'subscription' }]]);
    assert.equal(h.tunWrites(), 0);
    assert.deepEqual(h.changes, [['profiles', true], ['rules']]);
  });
}

test('sync all does not retry a stopped or failed TUN after backend publication', async () => {
  const result = { total: 2, succeeded: 1, failed: 1 };
  const h = harness(result);
  assert.equal(await h.service.syncAllSubscriptions(), result);
  assert.deepEqual(h.calls, [['subscription_sync_all', undefined]]);
  assert.equal(h.tunWrites(), 0);
});

test('a failed sync propagates the error without invalidating successful state or touching TUN', async () => {
  const error = new Error('candidate rejected');
  const h = harness(undefined, error);
  await assert.rejects(h.service.syncSubscription('subscription'), error);
  assert.deepEqual(h.changes, []);
  assert.equal(h.tunWrites(), 0);
});
