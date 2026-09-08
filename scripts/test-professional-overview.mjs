import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildOverview, capturePresentation, trafficUnavailableReason, formatUptime } from '../src/lib/components/overview/model.ts';
const now = 1000000;
const baseline = () => ({ now, connectionAt: now, connectionError: null, connection: { processState: 'running', coreAvailable: true, systemProxyEnabled: false, processPid: 42 }, tun: null, tunError: null, core: null, selfTest: null, selfTestAt: 0, mode: null, groups: [] });
test('nested selections display the confirmed path and the selected leaf probe', () => {
  const groups = [
    {name:'Proxy',kind:'selector',selected:'Auto',outbounds:[{tag:'Auto'}]},
    {name:'Auto',kind:'urltest',selected:'US',outbounds:[{tag:'US',alive:true,delayMs:441,lastCheckedUnixMs:now},{tag:'Japan',alive:true,delayMs:1,lastCheckedUnixMs:now}]},
  ];
  let model = buildOverview({...baseline(),groups});
  assert.equal(model.groups[0].selectedTag,'Auto');
  assert.equal(model.groups[0].selectionLabel,'Auto → US');
  assert.equal(model.groups[0].delay,'441 ms');
  assert.equal(model.groups[0].options[0].label,'Auto → US · 441 ms');
  groups[1].outbounds[0].alive = false;
  model = buildOverview({...baseline(),groups});
  assert.equal(model.groups[0].failed,true);
  assert.equal(model.groups[0].delay,'—');
  groups[1].outbounds[0].lastCheckedUnixMs = now - 400000;
  model = buildOverview({...baseline(),groups});
  assert.equal(model.groups[0].failed,false);
  assert.equal(model.groups[0].delay,'—');
});
test('missing members, cycles and multi-exit groups never invent a leaf probe', () => {
  for (const child of [
    {name:'Auto',kind:'urltest',selected:'missing',outbounds:[]},
    {name:'Auto',kind:'selector',selected:'Proxy',outbounds:[{tag:'Proxy'}]},
    {name:'Auto',kind:'loadbalance',selected:'US',outbounds:[{tag:'US',alive:true,delayMs:1,lastCheckedUnixMs:now}]},
  ]) {
    const groups=[{name:'Proxy',kind:'selector',selected:'Auto',outbounds:[{tag:'Auto'}]},child];
    const model=buildOverview({...baseline(),groups});
    assert.equal(model.groups[0].delay,'—');
    assert.equal(model.groups[0].failed,false);
  }
});
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


test('uptime advances in seconds, carries units and keeps the process start across observations', () => {
  for (const [ms, expected] of [[-1000,'0 秒'],[999,'0 秒'],[1000,'1 秒'],[59999,'59 秒'],[60000,'1 分 0 秒'],[3599999,'59 分 59 秒'],[3600000,'1 小时 0 分 0 秒'],[90061000,'1 天 1 小时 1 分 1 秒'],[NaN,'—']]) assert.equal(formatUptime(ms),expected);
  const input = baseline(); input.connection.startedAtUnixMs = now - 59999;
  assert.equal(buildOverview(input).uptime, '59 秒');
  assert.equal(buildOverview({...input, now:now+1000}).uptime, '1 分 0 秒');
  assert.equal(buildOverview({...input, connectionAt:now+1000, now:now+1000}).uptime, '1 分 0 秒');
  assert.equal(buildOverview({...input, connection:{processState:'stopped'}}).uptime, '—');
});

test('address preference permits either usable family without duplicate capability warnings', () => {
  for (const policy of ['prefer_ipv4', 'prefer_ipv6', undefined]) {
    for (const [ipv4, ipv6, summary] of [['available','unavailable','IPv4 出口可用'], ['unavailable','available','IPv6 出口可用'], ['available','available','IPv4 / IPv6 出口可用']]) {
      const tun = { enabled:true, healthy:true, supported:true, dualStack:true, addressFamilyPolicy:policy, ipv4Egress:{availability:ipv4}, ipv6Egress:{availability:ipv6} };
      const model = buildOverview({...baseline(), tun});
      assert.equal(model.tone, 'good'); assert.deepEqual(model.findings, []);
      assert.equal(model.egress.summary, summary);
      assert.equal(capturePresentation(model, true, true, true, false).healthy, true);
      if (ipv6 === 'unavailable') assert.equal(model.ipv6, '不可用', 'keep the factual capability in details');
    }
  }
});

test('required or completely missing egress remains actionable in both modes', () => {
  for (const [policy, dualStack, ipv4, ipv6] of [['ipv4_only',true,'unavailable','available'], ['ipv6_only',true,'available','unavailable'], ['ipv6_only',false,'available','available'], ['prefer_ipv4',true,'unavailable','unavailable'], ['prefer_ipv6',false,'unavailable','available']]) {
    const model = buildOverview({...baseline(), tun:{enabled:true,healthy:true,supported:true,dualStack,addressFamilyPolicy:policy,ipv4Egress:{availability:ipv4},ipv6Egress:{availability:ipv6}}});
    assert.equal(model.findings.length, 1); assert.equal(model.findings[0].target, 'tun');
    assert.equal(model.egress.summary, '没有可用出口');
    const lite = capturePresentation(model, true, true, true, false);
    assert.equal(lite.healthy, false); assert.equal(lite.label, 'TUN 运行异常');
  }
});

test('optional, unknown, stopped and stale egress do not invent required-family failures', () => {
  const tun = {enabled:true,healthy:true,supported:true,dualStack:true,addressFamilyPolicy:'ipv4_only',ipv4Egress:{availability:'available'},ipv6Egress:{availability:'unavailable'}};
  assert.equal(buildOverview({...baseline(),tun}).egress.summary,'IPv4 出口可用');
  const unknown = {...tun, ipv4Egress:{availability:'unknown'}};
  assert.equal(buildOverview({...baseline(),tun:unknown}).egress.summary,'出口状态待确认');
  assert.equal(buildOverview({...baseline(),tun:unknown}).findings.length,0);
  const missing = {...tun,ipv4Egress:{availability:'unavailable'}};
  assert.equal(buildOverview({...baseline(),tun:{...missing,enabled:false}}).findings.length,0);
  assert.equal(buildOverview({...baseline(),connectionAt:now-16000,tun:missing}).findings.some(f=>f.target==='tun'),false);
  const unhealthy = buildOverview({...baseline(),tun:{...missing,healthy:false,lastError:'路由恢复失败'}});
  assert.equal(unhealthy.findings.length,1); assert.equal(unhealthy.findings[0].detail,'路由恢复失败');
});


test('stopped TUN cleanup errors stay visible in both overview modes', () => {
  const model = buildOverview({...baseline(), tun:{enabled:false,desiredEnabled:false,supported:true,healthy:false,lastError:'route cleanup failed'}});
  assert.equal(model.tunLabel, '已停止 · 待处理');
  assert.equal(model.findings[0].detail, 'route cleanup failed');
  const lite = capturePresentation(model,false,false,false,false);
  assert.equal(lite.failed,true);
  assert.equal(lite.warning,true);
  assert.equal(lite.label,'TUN 停止后仍有错误');
});
