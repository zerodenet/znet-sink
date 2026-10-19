import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';

function harness(reply, failure) {
  const calls = [], changes = [];
  let tunWrites = 0;
  const deps = {
    '@tauri-apps/api/core': { invoke: async (command, args) => { calls.push([command, args]); if (failure) throw failure; return typeof reply === 'function' ? reply(command, args) : reply; } },
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

for (const operation of ['upsertProxyConfig','importProxyConfig']) {
  test(`saving an inactive configuration via ${operation} cannot reconcile or enable TUN`, async()=>{
    const h=harness({id:'inactive',active:false});
    await h.service[operation]({name:'saved'});
    assert.equal(h.tunWrites(),0);
    assert.deepEqual(h.changes,[['profiles',false],['rules']]);
  });
}
test('removing only a configuration source keeps runtime capture untouched',async()=>{
  const h=harness({removedProxyConfig:false,removedManagedRuleSetCount:0});
  await h.service.removeSubscription('source',false);
  assert.equal(h.tunWrites(),0);
  assert.deepEqual(h.calls,[['subscription_remove',{id:'source',removeAssociatedConfig:false}]]);
});


test('deleting an inactive configuration never reconciles TUN', async () => {
  const h=harness(command=>command==='proxy_config_get'?{id:'other',active:false}:undefined);
  await h.service.removeProxyConfig('other');
  assert.equal(h.tunWrites(),0);
  assert.deepEqual(h.calls,[['proxy_config_get',{id:'other'}],['proxy_config_remove',{id:'other'}]]);
});
test('deleting an inactive source and its target leaves current capture alone',async()=>{
  const h=harness(command=>command==='subscription_remove_preview'?{targetProxyConfig:{active:false}}:{removedProxyConfig:true});
  await h.service.removeSubscription('other',true);
  assert.equal(h.tunWrites(),0);
});
