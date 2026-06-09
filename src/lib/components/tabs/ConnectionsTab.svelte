<script lang="ts">
  import { getGuiConnections, getGuiRecentConnections, guiCloseConnection, queryFlows, closeFlow, handleAppError, type FlowInfo } from '$lib/services/core';
  import { store } from '$lib/services/store.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { coreEvents, type ConnectionDelta } from '$lib/services/core-events.svelte';
  import type { GuiConnectionItem } from '$lib/types/gui-api';

  type DisplayConnection = {
    flowId: string;
    source: string;
    destination: string;
    protocol: string;
    bytesUp: number;
    bytesDown: number;
    startedAtUnixMs: number;
    policyTag?: string;
    outboundTag?: string;
    routeMode?: string;
    inboundTag?: string;
    outcome?: string;
    throughputUpBps?: number;
    throughputDownBps?: number;
    updatedAtUnixMs?: number;
    durationMs?: number;
  };

  let connections = $state<DisplayConnection[]>([]);
  let loading = $state(true);
  let closingId = $state<string | null>(null);
  let expandedIds = $state<Set<string>>(new Set());

  function toggleExpand(id: string) {
    const next = new Set(expandedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    expandedIds = next;
  }

  async function refresh() {
    loading = true;
    try {
      connections = await fetchConnections();
    } catch {
      // Silent — keep stale data visible
    } finally {
      loading = false;
    }
  }

  async function fetchConnections(): Promise<DisplayConnection[]> {
    try {
      // Fetch both active and recent connections in parallel
      const [activeResult, recentResult] = await Promise.allSettled([
        getGuiConnections({ limit: 200 }),
        getGuiRecentConnections({ limit: 50 }),
      ]);

      const activeItems = activeResult.status === 'fulfilled'
        ? activeResult.value.items.map(mapGuiConnection)
        : [];

      const recentItems = recentResult.status === 'fulfilled'
        ? recentResult.value.items
            .filter(r => !activeItems.some(a => a.flowId === r.flowId))
            .map(mapGuiConnection)
        : [];

      // Deduplicate: active connections take priority
      const all = [...activeItems, ...recentItems];
      if (all.length > 0) return all;

      // If both failed with mode_restricted, fallback to raw IPC
      if (activeResult.status === 'rejected') {
        const appError = activeResult.reason as { code?: string };
        if (appError.code === 'mode_restricted') {
          const flows = await queryFlows();
          return flows.map(mapFlowInfo);
        }
        throw activeResult.reason;
      }

      return [];
    } catch (e) {
      const appError = e as { code?: string };
      if (appError.code === 'mode_restricted') {
        const flows = await queryFlows();
        return flows.map(mapFlowInfo);
      }
      throw e;
    }
  }

  function mapGuiConnection(c: GuiConnectionItem): DisplayConnection {
    return {
      flowId: c.flowId,
      source: c.source ?? '-',
      destination: c.destination,
      protocol: c.network,
      bytesUp: c.bytesUp,
      bytesDown: c.bytesDown,
      startedAtUnixMs: c.startedAtUnixMs ?? Date.now(),
      policyTag: c.policyTag,
      outboundTag: c.outboundTag,
      routeMode: c.routeMode,
      inboundTag: c.inboundTag,
      outcome: c.outcome,
      throughputUpBps: c.throughputUpBps,
      throughputDownBps: c.throughputDownBps,
      updatedAtUnixMs: c.updatedAtUnixMs,
      durationMs: c.durationMs,
    };
  }

  function mapFlowInfo(f: FlowInfo): DisplayConnection {
    return {
      flowId: f.flowId,
      source: f.source,
      destination: f.destination,
      protocol: f.protocol,
      bytesUp: f.bytesUp,
      bytesDown: f.bytesDown,
      startedAtUnixMs: f.startedAtUnixMs,
    };
  }

  async function handleClose(flowId: string) {
    closingId = flowId;
    try {
      try {
        await guiCloseConnection(flowId);
      } catch (e) {
        const appError = e as { code?: string };
        if (appError.code === 'mode_restricted') {
          await closeFlow(flowId);
        } else {
          throw e;
        }
      }
      connections = connections.filter(c => c.flowId !== flowId);
    } catch (e) {
      handleAppError(e, '关闭连接失败');
    } finally {
      closingId = null;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  function formatDuration(startedAtMs: number): string {
    const elapsed = Date.now() - startedAtMs;
    if (elapsed < 0) return '0s';
    const sec = Math.floor(elapsed / 1000);
    if (sec < 60) return `${sec}s`;
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ${sec % 60}s`;
    const hr = Math.floor(min / 60);
    return `${hr}h ${min % 60}m`;
  }

  function modeLabel(mode?: string): string {
    switch (mode) {
      case 'global': return '全局';
      case 'rule': return '规则';
      case 'direct': return '直连';
      default: return mode ?? '-';
    }
  }

  // 挂载：加载初始连接列表
  $effect(() => {
    refresh();
  });

  // 事件流重新订阅后对账（弥补断连期间丢失的事件）
  let _prevSubscribed = false;
  $effect(() => {
    const sub = coreEvents.status === 'subscribed';
    if (sub && !_prevSubscribed) {
      refresh();
    }
    _prevSubscribed = sub;
  });

  // 实时增量更新（来自内核事件流）
  $effect(() => {
    const seq = coreEvents.deltaSeq;
    if (seq === 0) return;

    const deltas = coreEvents.drainDeltas();
    if (deltas.length === 0) return;

    applyDeltas(deltas);
  });

  function applyDeltas(deltas: ConnectionDelta[]) {
    const addMap = new Map<string, DisplayConnection>();
    const updateMap = new Map<string, Partial<DisplayConnection>>();
    const removeSet = new Set<string>();

    for (const delta of deltas) {
      switch (delta.type) {
        case 'started':
          addMap.set(delta.connection.flowId, mapGuiConnection(delta.connection));
          removeSet.delete(delta.connection.flowId);
          updateMap.delete(delta.connection.flowId);
          break;
        case 'updated':
          if (!removeSet.has(delta.connection.flowId) && !addMap.has(delta.connection.flowId)) {
            const u = delta.connection;
            updateMap.set(delta.connection.flowId, {
              bytesUp: u.bytesUp,
              bytesDown: u.bytesDown,
              throughputUpBps: u.throughputUpBps,
              throughputDownBps: u.throughputDownBps,
              updatedAtUnixMs: u.updatedAtUnixMs ?? Date.now(),
              durationMs: u.durationMs,
              outcome: u.outcome,
              routeMode: u.routeMode,
            });
          }
          break;
        case 'closed':
          removeSet.add(delta.flowId);
          addMap.delete(delta.flowId);
          updateMap.delete(delta.flowId);
          break;
      }
    }

    if (addMap.size === 0 && updateMap.size === 0 && removeSet.size === 0) return;

    connections = [
      ...Array.from(addMap.values()),
      ...connections
        .filter(c => !removeSet.has(c.flowId) && !addMap.has(c.flowId))
        .map(c => {
          const update = updateMap.get(c.flowId);
          if (!update) return c;
          return {
            ...c,
            bytesUp: update.bytesUp ?? c.bytesUp,
            bytesDown: update.bytesDown ?? c.bytesDown,
            throughputUpBps: update.throughputUpBps ?? c.throughputUpBps,
            throughputDownBps: update.throughputDownBps ?? c.throughputDownBps,
            updatedAtUnixMs: update.updatedAtUnixMs ?? c.updatedAtUnixMs,
            durationMs: update.durationMs ?? c.durationMs,
            outcome: update.outcome ?? c.outcome,
            routeMode: update.routeMode ?? c.routeMode,
          };
        }),
    ];

    // 清理已移除连接的展开状态
    if (removeSet.size > 0) {
      const nextExpanded = new Set([...expandedIds].filter(id => !removeSet.has(id)));
      if (nextExpanded.size !== expandedIds.size) {
        expandedIds = nextExpanded;
      }
    }
  }
</script>

<div class="desk-card flex-1 overflow-hidden flex flex-col animate-fade-in">
  <!-- Panel header -->
  <div class="panel-header">
    <div class="panel-title-row">
      <span class="panel-title">连接</span>
      <span class="count-badge">{connections.length} 个</span>
    </div>
    <button class="action-btn" onclick={refresh}>
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
      </svg>
      刷新
    </button>
  </div>

  <!-- Content -->
  {#if loading && connections.length === 0}
    <div class="panel-empty">加载中...</div>
  {:else if connections.length === 0}
    <div class="panel-empty-block">
      <span class="empty-title">无连接</span>
      <span class="empty-desc">内核未运行或暂无流量</span>
    </div>
  {:else}
    <div class="list-scroll">
      {#each connections as conn (conn.flowId)}
        <div class="flow-group" class:expanded={expandedIds.has(conn.flowId)}>
          <div class="flow-row" onclick={() => toggleExpand(conn.flowId)} onkeydown={(e) => e.key === 'Enter' && toggleExpand(conn.flowId)} role="button" tabindex="0">
            <div class="flow-main">
              <div class="flow-top">
                <span class="flow-id">{conn.flowId}</span>
                <span class="row-tag flow-protocol">{conn.protocol}</span>
                {#if conn.policyTag}
                  <span class="row-tag flow-policy">{conn.policyTag}</span>
                {/if}
                {#if conn.routeMode}
                  <span class="row-tag flow-route-mode">{modeLabel(conn.routeMode)}</span>
                {/if}
              </div>
              <div class="flow-route">
                <span class="flow-src">{conn.source}</span>
                <span class="flow-arrow">→</span>
                <span class="flow-dst">{conn.destination}</span>
                {#if conn.outboundTag}
                  <span class="flow-outbound">{conn.outboundTag}</span>
                {/if}
              </div>
              <div class="flow-stats">
              <span class="flow-stat up">↑ {formatBytes(conn.bytesUp)}</span>
              <span class="flow-stat down">↓ {formatBytes(conn.bytesDown)}</span>
              <span class="flow-dur">{formatDuration(conn.startedAtUnixMs)}</span>
            </div>
            <svg width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="expand-chevron" class:expanded={expandedIds.has(conn.flowId)}>
              <polyline points="3 5 7 9 11 5"/>
            </svg>
          </div>

          {#if store.isActionOperable('core.flow.close')}
            <button
              class="flow-close"
              onclick={() => handleClose(conn.flowId)}
              disabled={closingId === conn.flowId}
              title="关闭连接"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
                <line x1="2" y1="2" x2="10" y2="10"/><line x1="10" y1="2" x2="2" y2="10"/>
              </svg>
            </button>
            {/if}
          </div>

          <!-- Expanded detail -->
          {#if expandedIds.has(conn.flowId)}
            <div class="flow-detail">
              <div class="detail-grid">
                {#if conn.inboundTag}
                  <div class="detail-item">
                    <span class="detail-key">入口</span>
                    <span class="detail-val">{conn.inboundTag}</span>
                  </div>
                {/if}
                {#if conn.outboundTag}
                  <div class="detail-item">
                    <span class="detail-key">出口</span>
                    <span class="detail-val">{conn.outboundTag}</span>
                  </div>
                {/if}
                {#if conn.policyTag}
                  <div class="detail-item">
                    <span class="detail-key">策略</span>
                    <span class="detail-val">{conn.policyTag}</span>
                  </div>
                {/if}
                {#if conn.routeMode}
                  <div class="detail-item">
                    <span class="detail-key">路由</span>
                    <span class="detail-val">{modeLabel(conn.routeMode)}</span>
                  </div>
                {/if}
                {#if conn.outcome}
                  <div class="detail-item">
                    <span class="detail-key">结果</span>
                    <span class="detail-val">{conn.outcome}</span>
                  </div>
                {/if}
                {#if conn.throughputDownBps !== undefined}
                  <div class="detail-item">
                    <span class="detail-key">下行速率</span>
                    <span class="detail-val">{formatBytes(conn.throughputDownBps)}/s</span>
                  </div>
                {/if}
                {#if conn.throughputUpBps !== undefined}
                  <div class="detail-item">
                    <span class="detail-key">上行速率</span>
                    <span class="detail-val">{formatBytes(conn.throughputUpBps)}/s</span>
                  </div>
                {/if}
                {#if conn.durationMs !== undefined}
                  <div class="detail-item">
                    <span class="detail-key">持续时间</span>
                    <span class="detail-val">{formatDuration(conn.startedAtUnixMs)}</span>
                  </div>
                {/if}
                {#if conn.updatedAtUnixMs}
                  <div class="detail-item">
                    <span class="detail-key">最后更新</span>
                    <span class="detail-val">{new Date(conn.updatedAtUnixMs).toLocaleTimeString('zh-CN', { hour12: false })}</span>
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 11px 14px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .panel-title-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .panel-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
    letter-spacing: -0.01em;
  }

  .count-badge {
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--muted);
    color: var(--muted-foreground);
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border-radius: 7px;
    font-size: 12px;
    font-weight: 500;
    background: var(--muted);
    color: var(--foreground);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: background 0.12s ease;
  }

  .action-btn:hover { background: var(--surface); }

  .panel-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .panel-empty-block {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 28px;
  }

  .empty-title { font-size: 12px; color: var(--muted-foreground); }

  .empty-desc {
    font-size: 12px;
    color: var(--muted-foreground);
    opacity: 0.6;
  }

  .list-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 5px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
  }

  .flow-group {
    border-radius: 8px;
    overflow: hidden;
  }

  .flow-group.expanded {
    background: var(--muted);
    border: 1px solid var(--border);
  }

  .flow-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 11px;
    cursor: pointer;
    transition: background 0.12s ease;
  }

  .flow-group.expanded .flow-row {
    border-bottom: 1px solid var(--border);
  }

  .flow-row:hover {
    background: var(--muted);
  }

  .flow-group.expanded .flow-row:hover {
    background: transparent;
  }

  .flow-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .flow-top {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .flow-id {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
    font-family: var(--font-mono);
  }

  .row-tag {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: 4px;
  }

  .flow-protocol {
    text-transform: uppercase;
    background: var(--muted);
    color: var(--muted-foreground);
  }

  .flow-policy {
    background: rgba(168, 85, 247, 0.1);
    color: #A855F7;
  }

  .flow-route-mode {
    background: rgba(59, 130, 246, 0.1);
    color: #3B82F6;
  }

  .flow-route {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--muted-foreground);
    overflow: hidden;
  }

  .flow-src, .flow-dst {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .flow-src { max-width: min(200px, 35%); }
  .flow-dst { max-width: min(240px, 45%); }

  .flow-arrow {
    flex-shrink: 0;
    font-size: 12px;
    opacity: 0.4;
    padding: 0 1px;
  }

  .flow-outbound {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.6;
    margin-left: 4px;
  }

  .flow-stats {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }

  .flow-stat {
    font-weight: 500;
    font-family: var(--font-mono);
  }

  .flow-stat.up { color: rgba(34, 197, 94, 0.85); }
  .flow-stat.down { color: rgba(59, 130, 246, 0.85); }

  .flow-dur {
    color: var(--muted-foreground);
    opacity: 0.6;
    font-family: var(--font-mono);
  }

  .flow-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    border: none;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s ease, background 0.12s ease, color 0.12s ease;
    flex-shrink: 0;
    margin-top: 2px;
  }

  .flow-row:hover .flow-close { opacity: 1; }

  .flow-close:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--destructive);
  }

  .flow-close:disabled { opacity: 0.3; cursor: not-allowed; }

  .expand-chevron {
    margin-top: 2px;
    flex-shrink: 0;
    opacity: 0.4;
    transition: transform 0.2s ease, opacity 0.12s ease;
  }

  .flow-row:hover .expand-chevron { opacity: 0.7; }
  .expand-chevron.expanded { transform: rotate(180deg); opacity: 0.7; }

  /* ---- Detail panel ---- */
  .flow-detail {
    padding: 10px 14px 12px;
  }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 6px 16px;
  }

  .detail-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .detail-key {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted-foreground);
    opacity: 0.7;
  }

  .detail-val {
    font-size: 12px;
    font-weight: 500;
    color: var(--foreground);
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
