<script lang="ts">
  import StorageInspectionPanel from '$lib/components/settings/StorageInspectionPanel.svelte';
  import { onMount, untrack } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Dialog from '$lib/components/ui/dialog';
  import { packetGateway } from '$lib/features/packet/client';
  import { PacketSession } from '$lib/features/packet/session';
  import { packetTime, physicalKey, safeGeneration } from '$lib/features/packet/types';
  import type { PacketTarget, PacketGateway } from '$lib/features/packet/types';
  import { getAppErrorMessage, getClientCoreSnapshot } from '$lib/services/core';
  import type { ClientCoreSnapshot } from '$lib/types/gui-api';
  let { gateway = packetGateway, readHost = getClientCoreSnapshot }: { gateway?: PacketGateway; readHost?: () => Promise<ClientCoreSnapshot> } = $props();
  const session = new PacketSession(untrack(() => gateway), value => { view = value; });
  let view = $state(session.view);
  let query = $state('');
  let pending = $state<PacketTarget | null>(null);
  let confirmOpen = $state(false);
  let hostError = $state<unknown>(null);
  let hostReady = $state(false);
  let hostGeneration = $state<number | null>(null);
  let scope: string | null = $state(null);
  let hostRevision = -1;
  let alive = true;
  const controlsReady = $derived(hostReady && view.snapshot?.clientScopeGeneration === hostGeneration);
  const rows = $derived((view.snapshot?.routes ?? []).filter(r => `${r.routeId} ${r.source} ${r.destination} ${r.inboundTag} ${r.outboundTag} ${r.endpoints.map(e => e.endpointId).join(' ')}`.toLowerCase().includes(query.toLowerCase())));
  function clearConfirmation() { pending = null; confirmOpen = false; }
  function applyScope(snapshot: ClientCoreSnapshot) {
    if (!alive) return;
    if (!safeGeneration(snapshot.revision) || !safeGeneration(snapshot.scopeGeneration)) {
      hostReady = false; hostError = { message: '客户端缺少正安全的作用域代次，控制暂不可用' };
      clearConfirmation(); session.invalidate(); return;
    }
    if (snapshot.revision < hostRevision) return;
    hostRevision = snapshot.revision;
    const next = JSON.stringify([snapshot.scope, snapshot.scopeGeneration]);
    if (scope !== null && scope !== next) { clearConfirmation(); session.invalidate(); }
    scope = next; hostGeneration = snapshot.scopeGeneration; hostReady = true; hostError = null;
  }
  async function readScope() {
    try { applyScope(await readHost()); }
    catch (error) { if (alive) { hostError = error; hostReady = false; clearConfirmation(); } }
  }
  async function refresh(clearError = false) {
    if (!clearError && view.failedOffset !== null) return;
    await readScope();
    if (!alive) return;
    if (clearError) { clearConfirmation(); await session.retry(); }
    else await session.refresh();
  }
  async function navigate(direction: 'next' | 'previous' | 'first') {
    clearConfirmation(); await readScope(); if (!alive) return;
    await session[direction]();
  }
  onMount(() => {
    let stop: (() => void) | undefined;
    void listen<ClientCoreSnapshot>('client-core:updated', event => {
      applyScope(event.payload);
      if (view.stale && !view.busy && view.failedOffset === null) void refresh();
    }).then(unlisten => {
      if (!alive) { unlisten(); return; }
      stop = unlisten; void refresh();
    }).catch(error => { if (alive) { hostError = error; hostReady = false; void session.refresh(); } });
    const focused = () => { if (!document.hidden) void refresh(); };
    window.addEventListener('focus', focused);
    document.addEventListener('visibilitychange', focused);
    const timer = setInterval(() => { if (!document.hidden && !confirmOpen) void refresh(); }, 5000);
    return () => { alive = false; clearInterval(timer); stop?.(); window.removeEventListener('focus', focused); document.removeEventListener('visibilitychange', focused); session.dispose(); };
  });
</script>
<section class="packet-panel" aria-label="Packet 路径">
  <header><div><h2>Packet 路径</h2><p>原生 IP Packet 会话，独立于 TCP / UDP Flow</p></div><Button variant="outline" disabled={view.loading || !!view.busy} onclick={() => refresh(true)}>重新检查</Button></header>
  <StorageInspectionPanel kind="packet" scopeKey={JSON.stringify([scope, view.snapshot?.hostSessionId])} />
  <Input aria-label="仅筛选本页 Packet 会话" placeholder="仅筛选本页地址、会话或端点" bind:value={query} />
  <p class="sample">仅筛选本页；每页独立读取实时会话，各页之间可能新增、结束或移位，不是冻结的完整历史。</p>
  {#if view.loading}<p role="status">正在读取 Packet 会话…</p>{/if}
  {#if hostError}<p class="notice" role="alert">{getAppErrorMessage(hostError, '客户端状态未确认')}；控制暂不可用。</p>{/if}
  {#if view.error}<p class="notice" role="alert">{getAppErrorMessage(view.error, 'Packet 路径读取或操作失败')}。请重新检查。</p>{/if}
  {#if view.refreshError}<p class="notice" role="alert">{getAppErrorMessage(view.refreshError, 'Packet 路径读取失败')}。缓存已过期，请重新检查。</p>{/if}
  {#if view.message}<p class="notice" role="status">{view.message}</p>{/if}
  {#if view.snapshot?.closeLedgerRecovery.state === 'blocked'}<p class="notice" role="alert">{view.snapshot.closeLedgerRecovery.reason ?? '关闭账本恢复未确认'}。保留只读查看；不会发送新的关闭，也不会把读取失败当作原会话已结束。</p>
  {:else if view.snapshot && (view.snapshot.closeLedgerRecovery.recoveredCount ?? 0) > 0}<p class="notice" role="status">已读取 {view.snapshot.closeLedgerRecovery.recoveredCount} 条宿主恢复记录；这是原操作元资料，不代表会话已关闭，不会自动重放。</p>{/if}
  {#if view.snapshot && !view.snapshot.supported}<p class="notice">{view.snapshot.reason}。保留此入口，更新内核后可重新检查。</p>
  {:else if view.snapshot}
    <p class="sample">采样：{packetTime(view.snapshot.observedAtUnixMs)}{view.stale ? ' · 已过期，控制暂不可用' : ''}</p>
    {#if view.snapshot.closeReason}<p class="notice">{view.snapshot.closeReason}</p>{/if}
    <p class="sample" aria-label="Packet 分页范围">本页偏移 {view.snapshot.offset}，读取 {view.snapshot.routes.length} 条；本次读取时共 {view.snapshot.total ?? '—'} 条，每页最多 {view.snapshot.limit} 条。</p>
    <nav class="actions" aria-label="Packet 分页">
      <Button variant="outline" disabled={view.loading || !!view.busy || view.stale || view.snapshot.offset === 0} onclick={() => navigate('previous')}>上一页</Button>
      <Button variant="outline" disabled={view.loading || !!view.busy || view.stale || view.snapshot.nextOffset === null} onclick={() => navigate('next')}>下一页</Button>
      {#if view.failedOffset !== null}<Button variant="outline" disabled={view.loading || !!view.busy} onclick={() => refresh(true)}>重试偏移 {view.failedOffset}</Button>{/if}
      {#if view.snapshot.offset > 0 || view.refreshError}<Button variant="outline" disabled={view.loading || !!view.busy} onclick={() => navigate('first')}>返回第一页重新检查</Button>{/if}
    </nav>
    {#if rows.length === 0 && !view.loading}<p>{view.snapshot.routes.length ? '本页没有匹配的 Packet 会话；可清除筛选后翻页' : view.snapshot.offset > 0 ? '此偏移当前没有 Packet 会话；会话数量可能已变化，可返回上一页或第一页' : '当前没有 Packet 会话'}</p>{/if}
    {#each rows as route (route.routeId)}
      {@const reason = session.reason(route)}
      <article>
        <div class="route-head"><strong>{route.source || '—'} → {route.destination || '—'}</strong><span>{route.state === 'active' ? '活动' : route.state === 'closing' ? '关闭中' : '已结束'}</span></div>
        <dl><dt>会话 ID</dt><dd>{route.routeId}</dd><dt>IP 协议</dt><dd>{route.ipProtocol}</dd><dt>路径方式</dt><dd>{route.translated ? '转换' : '原生'}</dd><dt>入站 / 出站</dt><dd>{route.inboundTag || '—'} / {route.outboundTag || '—'}</dd><dt>端点</dt><dd>{route.endpoints.length ? route.endpoints.map(e => `${e.endpointId} (generation ${e.generation ?? '—'})`).join('、') : '未提供'}</dd><dt>开始时间</dt><dd>{packetTime(route.startedAtUnixMs)}</dd></dl>
        <div class="actions"><Button variant="outline" disabled={view.loading || view.stale || !controlsReady || !!view.busy} onclick={() => session.inspect(route)}>详情</Button><Button variant="outline" disabled={!controlsReady || view.loading || !!reason || !!view.busy} onclick={() => { pending = session.target(route); confirmOpen = pending !== null; }}>关闭会话</Button></div>
        {#if reason}<p class="sample">{reason}</p>{/if}
        {#if view.blockedTargets.some(target => physicalKey(target) === physicalKey(route))}<p class="notice">原关闭结果未确认；只核对原会话，不自动重复关闭。</p>{/if}
      </article>
    {/each}
  {/if}
  {#if view.blockedTargets.length}
    <article aria-label="原 Packet 关闭屏障"><h3>在途或未确认的原目标（{view.blockedTargets.length}）</h3><p class="sample">最多128个物理目标；跨页、返回页面、宿主重建及配置切回均不解除。仅固定原操作的权威结束事实及本地账本持久确认可协调；恢复记录不授予新的关闭权限。</p>
      {#each view.blockedTargets as target (physicalKey(target))}
        {@const record = view.snapshot?.closeRecords.find(record => physicalKey(record.target) === physicalKey(target))}
        <div class="barrier"><p class="target">{target.routeId} · 实例 {target.coreInstanceId} · 配置版本 {target.configRevision}</p>
          {#if record}<p class="target">原操作：{record.operationId}</p><p class="sample">{record.recovered ? '从宿主持久记录恢复，仅供核对' : '本宿主已预留的原关闭操作'} · 原配置 {record.originProfileId ?? '未提供'} · 原客户端代次 {record.originClientScopeGeneration} · 发起于 {packetTime(record.reservedAtUnixMs)}</p>{/if}
          {#if view.snapshot?.coreInstanceId !== target.coreInstanceId}<p class="sample">原内核实例已变化；此记录仅作历史，不对当前同名会话执行操作。</p>{/if}
          <Button variant="outline" disabled={!controlsReady || view.stale || view.loading || !!view.busy || view.snapshot?.coreInstanceId !== target.coreInstanceId} onclick={() => session.reconcile(target)}>只读核对原会话</Button>
        </div>
      {/each}
    </article>
  {/if}
  {#if view.snapshot?.closeLedgerRecovery.state !== 'blocked' && (view.snapshot?.closeLedgerFull || view.blockedTargets.length >= 128)}<p class="notice">关闭目标预算已满，暂停新增关闭操作；读取、筛选和翻页仍可用，请先核对原目标。</p>{/if}
  {#if view.detail}<article aria-label="Packet 会话详情"><h3>会话详情</h3><dl><dt>会话 ID</dt><dd>{view.detail.routeId}</dd><dt>内核实例</dt><dd>{view.detail.coreInstanceId}</dd><dt>配置版本</dt><dd>{view.detail.configRevision}</dd><dt>状态</dt><dd>{view.detail.state}</dd><dt>源 / 目的</dt><dd>{view.detail.source} / {view.detail.destination}</dd></dl></article>{/if}
</section>
<Dialog.Root bind:open={confirmOpen}>
  <Dialog.Content class="packet-confirm">
    <Dialog.Header><Dialog.Title>关闭此 Packet 会话？</Dialog.Title><Dialog.Description>只关闭打开确认面板时的会话 ID；端点启用意图及其他 Flow 保持不变。应用可能建立新会话。</Dialog.Description></Dialog.Header>
    <Dialog.Body><p class="target">{pending?.routeId}</p><p>实例：{pending?.coreInstanceId}<br/>配置版本：{pending?.configRevision}</p></Dialog.Body>
    <Dialog.Footer><Button variant="outline" onclick={() => { confirmOpen = false; pending = null; }}>取消</Button><Button disabled={!pending || !controlsReady || view.stale || view.loading || !!view.busy} onclick={() => { const target = pending; confirmOpen = false; pending = null; if (target) void session.close(target); }}>确认关闭</Button></Dialog.Footer>
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
  .barrier {display:grid;gap:8px;border-top:1px solid var(--border);padding-top:8px;}
  :global(.packet-confirm button), :global(.packet-panel button), :global(.packet-panel input) { min-height: 44px; }
  @media (max-width: 360px) { .packet-panel { padding: 12px; } dl { grid-template-columns: 74px minmax(0, 1fr); } }
</style>
