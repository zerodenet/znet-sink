<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronRight, CircleCheck, Globe, RefreshCw, TriangleAlert } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import type { OverviewModel } from './model';
  import type { OverviewActions, OverviewFeedback, OverviewNetwork, OverviewProfiles } from './types';
  import OverviewDialogs from './OverviewDialogs.svelte';
  let { model, profiles, network, feedback, actions, busy = false, refreshing = false, canDisableTun = false, core, tun, traffic }: {
    model: OverviewModel; profiles: OverviewProfiles; network: OverviewNetwork; feedback: OverviewFeedback; actions: OverviewActions;
    busy?: boolean; refreshing?: boolean; canDisableTun?: boolean; core: Snippet; tun: Snippet; traffic: Snippet;
  } = $props();
  let inspect = $state(false);
  let policies = $state(false);
  let version = $state(false);
  let detailTab = $state('capture');
  const visiblePolicy = $derived(model.groups[0]);
  const issue = $derived(model.findings.find((finding) => finding.severity === 'error') ?? model.findings[0]);
  const attentionCount = $derived(new Set(model.findings.map((finding) => finding.target)).size);
  const tunIssue = $derived(model.findings.some((finding) => finding.target === 'tun'));
  function openChecks(tab = 'capture') { detailTab = tab; inspect = true; }
  export function showTunDetails() { openChecks('capture'); }
  function handleIssue() {
    if (issue?.target === 'nodes') policies = true;
    else openChecks(issue?.target === 'tun' ? 'capture' : 'checks');
  }
</script>

<div class="professional-overview" aria-label="专业运行概览">
  <div class="overview-toolbar">
    <div class="profile-area">
      <div class="profile-control"><span class="label">{feedback.pending === 'profile' ? '切换中…' : '当前配置'}</span><FieldSelect bind:value={() => profiles.selected, actions.chooseProfile} options={profiles.options} aria-label="当前配置" placeholder={profiles.loading ? '加载中…' : '未选择配置'} disabled={busy || profiles.loading || !profiles.options.length} /></div>
      {#if profiles.error}<p class="load-error" role="alert">{profiles.error} <button data-slot="surface-button" onclick={actions.refresh}>重试</button></p>{:else if !profiles.loading && !profiles.options.length}<button data-slot="surface-button" class="empty-link" onclick={() => actions.navigate('profiles')}>添加代理配置 →</button>{/if}

    </div>
    <div class="toolbar-end"><span class="uptime" title="内核运行时长">运行 {model.uptime}</span><Button variant="ghost" size="sm" onclick={() => version = true} title={model.version}>内核版本<ChevronRight size={12}/></Button></div>
  </div>
  <div class="control-grid">
    <div class="core-controls">{@render core()}</div>
    <section class="mode-card" aria-label="代理模式与策略">
      <header><span class="label">代理模式</span><span class="label" role="status">{feedback.pending === 'mode' ? '切换中…' : ''}</span></header>
      <SegmentedControl.Root bind:value={() => model.mode, (value) => { if (value === 'global' || value === 'rule' || value === 'direct') actions.setMode(value); }} disabled={!model.ready || busy} aria-label="选择代理模式" class="mode-segment">
        {#each [{value:'global',label:'全局'},{value:'rule',label:'规则'},{value:'direct',label:'直连'}] as mode}<SegmentedControl.Item value={mode.value} disabled={!model.availableModes.includes(mode.value as 'global' | 'rule' | 'direct')} style="flex:1;">{mode.label}</SegmentedControl.Item>{/each}
      </SegmentedControl.Root>
      <p class="mode-explanation">{!model.ready || !model.mode ? '等待内核确认代理模式' : model.mode === 'rule' ? '按规则选择直连或代理' : model.mode === 'global' ? '使用全局代理出口' : '直接连接目标服务器'}</p>

      <button data-slot="surface-button" class="policy-shortcut" onclick={() => policies = true} aria-label="查看与切换策略组">
        <span class="policy-heading"><span>{visiblePolicy?.name ?? '策略组'}{model.groups.length > 1 ? ` · 共 ${model.groups.length} 组` : ''}</span><span>{visiblePolicy?.switchable ? '切换' : '查看'}<ChevronRight size={12}/></span></span>
        <span class="policy-selection"><strong title={visiblePolicy?.selectionLabel}>{visiblePolicy?.selectionLabel ?? (model.ready ? '尚未配置策略组' : '等待内核确认')}</strong><span class:danger={visiblePolicy?.failed} title={visiblePolicy?.health}>{feedback.pending.startsWith('policy:') ? '切换中…' : model.mode === 'direct' && model.ready ? '直连模式' : visiblePolicy?.failed ? '探测失败' : visiblePolicy && visiblePolicy.delay !== '—' ? visiblePolicy.delay : '待探测'}</span></span>
      </button>
    </section>
    {@render tun()}
  </div>
  {#if issue}
    <section class="issue-bar" aria-label="需要处理">
      <TriangleAlert size={15}/><div><strong>{issue.title}{attentionCount > 1 ? ` · 共 ${attentionCount} 项需关注` : ''}</strong><span>{issue.detail}</span></div>
      <Button variant="outline" size="sm" onclick={handleIssue}>{issue.target === 'nodes' ? '切换策略' : '查看原因'}</Button>
      {#if tunIssue && canDisableTun}<Button variant="ghost" size="sm" disabled={busy} onclick={actions.toggleTun}>关闭 TUN</Button>{/if}
    </section>
  {/if}
  <div class="network-line" aria-label="网络检查摘要">
    <div class="network-summary"><span class="label">本地网络</span><div class="network-identity" title={network.error ?? network.description}>
      {#if network.ip && !network.error && !network.loading}
        {#if network.countryCode}<span class="network-flag fi fi-{network.countryCode}" role="img" aria-label={`国旗 ${network.countryCode.toUpperCase()}`}></span>{:else}<Globe size={15} aria-label="地区未知" />{/if}
      {/if}
      <div class="network-address"><strong class="mono">{network.loading ? '检测中…' : network.error ? '检测失败' : network.ip || '待检测'}</strong>
        {#if network.ip && !network.error && !network.loading}<span class="network-location">{network.location || '地区未知'}</span>{/if}
      </div>
    </div><span class="separator"></span><button data-slot="surface-button" class="dns-link" onclick={() => openChecks('dns')}>{!model.tunConfirmed ? 'DNS 待确认' : !model.tunSnapshot?.enabled ? 'TUN DNS 未接管' : model.tunSnapshot.dnsHijack ? model.tunSnapshot.fakeIpEnabled ? 'Fake-IP' : 'Real DNS' : '系统 DNS'}<ChevronRight size={11}/></button></div>
    <div class="check-actions"><button data-slot="surface-button" class="checks-link" class:warning={model.findings.length > 0} class:passed={model.selfTestPassed} onclick={() => openChecks('checks')}>{#if model.findings.length}<TriangleAlert size={12}/>{attentionCount} 项需关注{:else if model.selfTestPassed}<CircleCheck size={12}/>自测通过{:else}<RefreshCw size={12}/>自测待确认{/if}</button><Button variant="ghost" size="sm" onclick={actions.refresh} disabled={refreshing || busy || network.loading}><RefreshCw size={12} class={refreshing ? 'animate-spin' : ''}/>{refreshing ? '检测中' : '检查网络'}</Button></div>
  </div>

  <section class="traffic-panel" aria-label="实时流量">{@render traffic()}</section>
</div>
<OverviewDialogs {model} {network} {feedback} {actions} {busy} {refreshing} {canDisableTun} bind:inspect bind:policies bind:version bind:detailTab />
<style>
  .professional-overview { width:100%; flex:1; min-height:0; display:flex; flex-direction:column; gap:12px; overflow:auto; padding-right:2px; }
  .overview-toolbar { display:flex; align-items:center; justify-content:space-between; gap:12px; flex-shrink:0; }
  .profile-area { min-width:0; flex:1; }.profile-control, .toolbar-end { display:flex; align-items:center; gap:8px; min-width:0; }.profile-control > :global(*) { max-width:210px; }.profile-control .label { white-space:nowrap; }.toolbar-end { flex-shrink:0; }.uptime { font-size:11px; color:var(--muted-foreground); }
  .label { font-size:12px; font-weight:500; color:var(--muted-foreground); }
  .control-grid { display:grid; grid-template-columns:1.15fr .88fr 1.1fr; gap:12px; flex-shrink:0; align-items:stretch; }
  .mode-card { display:flex; flex-direction:column; gap:9px; min-height:96px; min-width:0; padding:11px 13px; background:var(--card); border:1px solid var(--border); border-radius:10px; box-shadow:0 1px 2px rgba(0,0,0,.04); transition:box-shadow .15s,transform .15s; }.mode-card:hover { box-shadow:0 2px 6px rgba(0,0,0,.07); transform:translateY(-.5px); }.mode-card header { display:flex; align-items:center; justify-content:space-between; }:global(.mode-segment) { width:100%; }.mode-explanation { font-size:11px; color:var(--muted-foreground); margin:0; }
  .core-controls { min-width:0; display:flex; flex-direction:column; }.core-controls > :global(.core-card) { width:100%; flex:1; }
  .core-controls :global(.core-meta-row:nth-child(1)) { grid-column:1/-1; grid-row:1; }
  .core-controls :global(.core-meta-row:nth-child(2)) { grid-column:1/-1; grid-row:2; }
  .policy-shortcut { display:flex; flex-direction:column; text-align:left; gap:4px; min-width:0; font-size:11px; border-top:1px solid var(--border); margin-top:auto; padding-top:8px; cursor:pointer; }.policy-heading, .policy-selection { display:flex; align-items:center; justify-content:space-between; gap:8px; width:100%; min-width:0; }.policy-heading { color:var(--muted-foreground); font-size:10px; }.policy-heading > span:last-child { display:flex; align-items:center; gap:2px; }.policy-selection strong { font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }.policy-selection > span { color:var(--muted-foreground); flex-shrink:0; font-variant-numeric:tabular-nums; }.policy-shortcut .danger, .danger { color:var(--destructive); }
  .network-identity { display:flex; align-items:center; gap:7px; min-width:0; }.network-flag { width:20px; height:15px; border-radius:2px; flex-shrink:0; }.network-address { display:flex; flex-direction:column; min-width:0; }.network-location { color:var(--muted-foreground); font-size:10px; overflow-wrap:anywhere; }.network-summary { min-width:0; }.network-address .mono { overflow-wrap:anywhere; }
  .network-line { display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:6px; padding:6px 12px; background:var(--card); border:1px solid var(--border); border-radius:8px; font-size:11px; flex-shrink:0; }.network-summary, .check-actions { display:flex; align-items:center; gap:10px; }.network-summary strong { font-size:12px; font-weight:600; }.mono { font-family:var(--font-mono); }.separator { height:13px; width:1px; background:var(--border); }.dns-link, .checks-link { display:flex; align-items:center; gap:4px; cursor:pointer; }.dns-link { color:var(--muted-foreground); }.checks-link { color:var(--muted-foreground); }.checks-link.passed { color:var(--success); }.checks-link.warning { color:var(--warning); }
  .traffic-panel { min-height:220px; height:250px; flex:1 0 220px; }
  .issue-bar { display:flex; align-items:center; gap:8px; padding:8px 12px; border-radius:8px; border:1px solid color-mix(in srgb,var(--warning) 30%,var(--border)); background:color-mix(in srgb,var(--warning) 6%,var(--card)); font-size:11px; flex-shrink:0; }.issue-bar > :global(svg) { color:var(--warning); flex-shrink:0; }.issue-bar > div { flex:1; min-width:0; }.issue-bar strong { display:block; font-weight:600; }.issue-bar span { display:block; color:var(--muted-foreground); margin-top:3px; }
  .load-error, .empty-link { margin:4px 0 0; font-size:11px; color:var(--destructive); }.empty-link, .load-error button { color:var(--primary); cursor:pointer; }
  button:focus-visible { outline:2px solid var(--ring); outline-offset:3px; }
  @media(max-width:760px) { .control-grid { grid-template-columns:1fr 1fr; }.control-grid > :global(.feature-card) { grid-column:1/-1; }.profile-control > :global(*) { max-width:150px; }.uptime { display:none; }.network-summary { flex-wrap:wrap; gap:7px; }.issue-bar { flex-wrap:wrap; } }
  @media(max-width:480px) { .control-grid { grid-template-columns:1fr; }.profile-control > :global(*) { max-width:120px; }.profile-control .label { display:none; } }
</style>
