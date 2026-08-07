<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getAppErrorMessage,
    getGuiConnections,
    getGuiDebugFrames,
    getGuiRecentConnections,
    guiCloseConnection,
    queryCore,
    queryFlows,
    closeFlow,
    handleAppError,
    type FlowInfo,
  } from '$lib/services/core';
  import { store } from '$lib/services/store.svelte';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { buildConnectionView, type DisplayConnection } from '$lib/services/connection-view';
  import {
    attachConnectionWireMetadata,
    buildConnectionWireIndex,
    type ConnectionWireIndex,
  } from '$lib/services/connection-wire';
  import type { GuiConnectionItem } from '$lib/types/gui-api';
  import ConnectionWireDetails from '$lib/components/ConnectionWireDetails.svelte';
  import { RefreshCw, Search } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Tabs from '$lib/components/AppTabs';

  const MAX_CONNECTIONS = 500;
  const MAX_RENDER = 120;
  const RECONCILE_INTERVAL_MS = 5_000;

  let activeSnapshot = $state<GuiConnectionItem[]>([]);
  let recentSnapshot = $state<GuiConnectionItem[]>([]);
  let wireIndex = $state<ConnectionWireIndex>({});
  let loading = $state(true);
  let refreshing = $state(false);
  let closingId = $state<string | null>(null);
  let expandedIds = $state<Set<string>>(new Set());
  let suppressedActiveIds = $state<Set<string>>(new Set());
  let activeTab = $state<'live' | 'history'>('live');
  let searchQuery = $state('');
  let flowSupported = $state(true);
  let historySupported = $state(true);
  let loadError = $state<string | null>(null);
  let partialError = $state<string | null>(null);
  let refreshGeneration = 0;
  let now = $state(Date.now());

  const connectionView = $derived(buildConnectionView({
    activeSnapshot: activeSnapshot.map((connection) => attachConnectionWireMetadata(connection, wireIndex)),
    recentSnapshot: recentSnapshot.map((connection) => attachConnectionWireMetadata(connection, wireIndex)),
    activeEvents: coreEvents.activeConnections.map((connection) => attachConnectionWireMetadata(connection, wireIndex)),
    recentEvents: coreEvents.connectionHistory.map((connection) => attachConnectionWireMetadata(connection, wireIndex)),
    limit: MAX_CONNECTIONS,
  }));
  const activeConnections = $derived(
    connectionView.active.filter((connection) => !suppressedActiveIds.has(connection.flowId)),
  );
  const connections = $derived([
    ...activeConnections,
    ...connectionView.recent,
  ]);

  function matchesSearch(connection: DisplayConnection): boolean {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return true;
    return (
      connection.destination.toLowerCase().includes(query)
      || connection.source.toLowerCase().includes(query)
      || connection.flowId.toLowerCase().includes(query)
      || (connection.policyTag?.toLowerCase().includes(query) ?? false)
      || (connection.outboundTag?.toLowerCase().includes(query) ?? false)
      || (connection.processName?.toLowerCase().includes(query) ?? false)
      || (connection.processPath?.toLowerCase().includes(query) ?? false)
      || (connection.matchedRule?.toLowerCase().includes(query) ?? false)
      || (connection.remoteDestination?.toLowerCase().includes(query) ?? false)
      || (connection.eventType?.toLowerCase().includes(query) ?? false)
      || connection.selectionChain.some((item) => item.toLowerCase().includes(query))
      || connection.relayChain.some((item) => item.toLowerCase().includes(query))
    );
  }

  const tabConnections = $derived(
    connections
      .filter((connection) => (activeTab === 'live' ? connection.origin === 'active' : connection.origin === 'recent'))
      .filter(matchesSearch),
  );
  const visibleConnections = $derived(tabConnections.slice(0, MAX_RENDER));
  const liveCount = $derived(activeConnections.length);
  const historyCount = $derived(connectionView.recent.length);

  function toggleExpand(flowId: string) {
    const next = new Set(expandedIds);
    if (next.has(flowId)) next.delete(flowId);
    else next.add(flowId);
    expandedIds = next;
  }

  async function refresh(showLoading = connections.length === 0, includeWireQueries = showLoading) {
    if (refreshing) return;
    const generation = ++refreshGeneration;
    refreshing = true;
    if (showLoading) loading = true;

    try {
      const [activeResult, recentResult, wireResult] = await Promise.allSettled([
        loadActiveConnections(),
        getGuiRecentConnections({ limit: MAX_CONNECTIONS }),
        loadWireIndex(includeWireQueries),
      ]);
      if (generation !== refreshGeneration) return;

      const errors: string[] = [];
      let successCount = 0;

      if (activeResult.status === 'fulfilled') {
        activeSnapshot = activeResult.value;
        flowSupported = true;
        successCount++;
      } else if (isUnsupportedError(activeResult.reason)) {
        flowSupported = false;
      } else {
        errors.push(`实时连接：${getAppErrorMessage(activeResult.reason, '查询失败')}`);
      }

      if (recentResult.status === 'fulfilled') {
        recentSnapshot = recentResult.value.items;
        historySupported = true;
        successCount++;
      } else if (isUnsupportedError(recentResult.reason) || isModeRestricted(recentResult.reason)) {
        historySupported = false;
      } else {
        errors.push(`连接记录：${getAppErrorMessage(recentResult.reason, '查询失败')}`);
      }

      if (wireResult.status === 'fulfilled') {
        wireIndex = mergeWireIndexes(wireIndex, wireResult.value);
      }

      if (successCount === 0 && errors.length > 0 && connections.length === 0) {
        loadError = errors.join('；');
        partialError = null;
      } else {
        loadError = null;
        partialError = errors.length > 0 ? `部分连接数据未能加载：${errors.join('；')}` : null;
      }
    } finally {
      if (generation === refreshGeneration) {
        refreshing = false;
        if (showLoading) loading = false;
      }
    }
  }

  async function loadActiveConnections(): Promise<GuiConnectionItem[]> {
    try {
      return (await getGuiConnections({ limit: MAX_CONNECTIONS })).items;
    } catch (error) {
      if (!isModeRestricted(error)) throw error;
      return (await queryFlows()).map(mapFlowInfo);
    }
  }

  async function loadWireIndex(includeQueries: boolean): Promise<ConnectionWireIndex> {
    const activeRawPromise = includeQueries
      ? queryCore({ active_flows: { limit: MAX_CONNECTIONS, filter: {} } })
      : Promise.resolve(undefined);
    const recentRawPromise = includeQueries
      ? queryCore({ recent_flows: { limit: MAX_CONNECTIONS, filter: {} } })
      : Promise.resolve(undefined);

    const [activeRaw, recentRaw, eventFrames] = await Promise.allSettled([
      activeRawPromise,
      recentRawPromise,
      getGuiDebugFrames({ frameType: 'event', limit: MAX_CONNECTIONS }),
    ]);

    return buildConnectionWireIndex({
      activeResponse: activeRaw.status === 'fulfilled' ? activeRaw.value : undefined,
      recentResponse: recentRaw.status === 'fulfilled' ? recentRaw.value : undefined,
      eventFrames: eventFrames.status === 'fulfilled' ? eventFrames.value.items : [],
    });
  }

  function mergeWireIndexes(
    current: ConnectionWireIndex,
    incoming: ConnectionWireIndex,
  ): ConnectionWireIndex {
    const merged: ConnectionWireIndex = { ...current };
    for (const [flowId, records] of Object.entries(incoming)) {
      merged[flowId] = [...(merged[flowId] ?? []), ...records].slice(-20);
    }
    return merged;
  }

  function mapFlowInfo(flow: FlowInfo): GuiConnectionItem {
    return {
      flowId: flow.flowId,
      source: flow.source,
      destination: flow.destination,
      network: flow.protocol,
      bytesUp: flow.bytesUp,
      bytesDown: flow.bytesDown,
      startedAtUnixMs: flow.startedAtUnixMs,
      selectionChain: [],
      relayChain: [],
    };
  }

  function isModeRestricted(error: unknown): boolean {
    return (error as { code?: string })?.code === 'mode_restricted';
  }

  function isUnsupportedError(error: unknown): boolean {
    const appError = error as { code?: string; message?: string };
    return appError?.code === 'unsupported'
      || appError?.code === 'not_supported'
      || /(?:not supported|unsupported|unknown.*(?:active_flows|recent_flows|flow))/i.test(appError?.message ?? '');
  }

  async function handleClose(flowId: string) {
    if (closingId !== null) return;
    closingId = flowId;
    try {
      try {
        await guiCloseConnection(flowId);
      } catch (error) {
        if (isModeRestricted(error)) await closeFlow(flowId);
        else throw error;
      }
      suppressedActiveIds = new Set([...suppressedActiveIds, flowId]);
      activeSnapshot = activeSnapshot.filter((connection) => connection.flowId !== flowId);
      void refresh(false, false);
    } catch (error) {
      handleAppError(error, '关闭连接失败');
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

  function formatDuration(startedAtMs?: number, durationMs?: number): string {
    if (durationMs === undefined && startedAtMs === undefined) return '-';
    const elapsed = Math.max(0, durationMs ?? (now - (startedAtMs ?? now)));
    const sec = Math.floor(elapsed / 1000);
    if (sec < 60) return `${sec}s`;
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ${sec % 60}s`;
    const hr = Math.floor(min / 60);
    return `${hr}h ${min % 60}m`;
  }

  function connectionOccurredAt(connection: DisplayConnection): number | undefined {
    if (connection.origin === 'recent') {
      return connection.endedAtUnixMs
        ?? connection.eventOccurredAtUnixMs
        ?? connection.updatedAtUnixMs
        ?? connection.startedAtUnixMs;
    }
    return connection.startedAtUnixMs
      ?? connection.eventOccurredAtUnixMs
      ?? connection.updatedAtUnixMs;
  }

  function formatListTimestamp(timestamp?: number): string {
    if (timestamp === undefined) return '-';
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return '-';
    return date.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  }

  function modeLabel(mode?: string): string {
    switch (mode) {
      case 'global': return '全局';
      case 'rule': return '规则';
      case 'direct': return '直连';
      default: return mode ?? '-';
    }
  }

  $effect(() => {
    const visibleIds = new Set(connections.map((connection) => connection.flowId));
    const nextExpanded = new Set([...expandedIds].filter((flowId) => visibleIds.has(flowId)));
    if (nextExpanded.size !== expandedIds.size) expandedIds = nextExpanded;

    const activeIds = new Set(connectionView.active.map((connection) => connection.flowId));
    const nextSuppressed = new Set([...suppressedActiveIds].filter((flowId) => activeIds.has(flowId)));
    if (nextSuppressed.size !== suppressedActiveIds.size) suppressedActiveIds = nextSuppressed;
  });

  onMount(() => {
    void refresh(true, true);
    const clockTimer = window.setInterval(() => {
      now = Date.now();
    }, 1_000);
    const reconcileTimer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh(false, false);
    }, RECONCILE_INTERVAL_MS);

    return () => {
      window.clearInterval(clockTimer);
      window.clearInterval(reconcileTimer);
    };
  });
</script>

<Tabs.Root bind:value={activeTab} class="desk-card flex-1 overflow-hidden flex flex-col gap-0 animate-fade-in">
 <!-- Panel header -->
 <div class="panel-header">
   <div class="panel-title-row">
     <span class="panel-title">连接</span>
     <span class="count-badge">{activeTab === 'live' ? liveCount : historyCount} 个</span>
   </div>
   <!-- Tab switcher: live connections vs connection history -->
   <Tabs.List class="tab-switcher" aria-label="连接数据范围">
     <Tabs.Trigger class="tab-btn" value="live">实时连接</Tabs.Trigger>
     <Tabs.Trigger class="tab-btn" value="history">连接记录</Tabs.Trigger>
   </Tabs.List>
   <div class="header-actions">
     <Button size="sm" onclick={() => refresh(false, true)} disabled={refreshing}>
       <RefreshCw class={refreshing ? 'animate-spin' : undefined} />
       {refreshing ? '刷新中...' : '刷新'}
     </Button>
   </div>
 </div>

 <!-- Search / filter -->
 <div class="search-bar">
   <div class="search-field">
     <span class="search-icon" aria-hidden="true">
       <Search size={14} strokeWidth={1.7} />
     </span>
     <input
       class="search-input"
       type="search"
       aria-label="搜索连接"
       placeholder="搜索地址、来源、标签或事件类型"
       bind:value={searchQuery}
     >
   </div>
 </div>

 {#if partialError}
   <div class="connection-warning" role="status">
     <span>{partialError}</span>
     <Button variant="outline" size="xs" onclick={() => refresh(false, true)}>重试</Button>
   </div>
 {/if}
 {#if activeTab === 'history' && !historySupported}
   <div class="connection-warning" role="status">
     <span>当前模式无法查询内核连接记录，暂时仅显示本次 GUI 会话收到的结束事件。</span>
   </div>
 {/if}
 {#if loadError && connections.length > 0}
   <div class="connection-warning error" role="alert">
     <span>刷新失败，当前仍显示上一批数据：{loadError}</span>
     <Button variant="outline" size="xs" onclick={() => refresh(false, true)}>重试</Button>
   </div>
 {/if}

 <!-- Content -->
 {#if loading && connections.length === 0}
   <div class="panel-empty">加载中...</div>
 {:else if loadError && connections.length === 0}
   <div class="panel-empty-block" role="alert">
     <span class="empty-title error-text">连接数据加载失败</span>
     <span class="empty-desc">{loadError}</span>
     <Button variant="outline" size="xs" onclick={() => refresh(true, true)}>重试</Button>
   </div>
 {:else if activeTab === 'live' && !flowSupported}
   <div class="panel-empty-block">
     <span class="empty-title">内核不支持实时连接</span>
     <span class="empty-desc">当前内核未声明 active_flows 能力</span>
   </div>
 {:else if connections.length === 0}
   <div class="panel-empty-block">
     <span class="empty-title">无连接</span>
     <span class="empty-desc">内核未运行或暂无流量</span>
   </div>
 {:else if tabConnections.length === 0}
   <div class="panel-empty-block">
     <span class="empty-title">{searchQuery ? '无匹配结果' : '无记录'}</span>
     <span class="empty-desc">{searchQuery ? '尝试更换搜索关键词' : (activeTab === 'live' ? '内核未运行或暂无活跃连接' : '暂无连接记录')}</span>
   </div>
 {:else}
   <div class="list-scroll">
     {#each visibleConnections as conn (conn.flowId)}
        <div class="flow-group" class:expanded={expandedIds.has(conn.flowId)}>
          <div class="flow-row">
            <button type="button" class="flow-open" onclick={() => toggleExpand(conn.flowId)}>
            <div class="flow-main">
              <div class="flow-top">
                <span class="flow-destination" title={conn.destination}>{conn.destination}</span>
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
                {#if conn.outboundTag}
                  <span class="flow-arrow">→</span>
                  <span class="flow-outbound">{conn.outboundTag}</span>
                {/if}
                <span class="flow-id-minor">#{conn.flowId}</span>
              </div>
              <div class="flow-stats">
                <span class="flow-stat up">↑ {formatBytes(conn.bytesUp)}</span>
                <span class="flow-stat down">↓ {formatBytes(conn.bytesDown)}</span>
                <span class="flow-dur">{formatDuration(conn.startedAtUnixMs, conn.durationMs)}</span>
                <span class="flow-occurred">{formatListTimestamp(connectionOccurredAt(conn))}</span>
              </div>
              <svg width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="expand-chevron" class:expanded={expandedIds.has(conn.flowId)}>
                <polyline points="3 5 7 9 11 5"/>
              </svg>
            </div>
            </button>

            {#if conn.origin === 'active' && store.isActionOperable('core.flow.close')}
              <button
                class="flow-close"
                onclick={(event) => {
                  event.stopPropagation();
                  handleClose(conn.flowId);
                }}
                disabled={closingId !== null}
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
                <div class="detail-item">
                  <span class="detail-key">来源</span>
                  <span class="detail-val" title={conn.source}>{conn.source}</span>
                </div>
                {#if conn.state}
                  <div class="detail-item">
                    <span class="detail-key">状态</span>
                    <span class="detail-val">{conn.state}</span>
                  </div>
                {/if}
                {#if conn.revision !== undefined}
                  <div class="detail-item">
                    <span class="detail-key">Revision</span>
                    <span class="detail-val">{conn.revision}</span>
                  </div>
                {/if}
                {#if conn.processName || conn.processPath || conn.processId}
                  <div class="detail-item">
                    <span class="detail-key">进程</span>
                    <span class="detail-val" title={conn.processPath}>{conn.processName ?? conn.processPath ?? `PID ${conn.processId}`}</span>
                  </div>
                {/if}
                {#if conn.targetHost || conn.targetIp}
                  <div class="detail-item">
                    <span class="detail-key">目标</span>
                    <span class="detail-val" title={conn.targetHost}>{conn.targetHost ?? conn.destination}{conn.targetIp && conn.targetIp !== conn.targetHost ? ` → ${conn.targetIp}` : ''}</span>
                  </div>
                {/if}
                {#if conn.sniffedHost}
                  <div class="detail-item">
                    <span class="detail-key">嗅探域名</span>
                    <span class="detail-val" title={conn.sniffedHost}>{conn.sniffedHost}</span>
                  </div>
                {/if}
                {#if conn.inboundTag}
                  <div class="detail-item">
                    <span class="detail-key">入口</span>
                    <span class="detail-val">{conn.inboundTag}{conn.inboundProtocol ? ` · ${conn.inboundProtocol}` : ''}</span>
                  </div>
                {/if}
                {#if conn.outboundTag}
                  <div class="detail-item">
                    <span class="detail-key">出口</span>
                    <span class="detail-val">{conn.outboundTag}{conn.outboundProtocol ? ` · ${conn.outboundProtocol}` : ''}</span>
                  </div>
                {/if}
                {#if conn.remoteDestination}
                  <div class="detail-item">
                    <span class="detail-key">实际远端</span>
                    <span class="detail-val">{conn.remoteDestination}</span>
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
                    <span class="detail-val">{modeLabel(conn.routeMode)}{conn.routeAction ? ` · ${conn.routeAction}` : ''}</span>
                  </div>
                {/if}
                {#if conn.matchedRule}
                  <div class="detail-item detail-wide">
                    <span class="detail-key">命中规则{conn.matchedRuleIndex !== undefined ? ` #${conn.matchedRuleIndex}` : ''}</span>
                    <span class="detail-val" title={conn.matchedRule}>{conn.matchedRule}</span>
                  </div>
                {/if}
                {#if conn.selectionChain.length > 0}
                  <div class="detail-item detail-wide">
                    <span class="detail-key">选择链</span>
                    <span class="detail-val" title={conn.selectionChain.join(' → ')}>{conn.selectionChain.join(' → ')}</span>
                  </div>
                {/if}
                {#if conn.relayChain.length > 0}
                  <div class="detail-item detail-wide">
                    <span class="detail-key">中继链</span>
                    <span class="detail-val" title={conn.relayChain.join(' → ')}>{conn.relayChain.join(' → ')}</span>
                  </div>
                {/if}
                {#if conn.outcome}
                  <div class="detail-item">
                    <span class="detail-key">结果</span>
                    <span class="detail-val">{conn.outcome}{conn.closeReason ? ` · ${conn.closeReason}` : ''}</span>
                  </div>
                {/if}
                {#if conn.failureMessage}
                  <div class="detail-item detail-wide failure-detail">
                    <span class="detail-key">失败{conn.failureStage ? ` · ${conn.failureStage}` : ''}{conn.failureCode ? ` · ${conn.failureCode}` : ''}</span>
                    <span class="detail-val" title={conn.failureMessage}>{conn.failureMessage}</span>
                  </div>
                {/if}
                {#if conn.inboundRxBytes !== undefined || conn.outboundTxBytes !== undefined}
                  <div class="detail-item detail-wide">
                    <span class="detail-key">方向流量</span>
                    <span class="detail-val">入站收 {formatBytes(conn.inboundRxBytes ?? 0)} · 出站发 {formatBytes(conn.outboundTxBytes ?? 0)} · 出站收 {formatBytes(conn.outboundRxBytes ?? 0)} · 入站发 {formatBytes(conn.inboundTxBytes ?? 0)}</span>
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
                    <span class="detail-val">{formatDuration(conn.startedAtUnixMs, conn.durationMs)}</span>
                  </div>
                {/if}
                <ConnectionWireDetails connection={conn} />
              </div>
            </div>
          {/if}
        </div>
     {/each}
     {#if tabConnections.length > visibleConnections.length}
       <div class="list-truncated">仅显示前 {visibleConnections.length} / {tabConnections.length} 条，请使用搜索缩小范围</div>
     {/if}
   </div>
 {/if}
</Tabs.Root>

<style>
  .panel-header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    column-gap: 12px;
    padding: 11px 14px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .panel-title-row {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
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

  .header-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 0;
  }

 /* ---- Tab switcher ---- */
  :global(.tab-switcher) {
   justify-self: center;
   white-space: nowrap;
    height: 36px;
 }

 :global(.tab-btn) {
   min-width: 76px;
   font-size: 12px;
 }

 /* ---- Search bar ---- */
 .search-bar {
   padding: 7px 14px;
   border-bottom: 1px solid var(--border);
   flex-shrink: 0;
 }

 .search-field {
   position: relative;
   display: flex;
   align-items: center;
   width: 100%;
   min-width: 0;
 }

  .search-icon {
   position: absolute;
    left: 9px;
   top: 50%;
   width: 14px;
   height: 14px;
   display: inline-flex;
   align-items: center;
   justify-content: center;
   transform: translateY(-50%);
   color: var(--muted-foreground);
   opacity: 0.55;
   pointer-events: none;
 }

  .search-input {
   width: 100%;
   min-width: 0;
    height: var(--control-height);
    padding: 0 10px 0 30px;
    border: 1px solid var(--input);
    border-radius: var(--control-radius);
    background: var(--background);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
   font-size: 12px;
    line-height: var(--control-height);
   color: var(--foreground);
   outline: none;
    appearance: none;
  }

  .search-input:focus {
    border-color: var(--ring);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 18%, transparent);
  }

 .search-input::-webkit-search-cancel-button {
   cursor: pointer;
 }

 .search-input::placeholder { color: var(--muted-foreground); opacity: 0.5; }

 .connection-warning {
   flex-shrink: 0;
   display: flex;
   align-items: center;
   justify-content: space-between;
   gap: 8px;
   padding: 6px 14px;
   border-bottom: 1px solid color-mix(in srgb, var(--warning) 20%, var(--border));
   background: color-mix(in srgb, var(--warning) 7%, transparent);
   color: var(--warning);
   font-size: 10.5px;
 }

 .connection-warning.error {
   border-bottom-color: color-mix(in srgb, var(--destructive) 20%, var(--border));
   background: color-mix(in srgb, var(--destructive) 7%, transparent);
   color: var(--destructive);
 }

 /* ---- Truncation footer ---- */
 .list-truncated {
   text-align: center;
   font-size: 11px;
   color: var(--muted-foreground);
   opacity: 0.5;
   padding: 8px;
 }

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

  .error-text { color: var(--destructive); }

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
    flex-shrink: 0;
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
    transition: background 0.12s ease;
  }

  .flow-open {
    flex: 1;
    min-width: 0;
    display: flex;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    outline: none;
  }

  .flow-open:focus-visible {
    border-radius: 6px;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--primary) 32%, transparent);
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

  .flow-destination {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--foreground);
    font-family: var(--font-mono);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .flow-id-minor {
    font-size: 10.5px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    opacity: 0.5;
    flex-shrink: 0;
    margin-left: auto;
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

  .flow-src {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: min(200px, 35%);
  }

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
    flex-wrap: wrap;
  }

  .flow-stat {
    font-weight: 500;
    font-family: var(--font-mono);
  }

  .flow-stat.up { color: rgba(34, 197, 94, 0.85); }
  .flow-stat.down { color: rgba(59, 130, 246, 0.85); }

  .flow-dur,
  .flow-occurred {
    color: var(--muted-foreground);
    opacity: 0.6;
    font-family: var(--font-mono);
  }

  .flow-occurred {
    margin-left: auto;
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

  .detail-wide {
    grid-column: 1 / -1;
  }

  .failure-detail .detail-key,
  .failure-detail .detail-val {
    color: var(--destructive);
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
