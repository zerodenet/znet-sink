<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Dialog from '$lib/components/ui/dialog';
  import { packetGateway } from '$lib/features/packet/client';
  import { PacketSession } from '$lib/features/packet/session';
  import { closeReason, packetTime } from '$lib/features/packet/types';
  import type { PacketTarget } from '$lib/features/packet/types';
  import { getAppErrorMessage, getClientCoreSnapshot } from '$lib/services/core';
  import type { ClientCoreSnapshot } from '$lib/types/gui-api';
  const session = new PacketSession(packetGateway, value => { view = value; });
  let view = $state(session.view);
  let query = $state('');
  let pending = $state<PacketTarget | null>(null);
  let confirmOpen = $state(false);
  let hostError = $state<unknown>(null);
  let hostReady = $state(false);
  let scope: string | null = null;
  let hostRevision = -1;
  const rows = $derived((view.snapshot?.routes ?? []).filter(r => `${r.routeId} ${r.source} ${r.destination} ${r.inboundTag} ${r.outboundTag} ${r.endpoints.map(e => e.endpointId).join(' ')}`.toLowerCase().includes(query.toLowerCase())));
  function applyScope(snapshot: ClientCoreSnapshot) {
    if (snapshot.revision < hostRevision) return;
    hostRevision = snapshot.revision;
    const next = JSON.stringify(snapshot.scope);
    if (scope !== null && scope !== next) { pending = null; confirmOpen = false; session.invalidate(); }
    scope = next; hostReady = true; hostError = null;
  }
  async function refresh(clearError = false) {
    try { applyScope(await getClientCoreSnapshot()); } catch (error) { hostError = error; hostReady = false; }
    await session.refresh(clearError);
  }
  onMount(() => {
    let mounted = true;
    let stop: (() => void) | undefined;
    void listen<ClientCoreSnapshot>('client-core:updated', event => {
      applyScope(event.payload);
      if (view.stale && !view.busy) void session.refresh();
    }).then(unlisten => {
      if (!mounted) { unlisten(); return; }
      stop = unlisten; void refresh();
    }).catch(error => { if (mounted) { hostError = error; hostReady = false; void session.refresh(); } });
    const focused = () => { if (!document.hidden) void refresh(); };
    window.addEventListener('focus', focused);
    document.addEventListener('visibilitychange', focused);
    const timer = setInterval(() => { if (!document.hidden && !confirmOpen) void refresh(); }, 5000);
    return () => { mounted = false; clearInterval(timer); stop?.(); window.removeEventListener('focus', focused); document.removeEventListener('visibilitychange', focused); session.dispose(); };
  });
</script>
<section class="packet-panel" aria-label="Packet 路径">
  <header><div><h2>Packet 路径</h2><p>原生 IP Packet 会话，独立于 TCP / UDP Flow</p></div><Button variant="outline" disabled={view.loading || !!view.busy} onclick={() => refresh(true)}>重新检查</Button></header>
  <Input aria-label="搜索 Packet 会话" placeholder="搜索地址、会话或端点" bind:value={query} />
  {#if view.loading}<p role="status">正在读取 Packet 会话…</p>{/if}
  {#if hostError}<p class="notice" role="alert">{getAppErrorMessage(hostError, '客户端状态未确认')}；控制暂不可用。</p>{/if}
  {#if view.error}<p class="notice" role="alert">{getAppErrorMessage(view.error, 'Packet 路径读取或操作失败')}。请重新检查。</p>{/if}
  {#if view.refreshError}<p class="notice" role="alert">{getAppErrorMessage(view.refreshError, 'Packet 路径读取失败')}。缓存已过期，请重新检查。</p>{/if}
  {#if view.message}<p class="notice" role="status">{view.message}</p>{/if}
  {#if view.snapshot && !view.snapshot.supported}<p class="notice">{view.snapshot.reason}。保留此入口，更新内核后可重新检查。</p>
  {:else if view.snapshot}
    <p class="sample">采样：{packetTime(view.snapshot.observedAtUnixMs)}{view.stale ? ' · 已过期，控制暂不可用' : ''}</p>
    {#if view.snapshot.closeReason}<p class="notice">{view.snapshot.closeReason}</p>{/if}
    {#if view.snapshot.nextOffset !== null}<p class="notice">当前显示前 {view.snapshot.routes.length} 条，共 {view.snapshot.total} 条；这是有界当前快照，不含无限历史。</p>{/if}
    {#if rows.length === 0 && !view.loading}<p>{view.snapshot.routes.length ? '没有匹配的 Packet 会话' : '当前没有 Packet 会话'}</p>{/if}
    {#each rows as route (route.routeId)}
      {@const reason = closeReason(view.snapshot, route, view.stale)}
      <article>
        <div class="route-head"><strong>{route.source || '—'} → {route.destination || '—'}</strong><span>{route.state === 'active' ? '活动' : route.state === 'closing' ? '关闭中' : '已结束'}</span></div>
        <dl><dt>会话 ID</dt><dd>{route.routeId}</dd><dt>IP 协议</dt><dd>{route.ipProtocol}</dd><dt>路径方式</dt><dd>{route.translated ? '转换' : '原生'}</dd><dt>入站 / 出站</dt><dd>{route.inboundTag || '—'} / {route.outboundTag || '—'}</dd><dt>端点</dt><dd>{route.endpoints.length ? route.endpoints.map(e => `${e.endpointId} (generation ${e.generation ?? '—'})`).join('、') : '未提供'}</dd><dt>开始时间</dt><dd>{packetTime(route.startedAtUnixMs)}</dd></dl>
        <div class="actions"><Button variant="outline" disabled={view.stale || !!view.busy} onclick={() => session.inspect(route)}>详情</Button><Button variant="outline" disabled={!hostReady || !!reason || !!view.busy || view.unconfirmed.includes(route.routeId)} onclick={() => { pending = session.target(route); confirmOpen = pending !== null; }}>关闭会话</Button></div>
        {#if reason}<p class="sample">{reason}</p>{/if}
        {#if view.unconfirmed.includes(route.routeId)}<p class="notice">原关闭结果未确认；只核对原会话，不自动重复关闭。</p>{/if}
      </article>
    {/each}
  {/if}
  {#if view.detail}<article aria-label="Packet 会话详情"><h3>会话详情</h3><dl><dt>会话 ID</dt><dd>{view.detail.routeId}</dd><dt>内核实例</dt><dd>{view.detail.coreInstanceId}</dd><dt>配置版本</dt><dd>{view.detail.configRevision}</dd><dt>状态</dt><dd>{view.detail.state}</dd><dt>源 / 目的</dt><dd>{view.detail.source} / {view.detail.destination}</dd></dl></article>{/if}
</section>
<Dialog.Root bind:open={confirmOpen}>
  <Dialog.Content class="packet-confirm">
    <Dialog.Header><Dialog.Title>关闭此 Packet 会话？</Dialog.Title><Dialog.Description>只关闭打开确认面板时的会话 ID；端点启用意图及其他 Flow 保持不变。应用可能建立新会话。</Dialog.Description></Dialog.Header>
    <Dialog.Body><p class="target">{pending?.routeId}</p><p>实例：{pending?.coreInstanceId}<br/>配置版本：{pending?.configRevision}</p></Dialog.Body>
    <Dialog.Footer><Button variant="outline" onclick={() => { confirmOpen = false; pending = null; }}>取消</Button><Button disabled={!pending || !hostReady || !!view.busy} onclick={() => { const target = pending; confirmOpen = false; pending = null; if (target) void session.close(target); }}>确认关闭</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  .packet-panel { min-width: 0; display: grid; gap: 12px; padding: 16px; }
  header,.route-head,.actions { display: flex; gap: 12px; justify-content: space-between; align-items: center; flex-wrap: wrap; }
  h2 { font-size: 20px; font-weight: 600; } h3 { font-weight: 600; }
  header p,.sample { color: var(--muted-foreground); font-size: 12px; }
  article { min-width: 0; border: 1px solid var(--border); border-radius: 12px; padding: 14px; display: grid; gap: 12px; }
  .route-head strong { min-width: 0; flex: 1 1 180px; }
  .route-head span { flex-shrink: 0; }
  strong,dd,.target { overflow-wrap: anywhere; word-break: break-word; }
  dl { display: grid; grid-template-columns: minmax(70px, 90px) minmax(0, 1fr); gap: 6px 12px; font-size: 13px; } dt { color: var(--muted-foreground); } dd { min-width: 0; }
  .notice { padding: 12px; background: var(--muted); border-radius: 9px; font-size: 13px; overflow-wrap: anywhere; }
  .actions { justify-content: flex-end; }
  :global(.packet-confirm button), :global(.packet-panel button), :global(.packet-panel input) { min-height: 44px; }
  @media (max-width: 360px) { .packet-panel { padding: 12px; } dl { grid-template-columns: 74px minmax(0, 1fr); } }
</style>
