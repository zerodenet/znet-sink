import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';

// Execute the complete production module. Only imported platform/state ports
// are replaced; no copy of the production function or its validator is used.
const source = readFileSync(new URL('../src/lib/services/config.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
});
function receipt() {
  return {
    status: 'result-unknown',
    source: { id: 'source', updatedAtUnixMs: '15', lastSyncAtUnixMs: '10' },
    material: {
      status: 'saved-valid', profileId: 'profile', updatedAtUnixMs: '11',
      active: false, validation: 'native-schema-only',
    },
    evidence: { candidate: 'not-recorded', receipt: 'not-recorded' },
    runtime: {
      status: 'readable', coreInstanceId: 'actual-instance', configRevision: '9007199254740993',
      reasonCode: 'runtime_identity_only', reason: 'Stable current identity only',
    },
    canResolve: false,
    reason: 'The original candidate and receipt were not durably recorded',
  };
}
function harness(payload = receipt()) {
  const calls = [];
  const mutations = [];
  const effects = name => (...args) => { mutations.push({ name, args }); };
  const ports = {
    '@tauri-apps/api/core': {
      invoke: async (name, args) => {
        calls.push({ name, args });
        assert.equal(name, 'subscription_reconcile_unknown', 'inspection must not invoke sync/apply/reset');
        return payload;
      },
    },
    '$lib/features/configuration/profile-activation': {
      ProfileActivation: class {
        constructor(options) { this.options = options; }
        activate = effects('profile activation');
      },
    },
    './core-events.svelte': { coreEvents: { start: effects('start observation'), stop: effects('stop observation') } },
    './proxy-config-signal.svelte': { proxyConfigSignal: { markChanged: effects('profile signal'), revision: 0 } },
    './rule-set-signal.svelte': { ruleSetSignal: { markChanged: effects('rule signal'), revision: 0 } },
    './tun': { reconcileGuiTunRuntime: effects('TUN reconciliation') },
  };
  const api = {};
  new Function('require', 'exports', 'console', outputText)(name => {
    assert.ok(Object.hasOwn(ports, name), `unmocked production dependency: ${name}`);
    return ports[name];
  }, api, { warn: effects('warning') });
  return {
    api, calls, mutations,
    run: (id = 'source', version = 15, target = 'profile') => api.reconcileUnknownSubscription(id, version, target),
    assertReadOnly() {
      assert.deepEqual(mutations, [], 'inspection must not mutate signals, activate profiles or reconcile TUN');
      for (const call of calls) assert.equal(call.name, 'subscription_reconcile_unknown');
    },
  };
}
async function rejectPatch(patch) {
  const value = receipt();
  patch(value);
  const state = harness(value);
  await assert.rejects(state.run(), /订阅结果核对回执未确认/);
  state.assertReadOnly();
}

test('stable current runtime stays unknown and u64 is never rounded; production API performs one read only', async () => {
  const value = receipt();
  const before = structuredClone(value);
  const state = harness(value);
  const result = await state.run();
  assert.equal(result.runtime.configRevision, '9007199254740993');
  assert.equal(result.status, 'result-unknown');
  assert.equal(result.canResolve, false);
  assert.deepEqual(state.calls, [{ name: 'subscription_reconcile_unknown', args: { id: 'source', expectedUpdatedAtUnixMs: '15' } }]);
  assert.deepEqual(value, before, 'inspection must not modify the original response');
  state.assertReadOnly();
});

test('unsafe, negative, fractional or nonnumeric expected source versions never invoke', async () => {
  for (const version of [Number.MAX_SAFE_INTEGER + 1, -1, 1.5, NaN, Infinity, '15', null, undefined]) {
    const state = harness();
    await assert.rejects(state.api.reconcileUnknownSubscription('source', version, 'profile'));
    assert.equal(state.calls.length, 0);
    state.assertReadOnly();
  }
});

test('missing or unfixed source and target inputs never invoke', async () => {
  for (const [id, target] of [['', 'profile'], [' source', 'profile'], ['source ', 'profile'], ['source', ''], ['source', undefined], ['source', 1]]) {
    const state = harness();
    await assert.rejects(state.api.reconcileUnknownSubscription(id, 15, target));
    assert.equal(state.calls.length, 0);
    state.assertReadOnly();
  }
});

test('returned source id must match the fixed request', async () => {
  await rejectPatch(value => { value.source.id = 'replacement'; });
});

test('returned source version must be the exact canonical decimal request', async () => {
  for (const version of ['16', '015', 15, '', null]) {
    await rejectPatch(value => { value.source.updatedAtUnixMs = version; });
  }
});

test('material identity cannot change from the confirmed UI scope', async () => {
  for (const target of ['replacement', null, undefined]) {
    await rejectPatch(value => { value.material.profileId = target; });
  }
});

test('invented success and unknown/unrecognized result states are refused', async () => {
  for (const status of ['confirmed', 'success', 'unknown', null, undefined]) {
    await rejectPatch(value => { value.status = status; });
  }
});

test('a readable runtime never grants resolution; canResolve must be exactly false', async () => {
  for (const resolve of [true, null, undefined, 0]) {
    await rejectPatch(value => { value.canResolve = resolve; });
  }
});

test('fabricated durable candidate or apply receipt cannot override the no-evidence contract', async () => {
  await rejectPatch(value => { value.evidence.candidate = 'recorded'; });
  await rejectPatch(value => { value.evidence.receipt = 'confirmed'; });
  await rejectPatch(value => { delete value.evidence; });
});

test('runtime statuses and reason codes must match the known read-only contract', async () => {
  await rejectPatch(value => { value.runtime.status = 'healthy'; });
  await rejectPatch(value => { value.runtime.reasonCode = 'applied'; });
  await rejectPatch(value => { value.runtime.reasonCode = 'mobile_runtime_bridge_not_adapted'; });
});

test('runtime u64 revisions reject numbers, noncanonical text and overflow; max u64 remains text', async () => {
  for (const version of [9007199254740992, '-1', '01', '1e3', '18446744073709551616', '0'.repeat(21), null]) {
    await rejectPatch(value => { value.runtime.configRevision = version; });
  }
  const value = receipt();
  value.runtime.configRevision = '18446744073709551615';
  const state = harness(value);
  assert.equal((await state.run()).runtime.configRevision, '18446744073709551615');
  state.assertReadOnly();
});

test('last successful source check is a lossless optional timestamp, never a guessed numeric zero', async () => {
  for (const version of [10, '01', '-1', '18446744073709551616', undefined]) {
    await rejectPatch(value => { value.source.lastSyncAtUnixMs = version; });
  }
  const value = receipt();
  value.source.lastSyncAtUnixMs = null;
  const state = harness(value);
  assert.equal((await state.run()).source.lastSyncAtUnixMs, null);
  state.assertReadOnly();
});

test('saved material is schema-only; invalid material and managed ownership remain separate unknown facts', async () => {
  for (const status of ['saved-valid', 'saved-invalid', 'ownership-mismatch']) {
    const value = receipt();
    value.material.status = status;
    value.material.validation = status === 'ownership-mismatch' ? 'not-inspected' : 'native-schema-only';
    const state = harness(value);
    assert.equal((await state.run()).material.status, status);
    state.assertReadOnly();
  }
  await rejectPatch(value => { value.material.validation = 'fully-applied'; });
  await rejectPatch(value => { value.material.active = null; });
  await rejectPatch(value => { value.material.updatedAtUnixMs = 11; });
  await rejectPatch(value => { value.material.status = 'healthy'; });
});

test('absent or unlinked material preserves null facts instead of claiming a saved candidate', async () => {
  for (const [status, target] of [['missing', 'profile'], ['not-linked', null]]) {
    const value = receipt();
    value.material = { status, profileId: target, updatedAtUnixMs: null, active: null, validation: 'not-available' };
    const state = harness(value);
    assert.equal((await state.run('source', 15, target)).material.status, status);
    state.assertReadOnly();
  }
  await rejectPatch(value => { value.material.status = 'missing'; });
  await rejectPatch(value => { value.material.status = 'not-linked'; });
});

test('mobile unsupported, unreadable and changed runtime have no invented instance/revision and remain unknown', async () => {
  for (const [status, reasonCode] of [['unsupported', 'mobile_runtime_bridge_not_adapted'], ['unsupported', 'kernel_identity_not_supported'], ['unavailable', 'runtime_identity_unreadable'], ['changed', 'runtime_identity_changed']]) {
    const value = receipt();
    value.runtime = { status, reasonCode, reason: 'Read cannot confirm original operation', coreInstanceId: null, configRevision: null };
    const state = harness(value);
    const result = await state.run();
    assert.equal(result.status, 'result-unknown');
    assert.equal(result.canResolve, false);
    state.assertReadOnly();
    value.runtime.coreInstanceId = 'invented-instance';
    await assert.rejects(state.run(), /回执未确认/);
    state.assertReadOnly();
  }
});

test('malformed DTO produces a fixed error without exposing reply body or URL tokens and never mutates', async () => {
  for (const broken of [null, {}, { ...receipt(), evidence: null }, { ...receipt(), material: null }, { ...receipt(), runtime: null }]) {
    const payload = broken && { ...broken, rawReply: 'https://example.invalid/?token=canary-private', body: 'password=canary-private' };
    const state = harness(payload);
    await assert.rejects(state.run(), error => {
      assert.match(error.message, /订阅结果核对回执未确认/);
      assert.doesNotMatch(error.message, /canary-private|rawReply|password|https:/);
      return true;
    });
    state.assertReadOnly();
  }
});
