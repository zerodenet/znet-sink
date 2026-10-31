<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { RefreshCw, Search, Check, Plus, Network, MoreHorizontal, Pencil, Trash2 } from '@lucide/svelte';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { endpointGateway } from '$lib/features/endpoints/client';
  import { EndpointSession } from '$lib/features/endpoints/session';
  import { catalogSupported } from '$lib/features/endpoints/policy';
  import { getAppErrorMessage, getClientCoreSnapshot } from '$lib/services/core';
  import type { ClientCoreSnapshot } from '$lib/types/gui-api';
  import EndpointCard from './EndpointCard.svelte';
  import EndpointEditor from './EndpointEditor.svelte';
  import type { UserEndpoint } from '$lib/features/endpoints/library';
  let editor: { edit: (tag?: string | null) => Promise<void>; remove: (tag: string) => Promise<void>; refresh: () => Promise<void> } | undefined = $state();
  let userEndpoints = $state<UserEndpoint[]>([]);
  let userBusy = $state(false);
  import type { TrafficView, TrafficSession } from '$lib/features/traffic/session';
  import { endpointObservation } from '$lib/features/endpoints/observation';
  import { supported } from '$lib/features/traffic/policy';
  let { trafficView, trafficSession }: { trafficView?: TrafficView; trafficSession?: TrafficSession } = $props();
  const session = new EndpointSession(endpointGateway, (next) => { view = next; });
  let view = $state(session.view);
  let query = $state('');
  const endpoints = $derived((view.catalog?.endpoints ?? []).filter(row => `${row.tag} ${row.protocol} ${row.endpoint_id}`.toLowerCase().includes(query.toLowerCase())));
  const configured = $derived(userEndpoints.filter(row => !(view.catalog?.endpoints ?? []).some(endpoint => endpoint.tag === row.tag && endpoint.protocol === row.protocol.type)));
  const configuredMatches = $derived(configured.filter(row => `${row.tag} ${row.protocol.type}`.toLowerCase().includes(query.toLowerCase())));
  function userOwned(tag: string, protocol: string) { return userEndpoints.some(row => row.tag === tag && row.protocol.type === protocol); }
  $effect(() => { trafficSession?.watch(endpoints.map(row => endpointObservation(row, trafficView?.rows ?? {}).key)); });
  let hostReady = $state(false);
  let hostError = $state<unknown>(null);
  let hostScope: string | undefined;
  let hostRevision = -1;
  let hostListening = false;
  let scopeSequence = 0;
  let mounted = false;
  const panelError = $derived(hostError ? `${getAppErrorMessage(hostError,'客户端状态读取失败')} 客户端状态未确认，端点控制暂不可用。`
    : view.error ? getAppErrorMessage(view.error,'端点操作失败')
    : view.refreshError ? `${getAppErrorMessage(view.refreshError,'端点目录刷新失败')} 数据已过期，控制暂不可用。`
    : view.detailsError ? `${getAppErrorMessage(view.detailsError,'协议详情刷新失败')} 旧详情已隐藏。` : null);
  const panelStatus = $derived(hostError ? '状态未确认' : view.error ? '操作失败' : view.refreshError ? '同步失败'
    : view.detailsError ? '详情过期' : view.detailsLoading ? '读取详情' : view.loading ? '正在同步' : view.stale ? '待同步' : view.result ? '已生效' : '已同步');
  function scopeKeyOf(snapshot: ClientCoreSnapshot) {
    const { profileId, configRevision, coreInstanceId } = snapshot.scope;
    return JSON.stringify([profileId ?? null, configRevision, coreInstanceId]);
  }
  async function refreshHostAndDirectory() {
    const sequence = scopeSequence;
    try {
      const snapshot = await getClientCoreSnapshot();
      if (!mounted) return;
      if (sequence === scopeSequence && snapshot.revision >= hostRevision) {
        const next = scopeKeyOf(snapshot);
        if (hostScope !== undefined && hostScope !== next) session.invalidate();
        hostScope = next;
        hostRevision = snapshot.revision;
        hostReady = hostListening;
        if (hostListening) hostError = null;
      }
    } catch (error) {
      if (!mounted) return;
      if (sequence === scopeSequence) { hostReady = false; hostError = error; }
    }
    if (mounted) await session.refresh();
  }
  onMount(() => {
    mounted = true;
    let unlisten: (() => void) | undefined;
    // Poll only while this management view is mounted and visible. This also
    // works with kernels that have no endpoint event stream yet.
    const timer = setInterval(() => { if (!document.hidden) void session.refresh(false, true); }, 5000);
    const focused = () => { void session.refresh(false, true); };
    window.addEventListener('focus', focused);
    void listen<ClientCoreSnapshot>('client-core:updated', event => {
      if (event.payload.revision < hostRevision) return;
      hostRevision = event.payload.revision;
      const next = scopeKeyOf(event.payload);
      scopeSequence++;
      hostReady = true;
      hostError = null;
      if (hostScope !== next) { hostScope = next; session.invalidate(); void session.refresh(); }
    }).then(stop => {
      if (!mounted) { stop(); return; }
      unlisten = stop;
      hostListening = true;
      void refreshHostAndDirectory();
    }).catch(error => {
      if (!mounted) return;
      hostError = error;
      hostReady = false;
      void session.refresh();
    });
    return () => { mounted = false; clearInterval(timer); window.removeEventListener('focus', focused); unlisten?.(); session.dispose(); };
  });
</script>
<section class="endpoints-panel" aria-label="端点管理">
  <EndpointEditor bind:this={editor} bind:busy={userBusy} onChange={rows => userEndpoints = rows} disabled={!!view.busy} onSaved={() => { session.invalidate(); void refreshHostAndDirectory(); }} />
  <header><div class="heading"><h2>端点</h2><span class="count">{(view.catalog?.endpoints.length ?? 0) + configured.length}</span></div><div class="toolbar"><span class="panel-status" class:failed={!!panelError} class:confirmed={!!view.result && !panelError} role={panelError ? 'alert' : 'status'} title={panelError ?? view.result ?? (view.stale ? '保留上次观测，等待目录确认；控制暂不可用。' : '端点目录已同步')} aria-label="端点目录状态">{#if view.result && !panelError}<Check size={13}/>{:else}<i></i>{/if}<span>{panelStatus}</span>{#if panelError}<span class="sr-only">{panelError}</span>{/if}</span><Button variant="ghost" size="icon-sm" aria-label="刷新端点" title="刷新端点" disabled={view.loading || !!view.busy} onclick={() => { void editor?.refresh(); void refreshHostAndDirectory(); }}><RefreshCw size={15} class={view.loading ? 'animate-spin' : ''} /></Button><Button size="sm" disabled={!!view.busy || userBusy} onclick={() => editor?.edit()}><Plus size={14} />创建端点</Button></div></header>
  {#if view.busy}<p role="status" class="sr-only">正在等待内核确认</p>{/if}
  {#if view.catalog && !catalogSupported(view.catalog)}
    <p class="empty">当前内核未声明兼容的端点目录能力。用户端点可继续创建和编辑；运行状态需要兼容内核确认。</p>
  {/if}
  <label class="search"><Search size={15} /><Input aria-label="搜索端点" placeholder="搜索端点" bind:value={query} /></label>
  <div class="endpoint-grid">
    {#if view.catalog}
      {#each endpoints as endpoint (endpoint.core_instance_id + ':' + endpoint.endpoint_id)}
        <EndpointCard {endpoint} userOwned={userOwned(endpoint.tag, endpoint.protocol)} managementDisabled={!!view.busy || userBusy} onEdit={() => editor?.edit(endpoint.tag)} onDelete={() => editor?.remove(endpoint.tag)} observation={endpointObservation(endpoint, trafficView?.rows ?? {}).observation} observationAvailable={supported(trafficView?.discovery ?? null)} observationStale={trafficView?.stale ?? true} pending={view.busy === endpoint.endpoint_id} traffic={view.traffic[endpoint.endpoint_id]} stale={view.stale} details={view.details?.endpoint_id === endpoint.endpoint_id ? view.details : null} detailsError={view.detailsError ?? view.refreshError} catalog={view.catalog} disabled={!hostReady || view.stale || !!view.busy || userBusy} inspecting={view.loading || view.detailsLoading} onAction={(row, action) => { void session.act(row, action); }} onInspect={row => { void session.inspect(row); }} />
      {/each}
    {/if}
    {#each configuredMatches as row (row.tag)}
      <article class="endpoint-card configured-card" aria-label={`端点 ${row.tag}`}>
        <header><div class="configured-identity"><Network size={20}/><div><h3>{row.tag}</h3><span>{row.protocol.type}</span></div></div><div class="configured-tools"><span title="用户创建，自动加入所有配置">用户</span><details>
          <summary aria-label={`${row.tag} 更多操作`} title="更多操作"><MoreHorizontal size={18}/></summary>
          <div class="configured-menu">
            <Button variant="ghost" size="sm" disabled={userBusy || !!view.busy} onclick={event => { event.currentTarget.closest('details')?.removeAttribute('open'); void editor?.edit(row.tag); }}><Pencil size={14}/>编辑端点</Button>
            <Button variant="ghost" size="sm" disabled={userBusy || !!view.busy} onclick={event => { event.currentTarget.closest('details')?.removeAttribute('open'); void editor?.remove(row.tag); }}><Trash2 size={14}/>删除端点</Button>
          </div></details></div></header>
        <p class="configured-status">尚未在运行态确认</p><p>已保存，将自动加入当前及后续切换的配置。</p>
      </article>
    {/each}
    {#if !endpoints.length && !configuredMatches.length}
      <p class="empty">{query ? '无匹配端点' : view.loading ? '正在读取端点…' : '暂无端点，可点击“创建端点”添加'}</p>
    {/if}
  </div>

</section>
<style>
  .endpoints-panel { flex:1; min-width:0; overflow:auto; overflow-y:scroll; scrollbar-gutter:stable; padding:20px; } header { display:flex; justify-content:space-between; align-items:center; gap:12px; flex-wrap:wrap; } h2 { font-size:16px; font-weight:600; } p { margin:8px 0; font-size:12px; color:var(--muted-foreground); }
  .search { display:flex; align-items:center; gap:12px; font-size:12px; margin:16px 0; } .search :global(input) { flex:1; min-width:0; padding:9px; border:1px solid var(--border); border-radius:8px; background:var(--background); }
  .heading,.toolbar,.panel-status { display:flex; align-items:center; gap:8px; } .count { font-size:11px; color:var(--muted-foreground); background:var(--muted); border-radius:5px; padding:2px 6px; } .panel-status { width:8em; font-size:11px; white-space:nowrap; color:var(--muted-foreground); } .panel-status i { width:6px; height:6px; border-radius:50%; background:currentColor; flex-shrink:0; } .panel-status.failed { color:var(--destructive); } .panel-status.confirmed { color:var(--success,#16a34a); } .search { position:relative; color:var(--muted-foreground); } .search> :global(svg) { position:absolute; left:10px; } .search :global(input) { padding-left:33px; }
  .configured-card { border:1px solid var(--border); border-radius:12px; padding:18px; background:var(--card); min-width:0; } .configured-card h3 { font-size:14px; font-weight:600; overflow-wrap:anywhere; } .configured-identity,.configured-tools { display:flex; align-items:center; gap:10px; min-width:0; } .configured-identity>div { min-width:0; } .configured-tools>span,.configured-identity span { font-size:11px; color:var(--muted-foreground); } .configured-status { margin-top:28px; } .configured-tools details { position:relative; } .configured-tools summary { list-style:none; cursor:pointer; padding:4px; border-radius:4px; } .configured-tools summary::-webkit-details-marker { display:none; } .configured-menu { position:absolute; right:0; top:100%; z-index:10; min-width:140px; padding:4px; border:1px solid var(--border); background:var(--popover); border-radius:8px; box-shadow:0 8px 24px #0002; display:grid; }
  .endpoint-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,300px),1fr)); gap:16px; } .empty { padding:24px 0; }
</style>
