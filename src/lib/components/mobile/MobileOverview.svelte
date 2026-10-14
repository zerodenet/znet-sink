<script lang="ts">
  import { onMount } from 'svelte';
  import { ChartNoAxesCombined, ChevronRight, Power, RefreshCw } from '@lucide/svelte';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  import { getMobilePlatformSnapshot, type MobilePlatformSnapshot } from '$lib/features/mobile/runtime';
  import { CaptureLedger, overviewCounters, trafficShare } from '$lib/features/mobile/overview';
  import { useTrafficWorkspace } from '$lib/features/traffic/context';
  import { formatBytes, formatRate } from '$lib/features/traffic/history';
  import { displayRate } from '$lib/features/traffic/presentation';
  import { scopeKey } from '$lib/features/traffic/types';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { listProxyConfigs, type ProxyConfigProfile } from '$lib/services/config';
  import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
  import { getAppErrorMessage, getClientCoreSnapshot } from '$lib/services/core';
  import type { ClientScope } from '$lib/types/gui-api';
  import { controlScope } from '$lib/features/nodes/scope';
  import { resolveEffectiveNodeSelection } from '$lib/components/tabs/nodes-view-model';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  const navigation = useMobileNavigation();
  const workspace = useTrafficWorkspace();
  const ledger = new CaptureLedger();
  let view = $state(workspace.session.view);
  let platform = $state<MobilePlatformSnapshot | null>(null);
  let clientScope = $state<ClientScope | null>(null);
  let profiles = $state<ProxyConfigProfile[]>([]);
  let loadError = $state<string | null>(null);
  let feedback = $state<string | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let now = $state(Date.now());
  let session = $state({ up: null as bigint | null, down: null as bigint | null, at: null as string | null });
  let outletOpen = $state(false);
  let outletTarget = $state('');
  let outletTargets = $state<string[]>([]);
  let outletScope = $state('');
  let outletClientScope = $state<ClientScope | null>(null);
  let alive = true;
  let request = 0;

  const row = $derived(view.rows[scopeKey({ kind: 'global' })]);
  const counters = $derived(overviewCounters(row));
  const activeProfile = $derived(profiles.find(profile => profile.active));
  const runtimeFresh = $derived(!guiState.connectionError && guiState.connectionUpdatedAt > 0 && now - guiState.connectionUpdatedAt <= 15_000);
  const ready = $derived(runtimeFresh && guiState.connection?.coreAvailable === true);
  const modes = $derived(view.discovery?.capabilities.contracts?.controlApi?.minimumSupported === 1 && ready && clientScope?.profileId === activeProfile?.id ? guiState.proxyMode?.availableModes ?? [] : []);
  const sampled = $derived(!!row && !view.stale && !view.error && now - row.seenAt <= 10_000);
  const scope = $derived(JSON.stringify([activeProfile?.id, row?.snapshot.core_instance_id, row?.snapshot.config_revision, clientScope]));
  const sessionTotal = $derived(session.up === null || session.down === null ? null : session.up + session.down);
  const share = $derived(trafficShare(session.up, session.down));
  const sampleTime = $derived(row && Number.isSafeInteger(Number(row.snapshot.sampled_at_unix_ms)) && Number(row.snapshot.sampled_at_unix_ms) <= 8.64e15 ? new Date(Number(row.snapshot.sampled_at_unix_ms)).toLocaleString() : '未提供');
  const currentTarget = $derived(guiState.proxyMode?.globalOutbound);
  const effective = $derived(ready && currentTarget ? resolveEffectiveNodeSelection(guiState.policyGroups, currentTarget) : null);
  const vpnLabel = $derived(!platform ? 'VPN 状态待核对' : platform.adaptationState === 'not-adapted' ? 'VPN 适配待完成' : mobilePlatform.responsivePreview ? '移动布局预览' : 'VPN 状态未确认');

  async function refresh() {
    const id = ++request;
    loading = true;
    const results = await Promise.allSettled([getMobilePlatformSnapshot(), listProxyConfigs(), getClientCoreSnapshot()]);
    if (!alive || request !== id) return;
    if (results[0].status === 'fulfilled') platform = results[0].value;
    if (results[1].status === 'fulfilled') profiles = results[1].value;
    if (results[2].status === 'fulfilled') clientScope = results[2].value.scope;
    else clientScope = null;
    loadError = results.filter(result => result.status === 'rejected').map(result => getAppErrorMessage((result as PromiseRejectedResult).reason, '状态读取失败')).join('；') || null;
    loading = false;
  }
  onMount(() => {
    const stop = workspace.connect(next => view = next);
    const unsubscribe = proxyConfigSignal.onActiveChanged(() => { outletOpen = false; feedback = '配置已变化，请重新选择当前配置的出口'; void refresh(); });
    const clock = window.setInterval(() => now = Date.now(), 1000);
    const resume = () => { if (document.visibilityState === 'visible') { ledger.interrupt(); void refresh(); void workspace.session.refresh(true); } };
    document.addEventListener('visibilitychange', resume);
    void refresh();
    return () => { alive = false; request++; stop(); unsubscribe(); window.clearInterval(clock); document.removeEventListener('visibilitychange', resume); };
  });
  $effect(() => {
    if (!sampled || document.visibilityState === 'hidden') ledger.interrupt();
    ledger.update(platform?.vpnState === 'taken-over' && platform.vpnSupported, sampled ? row?.snapshot : undefined);
    session = { up: ledger.up, down: ledger.down, at: ledger.sampledAt };
  });

  function statistics() {
    workspace.statistics = { kind: 'global', plane: 'flow', query: '', offset: 0 };
    navigation.open('traffic');
  }
  function chooseOutlet() {
    outletTarget = '';
    // Static identities are mapped by Rust. Final admission still belongs to
    // the existing backend global-target validator and confirmed apply chain.
    outletTargets = [...new Set([
      ...guiState.configNodes.map(node => node.tag),
      ...guiState.configPolicyGroups.filter(group => ['urltest', 'url_test', 'fallback'].includes((group.kind ?? "").toLowerCase())).map(group => group.name),
    ])];
    outletScope = scope;
    outletClientScope = clientScope ? { ...clientScope } : null;
    outletOpen = true;
  }
  async function setMode(mode: 'global' | 'rule' | 'direct', target?: string) {
    if (busy || !modes.includes(mode) || !ready) return;
    if (mode === 'global' && !target) { chooseOutlet(); return; }
    if (target && (outletScope !== scope || !outletTargets.includes(target))) { feedback = '配置或内核实例已变化，请重新选择出口'; return; }
    const submittedScope = scope;
    let expectedScope: ClientScope;
    const targetScope = target ? outletClientScope : clientScope;
    if (!targetScope) return;
    try { expectedScope = controlScope(targetScope); }
    catch (error) { feedback = getAppErrorMessage(error, '控制身份不可用，请重新检查'); return; }
    busy = true; feedback = null;
    try {
      const result = await guiState.setProxyMode(mode, { notify: false, globalOutbound: target, expectedScope });
      if (!alive || submittedScope !== scope) return;
      feedback = result.ok ? '模式与出口已确认' : result.message;
      if (result.ok) outletOpen = false;
    } catch (error) {
      if (alive && submittedScope === scope) feedback = getAppErrorMessage(error, '结果未确认，请重新检查');
    } finally { busy = false; }
  }
</script>

<div class="mobile-overview">
  <div class="traffic-heading"><span>本次代理流量</span><button data-slot="surface-button" class="statistics" onclick={statistics}><ChartNoAxesCombined size={18}/><span>统计</span></button></div>
  <div class="session-total">{formatBytes(sessionTotal)}</div>
  <div class="traffic-ring" style={share === null ? '' : `--share:${share}%`} class:measured={share !== null} aria-label={share === null ? '流量构成尚无有效采样' : '本次上传与下载构成'}>
    <button data-slot="surface-button" class="power" disabled aria-label="开启 VPN" aria-describedby="mobile-vpn-reason"><Power size={32}/></button>
  </div>
  <div class="rates"><div><span>↑ 上传</span><strong>{formatRate(sampled ? displayRate(row, 'flow.bytes_up') : null)}</strong></div><div><span>↓ 下载</span><strong>{formatRate(sampled ? displayRate(row, 'flow.bytes_down') : null)}</strong></div></div>
  <div class="vpn-feedback" role="status"><strong>{vpnLabel}</strong><p id="mobile-vpn-reason">{platform?.reason ?? (mobilePlatform.responsivePreview ? '当前展示移动布局。真实手机接管需原生运行服务与数据面确认。' : '尚未取得原生服务状态，不能确认接管。')}</p><Button variant="ghost" disabled={loading} onclick={() => { void refresh(); void workspace.session.refresh(true); }}><RefreshCw size={16}/>重新检查</Button></div>
  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}
  {#if platform?.unsupportedCapabilities?.length}
    <details class="card" aria-label="暂不支持的移动能力">
      <summary>暂不支持的能力与原因</summary>
      {#each platform.unsupportedCapabilities as limitation (limitation.capabilityId)}
        <p class="hint">{limitation.reason}</p>
      {/each}
    </details>
  {/if}
  <section class="card" aria-label="代理模式"><span class="label">代理模式</span><div class="modes">{#each [{value:'rule',label:'规则'},{value:'direct',label:'直连'},{value:'global',label:'全局'}] as mode}<button data-slot="surface-button" class:chosen={ready && guiState.proxyMode?.currentMode === mode.value} aria-pressed={ready && guiState.proxyMode?.currentMode === mode.value} disabled={busy || !modes.includes(mode.value as 'rule'|'direct'|'global')} onclick={() => void setMode(mode.value as 'rule'|'direct'|'global')}>{mode.label}</button>{/each}</div>{#if !modes.length}<p class="hint">内核控制能力待确认；本地配置仍可查看和编辑。</p>{/if}</section>
  {#if ready && guiState.proxyMode?.currentMode === 'global'}<button data-slot="surface-button" class="card entry" onclick={chooseOutlet}><span><span class="label">全局出口</span><strong>{currentTarget ?? '请重新选择出口'}</strong></span><ChevronRight size={18}/></button>{/if}
  {#if feedback}<p role="status" class="hint">{feedback}</p>{/if}
  <button data-slot="surface-button" class="card entry" onclick={() => navigation.selectTab('configuration')}><span><span class="label">当前配置</span><strong>{activeProfile?.name ?? (loading ? '加载中…' : '添加配置')}</strong></span><ChevronRight size={18}/></button>
  <button data-slot="surface-button" class="card entry" onclick={() => navigation.selectTab('nodes')}><span><span class="label">当前有效出口</span><strong>{ready && guiState.proxyMode?.currentMode === 'direct' ? '直连' : effective?.leafTag ?? '—'}</strong>{#if effective?.groupPath.length}<small>{effective.groupPath.join(' → ')} → {effective.leafTag ?? '待确认'}</small>{/if}</span><ChevronRight size={18}/></button>
  <div class="cumulative"><span>累计上传 {formatBytes(counters.up)}</span><span>累计下载 {formatBytes(counters.down)}</span></div>
  {#if row && (!sampled || view.error)}<p class="hint">累计快照采样于 {sampleTime}；实时速率待核对。</p>{/if}
  {#if view.error}<p class="error" role="alert">{getAppErrorMessage(view.error, '统计读取中断')}</p>{/if}
</div>
<Dialog.Root bind:open={outletOpen}><Dialog.Content><Dialog.Header><Dialog.Title>选择全局出口</Dialog.Title><Dialog.Description>选择当前配置的节点或自动组。手动组成员选择在节点页面独立管理。</Dialog.Description></Dialog.Header><Dialog.Body><label class="outlet-field">出口<FieldSelect bind:value={outletTarget} disabled={busy || outletScope !== scope} placeholder="请选择" aria-label="全局出口" options={outletTargets.map(target => ({value:target,label:target}))} /></label>{#if !outletTargets.length}<p>当前配置没有允许的出口，请先维护配置。</p>{/if}{#if outletScope !== scope}<p role="alert">配置或内核实例已变化，请关闭并重新选择。</p>{/if}{#if feedback}<p role="status">{feedback}</p>{/if}</Dialog.Body><Dialog.Footer><Button variant="outline" disabled={busy} onclick={() => outletOpen = false}>取消</Button><Button disabled={busy || !outletTarget || outletScope !== scope} onclick={() => void setMode('global', outletTarget)}>{busy ? '提交中…' : '确认应用'}</Button></Dialog.Footer></Dialog.Content></Dialog.Root>

<style>
  .mobile-overview { padding: 4px 0 12px; min-width:0; overflow-wrap:anywhere; }
  .traffic-heading { display:flex; justify-content:space-between; align-items:center; gap:8px; }
  .traffic-heading > span,.label { color:var(--muted-foreground); font-size:0.875rem; }
  .statistics { min-height:44px; padding:8px 12px; display:flex; align-items:center; gap:6px; border-radius:9px; background:var(--accent); color:var(--accent-foreground); }
  .session-total { font-size:1.8rem; margin:4px 0 8px; font-weight:500; }
  .traffic-ring { width:152px; height:152px; padding:4px; border-radius:50%; background:var(--border); margin:8px auto 14px; display:flex; align-items:center; justify-content:center; }
  .traffic-ring.measured { background:conic-gradient(#168348 var(--share), #2563eb 0); }
  .power { width:96px; height:96px; display:flex; align-items:center; justify-content:center; border:1px solid var(--border); border-radius:50%; background:var(--background); color:var(--muted-foreground); }
  .rates { display:flex; justify-content:center; gap:24px; flex-wrap:wrap; text-align:center; }
  .rates > div { display:flex; flex-direction:column; gap:3px; min-width:100px; }.rates span { font-size:0.75rem; color:var(--muted-foreground); }.rates strong { font-size:1rem; font-weight:500; }
  .vpn-feedback { text-align:center; padding:12px 0 0; }.vpn-feedback strong { font-weight:500; }.vpn-feedback p,.hint { font-size:0.8rem; color:var(--muted-foreground); margin:6px 0; }
  .card { margin:10px 0; padding:12px 14px; border-radius:12px; background:var(--card); border:1px solid var(--border); width:100%; }
  details.card summary { min-height:44px; display:flex; align-items:center; cursor:pointer; }
  .modes { display:flex; margin-top:8px; padding:4px; background:var(--muted); border-radius:9px; gap:4px; }.modes button { flex:1; min-height:44px; border-radius:7px; }.modes .chosen { background:var(--popover); }.modes button:disabled { opacity:0.5; }
  .entry { display:flex; align-items:center; justify-content:space-between; text-align:left; gap:8px; min-height:64px; }.entry > span { display:flex; flex-direction:column; gap:4px; min-width:0; }.entry strong { font-weight:500; }.entry small { color:var(--muted-foreground); }.entry > :global(svg) { flex-shrink:0; }
  .cumulative { display:flex; justify-content:space-between; gap:8px; flex-wrap:wrap; color:var(--muted-foreground); font-size:0.75rem; margin:14px 0 0; }.error { color:var(--destructive); font-size:0.85rem; }
  .outlet-field { display:flex; flex-direction:column; gap:8px; }.outlet-field :global(.znet-field) { min-height:44px; width:100%; font-size:1rem; border:1px solid var(--border); border-radius:8px; background:var(--popover); color:var(--foreground); padding:8px; }
</style>
