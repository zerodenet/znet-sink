<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { RefreshCw, Search, Check } from '@lucide/svelte';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { endpointGateway } from '$lib/features/endpoints/client';
  import { EndpointSession } from '$lib/features/endpoints/session';
  import { catalogSupported } from '$lib/features/endpoints/policy';
  import { getAppErrorMessage, getClientCoreSnapshot } from '$lib/services/core';
  import type { ClientCoreSnapshot } from '$lib/types/gui-api';
  import EndpointCard from './EndpointCard.svelte';
  import type { TrafficView, TrafficSession } from '$lib/features/traffic/session';
  import { endpointObservation } from '$lib/features/endpoints/observation';
  import { supported } from '$lib/features/traffic/policy';
  let { trafficView, trafficSession }: { trafficView?: TrafficView; trafficSession?: TrafficSession } = $props();
  const session = new EndpointSession(endpointGateway, (next) => { view = next; });
  let view = $state(session.view);
  let query = $state('');
  const endpoints = $derived((view.catalog?.endpoints ?? []).filter(row => `${row.tag} ${row.protocol} ${row.endpoint_id}`.toLowerCase().includes(query.toLowerCase())));
  $effect(() => { trafficSession?.watch(endpoints.map(row => endpointObservation(row, trafficView?.rows ?? {}).key)); });
  let hostReady = $state(false);
  let hostError = $state<unknown>(null);
  let hostScope: string | undefined;
  let hostRevision = -1;
  let hostListening = false;
  let scopeSequence = 0;
  let mounted = false;
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
  <header><div class="heading"><h2>端点</h2><span class="count">{view.catalog?.endpoints.length ?? '—'}</span></div><div class="toolbar">{#if view.result}<span class="result" role="status" title={view.result}><Check size={13} />已生效</span>{/if}<Button variant="ghost" size="icon-sm" aria-label="刷新端点" title="刷新端点" disabled={view.loading || !!view.busy} onclick={() => refreshHostAndDirectory()}><RefreshCw size={15} class={view.loading ? 'animate-spin' : ''} /></Button></div></header>
  {#if hostError}<p role="alert" class="error">{getAppErrorMessage(hostError, '客户端状态读取失败')} 客户端状态未确认，端点控制暂不可用。</p>{/if}
  {#if view.error}<p role="alert" class="error">{getAppErrorMessage(view.error, '端点操作失败')} {view.stale ? '数据已过期，控制暂不可用。' : ''}</p>{/if}
  {#if view.refreshError}<p role="alert" class="error">{getAppErrorMessage(view.refreshError, '端点目录刷新失败')} 数据已过期，控制暂不可用。</p>{/if}
  {#if view.detailsError}<p role="alert" class="error">{getAppErrorMessage(view.detailsError, '协议详情刷新失败')} 旧详情已隐藏。</p>{/if}
  {#if view.detailsLoading}<p role="status">正在刷新协议详情…</p>{/if}
  {#if view.busy}<p role="status" class="sr-only">正在等待内核确认</p>{/if}
  {#if view.catalog && !catalogSupported(view.catalog)}
    <p class="empty">当前内核未声明兼容的端点目录能力。普通节点仍可在策略组和全部节点中使用；升级至支持端点目录的内核后可管理端点。</p>
  {:else if view.catalog}
    <label class="search"><Search size={15} /><Input aria-label="搜索端点" placeholder="搜索端点" bind:value={query} /></label>
    <div class="endpoint-grid">
      {#each endpoints as endpoint (endpoint.core_instance_id + ':' + endpoint.endpoint_id)}
        <EndpointCard {endpoint} observation={endpointObservation(endpoint, trafficView?.rows ?? {}).observation} observationAvailable={supported(trafficView?.discovery ?? null)} observationStale={trafficView?.stale ?? true} pending={view.busy === endpoint.endpoint_id} traffic={view.traffic[endpoint.endpoint_id]} stale={view.stale} details={view.details?.endpoint_id === endpoint.endpoint_id ? view.details : null} catalog={view.catalog} disabled={!hostReady || view.stale || !!view.busy} inspecting={view.loading || view.detailsLoading} onAction={(row, action) => { void session.act(row, action); }} onInspect={row => { void session.inspect(row); }} />
      {:else}<p class="empty">{query ? '无匹配端点' : '当前内核配置没有声明端点'}</p>{/each}
    </div>
  {:else if view.loading}<p class="empty">正在读取内核端点目录…</p>{/if}

</section>
<style>
  .endpoints-panel { flex:1; min-width:0; overflow:auto; padding:20px; } header { display:flex; justify-content:space-between; align-items:center; gap:12px; } h2 { font-size:16px; font-weight:600; } p { margin:8px 0; font-size:12px; color:var(--muted-foreground); } .error { color:var(--destructive); overflow-wrap:anywhere; }
  .search { display:flex; align-items:center; gap:12px; font-size:12px; margin:16px 0; } .search :global(input) { flex:1; min-width:0; padding:9px; border:1px solid var(--border); border-radius:8px; background:var(--background); }
  .heading,.toolbar,.result { display:flex; align-items:center; gap:8px; } .count { font-size:11px; color:var(--muted-foreground); background:var(--muted); border-radius:5px; padding:2px 6px; } .result { font-size:11px; color:var(--success,#16a34a); } .search { position:relative; color:var(--muted-foreground); } .search> :global(svg) { position:absolute; left:10px; } .search :global(input) { padding-left:33px; }
  .endpoint-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,300px),1fr)); gap:16px; } .empty { padding:24px 0; }
</style>
