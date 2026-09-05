import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildOverview } from '../src/lib/components/overview/model.ts';
const now = 1000000;
const baseline = () => ({ now, connectionAt: now, connectionError: null, connection: { processState: 'running', coreAvailable: true, systemProxyEnabled: false, processPid: 42 }, tun: null, tunError: null, core: null, selfTest: null, selfTestAt: 0, mode: null, groups: [] });
test('process existence alone never reports control-plane readiness', () => {
  const input = baseline(); input.connection.coreAvailable = false;
  const model = buildOverview(input);
  assert.equal(model.ready, false);
  assert.equal(model.tone, 'error');
  assert.match(model.findings[0].title, /控制接口未就绪/);
});
test('old or failed snapshots never label retained proxy state as confirmed', () => {
  for (const patch of [{ connectionAt: now - 16000 }, { connectionError: 'IPC disconnected' }]) {
    const input = { ...baseline(), ...patch };
    input.connection.systemProxyEnabled = true;
    const model = buildOverview(input);
    assert.equal(model.ready, false); assert.equal(model.proxy, '状态待确认');
  }
});
test('unhealthy tun and desired versus observed mismatch are actionable', () => {
  for (const tun of [{ enabled: true, healthy: false }, { desiredEnabled: true, enabled: false }]) {
    const model = buildOverview({ ...baseline(), tun });
    assert.equal(model.tone, 'error'); assert.equal(model.findings[0].target, 'tun');
  }
});
test('dns interception disabled does not imply broken dns or fake-ip', () => {
  const model = buildOverview({ ...baseline(), tun: { enabled: true, healthy: true, supported: true, dnsHijack: false } });
  assert.equal(model.tone, 'good'); assert.equal(model.dns, '不拦截 · 跟随系统 DNS');
});
test('old probes are not current failures and stopped core exposes no active choice', () => {
  const groups = [{ name: 'auto', selected: 'node-a', outbounds: [{ tag: 'node-a', alive: false, lastCheckedUnixMs: now - 400000 }] }];
  const model = buildOverview({ ...baseline(), groups });
  assert.equal(model.groups[0].failed, false); assert.equal(model.findings.length, 0);
  const stopped = buildOverview({ ...baseline(), groups, connection: { processState: 'stopped', coreAvailable: false } });
  assert.equal(stopped.groups[0].selected, '待内核确认');
});
test('recent selected-node failures are prioritised without inventing a global active node', () => {
  const groups = ['one', 'two'].map((name, i) => ({ name, selected: `node-${i}`, outbounds: [{ tag: `node-${i}`, alive: i === 0, lastCheckedUnixMs: now, delayMs: 10 }] }));
  const model = buildOverview({ ...baseline(), groups });
  assert.equal(model.groups[0].name, 'two');
  assert.equal(model.findings[0].target, 'nodes');
  assert.equal(model.groups.length, 2);
});
test('stale self-test failures retain their timestamp without overriding current health', () => {
  const selfTest = { checks: [], blockingIssues: ['old failure'] };
  const model = buildOverview({ ...baseline(), selfTest, selfTestAt: now - 61000 });
  assert.equal(model.selfTestStale, true); assert.equal(model.tone, 'good');
});
