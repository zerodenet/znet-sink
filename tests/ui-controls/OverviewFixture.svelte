<script lang="ts">
  import ProfessionalOverview from '$lib/components/overview/ProfessionalOverview.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import { buildOverview, type OverviewInput } from '$lib/components/overview/model';
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
    groups: [{ name: '自动选择', kind: 'urltest', selected: '新加坡 01', outbounds: [{ tag: '新加坡 01', type: 'vless', alive: true, delayMs: 38, lastCheckedUnixMs: now }] }, { name: '工作网络', kind: 'selector', selected: '日本 02', outbounds: [{ tag: '日本 02', type: 'trojan', alive: scenario !== 'failure', delayMs: 72, lastCheckedUnixMs: now }] }],
  });
  const model = $derived(buildOverview(input));
  const history = Array.from({ length: 120 }, (_, i) => ({ down: .5 + Math.sin(i / 8) * .3, up: .08 + Math.cos(i / 10) * .05 }));
  overviewData.applyTrafficRateSample({ sampledAtUnixMs: now, stable: true, uploadBytesPerSec: 80000, downloadBytesPerSec: 500000, totalUploadBytes: 12000000, totalDownloadBytes: 320000000, connectionCount: 26 });
  let action = $state('');
</script>
<ProfessionalOverview {model} navigate={(target) => action = target} refresh={() => { input.connectionAt = now; action = 'refresh'; }} start={() => action = 'start'} restart={() => action = 'restart'} setMode={(mode) => { input.mode!.currentMode = mode; action = mode; }}>
  {#snippet traffic()}<TrafficChart {history} unavailableReason={scenario === 'stale' ? '流量采样已过期，等待恢复' : scenario === 'stopped' ? '内核未就绪，暂停展示实时速率' : null} />{/snippet}
</ProfessionalOverview>
<output aria-label="概览操作">{action}</output>
