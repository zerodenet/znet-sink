<script lang="ts">
  import ProfessionalOverview from '$lib/components/overview/ProfessionalOverview.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import { buildOverview, type OverviewInput } from '$lib/components/overview/model';
  import TunControl from '$lib/components/overview/TunControl.svelte';
  import { OverviewOperations } from '$lib/components/overview/operations.svelte';
  import type { OverviewActions } from '$lib/components/overview/types';
  import { Button } from '$lib/components/ui/button';
  import { onDestroy } from 'svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  const scenario = new URLSearchParams(window.location.search).get('mode');
  const now = Date.now();
  let input = $state<OverviewInput>({
    now, connectionAt: scenario === 'stale' ? now - 20000 : now, connectionError: null,
    connection: { state: 'connected', processState: scenario === 'stopped' ? 'stopped' : 'running', coreAvailable: scenario !== 'stopped', processPid: 321, startedAtUnixMs: now - 3800000, systemProxyEnabled: true, localProxyHost: '127.0.0.1', localProxyPort: 7890 },
    core: { coreState: 'running', version: '0.0.16-rc.202609051609' },
    tun: { key: 'tun', supported: true, enabled: true, state: 'running', healthy: scenario !== 'failure', desiredEnabled: true, name: 'tun0', mtu: 1500, addresses: ['10.66.0.1/24'], autoRoute: true, dualStack: true, strictRoute: false, dnsHijack: false, fakeIpEnabled: false, dnsHijackedQueries: 0, ipv4Egress: { availability: 'available', interface: 'eth0' }, ipv6Egress: { availability: 'available', interface: 'eth0' }, networkGeneration: 4, ipv6ToIpv4Fallbacks: 0, managedByConfig: false, lastError: scenario === 'failure' ? '默认路由已变化，出口恢复失败' : undefined },
    tunError: null, mode: { currentMode: 'rule', availableModes: ['rule', 'global', 'direct'] },
    selfTestAt: now, selfTest: { ready: true, blockingIssues: [], warningCount: 0, activeProxyConfigId: 'main', activeProxyConfigName: '日常网络配置', suggestedFlow: 'ready', checks: [{ key: 'control', status: 'pass', message: '控制接口已响应' }] },
    groups: [{ name: '自动选择', kind: 'urltest', selected: '新加坡 01', outbounds: [{ tag: '新加坡 01', type: 'vless', alive: true, delayMs: 38, lastCheckedUnixMs: now }] }, { name: '工作网络', kind: 'selector', selected: '日本 02', outbounds: [{ tag: '日本 02', type: 'trojan', alive: scenario !== 'failure', delayMs: 72, lastCheckedUnixMs: now }, { tag: '备用节点', type: 'trojan', alive: true, delayMs: 52, lastCheckedUnixMs: now }] }],
  });
  if (scenario === 'empty') { input.groups = []; input.selfTest = null; }
  const model = $derived(buildOverview(input));
  const history = Array.from({ length: 120 }, (_, i) => ({ down: .5 + Math.sin(i / 8) * .3, up: .08 + Math.cos(i / 10) * .05 }));
  overviewData.applyTrafficRateSample({ sampledAtUnixMs: now, stable: true, uploadBytesPerSec: 80000, downloadBytesPerSec: 500000, totalUploadBytes: 12000000, totalDownloadBytes: 320000000, connectionCount: 26 });
  let action = $state('');
  let selected = $state('main');
  let view = $state<{ showTunDetails: () => void }>();
  const operations = new OverviewOperations();
  const busy = $derived(!!operations.feedback.pending);
  const profiles = $derived({ selected: scenario === 'empty' ? '' : selected, loading: false, error: null, options: scenario === 'empty' ? [] : [{value:'main',label:'日常网络配置'},{value:'work',label:'工作配置'}] });
  const network = { ip:'192.0.2.18', description:'测试网络', loading:false, error:null };
  async function change(effect: () => void) {
    await new Promise(resolve => setTimeout(resolve, 500));
    if (scenario === 'action-failure') throw new Error('内核拒绝切换，原设置保持不变');
    effect(); return '设置已生效';
  }
  const actions: OverviewActions = {
    navigate: target => { action = target; },
    refresh: () => { void operations.run('checks', () => change(() => { input.connectionAt = Date.now(); action = 'refresh'; })); },
    chooseProfile: id => { if (id !== selected) void operations.run('profile', () => change(() => { selected = id; })); },
    choosePolicy: (name, target) => { void operations.run(`policy:${name}`, () => change(() => { input.groups.find(group => group.name === name)!.selected = target; })); },
    setMode: mode => { if (input.mode!.currentMode !== mode) void operations.run('mode', () => change(() => { input.mode!.currentMode = mode; })); },
    toggleTun: () => { void operations.run('tun', () => change(() => { input.tun!.enabled = !input.tun!.enabled; input.tun!.desiredEnabled = input.tun!.enabled; })); },
  };
  onDestroy(() => operations.destroy());
</script>
<ProfessionalOverview bind:this={view} {model} {profiles} {network} feedback={operations.feedback} {actions} {busy} refreshing={operations.feedback.pending === 'checks'} canDisableTun={input.tun?.enabled}>
  {#snippet core()}<section class="fixture-core"><span>内核状态</span><strong>{model.stale ? '状态待确认' : model.ready ? '服务中' : '已停止'}</strong><p>PID {model.pid} · {model.endpoint}</p><Button variant="outline" size="sm" disabled={busy || model.stale} onclick={() => action = 'restart'}>重启内核</Button><Button variant="outline" size="sm" disabled={busy} onclick={() => action = 'system-proxy'}>关闭系统代理</Button></section>{/snippet}
  {#snippet tun()}<TunControl {model} onInspect={() => view?.showTunDetails()} onToggle={actions.toggleTun} switchOn={input.tun?.enabled ?? false} canToggle={!busy} switching={operations.feedback.pending === 'tun'} stackLabel={model.ready ? 'Zero Stack' : '待确认'} stackReady={model.ready} feedback={operations.feedback} />{/snippet}
  {#snippet traffic()}<TrafficChart {history} unavailableReason={scenario === 'stale' ? '流量采样已过期，等待恢复' : scenario === 'stopped' ? '内核未就绪，暂停展示实时速率' : null} />{/snippet}
</ProfessionalOverview>
<output aria-label="概览操作">{action}</output>
<style>.fixture-core { display:flex; flex-direction:column; gap:5px; padding:11px 13px; background:var(--card); border:1px solid var(--border); border-radius:10px; font-size:11px; flex:1; }.fixture-core p { margin:0; }</style>
