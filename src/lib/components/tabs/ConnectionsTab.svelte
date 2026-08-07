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
    mergeConnectionWireIndexes,
    type ConnectionWireIndex,
  } from '$lib/services/connection-wire';
  import type { GuiConnectionItem } from '$lib/types/gui-api';
  import ConnectionDetailsDrawer from '$lib/components/ConnectionDetailsDrawer.svelte';
  import { AlertTriangle, Eye, MoreHorizontal, RefreshCw, Search } from '@lucide/svelte';
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
  let terminatingId = $state<string | null>(null);
  let suppressedActiveIds = $state<Set<string>>(new Set());
  let selectedKey = $state<string | null>(null);
  let actionMenuKey = $state<string | null>(null);
  let terminateConfirmKey = $state<string | null>(null);
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

  function connectionKey(connection: DisplayConnection): string {
    const lifetime = connection.startedAtUnixMs
      ?? connection.endedAtUnixMs
      ?? connection.eventOccurredAtUnixMs
      ?? 0;
    return `${connection.origin}:${connection.flowId}:${lifetime}`;
  }

  const selectedConnection = $derived(
    connections.find((connection) => connectionKey(connection) === selectedKey) ?? null,
  );
  const terminateConfirmConnection = $derived(
    connections.find((connection) => connectionKey(connection) === terminateConfirmKey) ?? null,
  );

  function isNumber(value: unknown): value is number {
    return typeof value === 'number' && Number.isFinite(value);
  }

  function hasText(value: unknown): value is string {
    return typeof value === 'string' && value.trim().length > 0 && value !== '-';
  }

  function matchesSearch(connection: DisplayConnection): boolean {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return true;
    return (
      connection.destination.toLowerCase().includes(query)
      || (hasText(connection.source) && connection.source.toLowerCase().includes(query))
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
        wireIndex = mergeConnectionWireIndexes(wireIndex, wireResult.value);
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

  function requestTerminate(connection: DisplayConnection) {
    actionMenuKey = null;
    terminateConfirmKey = connectionKey(connection);
  }

  async function handleTerminate(connection: DisplayConnection) {
    if (terminatingId !== null || connection.origin !== 'active') return;
    terminatingId = connection.flowId;
    try {
      try {
        await guiCloseConnection(connection.flowId);
      } catch (error) {
        if (isModeRestricted(error)) await closeFlow(connection.flowId);
        else throw error;
      }
      suppressedActiveIds = new Set([...suppressedActiveIds, connection.flowId]);
      activeSnapshot = activeSnapshot.filter((item) => item.flowId !== connection.flowId);
      terminateConfirmKey = null;
      if (selectedKey === connectionKey(connection)) selectedKey = null;
      void refresh(false, false);
    } catch (error) {
      handleAppError(error, '终止连接失败');
    } finally {
      terminatingId = null;
    }
  }

  function openDetails(connection: DisplayConnection) {
    selectedKey = connectionKey(connection);
    actionMenuKey = null;
  }

  function sourceLabel(connection: DisplayConnection): string {
    if (hasText(connection.processName)) return connection.processName;
    if (hasText(connection.source)) return connection.source;
    if (hasText(connection.inboundTag)) return `入口 ${connection.inboundTag}`;
    return '来源未提供';
  }

  function formatBytes(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  function formatDuration(startedAtMs?: number, durationMs?: number): string {
    if (!isNumber(durationMs) && !isNumber(startedAtMs)) return '时长未提供';
    const elapsed = Math.max(0, durationMs ?? (now - (startedAtMs ?? now)));
    const sec = Math.floor(elapsed / 1_000);
    if (sec < 60) return `${sec}s`;
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ${sec % 60}s`;
    const hr = Math.floor(min / 60);
    return `${hr}h ${min % 60}m`;
  }

  function formatRate(value: unknown): string | null {
    return isNumber(value) ? `${formatBytes(value)}/s` : null;
  }

  function listMetric(connection: DisplayConnection, direction: 'up' | 'down'): string {
    const rate = direction === 'up'
      ? formatRate(connection.throughputUpBps)
      : formatRate(connection.throughputDownBps);
    if (connection.origin === 'active' && rate) return rate;
    return formatBytes(direction === 'up' ? connection.bytesUp : connection.bytesDown);
  }

  function connectionTimestamp(connection: DisplayConnection): number | undefined {
    if (connection.origin === 'recent') {
      return connection.endedAtUnixMs
        ?? connection.eventOccurredAtUnixMs
        ?? connection.lastActivityAtUnixMs
        ?? connection.startedAtUnixMs;
    }
    return connection.startedAtUnixMs
      ?? connection.eventOccurredAtUnixMs
      ?? connection.lastActivityAtUnixMs;
  }

  function formatListTimestamp(timestamp?: number): string {
    if (!isNumber(timestamp)) return '时间未提供';
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return '时间未提供';
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
      default: return mode ?? '';
    }
  }

  $effect(() => {
    const keys = new Set(connections.map(connectionKey));
    if (selectedKey && !keys.has(selectedKey)) selectedKey = null;
    if (terminateConfirmKey && !keys.has(terminateConfirmKey)) terminateConfirmKey = null;
    if (actionMenuKey && !keys.has(actionMenuKey)) actionMenuKey = null;

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

<Tabs.Root bind:value={activeTab} class="connections-shell desk-card flex-1 overflow-hidden flex flex-col gap-0 animate-fade-in">
  <div class="panel-header">
    <div class="panel-title-row">
      <span class="panel-title">连接</span>
      <span class="count-badge">{activeTab === 'live' ? liveCount : historyCount}</span>
    </div>
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

  <div class="search-bar">
    <div class="search-field">
      <span class="search-icon" aria-hidden="true"><Search size={14} strokeWidth={1.7} /></span>
      <input class="search-input" type="search" aria-label="搜索连接" placeholder="搜索目标、来源、进程、出口或事件类型" bind:value={searchQuery}>
    </div>
  </div>

  {#if partialError}
    <div class="connection-warning" role="status"><span>{partialError}</span><Button variant="outline" size="xs" onclick={() => refresh(false, true)}>重试</Button></div>
  {/if}
  {#if activeTab === 'history' && !historySupported}
    <div class="connection-warning" role="status"><span>当前模式无法查询内核连接记录，暂时仅显示本次客户端会话收到的结束事件。</span></div>
  {/if}
  {#if loadError && connections.length > 0}
    <div class="connection-warning error" role="alert"><span>刷新失败，当前仍显示上一批数据：{loadError}</span><Button variant="outline" size="xs" onclick={() => refresh(false, true)}>重试</Button></div>
  {/if}

  {#if loading && connections.length === 0}
    <div class="panel-empty">加载中...</div>
  {:else if loadError && connections.length === 0}
    <div class="panel-empty-block" role="alert"><span class="empty-title error-text">连接数据加载失败</span><span class="empty-desc">{loadError}</span><Button variant="outline" size="xs" onclick={() => refresh(true, true)}>重试</Button></div>
  {:else if activeTab === 'live' && !flowSupported}
    <div class="panel-empty-block"><span class="empty-title">内核不支持实时连接</span><span class="empty-desc">当前内核未声明 active_flows 能力</span></div>
  {:else if connections.length === 0}
    <div class="panel-empty-block"><span class="empty-title">无连接</span><span class="empty-desc">内核未运行或暂无流量</span></div>
  {:else if tabConnections.length === 0}
    <div class="panel-empty-block"><span class="empty-title">{searchQuery ? '无匹配结果' : '无记录'}</span><span class="empty-desc">{searchQuery ? '尝试更换搜索关键词' : (activeTab === 'live' ? '暂无活动连接' : '暂无连接记录')}</span></div>
  {:else}
    <div class="list-scroll" onclick={() => actionMenuKey = null}>
      {#each visibleConnections as connection (connectionKey(connection))}
        {@const key = connectionKey(connection)}
        <article class="flow-row">
          <button type="button" class="flow-open" onclick={() => openDetails(connection)}>
            <div class="flow-heading">
              <div class="flow-title">
                <span class="flow-destination" title={connection.destination}>{connection.destination}</span>
                <span class="row-tag flow-protocol">{connection.protocol.toUpperCase()}</span>
                {#if connection.policyTag}<span class="row-tag flow-policy">{connection.policyTag}</span>{/if}
                {#if connection.routeMode}<span class="row-tag flow-route-mode">{modeLabel(connection.routeMode)}</span>{/if}
              </div>
              <span class="flow-time-label">{connection.origin === 'active' ? '开始' : '结束'} {formatListTimestamp(connectionTimestamp(connection))}</span>
            </div>

            <div class="flow-route">
              <span class:missing-source={sourceLabel(connection) === '来源未提供'} class="flow-source">{sourceLabel(connection)}</span>
              {#if connection.outboundTag}<span class="flow-arrow">→</span><span class="flow-outbound">{connection.outboundTag}</span>{/if}
              <span class="flow-id">#{connection.flowId}</span>
            </div>

            <div class="flow-stats">
              <span class="flow-stat up">↑ {listMetric(connection, 'up')}</span>
              <span class="flow-stat down">↓ {listMetric(connection, 'down')}</span>
              <span class="flow-duration">{formatDuration(connection.startedAtUnixMs, connection.durationMs)}</span>
              {#if connection.origin === 'active' && !formatRate(connection.throughputUpBps) && !formatRate(connection.throughputDownBps)}
                <span class="rate-unavailable">速率未提供，显示累计流量</span>
              {/if}
            </div>
          </button>

          {#if connection.origin === 'active' && store.isActionOperable('core.flow.close')}
            <div class="row-actions">
              <button type="button" class="more-button" aria-label="连接操作" title="连接操作" onclick={(event) => { event.stopPropagation(); actionMenuKey = actionMenuKey === key ? null : key; }}><MoreHorizontal size={16} /></button>
              {#if actionMenuKey === key}
                <div class="action-menu" role="menu" onclick={(event) => event.stopPropagation()}>
                  <button type="button" role="menuitem" onclick={() => openDetails(connection)}><Eye size={14} />查看详情</button>
                  <button type="button" class="danger" role="menuitem" onclick={() => requestTerminate(connection)}><AlertTriangle size={14} />终止连接</button>
                </div>
              {/if}
            </div>
          {/if}
        </article>
      {/each}
      {#if tabConnections.length > visibleConnections.length}<div class="list-truncated">仅显示前 {visibleConnections.length} / {tabConnections.length} 条，请使用搜索缩小范围</div>{/if}
    </div>
  {/if}

  <ConnectionDetailsDrawer
    connection={selectedConnection}
    canTerminate={selectedConnection?.origin === 'active' && store.isActionOperable('core.flow.close')}
    terminating={selectedConnection ? terminatingId === selectedConnection.flowId : false}
    onclose={() => selectedKey = null}
    onrequestterminate={requestTerminate}
  />

  {#if terminateConfirmConnection}
    <div class="confirm-backdrop" role="presentation" onclick={(event) => { if (event.currentTarget === event.target && terminatingId === null) terminateConfirmKey = null; }}>
      <div class="confirm-dialog" role="alertdialog" aria-modal="true" aria-label="终止连接确认">
        <div class="confirm-icon"><AlertTriangle size={18} /></div>
        <div class="confirm-content"><h3>终止这个活动连接？</h3><p>内核将立即取消到 <strong>{terminateConfirmConnection.destination}</strong> 的连接。对应应用可能会自动重新发起连接。</p></div>
        <div class="confirm-actions">
          <Button variant="outline" size="sm" disabled={terminatingId !== null} onclick={() => terminateConfirmKey = null}>取消</Button>
          <button class="confirm-terminate" type="button" disabled={terminatingId !== null} onclick={() => handleTerminate(terminateConfirmConnection)}>{terminatingId ? '终止中...' : '终止连接'}</button>
        </div>
      </div>
    </div>
  {/if}
</Tabs.Root>

<style>
  :global(.connections-shell) { position: relative; }
  .panel-header { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; column-gap: 12px; padding: 11px 14px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .panel-title-row { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .panel-title { font-size: 13px; font-weight: 600; color: var(--foreground); }
  .count-badge { min-width: 26px; padding: 2px 7px; border-radius: 5px; background: var(--muted); color: var(--muted-foreground); font-family: var(--font-mono); font-size: 11px; font-weight: 600; text-align: center; }
  .header-actions { display: flex; justify-content: flex-end; }
  :global(.tab-switcher) { justify-self: center; height: 36px; white-space: nowrap; }
  :global(.tab-btn) { min-width: 76px; font-size: 12px; }
  .search-bar { padding: 7px 14px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .search-field { position: relative; display: flex; align-items: center; }
  .search-icon { position: absolute; left: 9px; top: 50%; display: inline-flex; transform: translateY(-50%); color: var(--muted-foreground); opacity: 0.55; pointer-events: none; }
  .search-input { width: 100%; height: var(--control-height); padding: 0 10px 0 30px; border: 1px solid var(--input); border-radius: var(--control-radius); background: var(--background); color: var(--foreground); outline: none; font-size: 12px; }
  .search-input:focus { border-color: var(--ring); box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 18%, transparent); }
  .search-input::placeholder { color: var(--muted-foreground); opacity: 0.55; }
  .connection-warning { flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 14px; border-bottom: 1px solid color-mix(in srgb, var(--warning) 20%, var(--border)); background: color-mix(in srgb, var(--warning) 7%, transparent); color: var(--warning); font-size: 10.5px; }
  .connection-warning.error { border-bottom-color: color-mix(in srgb, var(--destructive) 20%, var(--border)); background: color-mix(in srgb, var(--destructive) 7%, transparent); color: var(--destructive); }
  .panel-empty, .panel-empty-block { flex: 1; display: flex; align-items: center; justify-content: center; color: var(--muted-foreground); font-size: 12px; }
  .panel-empty-block { flex-direction: column; gap: 5px; padding: 28px; }
  .empty-desc { opacity: 0.6; }
  .error-text { color: var(--destructive); }
  .list-scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 6px; }
  .flow-row { position: relative; display: flex; align-items: stretch; gap: 4px; border: 1px solid transparent; border-radius: 9px; transition: border-color 0.12s ease, background 0.12s ease; }
  .flow-row + .flow-row { margin-top: 3px; }
  .flow-row:hover { border-color: var(--border); background: color-mix(in srgb, var(--muted) 62%, transparent); }
  .flow-open { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 5px; padding: 10px 11px; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .flow-open:focus-visible { outline: 2px solid color-mix(in srgb, var(--primary) 35%, transparent); outline-offset: -2px; border-radius: 8px; }
  .flow-heading, .flow-title, .flow-route, .flow-stats { display: flex; align-items: center; min-width: 0; }
  .flow-heading { gap: 10px; }
  .flow-title { flex: 1; gap: 6px; }
  .flow-destination { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--foreground); font-family: var(--font-mono); font-size: 12.5px; font-weight: 650; }
  .flow-time-label { flex-shrink: 0; color: var(--muted-foreground); font-family: var(--font-mono); font-size: 10.5px; opacity: 0.72; }
  .row-tag { flex-shrink: 0; padding: 1px 5px; border-radius: 4px; font-size: 10px; font-weight: 700; }
  .flow-protocol { background: var(--muted); color: var(--muted-foreground); }
  .flow-policy { background: rgb(168 85 247 / 0.1); color: #a855f7; }
  .flow-route-mode { background: rgb(59 130 246 / 0.1); color: #3b82f6; }
  .flow-route { gap: 5px; overflow: hidden; color: var(--muted-foreground); font-size: 11px; }
  .flow-source, .flow-outbound { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); }
  .flow-source { max-width: 40%; }
  .flow-source.missing-source { opacity: 0.58; font-family: inherit; }
  .flow-arrow { opacity: 0.4; }
  .flow-outbound { opacity: 0.78; }
  .flow-id { margin-left: auto; font-family: var(--font-mono); opacity: 0.45; }
  .flow-stats { gap: 10px; flex-wrap: wrap; font-size: 11px; }
  .flow-stat { font-family: var(--font-mono); font-weight: 550; }
  .flow-stat.up { color: rgb(34 197 94 / 0.88); }
  .flow-stat.down { color: rgb(59 130 246 / 0.88); }
  .flow-duration, .rate-unavailable { color: var(--muted-foreground); font-family: var(--font-mono); opacity: 0.65; }
  .rate-unavailable { margin-left: auto; font-family: inherit; font-size: 10px; }
  .row-actions { position: relative; display: flex; align-items: center; padding-right: 6px; }
  .more-button { width: 28px; height: 28px; display: inline-flex; align-items: center; justify-content: center; border: 0; border-radius: 7px; background: transparent; color: var(--muted-foreground); cursor: pointer; opacity: 0; }
  .flow-row:hover .more-button, .more-button:focus-visible { opacity: 1; }
  .more-button:hover { background: var(--muted); color: var(--foreground); }
  .action-menu { position: absolute; top: 36px; right: 6px; z-index: 20; width: 132px; padding: 4px; border: 1px solid var(--border); border-radius: 8px; background: var(--popover, var(--background)); box-shadow: 0 8px 24px rgb(0 0 0 / 0.16); }
  .action-menu button { width: 100%; display: flex; align-items: center; gap: 7px; border: 0; border-radius: 6px; padding: 7px 8px; background: transparent; color: var(--foreground); font-size: 11px; text-align: left; cursor: pointer; }
  .action-menu button:hover { background: var(--muted); }
  .action-menu button.danger { color: var(--destructive); }
  .list-truncated { padding: 9px; color: var(--muted-foreground); font-size: 11px; text-align: center; opacity: 0.58; }
  .confirm-backdrop { position: absolute; inset: 0; z-index: 70; display: flex; align-items: center; justify-content: center; padding: 20px; background: rgb(0 0 0 / 0.35); backdrop-filter: blur(1px); }
  .confirm-dialog { width: min(420px, 100%); display: grid; grid-template-columns: auto 1fr; gap: 12px; padding: 17px; border: 1px solid var(--border); border-radius: 11px; background: var(--background); box-shadow: 0 18px 50px rgb(0 0 0 / 0.24); }
  .confirm-icon { width: 34px; height: 34px; display: inline-flex; align-items: center; justify-content: center; border-radius: 9px; background: color-mix(in srgb, var(--destructive) 10%, transparent); color: var(--destructive); }
  .confirm-content h3 { margin: 1px 0 5px; font-size: 13px; }
  .confirm-content p { margin: 0; color: var(--muted-foreground); font-size: 11px; line-height: 1.55; }
  .confirm-content strong { color: var(--foreground); font-family: var(--font-mono); }
  .confirm-actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: 7px; margin-top: 3px; }
  .confirm-terminate { border: 1px solid color-mix(in srgb, var(--destructive) 45%, var(--border)); border-radius: 7px; padding: 6px 10px; background: var(--destructive); color: var(--destructive-foreground, white); font-size: 11px; font-weight: 700; cursor: pointer; }
  .confirm-terminate:disabled { opacity: 0.55; cursor: not-allowed; }
  @media (max-width: 720px) { .panel-header { grid-template-columns: 1fr auto; } :global(.tab-switcher) { grid-column: 1 / -1; grid-row: 2; justify-self: stretch; margin-top: 8px; } .flow-time-label { display: none; } .rate-unavailable { width: 100%; margin-left: 0; } }
</style>
