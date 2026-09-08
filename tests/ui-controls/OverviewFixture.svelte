<script lang="ts">
  import ProfessionalOverview from '$lib/components/overview/ProfessionalOverview.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import { buildOverview, type OverviewInput } from '$lib/components/overview/model';
  import TunControl from '$lib/components/overview/TunControl.svelte';
  import { OverviewOperations } from '$lib/components/overview/operations.svelte';
  import type { OverviewActions } from '$lib/components/overview/types';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { onDestroy } from 'svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  const scenario = new URLSearchParams(window.location.search).get('mode');
  const now = Date.now();
  let input = $state<OverviewInput>({
    now, connectionAt: scenario === 'stale' ? now - 20000 : now, connectionError: null,
    connection: { state: 'connected', processState: scenario === 'stopped' || scenario === 'external' ? 'stopped' : 'running', coreAvailable: scenario !== 'stopped', processPid: 321, startedAtUnixMs: now - 3800000, systemProxyEnabled: true, localProxyHost: '127.0.0.1', localProxyPort: 7890 },
    core: { coreState: 'running', version: '0.0.16-rc.202609051609' },
    tun: { key: 'tun', supported: true, enabled: true, state: 'running', healthy: scenario !== 'failure', desiredEnabled: true, name: 'tun0', mtu: 1500, addresses: ['10.66.0.1/24'], autoRoute: true, dualStack: true, strictRoute: false, dnsHijack: false, fakeIpEnabled: false, dnsHijackedQueries: 0, ipv4Egress: { availability: 'available', interface: 'eth0' }, ipv6Egress: { availability: 'available', interface: 'eth0' }, addressFamilyPolicy: 'prefer_ipv4', networkGeneration: 4, ipv6ToIpv4Fallbacks: 0, managedByConfig: false, lastError: scenario === 'failure' ? '默认路由已变化，出口恢复失败' : undefined },
    tunError: null, mode: { currentMode: 'rule', availableModes: ['rule', 'global', 'direct'] },
    selfTestAt: now, selfTest: { ready: true, blockingIssues: [], warningCount: 0, activeProxyConfigId: 'main', activeProxyConfigName: '日常网络配置', suggestedFlow: 'ready', checks: [{ key: 'control', status: 'pass', message: '控制接口已响应' }] },
    groups: [{ name: '自动选择', kind: 'urltest', selected: '新加坡 01', outbounds: [{ tag: '新加坡 01', type: 'vless', alive: true, delayMs: 38, lastCheckedUnixMs: now }] }, { name: '工作网络', kind: 'selector', selected: '日本 02', outbounds: [{ tag: '日本 02', type: 'trojan', alive: scenario !== 'failure', delayMs: 72, lastCheckedUnixMs: now }, { tag: '备用节点', type: 'trojan', alive: true, delayMs: 52, lastCheckedUnixMs: now }] }],
  });
  if (scenario === 'nested-policy') input.groups.unshift({ name: '节点选择', kind: 'selector', selected: '自动选择', outbounds: [{tag:'自动选择',type:'urltest'}] });
  if (scenario === 'empty') { input.groups = []; input.selfTest = null; }
  if (scenario === 'ipv4-only-network' || scenario === 'ipv6-required') { input.tun!.ipv6Egress = { availability:'unavailable' }; input.tun!.addressFamilyPolicy = scenario === 'ipv6-required' ? 'ipv6_only' : 'prefer_ipv4'; }
  const model = $derived(buildOverview(input));
  const history = Array.from({ length: 120 }, (_, i) => ({ down: .5 + Math.sin(i / 8) * .3, up: .08 + Math.cos(i / 10) * .05 }));
  overviewData.applyTrafficRateSample({ sampledAtUnixMs: now, stable: true, uploadBytesPerSec: 80000, downloadBytesPerSec: 500000, totalUploadBytes: 12000000, totalDownloadBytes: 320000000, connectionCount: 26 });
  let action = $state('');
  let selected = $state('main');
  let view = $state<{ showTunDetails: () => void }>();
  const operations = new OverviewOperations();
  const busy = $derived(!!operations.feedback.pending || scenario === 'busy');
  // Mount the production card. Only its state/command boundary is simulated.
  $effect(() => {
    Object.assign(guiState, { connection:input.connection, selfTest:input.selfTest,
      isInitializing:false,isStartingCore:false,isStoppingCore:false,isConnecting:false,isDisconnecting:false,isSwitchingSystemProxy:operations.feedback.pending === 'system-proxy',
      canRestartCore:true,canStartCore:true,canDisableSystemProxy:true,canEnableSystemProxy:true,
      restartCore:async()=>{action='restart';},startCore:async()=>{action='start';},
    });
    store.uiMode='pro';
  });
  const profiles = $derived({ selected: scenario === 'empty' ? '' : selected, loading: false, error: null, options: scenario === 'empty' ? [] : [{value:'main',label:'日常网络配置'},{value:'work',label:'工作配置'}] });
  const network = { ip:'192.0.2.18', description:'美国 · California · Los Angeles', countryCode:'us', location:'美国 · California · Los Angeles', loading:false, error:null };
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
    recoverTun: () => { void operations.run('tun', () => change(() => { input.tun!.healthy = true; input.tun!.lastError = undefined; action = 'tun-recover'; })); },
    toggleTun: () => { void operations.run('tun', () => change(() => { input.tun!.enabled = !input.tun!.enabled; input.tun!.desiredEnabled = input.tun!.enabled; })); },
  };
  onDestroy(() => operations.destroy());
</script>
<ProfessionalOverview bind:this={view} {model} {profiles} {network} feedback={operations.feedback} {actions} {busy} refreshing={operations.feedback.pending === 'checks'} canDisableTun={input.tun?.enabled}>
  {#snippet core()}<CoreStatusCard {busy} stateUnknown={model.stale} onToggleSystemProxy={()=>{void operations.run('system-proxy', () => change(() => { input.connection!.systemProxyEnabled = !input.connection!.systemProxyEnabled; action='system-proxy'; }));}} />{/snippet}
  {#snippet tun()}<TunControl {model} onInspect={() => view?.showTunDetails()} onToggle={actions.toggleTun} onRecover={actions.recoverTun} switchOn={input.tun?.enabled ?? false} canToggle={!busy} switching={operations.feedback.pending === 'tun'} stackLabel={model.ready ? 'Zero Stack' : '待确认'} stackReady={model.ready} />{/snippet}
  {#snippet traffic()}<TrafficChart {history} unavailableReason={scenario === 'stale' ? '流量采样已过期，等待恢复' : scenario === 'stopped' ? '内核未就绪，暂停展示实时速率' : null} />{/snippet}
</ProfessionalOverview>
<output aria-label="概览操作" style="height:24px;flex-shrink:0">{action}</output>
