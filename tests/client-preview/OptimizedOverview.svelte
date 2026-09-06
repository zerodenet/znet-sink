<script lang="ts">
  import { onMount } from 'svelte';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import TunControl from './TunControl.svelte';
  import OperationFeedback from './OperationFeedback.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import { Button } from '$lib/components/ui/button';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import * as Dialog from '$lib/components/ui/dialog';
  import { ChevronRight, CircleCheck, RefreshCw, TriangleAlert } from '@lucide/svelte';
  import { buildOverview } from '$lib/components/overview/model';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState, preview } from './state.svelte';

  let now = $state(Date.now());
  let inspect = $state(false);
  let policies = $state(false);
  let version = $state(false);
  let detailTab = $state('capture');
  let testing = $state(false);
  let testAt = $state(0);
  const model = $derived(buildOverview({ now, connection: guiState.connection, connectionAt: guiState.connectionUpdatedAt, connectionError: guiState.connectionError, core: guiState.coreOverview, tun: guiState.tunStatus, tunError: guiState.tunStatusError, selfTest: guiState.selfTest, selfTestAt: guiState.selfTestUpdatedAt, mode: guiState.proxyMode, groups: guiState.policyGroups }));
  const attentionCount = $derived(new Set(model.findings.map(finding => finding.target)).size);
  const failures = $derived(model.groups.filter(group => group.failed));
  const visiblePolicy = $derived(model.groups[0]);
  const tunFailed = $derived(guiState.tunStatus.enabled && !guiState.tunStatus.healthy);
  const configOptions = [{ value: 'demo', label: '日常网络' }, { value: 'work', label: '工作配置' }];
  const selectedConfig = $derived(guiState.selfTest.activeProxyConfigId);
  const trafficUnavailable = $derived(!model.ready ? '内核状态未确认，暂停展示实时速率' : null);
  onMount(() => { const timer = window.setInterval(() => now = Date.now(), 1000); return () => window.clearInterval(timer); });
  async function runChecks() {
    if (testing) return;
    testing = true;
    try { await Promise.all([guiState.refreshAll(), guiState.probeNetwork()]); testAt = Date.now(); }
    finally { testing = false; }
  }
  function openChecks(tab = 'capture') { detailTab = tab; inspect = true; }
</script>

<div class="optimized-overview" aria-label="优化版专业概览">
  <div class="overview-toolbar">
    <div class="profile-area"><div class="profile-control"><span class="label">当前配置</span><FieldSelect bind:value={() => selectedConfig, (value) => void guiState.chooseProfile(value)} options={configOptions} aria-label="当前配置" disabled={!!preview.pending} /></div><OperationFeedback target="profile" pendingLabel="正在应用配置，等待内核确认…" /></div>
    <div class="toolbar-end"><span class="uptime" title="内核运行时长">运行 {model.uptime}</span><Button variant="ghost" size="sm" onclick={() => version = true} title={model.version}>内核版本<ChevronRight size={12}/></Button></div>
  </div>

  <div class="control-grid">
    <div class="core-controls"><CoreStatusCard /><OperationFeedback target="system-proxy" /></div>
    <section class="mode-card" aria-label="代理模式与策略">
      <header><span class="label">代理模式</span></header>
      <SegmentedControl.Root bind:value={() => model.mode, (value) => { if (['global', 'rule', 'direct'].includes(value)) void guiState.setProxyMode(value); }} disabled={!model.ready || !!preview.pending} aria-label="选择代理模式" class="mode-segment">
        {#each [{value:'global',label:'全局'},{value:'rule',label:'规则'},{value:'direct',label:'直连'}] as mode}<SegmentedControl.Item value={mode.value} style="flex:1;">{mode.label}</SegmentedControl.Item>{/each}
      </SegmentedControl.Root>
      <p class="mode-explanation">{model.mode === 'rule' ? '按规则选择直连或代理' : model.mode === 'global' ? '使用全局代理出口' : '直接连接目标服务器'}</p>
      <OperationFeedback target="mode" />
      <button data-slot="surface-button" class="policy-shortcut" onclick={() => policies = true} aria-label="查看与切换策略组">
        <span class="policy-heading"><span>{visiblePolicy?.name ?? '策略组'}{model.groups.length > 1 ? ` · 共 ${model.groups.length} 组` : ''}</span><span>切换<ChevronRight size={12}/></span></span>
        <span class="policy-selection"><strong title={visiblePolicy?.selected}>{visiblePolicy?.selected ?? '尚未配置'}</strong><span class:danger={visiblePolicy?.failed} title={visiblePolicy?.health}>{preview.pending.startsWith('policy:') ? '切换中…' : model.mode === 'direct' ? '直连模式' : visiblePolicy?.failed ? '探测失败' : visiblePolicy?.delay !== '—' ? visiblePolicy?.delay : '待探测'}</span></span>
      </button>
    </section>
    <TunControl {model} onInspect={() => openChecks('capture')} />
  </div>

  {#if tunFailed || failures.length || model.stale}
    <section class="issue-bar" aria-label="需要处理">
      <TriangleAlert size={15}/><div><strong>{model.stale ? '运行状态更新中断' : tunFailed ? 'TUN 出口恢复失败' : '当前策略出口探测失败'}</strong><span>{model.stale ? '上次状态不能代表当前网络，请重新检查。' : tunFailed ? '已开启接管，但 IPv4 出口不可用。' : failures.map(group => `${group.name} → ${group.selected}`).join('；')}</span></div>
      <Button variant="outline" size="sm" onclick={() => tunFailed || model.stale ? openChecks('capture') : policies = true}>{tunFailed || model.stale ? '查看原因' : '切换策略'}</Button>
      {#if tunFailed}<Button variant="ghost" size="sm" disabled={guiState.isSwitchingTun} onclick={() => void guiState.toggleTun()}>关闭 TUN</Button>{/if}
    </section>
  {/if}

  <div class="network-line" aria-label="网络检查摘要">
    <div class="network-summary"><span class="label">本地网络</span><strong class="mono">{guiState.networkProbe.ip}</strong><span class="separator"></span><button data-slot="surface-button" class="dns-link" onclick={() => openChecks('dns')}>{!model.ready ? 'DNS 待确认' : !guiState.isTunEnabled ? 'TUN DNS 未接管' : guiState.tunStatus.dnsHijack ? guiState.tunStatus.fakeIpEnabled ? 'Fake-IP' : 'Real DNS' : '系统 DNS'}<ChevronRight size={11}/></button></div>
    <div class="check-actions"><button data-slot="surface-button" class="checks-link" class:warning={model.findings.length > 0} onclick={() => openChecks()}>{#if model.findings.length}<TriangleAlert size={12}/>{attentionCount} 项需关注{:else}<CircleCheck size={12}/>自测通过{/if}</button><Button variant="ghost" size="sm" onclick={() => void runChecks()} disabled={testing}><RefreshCw size={12} class={testing ? 'animate-spin' : ''}/>{testing ? '检测中' : '检查网络'}</Button></div>
  </div>

  <section class="traffic-panel" aria-label="实时流量"><TrafficChart history={overviewData.speedHistory} unavailableReason={trafficUnavailable}/></section>

</div>

<Dialog.Root bind:open={policies}>
  <Dialog.Content class="max-w-[520px]">
    <Dialog.Header><Dialog.Title>当前策略选择</Dialog.Title><Dialog.Description>按策略组切换出口，规则继续引用原策略组。</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      {#each guiState.policyGroups as group}
        <div class="policy-edit"><div class="policy-name"><strong>{group.name}</strong><span>{group.kind === 'selector' ? '手动选择' : '自动测速'}</span></div><FieldSelect aria-label={`${group.name} 当前出口`} bind:value={() => group.selected, (value) => void guiState.choosePolicy(group.name, value)} disabled={!!preview.pending || !model.ready} options={group.outbounds.map(outbound => ({value:outbound.tag,label:`${outbound.tag} · ${outbound.lastCheckedUnixMs && now - outbound.lastCheckedUnixMs <= 300000 ? outbound.alive ? `${outbound.delayMs} ms` : '探测失败' : '待探测'}`}))} /><p>已生效：{group.selected}</p><OperationFeedback target={`policy:${group.name}`} /></div>
      {/each}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" onclick={() => policies = false}>完成</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={inspect}>
  <Dialog.Content class="max-w-[560px]">
    <Dialog.Header><Dialog.Title>网络检查</Dialog.Title><Dialog.Description>先确认接管，再检查解析与出口。以下为预览模拟数据。</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      <SegmentedControl.Root value={detailTab} aria-label="检查项目" onValueChange={(value) => detailTab = value}>
        {#each [{value:'capture',label:'流量接管'},{value:'dns',label:'域名解析'},{value:'egress',label:'网络出口'}] as tab}<SegmentedControl.Item value={tab.value}>{tab.label}</SegmentedControl.Item>{/each}
      </SegmentedControl.Root>
      <dl class="diagnostic-facts">
        {#if detailTab === 'capture'}
          <div><dt>系统代理</dt><dd>{model.proxy} · {model.endpoint}</dd></div><div><dt>TUN 状态</dt><dd class:danger={tunFailed}>{model.tunLabel}</dd></div><div><dt>接管参数</dt><dd>{model.tunDetails}</dd></div><div><dt>地址 / 配置来源</dt><dd>{guiState.isTunEnabled ? guiState.tunStatus.addresses.join(' · ') : '—'} · ZNet-Sink 缺省</dd></div><div><dt>实际出口</dt><dd>IPv4：{model.ipv4} · IPv6：{model.ipv6}</dd></div><div><dt>网络代次 / 回退</dt><dd>{model.networkGeneration} / IPv6 → IPv4 {guiState.tunStatus.ipv6ToIpv4Fallbacks} 次</dd></div>
          {#if tunFailed}<div><dt>失败原因</dt><dd class="danger">IPv4 默认路由不可用，TUN 出口恢复失败。</dd></div>{/if}
        {:else if detailTab === 'dns'}
          <div><dt>当前解析路径</dt><dd>{model.dns}</dd></div><div><dt>DNS 拦截次数</dt><dd>{guiState.isTunEnabled ? guiState.tunStatus.dnsHijackedQueries : '—'}</dd></div><div><dt>判断范围</dt><dd>这表示解析配置与接管状态；具体域名的解析结果需单独查询。</dd></div>
        {:else}
          <div><dt>IPv4 出口</dt><dd class:danger={tunFailed}>{model.ipv4}</dd></div><div><dt>IPv6 出口</dt><dd>{model.ipv6}</dd></div><div><dt>本地网络检测</dt><dd>{guiState.networkProbe.ip} · 演示数据</dd></div><div><dt>策略组探测</dt><dd>{failures.length ? `${failures.length} 个当前出口探测失败` : '当前选择有近期成功探测'}</dd></div>
        {/if}
      </dl>
      {#if testAt}<p class="check-feedback" role="status">已重新读取接管与本地网络状态{tunFailed ? '，TUN 出口问题仍存在。' : '。'}</p>{/if}
      <OperationFeedback target="tun" />
    </Dialog.Body>
    <Dialog.Footer>{#if tunFailed}<Button variant="outline" disabled={guiState.isSwitchingTun} onclick={() => void guiState.toggleTun()}>关闭 TUN</Button>{/if}<Button variant="outline" disabled={testing} onclick={() => void runChecks()}>{testing ? '检查中…' : '重新检查'}</Button><Button onclick={() => inspect = false}>完成</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={version}>
  <Dialog.Content class="max-w-[460px]"><Dialog.Header><Dialog.Title>当前内核</Dialog.Title><Dialog.Description>版本摘要保留在概览，更新与安装由设置中的版本管理负责。</Dialog.Description></Dialog.Header><Dialog.Body><dl class="diagnostic-facts"><div><dt>运行版本</dt><dd>{model.version}</dd></div><div><dt>运行时长</dt><dd>{model.uptime}</dd></div></dl></Dialog.Body><Dialog.Footer><Button variant="outline" onclick={() => version = false}>关闭</Button></Dialog.Footer></Dialog.Content>
</Dialog.Root>

<style>
  .optimized-overview { width:100%; flex:1; min-height:0; display:flex; flex-direction:column; gap:12px; overflow:auto; padding-right:2px; }
  .overview-toolbar { display:flex; align-items:center; justify-content:space-between; gap:12px; flex-shrink:0; }
  .profile-area { min-width:0; flex:1; }.profile-control, .toolbar-end { display:flex; align-items:center; gap:8px; min-width:0; }.profile-control > :global(*) { max-width:210px; }.profile-control .label { white-space:nowrap; }.toolbar-end { flex-shrink:0; }.uptime { font-size:11px; color:var(--muted-foreground); }
  .label { font-size:12px; font-weight:500; color:var(--muted-foreground); }
  .control-grid { display:grid; grid-template-columns:1.15fr .88fr 1.1fr; gap:12px; flex-shrink:0; align-items:stretch; }
  .mode-card { display:flex; flex-direction:column; gap:9px; min-height:96px; min-width:0; padding:11px 13px; background:var(--card); border:1px solid var(--border); border-radius:10px; box-shadow:0 1px 2px rgba(0,0,0,.04); transition:box-shadow .15s,transform .15s; }.mode-card:hover { box-shadow:0 2px 6px rgba(0,0,0,.07); transform:translateY(-.5px); }.mode-card header { display:flex; align-items:center; justify-content:space-between; }:global(.mode-segment) { width:100%; }.mode-explanation { font-size:11px; color:var(--muted-foreground); margin:0; }
  .core-controls { min-width:0; display:flex; flex-direction:column; }.core-controls > :global(.core-card) { width:100%; flex:1; }
  .core-controls :global(.core-meta-row:nth-child(1)) { grid-column:1/-1; grid-row:1; }
  .core-controls :global(.core-meta-row:nth-child(2)) { grid-column:1/-1; grid-row:2; }
  .policy-shortcut { display:flex; flex-direction:column; text-align:left; gap:4px; min-width:0; font-size:11px; border-top:1px solid var(--border); margin-top:auto; padding-top:8px; cursor:pointer; }.policy-heading, .policy-selection { display:flex; align-items:center; justify-content:space-between; gap:8px; width:100%; min-width:0; }.policy-heading { color:var(--muted-foreground); font-size:10px; }.policy-heading > span:last-child { display:flex; align-items:center; gap:2px; }.policy-selection strong { font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }.policy-selection > span { color:var(--muted-foreground); flex-shrink:0; font-variant-numeric:tabular-nums; }.policy-shortcut .danger, .danger { color:var(--destructive); }
  .network-line { display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:6px; padding:6px 12px; background:var(--card); border:1px solid var(--border); border-radius:8px; font-size:11px; flex-shrink:0; }.network-summary, .check-actions { display:flex; align-items:center; gap:10px; }.network-summary strong { font-size:12px; font-weight:600; }.mono { font-family:var(--font-mono); }.separator { height:13px; width:1px; background:var(--border); }.dns-link, .checks-link { display:flex; align-items:center; gap:4px; cursor:pointer; }.dns-link { color:var(--muted-foreground); }.checks-link { color:var(--success); }.checks-link.warning { color:var(--warning); }
  .traffic-panel { min-height:220px; height:250px; flex:1 0 220px; }
  .issue-bar { display:flex; align-items:center; gap:8px; padding:8px 12px; border-radius:8px; border:1px solid color-mix(in srgb,var(--warning) 30%,var(--border)); background:color-mix(in srgb,var(--warning) 6%,var(--card)); font-size:11px; flex-shrink:0; }.issue-bar > :global(svg) { color:var(--warning); flex-shrink:0; }.issue-bar > div { flex:1; min-width:0; }.issue-bar strong { display:block; font-weight:600; }.issue-bar span { display:block; color:var(--muted-foreground); margin-top:3px; }
  .policy-edit + .policy-edit { margin-top:18px; }.policy-name { display:flex; justify-content:space-between; align-items:center; margin-bottom:8px; font-size:12px; }.policy-name span, .policy-edit p { color:var(--muted-foreground); font-size:11px; }.policy-edit p { margin:6px 0 0; }.diagnostic-facts { margin:12px 0 0; }.diagnostic-facts > div { display:grid; grid-template-columns:100px minmax(0,1fr); gap:10px; padding:10px 0; border-bottom:1px solid var(--border); }dt { font-size:12px; color:var(--muted-foreground); }dd { font-size:12px; margin:0; overflow-wrap:anywhere; }.check-feedback { font-size:12px; color:var(--muted-foreground); }
  button:focus-visible { outline:2px solid var(--ring); outline-offset:3px; }
  @media(max-width:760px) { .control-grid { grid-template-columns:1fr 1fr; }.control-grid > :global(.feature-card) { grid-column:1/-1; }.profile-control > :global(*) { max-width:150px; }.uptime { display:none; }.network-summary { flex-wrap:wrap; gap:7px; }.issue-bar { flex-wrap:wrap; } }
  @media(max-width:480px) { .control-grid { grid-template-columns:1fr; }.profile-control > :global(*) { max-width:120px; }.profile-control .label { display:none; } }
</style>
