<script lang="ts">
  import { onMount } from 'svelte';
  import {
    closeFlow,
    getAppErrorMessage,
    getGuiConnections,
    getGuiRecentConnections,
    guiCloseConnection,
    handleAppError,
    queryFlows,
    type FlowInfo,
  } from '$lib/services/core';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import {
    buildConnectionView,
    type DisplayConnection,
  } from '$lib/services/connection-view';
  import { store } from '$lib/services/store.svelte';
  import type { GuiConnectionItem } from '$lib/types/gui-api';
  import { Activity, Clock3, RefreshCw, Search, X } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Tabs from '$lib/components/AppTabs';

  const MAX_CONNECTIONS = 500;
  const MAX_RENDER = 120;
  const RECONCILE_INTERVAL_MS = 5_000;

  let activeSnapshot = $state<GuiConnectionItem[]>([]);
  let recentSnapshot = $state<GuiConnectionItem[]>([]);
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
    activeSnapshot,
    recentSnapshot,
    activeEvents: coreEvents.activeConnections,
    recentEvents: coreEvents.connectionHistory,
    limit: MAX_CONNECTIONS,
  }));

  const liveConnections = $derived(
    connectionView.active.filter((connection) => !suppressedActiveIds.has(connection.flowId)),
  );
  const historyConnections = $derived(connectionView.recent);
  const tabConnections = $derived(
    (activeTab === 'live' ? liveConnections : historyConnections).filter(matchesSearch),
  );
  const visibleConnections = $derived(tabConnections.slice(0, MAX_RENDER));
  const liveStatus = $derived(
    coreEvents.status === 'subscribed'
      ? '事件实时'
      : coreEvents.status === 'reconnecting'
        ? '事件重连中'
        : '快照同步',
  );

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
      || connection.selectionChain.some((item) => item.toLowerCase().includes(query))
      || connection.relayChain.some((item) => item.toLowerCase().includes(query))
    );
  }

  function toggleExpand(flowId: string) {
    const next = new Set(expandedIds);
    if (next.has(flowId)) next.delete(flowId);
    else next.add(flowId);
    expandedIds = next;
  }

  async function refresh(showLoading = false) {
    const generation = ++refreshGeneration;
    if (showLoading) loading = true;
    refreshing = true;

    const [activeResult, recentResult] = await Promise.allSettled([
      loadActiveConnections(),
      getGuiRecentConnections({ limit: MAX_CONNECTIONS }),
    ]);

    if (generation !== refreshGeneration) return;

    const warnings: string[] = [];
    let successfulQueries = 0;

    if (activeResult.status === 'fulfilled') {
      activeSnapshot = activeResult.value;
      flowSupported = true;
      successfulQueries++;
    } else if (isUnsupportedError(activeResult.reason)) {
      flowSupported = false;
    } else {
      warnings.push(`实时连接：${getAppErrorMessage(activeResult.reason, '查询失败')}`);
    }

    if (recentResult.status === 'fulfilled') {
      recentSnapshot = recentResult.value.items;
      historySupported = true;
      successfulQueries++;
    } else if (isUnsupportedError(recentResult.reason) || isModeRestricted(recentResult.reason)) {
      historySupported = false;
    } else {
      warnings.push(`连接记录：${getAppErrorMessage(recentResult.reason, '查询失败')}`);
    }

    if (successfulQueries === 0 && warnings.length > 0 && liveConnections.length === 0 && historyConnections.length === 0) {
      loadError = warnings.join('；');
      partialError = null;
    } else {
      loadError = null;
      partialError = warnings.length > 0 ? `部分连接数据未能同步：${warnings.join('；')}` : null;
    }

    loading = false;
    refreshing = false;
  }

  async function loadActiveConnections(): Promise<GuiConnectionItem[]> {
    try {
      const result = await getGuiConnections({ limit: MAX_CONNECTIONS });
      return result.items;
    } catch (error) {
      if (!isModeRestricted(error)) throw error;
      return (await queryFlows()).map(mapFlowInfo);
    }
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
      void refresh(false);
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
    const seconds = Math.floor(elapsed / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
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
    const visibleIds = new Set([...liveConnections, ...historyConnections].map((connection) => connection.flowId));
    const nextExpanded = new Set([...expandedIds].filter((flowId) => visibleIds.has(flowId)));
    if (nextExpanded.size !== expandedIds.size) expandedIds = nextExpanded;

    const activeIds = new Set(connectionView.active.map((connection) => connection.flowId));
    const nextSuppressed = new Set([...suppressedActiveIds].filter((flowId) => activeIds.has(flowId)));
    if (nextSuppressed.size !== suppressedActiveIds.size) suppressedActiveIds = nextSuppressed;
  });

  onMount(() => {
    void refresh(true);

    const clockTimer = window.setInterval(() => {
      now = Date.now();
    }, 1_000);
    const reconcileTimer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh(false);
    }, RECONCILE_INTERVAL_MS);

    return () => {
      window.clearInterval(clockTimer);
      window.clearInterval(reconcileTimer);
    };
  });
</script>

<Tabs.Root bind:value={activeTab} class="desk-card flex min-h-0 flex-1 flex-col gap-0 overflow-hidden animate-fade-in">
  <div class="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3 border-b border-border px-4 py-3">
    <div class="flex min-w-0 items-center gap-2">
      <span class="text-sm font-semibold">连接</span>
      <span class="rounded-full border border-border bg-muted/40 px-2 py-0.5 text-[11px] tabular-nums text-muted-foreground">
        {activeTab === 'live' ? liveConnections.length : historyConnections.length} 个
      </span>
      {#if activeTab === 'live'}
        <span class="hidden items-center gap-1 text-[11px] text-muted-foreground sm:flex" title={`事件状态：${coreEvents.status}`}>
          <Activity size={12} class={coreEvents.status === 'subscribed' ? 'text-emerald-500' : 'text-amber-500'} />
          {liveStatus}
        </span>
      {/if}
    </div>

    <Tabs.List class="flex rounded-lg border border-border bg-muted/35 p-0.5" aria-label="连接数据范围">
      <Tabs.Trigger class="rounded-md px-3 py-1.5 text-xs data-[state=active]:bg-background data-[state=active]:shadow-sm" value="live">
        实时连接
      </Tabs.Trigger>
      <Tabs.Trigger class="rounded-md px-3 py-1.5 text-xs data-[state=active]:bg-background data-[state=active]:shadow-sm" value="history">
        连接记录
      </Tabs.Trigger>
    </Tabs.List>

    <div class="flex justify-end">
      <Button size="sm" variant="outline" onclick={() => refresh(false)} disabled={refreshing}>
        <RefreshCw size={14} class={refreshing ? 'animate-spin' : undefined} />
        {refreshing ? '同步中' : '刷新'}
      </Button>
    </div>
  </div>

  <div class="border-b border-border px-4 py-2.5">
    <div class="relative max-w-md">
      <Search size={14} class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
      <input
        class="h-8 w-full rounded-md border border-input bg-background pl-9 pr-3 text-xs outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/20"
        type="search"
        aria-label="搜索连接"
        placeholder="搜索地址、来源、进程或标签"
        bind:value={searchQuery}
      >
    </div>
  </div>

  {#if partialError}
    <div class="flex items-center justify-between gap-3 border-b border-amber-500/20 bg-amber-500/8 px-4 py-2 text-xs text-amber-700 dark:text-amber-300" role="status">
      <span>{partialError}</span>
      <Button variant="outline" size="xs" onclick={() => refresh(false)}>重试</Button>
    </div>
  {/if}

  {#if activeTab === 'history' && !historySupported}
    <div class="flex items-center gap-2 border-b border-border bg-muted/20 px-4 py-2 text-xs text-muted-foreground" role="status">
      <Clock3 size={13} />
      当前模式无法查询内核 recent_flows，暂时仅显示本次 GUI 会话收到的结束事件。
    </div>
  {/if}

  {#if loading && liveConnections.length === 0 && historyConnections.length === 0}
    <div class="flex flex-1 items-center justify-center text-sm text-muted-foreground">加载连接数据...</div>
  {:else if loadError && liveConnections.length === 0 && historyConnections.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
      <span class="text-sm font-medium text-destructive">连接数据加载失败</span>
      <span class="max-w-xl text-xs text-muted-foreground">{loadError}</span>
      <Button variant="outline" size="sm" onclick={() => refresh(true)}>重试</Button>
    </div>
  {:else if activeTab === 'live' && !flowSupported && liveConnections.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-1 text-center">
      <span class="text-sm font-medium">内核不支持实时连接查询</span>
      <span class="text-xs text-muted-foreground">当前内核未提供 active_flows 能力</span>
    </div>
  {:else if tabConnections.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-1 text-center">
      <span class="text-sm font-medium">{searchQuery ? '无匹配结果' : (activeTab === 'live' ? '暂无活跃连接' : '暂无连接记录')}</span>
      <span class="text-xs text-muted-foreground">
        {searchQuery ? '尝试更换搜索关键词' : (activeTab === 'live' ? '新连接会通过内核事件即时显示' : '已结束的连接会显示在这里')}
      </span>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto px-3 py-2">
      <div class="space-y-1.5">
        {#each visibleConnections as connection (connection.flowId)}
          <div class="overflow-hidden rounded-lg border border-border bg-background transition hover:border-foreground/15">
            <div class="flex items-stretch">
              <button
                type="button"
                class="min-w-0 flex-1 px-3 py-2.5 text-left"
                onclick={() => toggleExpand(connection.flowId)}
                aria-expanded={expandedIds.has(connection.flowId)}
              >
                <div class="flex min-w-0 items-center gap-2">
                  <span class="truncate text-xs font-medium" title={connection.destination}>{connection.destination}</span>
                  <span class="shrink-0 rounded border border-border bg-muted/40 px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground">{connection.protocol}</span>
                  {#if connection.policyTag}
                    <span class="hidden max-w-40 truncate rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground sm:inline" title={connection.policyTag}>{connection.policyTag}</span>
                  {/if}
                  {#if connection.routeMode}
                    <span class="shrink-0 rounded bg-primary/8 px-1.5 py-0.5 text-[10px] text-primary">{modeLabel(connection.routeMode)}</span>
                  {/if}
                </div>

                <div class="mt-1 flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
                  <span class="truncate" title={connection.source}>{connection.source}</span>
                  {#if connection.outboundTag}
                    <span>→</span>
                    <span class="truncate" title={connection.outboundTag}>{connection.outboundTag}</span>
                  {/if}
                  <span class="ml-auto shrink-0 font-mono text-[10px] opacity-70">#{connection.flowId}</span>
                </div>

                <div class="mt-1.5 flex items-center gap-3 text-[11px] tabular-nums">
                  <span class="text-amber-600 dark:text-amber-400">↑ {formatBytes(connection.bytesUp)}</span>
                  <span class="text-sky-600 dark:text-sky-400">↓ {formatBytes(connection.bytesDown)}</span>
                  <span class="ml-auto text-muted-foreground">{formatDuration(connection.startedAtUnixMs, connection.durationMs)}</span>
                </div>
              </button>

              {#if connection.origin === 'active' && store.isActionOperable('core.flow.close')}
                <button
                  class="flex w-10 shrink-0 items-center justify-center border-l border-border text-muted-foreground transition hover:bg-destructive/8 hover:text-destructive disabled:opacity-40"
                  onclick={(event) => {
                    event.stopPropagation();
                    handleClose(connection.flowId);
                  }}
                  disabled={closingId !== null}
                  title="关闭连接"
                  aria-label={`关闭连接 ${connection.flowId}`}
                >
                  <X size={14} />
                </button>
              {/if}
            </div>

            {#if expandedIds.has(connection.flowId)}
              <div class="border-t border-border bg-muted/15 px-3 py-3">
                <div class="grid gap-x-6 gap-y-2 text-[11px] sm:grid-cols-2">
                  <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                    <span class="text-muted-foreground">来源</span>
                    <span class="truncate" title={connection.source}>{connection.source}</span>
                  </div>
                  {#if connection.processName || connection.processPath || connection.processId}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">进程</span>
                      <span class="truncate" title={connection.processPath}>{connection.processName ?? connection.processPath ?? `PID ${connection.processId}`}</span>
                    </div>
                  {/if}
                  {#if connection.targetHost || connection.targetIp}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">目标</span>
                      <span class="truncate" title={connection.targetHost}>{connection.targetHost ?? connection.destination}{connection.targetIp && connection.targetIp !== connection.targetHost ? ` → ${connection.targetIp}` : ''}</span>
                    </div>
                  {/if}
                  {#if connection.inboundTag}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">入口</span>
                      <span class="truncate">{connection.inboundTag}{connection.inboundProtocol ? ` · ${connection.inboundProtocol}` : ''}</span>
                    </div>
                  {/if}
                  {#if connection.outboundTag}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">出口</span>
                      <span class="truncate">{connection.outboundTag}{connection.outboundProtocol ? ` · ${connection.outboundProtocol}` : ''}</span>
                    </div>
                  {/if}
                  {#if connection.remoteDestination}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">实际远端</span>
                      <span class="truncate">{connection.remoteDestination}</span>
                    </div>
                  {/if}
                  {#if connection.matchedRule}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2 sm:col-span-2">
                      <span class="text-muted-foreground">命中规则{connection.matchedRuleIndex !== undefined ? ` #${connection.matchedRuleIndex}` : ''}</span>
                      <span class="truncate" title={connection.matchedRule}>{connection.matchedRule}</span>
                    </div>
                  {/if}
                  {#if connection.selectionChain.length > 0}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2 sm:col-span-2">
                      <span class="text-muted-foreground">选择链</span>
                      <span class="truncate" title={connection.selectionChain.join(' → ')}>{connection.selectionChain.join(' → ')}</span>
                    </div>
                  {/if}
                  {#if connection.relayChain.length > 0}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2 sm:col-span-2">
                      <span class="text-muted-foreground">中继链</span>
                      <span class="truncate" title={connection.relayChain.join(' → ')}>{connection.relayChain.join(' → ')}</span>
                    </div>
                  {/if}
                  {#if connection.outcome || connection.closeReason}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">结果</span>
                      <span class="truncate">{connection.outcome ?? '-'}{connection.closeReason ? ` · ${connection.closeReason}` : ''}</span>
                    </div>
                  {/if}
                  {#if connection.failureMessage}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2 text-destructive sm:col-span-2">
                      <span>失败{connection.failureStage ? ` · ${connection.failureStage}` : ''}</span>
                      <span class="truncate" title={connection.failureMessage}>{connection.failureMessage}</span>
                    </div>
                  {/if}
                  {#if connection.throughputDownBps !== undefined || connection.throughputUpBps !== undefined}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">实时速率</span>
                      <span>↑ {formatBytes(connection.throughputUpBps ?? 0)}/s · ↓ {formatBytes(connection.throughputDownBps ?? 0)}/s</span>
                    </div>
                  {/if}
                  {#if connection.updatedAtUnixMs}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">最后更新</span>
                      <span>{new Date(connection.updatedAtUnixMs).toLocaleTimeString('zh-CN', { hour12: false })}</span>
                    </div>
                  {/if}
                  {#if connection.endedAtUnixMs}
                    <div class="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
                      <span class="text-muted-foreground">结束时间</span>
                      <span>{new Date(connection.endedAtUnixMs).toLocaleTimeString('zh-CN', { hour12: false })}</span>
                    </div>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>

      {#if tabConnections.length > visibleConnections.length}
        <div class="py-3 text-center text-[11px] text-muted-foreground">
          当前显示前 {visibleConnections.length} / {tabConnections.length} 条，请使用搜索缩小范围。
        </div>
      {/if}
    </div>
  {/if}
</Tabs.Root>
