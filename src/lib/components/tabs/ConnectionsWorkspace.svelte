<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import {
    closeFlow,
    getAppErrorMessage,
    getGuiDebugFrames,
    guiCloseConnection,
    handleAppError,
  } from '$lib/services/core';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { store } from '$lib/services/store.svelte';
  import { buildConnectionView, type DisplayConnection } from '$lib/services/connection-view';
  import {
    buildPersistedConnectionHistory,
    type PersistedConnection,
  } from '$lib/services/connection-history';
  import { success as showSuccessToast, warning as showWarningToast } from '$lib/services/toast.svelte';
  import ActionConfirmDialog from '$lib/components/ActionConfirmDialog.svelte';
  import ConnectionDetailsDrawer from '$lib/components/ConnectionDetailsDrawer.svelte';
  import * as Tabs from '$lib/components/AppTabs';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Spinner } from '$lib/components/ui/Spinner';
  import {
    ChevronLeft,
    ChevronRight,
    CircleX,
    MoreHorizontal,
    Pause,
    Play,
    RotateCcw,
    Search,
    SlidersHorizontal,
    Trash2,
    X,
  } from '@lucide/svelte';

  const HISTORY_SCOPE = 'connection-history';
  const HISTORY_PAGE_SIZE = 50;
  const LIVE_RENDER_LIMIT = 200;
  const CLOSE_CONCURRENCY = 8;

  let activeTab = $state<'live' | 'history'>('live');
  let searchQuery = $state('');
  let protocolFilter = $state('all');
  let outboundFilter = $state('all');
  let resultFilter = $state('all');
  let filtersOpen = $state(false);
  let actionsOpen = $state(false);

  let livePaused = $state(false);
  let pausedSnapshot = $state<DisplayConnection[]>([]);
  let pausedAtTick = $state(0);

  let historyPages = $state<PersistedConnection[][]>([]);
  let historyPageBeforeIds = $state<Array<number | undefined>>([undefined]);
  let historyPageHasMore = $state<boolean[]>([]);
  let historyPageIndex = $state(0);
  let historyLoading = $state(true);
  let historyError = $state<string | null>(null);
  let historyStale = $state(false);
  let latestObservedHistoryKey = '';
  let historyFilterSignature = '';
  let historyRequestGeneration = 0;

  let selectedKey = $state<string | null>(null);
  let singleConfirmKey = $state<string | null>(null);
  let closeAllConfirm = $state(false);
  let clearHistoryConfirm = $state(false);
  let terminatingIds = $state<Set<string>>(new Set());
  let closingAll = $state(false);
  let clearingHistory = $state(false);
  let suppressedActiveIds = $state<Set<string>>(new Set());
  let now = $state(Date.now());

  const liveView = $derived(buildConnectionView({
    activeSnapshot: [],
    recentSnapshot: [],
    activeEvents: coreEvents.activeConnections,
    recentEvents: [],
    limit: 500,
  }).active.filter((connection) => !suppressedActiveIds.has(connection.flowId)));

  const historyView = $derived(buildConnectionView({
    activeSnapshot: [],
    recentSnapshot: historyPages[historyPageIndex] ?? [],
    activeEvents: [],
    recentEvents: [],
    limit: HISTORY_PAGE_SIZE,
  }).recent);

  const liveSource = $derived(livePaused ? pausedSnapshot : liveView);
  const currentSource = $derived(activeTab === 'live' ? liveSource : historyView);

  function connectionKey(connection: DisplayConnection): string {
    const lifetime = connection.startedAtUnixMs
      ?? connection.endedAtUnixMs
      ?? connection.eventOccurredAtUnixMs
      ?? 0;
    return `${connection.origin}:${connection.flowId}:${lifetime}`;
  }

  const selectedConnection = $derived(
    [...liveSource, ...historyView].find((connection) => connectionKey(connection) === selectedKey) ?? null,
  );
  const singleConfirmConnection = $derived(
    liveView.find((connection) => connectionKey(connection) === singleConfirmKey) ?? null,
  );

  const protocolOptions = $derived.by(() => {
    const values = new Set(currentSource.map((connection) => connection.protocol).filter(Boolean));
    if (protocolFilter !== 'all') values.add(protocolFilter);
    return [...values].sort();
  });
  const outboundOptions = $derived.by(() => {
    const values = new Set(currentSource
      .map((connection) => connection.outboundTag)
      .filter((value): value is string => Boolean(value)));
    if (outboundFilter !== 'all') values.add(outboundFilter);
    return [...values].sort();
  });
  const resultOptions = $derived.by(() => {
    const values = new Set(historyView
      .map((connection) => connection.outcome ?? connection.closeReason)
      .filter((value): value is string => Boolean(value)));
    if (resultFilter !== 'all') values.add(resultFilter);
    return [...values].sort();
  });

  const structuredFilterCount = $derived(
    Number(protocolFilter !== 'all')
      + Number(outboundFilter !== 'all')
      + Number(activeTab === 'history' && resultFilter !== 'all'),
  );

  function hasText(value: unknown): value is string {
    return typeof value === 'string' && value.trim().length > 0 && value !== '-';
  }

  function isNumber(value: unknown): value is number {
    return typeof value === 'number' && Number.isFinite(value);
  }

  function matchesFilters(connection: DisplayConnection): boolean {
    if (protocolFilter !== 'all' && connection.protocol !== protocolFilter) return false;
    if (outboundFilter !== 'all' && connection.outboundTag !== outboundFilter) return false;
    if (
      activeTab === 'history'
      && resultFilter !== 'all'
      && connection.outcome !== resultFilter
      && connection.closeReason !== resultFilter
    ) return false;

    const query = searchQuery.trim().toLowerCase();
    if (!query) return true;
    return [
      connection.destination,
      connection.source,
      connection.flowId,
      connection.policyTag,
      connection.outboundTag,
      connection.processName,
      connection.processPath,
      connection.matchedRule,
      connection.remoteDestination,
      connection.eventType,
      ...connection.selectionChain,
      ...connection.relayChain,
    ].some((value) => hasText(value) && value.toLowerCase().includes(query));
  }

  const filteredConnections = $derived(currentSource.filter(matchesFilters));
  const visibleConnections = $derived(
    activeTab === 'live'
      ? filteredConnections.slice(0, LIVE_RENDER_LIMIT)
      : filteredConnections,
  );
  const pendingLiveEvents = $derived(
    livePaused ? Math.max(0, coreEvents.connectionTick - pausedAtTick) : 0,
  );

  function resetStructuredFilters() {
    protocolFilter = 'all';
    outboundFilter = 'all';
    resultFilter = 'all';
  }

  function clearSearch() {
    searchQuery = '';
  }

  function historySignature(): string {
    return JSON.stringify([
      searchQuery.trim(),
      protocolFilter,
      outboundFilter,
      resultFilter,
    ]);
  }

  function resetHistoryPagination() {
    historyRequestGeneration += 1;
    historyPages = [];
    historyPageBeforeIds = [undefined];
    historyPageHasMore = [];
    historyPageIndex = 0;
    if (selectedKey?.startsWith('recent:')) selectedKey = null;
  }

  function togglePause() {
    if (livePaused) {
      livePaused = false;
      pausedSnapshot = [];
      return;
    }
    pausedSnapshot = liveView.map((connection) => ({ ...connection }));
    pausedAtTick = coreEvents.connectionTick;
    livePaused = true;
  }

  async function loadHistoryPage(index: number, force = false) {
    if (!force && historyPages[index]) {
      historyPageIndex = index;
      return;
    }

    const generation = ++historyRequestGeneration;
    const beforeId = historyPageBeforeIds[index];
    historyLoading = true;
    historyError = null;
    try {
      const result = await getGuiDebugFrames({
        frameType: HISTORY_SCOPE,
        limit: HISTORY_PAGE_SIZE,
        beforeId,
        search: searchQuery.trim() || undefined,
        protocol: protocolFilter === 'all' ? undefined : protocolFilter,
        outbound: outboundFilter === 'all' ? undefined : outboundFilter,
        outcome: resultFilter === 'all' ? undefined : resultFilter,
      });
      if (generation !== historyRequestGeneration) return;

      const page = buildPersistedConnectionHistory(result.items, HISTORY_PAGE_SIZE);
      const nextPages = [...historyPages];
      nextPages[index] = page;
      historyPages = nextPages;

      const nextHasMore = [...historyPageHasMore];
      nextHasMore[index] = result.hasMore;
      historyPageHasMore = nextHasMore;

      const nextBeforeIds = [...historyPageBeforeIds];
      nextBeforeIds[index + 1] = result.items[0]?.id;
      historyPageBeforeIds = nextBeforeIds;
      historyPageIndex = index;
      historyStale = false;
    } catch (error) {
      if (generation !== historyRequestGeneration) return;
      historyError = getAppErrorMessage(error, '读取连接记录失败');
    } finally {
      if (generation === historyRequestGeneration) historyLoading = false;
    }
  }

  async function refreshFirstHistoryPage() {
    resetHistoryPagination();
    await loadHistoryPage(0, true);
  }

  async function clearHistory() {
    if (clearingHistory) return;
    clearingHistory = true;
    try {
      await invoke('gui_debug_clear', { scope: HISTORY_SCOPE });
      clearHistoryConfirm = false;
      await refreshFirstHistoryPage();
      showSuccessToast('连接记录已清空');
    } catch (error) {
      handleAppError(error, '清空连接记录失败');
    } finally {
      clearingHistory = false;
    }
  }

  function isModeRestricted(error: unknown): boolean {
    return (error as { code?: string })?.code === 'mode_restricted';
  }

  async function closeConnection(flowId: string) {
    try {
      await guiCloseConnection(flowId);
    } catch (error) {
      if (isModeRestricted(error)) await closeFlow(flowId);
      else throw error;
    }
  }

  async function closeSingle(connection: DisplayConnection) {
    if (terminatingIds.has(connection.flowId)) return;
    terminatingIds = new Set([...terminatingIds, connection.flowId]);
    try {
      await closeConnection(connection.flowId);
      suppressedActiveIds = new Set([...suppressedActiveIds, connection.flowId]);
      pausedSnapshot = pausedSnapshot.filter((item) => item.flowId !== connection.flowId);
      singleConfirmKey = null;
      if (selectedKey === connectionKey(connection)) selectedKey = null;
    } catch (error) {
      handleAppError(error, '终止连接失败');
    } finally {
      const next = new Set(terminatingIds);
      next.delete(connection.flowId);
      terminatingIds = next;
    }
  }

  async function closeAllConnections() {
    if (closingAll) return;
    const ids = [...new Set(liveView.map((connection) => connection.flowId))];
    if (ids.length === 0) {
      closeAllConfirm = false;
      return;
    }

    closingAll = true;
    let failed = 0;
    const closed = new Set<string>();
    try {
      for (let index = 0; index < ids.length; index += CLOSE_CONCURRENCY) {
        const batch = ids.slice(index, index + CLOSE_CONCURRENCY);
        const results = await Promise.allSettled(batch.map((flowId) => closeConnection(flowId)));
        results.forEach((result, resultIndex) => {
          const flowId = batch[resultIndex];
          if (result.status === 'fulfilled') closed.add(flowId);
          else failed += 1;
        });
      }

      suppressedActiveIds = new Set([...suppressedActiveIds, ...closed]);
      pausedSnapshot = pausedSnapshot.filter((connection) => !closed.has(connection.flowId));
      closeAllConfirm = false;
      if (failed > 0) {
        showWarningToast(`已关闭 ${closed.size} 条连接，${failed} 条失败`);
      } else {
        showSuccessToast(`已关闭 ${closed.size} 条连接`);
      }
    } finally {
      closingAll = false;
    }
  }

  function eventStatusLabel(): string {
    switch (coreEvents.status) {
      case 'subscribed': return '事件流正常';
      case 'reconnecting': return '事件流重连中';
      case 'offline': return '内核离线';
      case 'error': return '事件流异常';
      default: return '等待事件流';
    }
  }

  function eventStatusClass(): string {
    switch (coreEvents.status) {
      case 'subscribed': return 'text-emerald-600';
      case 'reconnecting':
      case 'offline': return 'text-amber-600';
      case 'error': return 'text-destructive';
      default: return 'text-muted-foreground';
    }
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

  function listMetric(connection: DisplayConnection, direction: 'up' | 'down'): string {
    const rate = direction === 'up'
      ? connection.throughputUpBps
      : connection.throughputDownBps;
    if (connection.origin === 'active' && isNumber(rate)) return `${formatBytes(rate)}/s`;
    return formatBytes(direction === 'up' ? connection.bytesUp : connection.bytesDown);
  }

  function formatDuration(connection: DisplayConnection): string {
    const elapsed = connection.durationMs
      ?? (connection.startedAtUnixMs ? Math.max(0, now - connection.startedAtUnixMs) : undefined);
    if (elapsed === undefined) return '时长未提供';
    const seconds = Math.floor(elapsed / 1_000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
    return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
  }

  function formatTimestamp(connection: DisplayConnection): string {
    const timestamp = connection.origin === 'active'
      ? connection.startedAtUnixMs
      : connection.endedAtUnixMs ?? connection.eventOccurredAtUnixMs;
    if (!timestamp) return '时间未提供';
    return new Date(timestamp).toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  }

  function openDetails(connection: DisplayConnection) {
    selectedKey = connectionKey(connection);
  }

  function requestSingleTerminate(connection: DisplayConnection) {
    singleConfirmKey = connectionKey(connection);
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    actionsOpen = false;
    filtersOpen = false;
  }

  $effect(() => {
    if (activeTab !== 'history') return;
    const signature = historySignature();
    if (signature === historyFilterSignature && !historyStale) return;
    historyFilterSignature = signature;

    const timer = window.setTimeout(() => {
      void refreshFirstHistoryPage();
    }, 250);
    return () => window.clearTimeout(timer);
  });

  $effect(() => {
    const latest = coreEvents.connectionHistory[0];
    const key = latest
      ? `${latest.flowId}:${latest.startedAtUnixMs ?? ''}:${latest.endedAtUnixMs ?? ''}`
      : '';
    if (!key || key === latestObservedHistoryKey) return;
    latestObservedHistoryKey = key;
    historyStale = true;
    if (activeTab === 'history' && historyPageIndex === 0 && !historyLoading) {
      void refreshFirstHistoryPage();
    }
  });

  $effect(() => {
    const currentIds = new Set(liveView.map((connection) => connection.flowId));
    const nextSuppressed = new Set([...suppressedActiveIds].filter((flowId) => currentIds.has(flowId)));
    if (nextSuppressed.size !== suppressedActiveIds.size) suppressedActiveIds = nextSuppressed;
  });

  onMount(() => {
    historyFilterSignature = historySignature();
    void refreshFirstHistoryPage();
    const clock = window.setInterval(() => {
      now = Date.now();
    }, 1_000);
    return () => window.clearInterval(clock);
  });
</script>

<svelte:window onclick={() => actionsOpen = false} onkeydown={handleWindowKeydown} />

<Tabs.Root bind:value={activeTab} class="connections-shell desk-card flex min-h-0 flex-1 flex-col gap-0 overflow-hidden animate-fade-in">
  <header class="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3 border-b border-border px-3.5 py-2.5">
    <div class="flex min-w-0 items-center gap-2">
      <span class="text-[13px] font-semibold text-foreground">连接</span>
      <Badge
        variant="secondary"
        class="h-5 min-w-6 rounded-md px-1.5 font-mono text-[10px]"
        title={activeTab === 'history' ? '当前页记录数' : '当前活动连接数'}
      >
        {activeTab === 'live' ? liveSource.length : historyView.length}
      </Badge>
    </div>

    <Tabs.List class="h-8 justify-self-center" aria-label="连接数据范围">
      <Tabs.Trigger class="min-w-[76px] text-xs" value="live">实时连接</Tabs.Trigger>
      <Tabs.Trigger class="min-w-[76px] text-xs" value="history">连接记录</Tabs.Trigger>
    </Tabs.List>

    <div
      class={`flex items-center justify-self-end gap-1.5 text-[10.5px] ${eventStatusClass()}`}
      title={coreEvents.lastError ?? eventStatusLabel()}
      aria-label={eventStatusLabel()}
    >
      <span class="size-1.5 rounded-full bg-current opacity-75"></span>
      <span class="hidden sm:inline">{eventStatusLabel()}</span>
    </div>
  </header>

  <div class="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">
    <div class="relative min-w-[220px] max-w-md flex-1">
      <Search class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <Input
        type="search"
        class="h-[30px] pl-8 pr-8 text-xs"
        aria-label="搜索连接"
        placeholder="搜索目标、进程、出口、规则或 ID"
        bind:value={searchQuery}
      />
      {#if searchQuery}
        <Button
          variant="ghost"
          size="icon-xs"
          class="absolute right-0.5 top-1/2 -translate-y-1/2 text-muted-foreground"
          title="清除搜索"
          aria-label="清除搜索"
          onclick={clearSearch}
        >
          <X class="size-3.5" />
        </Button>
      {/if}
    </div>

    <Button
      variant="outline"
      size="sm"
      aria-pressed={filtersOpen}
      onclick={() => filtersOpen = !filtersOpen}
    >
      <SlidersHorizontal data-icon="inline-start" class="size-3.5" />
      筛选
      {#if structuredFilterCount > 0}
        <span class="ml-0.5 inline-flex min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[9px] leading-4 text-primary-foreground">
          {structuredFilterCount}
        </span>
      {/if}
    </Button>

    <div class="ml-auto flex items-center gap-2">
      {#if activeTab === 'live'}
        <Button
          variant={livePaused ? 'secondary' : 'outline'}
          size="sm"
          aria-pressed={livePaused}
          onclick={togglePause}
        >
          {#if livePaused}
            <Play data-icon="inline-start" class="size-3.5" />继续查看
          {:else}
            <Pause data-icon="inline-start" class="size-3.5" />暂停查看
          {/if}
        </Button>
      {/if}

      <div class="relative">
        <Button
          variant="ghost"
          size="icon-sm"
          title="更多操作"
          aria-label="更多操作"
          aria-haspopup="menu"
          aria-expanded={actionsOpen}
          onclick={(event) => {
            event.stopPropagation();
            actionsOpen = !actionsOpen;
          }}
        >
          <MoreHorizontal class="size-4" />
        </Button>

        {#if actionsOpen}
          <div
            class="absolute right-0 top-[calc(100%+6px)] z-50 w-52 rounded-lg border border-border bg-popover p-1 shadow-lg"
            role="menu"
            aria-label="连接操作"
          >
            {#if activeTab === 'live'}
              <Button
                variant="ghost"
                size="sm"
                class="w-full justify-start text-destructive hover:text-destructive"
                role="menuitem"
                disabled={liveView.length === 0 || closingAll || !store.isActionOperable('core.flow.close')}
                onclick={() => {
                  actionsOpen = false;
                  closeAllConfirm = true;
                }}
              >
                <CircleX data-icon="inline-start" class="size-3.5" />
                关闭全部连接
              </Button>
            {:else}
              <Button
                variant="ghost"
                size="sm"
                class="w-full justify-start text-destructive hover:text-destructive"
                role="menuitem"
                disabled={clearingHistory}
                onclick={() => {
                  actionsOpen = false;
                  clearHistoryConfirm = true;
                }}
              >
                <Trash2 data-icon="inline-start" class="size-3.5" />
                清空连接记录
              </Button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>

  {#if filtersOpen}
    <div class="flex flex-wrap items-center gap-2 border-b border-border bg-muted/20 px-3 py-2">
      <span class="mr-1 text-[10.5px] font-medium text-muted-foreground">筛选条件</span>

      <Select.Root type="single" bind:value={protocolFilter}>
        <Select.Trigger size="sm" class="w-full sm:w-[142px]" aria-label="按协议过滤">
          {protocolFilter === 'all' ? '全部协议' : protocolFilter.toUpperCase()}
        </Select.Trigger>
        <Select.Content>
          <Select.Item value="all" label="全部协议" />
          {#each protocolOptions as protocol}
            <Select.Item value={protocol} label={protocol.toUpperCase()} />
          {/each}
        </Select.Content>
      </Select.Root>

      <Select.Root type="single" bind:value={outboundFilter}>
        <Select.Trigger size="sm" class="w-full sm:w-[160px]" aria-label="按出口过滤">
          {outboundFilter === 'all' ? '全部出口' : outboundFilter}
        </Select.Trigger>
        <Select.Content>
          <Select.Item value="all" label="全部出口" />
          {#each outboundOptions as outbound}
            <Select.Item value={outbound} label={outbound} />
          {/each}
        </Select.Content>
      </Select.Root>

      {#if activeTab === 'history'}
        <Select.Root type="single" bind:value={resultFilter}>
          <Select.Trigger size="sm" class="w-full sm:w-[150px]" aria-label="按结果过滤">
            {resultFilter === 'all' ? '全部结果' : resultFilter}
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="all" label="全部结果" />
            {#each resultOptions as result}
              <Select.Item value={result} label={result} />
            {/each}
          </Select.Content>
        </Select.Root>
      {/if}

      {#if structuredFilterCount > 0}
        <Button variant="ghost" size="xs" class="text-muted-foreground" onclick={resetStructuredFilters}>
          <RotateCcw data-icon="inline-start" class="size-3.5" />重置
        </Button>
      {/if}
    </div>
  {/if}

  {#if livePaused && activeTab === 'live'}
    <div class="flex items-center gap-2 border-b border-border bg-muted/30 px-3 py-2 text-[10.5px] text-muted-foreground" role="status">
      <Pause class="size-3.5" />
      <span>列表显示已暂停，后台仍在接收连接事件。</span>
      <Badge variant="outline" class="ml-auto h-5 rounded-md px-1.5 text-[9.5px]">
        {pendingLiveEvents > 0 ? `${pendingLiveEvents} 个新事件` : '暂无新事件'}
      </Badge>
    </div>
  {/if}

  {#if historyError && activeTab === 'history'}
    <div class="flex items-center gap-3 border-b border-border bg-destructive/5 px-3 py-2 text-xs text-destructive">
      <span class="min-w-0 flex-1 truncate">{historyError}</span>
      <Button variant="ghost" size="xs" class="text-destructive hover:text-destructive" onclick={() => refreshFirstHistoryPage()}>
        重试
      </Button>
    </div>
  {/if}

  {#if activeTab === 'history' && historyLoading}
    <div class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
      <Spinner size="sm" color="default" />
      加载连接记录…
    </div>
  {:else if currentSource.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-1.5 px-6 text-center">
      <strong class="text-xs font-semibold text-foreground">{activeTab === 'live' ? '暂无活动连接' : '暂无连接记录'}</strong>
      <span class="text-[11px] text-muted-foreground">{activeTab === 'live' ? '连接事件到达后会自动显示' : '完成的连接会保存在本地记录中'}</span>
    </div>
  {:else if filteredConnections.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-1.5 px-6 text-center">
      <strong class="text-xs font-semibold text-foreground">无匹配结果</strong>
      <span class="text-[11px] text-muted-foreground">调整搜索关键词或筛选条件</span>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#each visibleConnections as connection (connectionKey(connection))}
        <article class="group relative flex border-b border-border/70 transition-colors hover:bg-muted/40">
          <button
            type="button"
            class="flex min-w-0 flex-1 flex-col gap-1.5 bg-transparent px-3.5 py-2.5 pr-12 text-left text-foreground outline-none focus-visible:bg-muted/50"
            aria-label={`查看连接 ${connection.destination}`}
            onclick={() => openDetails(connection)}
          >
            <div class="flex min-w-0 items-center justify-between gap-3">
              <div class="flex min-w-0 items-center gap-1.5 overflow-hidden">
                <span class="truncate font-mono text-xs font-semibold" title={connection.destination}>{connection.destination}</span>
                <Badge variant="secondary" class="h-[18px] rounded px-1.5 text-[9px] font-semibold">
                  {connection.protocol.toUpperCase()}
                </Badge>
                {#if connection.policyTag}
                  <Badge variant="outline" class="h-[18px] max-w-32 truncate rounded px-1.5 text-[9px]">
                    {connection.policyTag}
                  </Badge>
                {/if}
                {#if connection.outcome}
                  <Badge variant="outline" class="h-[18px] rounded px-1.5 text-[9px]">
                    {connection.outcome}
                  </Badge>
                {/if}
              </div>
              <span class="shrink-0 font-mono text-[10px] text-muted-foreground">{formatTimestamp(connection)}</span>
            </div>

            <div class="flex min-w-0 items-center gap-1.5 font-mono text-[10.5px] text-muted-foreground">
              <span class="truncate">{sourceLabel(connection)}</span>
              {#if connection.outboundTag}
                <span class="opacity-50">→</span>
                <span class="truncate font-semibold text-foreground">{connection.outboundTag}</span>
              {/if}
              <span class="ml-auto shrink-0 opacity-55">#{connection.flowId}</span>
            </div>

            <div class="flex items-center gap-3 font-mono text-[10.5px] text-muted-foreground">
              <span>↑ {listMetric(connection, 'up')}</span>
              <span>↓ {listMetric(connection, 'down')}</span>
              <span>{formatDuration(connection)}</span>
            </div>
          </button>

          {#if connection.origin === 'active' && store.isActionOperable('core.flow.close')}
            <Button
              variant="ghost"
              size="icon-sm"
              class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100 focus-visible:opacity-100"
              disabled={terminatingIds.has(connection.flowId)}
              title="终止连接"
              aria-label={`终止连接 ${connection.destination}`}
              onclick={() => requestSingleTerminate(connection)}
            >
              <CircleX class="size-3.5" />
            </Button>
          {/if}
        </article>
      {/each}

      {#if activeTab === 'live' && filteredConnections.length > visibleConnections.length}
        <div class="px-3 py-2 text-center text-[10.5px] text-muted-foreground">
          仅渲染前 {visibleConnections.length} / {filteredConnections.length} 条，请使用筛选缩小范围
        </div>
      {/if}
    </div>
  {/if}

  {#if activeTab === 'history'}
    <footer class="flex items-center justify-between gap-3 border-t border-border px-3 py-2 text-[10.5px] text-muted-foreground">
      <span>第 {historyPageIndex + 1} 页 · 本页 {historyView.length} 条</span>
      <div class="flex items-center gap-1.5">
        <Button
          variant="outline"
          size="xs"
          disabled={historyPageIndex === 0 || historyLoading}
          onclick={() => loadHistoryPage(historyPageIndex - 1)}
        >
          <ChevronLeft data-icon="inline-start" class="size-3.5" />上一页
        </Button>
        <Button
          variant="outline"
          size="xs"
          disabled={!historyPageHasMore[historyPageIndex] || historyLoading}
          onclick={() => loadHistoryPage(historyPageIndex + 1)}
        >
          下一页<ChevronRight data-icon="inline-end" class="size-3.5" />
        </Button>
      </div>
    </footer>
  {/if}

  <ConnectionDetailsDrawer
    connection={selectedConnection}
    canTerminate={selectedConnection?.origin === 'active' && store.isActionOperable('core.flow.close')}
    terminating={selectedConnection ? terminatingIds.has(selectedConnection.flowId) : false}
    onclose={() => selectedKey = null}
    onrequestterminate={requestSingleTerminate}
  />
</Tabs.Root>

<ActionConfirmDialog
  open={Boolean(singleConfirmConnection)}
  title="终止这个连接？"
  description={singleConfirmConnection
    ? `内核将立即关闭到 ${singleConfirmConnection.destination} 的连接，对应应用可能重新建立连接。`
    : ''}
  confirmLabel="终止连接"
  busyLabel="终止中…"
  busy={singleConfirmConnection ? terminatingIds.has(singleConfirmConnection.flowId) : false}
  destructive
  onClose={() => singleConfirmKey = null}
  onConfirm={() => singleConfirmConnection && closeSingle(singleConfirmConnection)}
/>

<ActionConfirmDialog
  open={closeAllConfirm}
  title="关闭当前全部连接？"
  description={`将尝试关闭当前检测到的 ${liveView.length} 条活动连接。应用可能立即重新建立部分连接。`}
  confirmLabel="关闭全部"
  busyLabel="关闭中…"
  busy={closingAll}
  destructive
  onClose={() => closeAllConfirm = false}
  onConfirm={closeAllConnections}
/>

<ActionConfirmDialog
  open={clearHistoryConfirm}
  title="清空本地连接记录？"
  description="只删除客户端保存的连接历史，不会关闭活动连接，也不会清空其他诊断日志。"
  confirmLabel="清空记录"
  busyLabel="清空中…"
  busy={clearingHistory}
  destructive
  onClose={() => clearHistoryConfirm = false}
  onConfirm={clearHistory}
/>

<style>
  :global(.connections-shell) {
    position: relative;
  }
</style>
