import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildOverview, capturePresentation, trafficUnavailableReason } from '../src/lib/components/overview/model.ts';
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

test('missing or expired self test never claims it passed', () => {
  assert.equal(buildOverview(baseline()).selfTestPassed, false);
  const selfTest = { ready: true, checks: [{status:'pass'}], blockingIssues: [] };
  assert.equal(buildOverview({ ...baseline(), selfTest, selfTestAt: now }).selfTestPassed, true);
  assert.equal(buildOverview({ ...baseline(), selfTest, selfTestAt: now - 61000 }).selfTestPassed, false);
});
test('selector members alone are manually switchable and stale latency is not repeated in options', () => {
  const groups = ['selector','urltest'].map((kind) => ({name:kind,kind,selected:'n',outbounds:[{tag:'n',alive:true,delayMs:42,lastCheckedUnixMs:now-400000}]}));
  const model = buildOverview({...baseline(),groups});
  assert.equal(model.groups[0].switchable,true); assert.equal(model.groups[1].switchable,false);
  assert.equal(model.groups[0].options[0].label,'n · 待探测');
});
test('failed policy reads invalidate selection even while connection reads remain fresh', () => {
  const groups = [{name:'p',kind:'selector',selected:'n',outbounds:[{tag:'n',alive:true,delayMs:42,lastCheckedUnixMs:now}]}];
  for (const observation of [{groupsAt:now,groupsError:'IPC timeout'},{groupsAt:now-16000}]) {
    const model=buildOverview({...baseline(),groups,...observation});
    assert.equal(model.ready,true); assert.equal(model.groupsReady,false);
    assert.equal(model.groups[0].selectedTag,''); assert.equal(model.groups[0].delay,'—');
    assert.ok(model.findings.some(finding=>finding.target==='nodes'));
  }
});

test('Lite shares the professional freshness, partial capture and cleanup semantics', () => {
  for (const proxy of [false, true]) for (const enabled of [false, true]) for (const desired of [false, true]) {
    const input = { ...baseline(), connection: { ...baseline().connection, systemProxyEnabled: proxy }, tun: { enabled, desiredEnabled: desired, healthy: true, supported: true } };
    const model = buildOverview(input);
    const lite = capturePresentation(model, proxy, enabled, desired, false);
    assert.equal(lite.powerOn, proxy || enabled || desired);
    assert.equal(lite.healthy, proxy && enabled);
    const stale = buildOverview({ ...input, connectionAt: now - 16000 });
    assert.equal(capturePresentation(stale, proxy, enabled, desired, false).healthy, false);
    assert.equal(capturePresentation(stale, proxy, enabled, desired, false).label, '运行状态待确认');
    assert.equal(capturePresentation(stale, proxy, enabled, desired, false).powerOn, lite.powerOn);
  }
});

test('TUN failure never becomes a healthy Lite power indication', () => {
  const model = buildOverview({ ...baseline(), tun: { enabled: true, healthy: false, lastError: 'route failed' } });
  const lite = capturePresentation(model, true, true, true, false);
  assert.equal(lite.healthy, false); assert.equal(lite.powerOn, true);
  assert.equal(lite.label, 'TUN 运行异常'); assert.equal(lite.warning, true);
});

test('both overview modes share one traffic availability boundary', () => {
  const model = buildOverview(baseline());
  assert.equal(trafficUnavailableReason(model, true, now, true, now), null);
  assert.equal(trafficUnavailableReason(model, true, now - 11000, true, now), '流量采样已过期，等待恢复');
  assert.equal(trafficUnavailableReason(model, true, 0, true, now), '等待第一份流量采样');
  assert.equal(trafficUnavailableReason(model, true, now, false, now), '正在建立流量采样基线');
  assert.equal(trafficUnavailableReason(model, false, now, true, now), '内核不支持流量查询');
  assert.equal(trafficUnavailableReason(buildOverview({ ...baseline(), connectionAt: 0 }), true, now, true, now), '内核未就绪，暂停展示实时速率');
});
