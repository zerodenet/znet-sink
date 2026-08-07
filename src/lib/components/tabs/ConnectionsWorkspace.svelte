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
  import ConnectionDetailsDrawer from '$lib/components/ConnectionDetailsDrawer.svelte';
  import * as Tabs from '$lib/components/AppTabs';
  import {
    AlertTriangle,
    ChevronLeft,
    ChevronRight,
    Pause,
    Play,
    RotateCcw,
    Search,
    Trash2,
    XCircle,
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

  let livePaused = $state(false);
  let pausedSnapshot = $state<DisplayConnection[]>([]);
  let pausedAtTick = $state(0);

  let historyPages = $state<PersistedConnection[][]>([]);
  let historyPageBeforeIds = $state<Array<number | undefined>>([undefined]);
  let historyPageHasMore = $state<boolean[]>([]);
  let historyPageIndex = $state(0);
  let historyLoading = $state(true);
  let historyError = $state<string | null>(null);
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

  function hasText(value: unknown): value is string {
    return typeof value === 'string' && value.trim().length > 0 && value !== '-';
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
  const hasFilters = $derived(
    Boolean(searchQuery.trim())
      || protocolFilter !== 'all'
      || outboundFilter !== 'all'
      || resultFilter !== 'all',
  );

  function resetFilters() {
    searchQuery = '';
    protocolFilter = 'all';
    outboundFilter = 'all';
    resultFilter = 'all';
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

    if (force && index === 0) {
      resetHistoryPagination();
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
    } catch (error) {
      if (generation !== historyRequestGeneration) return;
      historyError = getAppErrorMessage(error, '读取连接记录失败');
    } finally {
      if (generation === historyRequestGeneration) historyLoading = false;
    }
  }

  async function clearHistory() {
    if (clearingHistory) return;
    clearingHistory = true;
    try {
      await invoke('gui_debug_clear', { scope: HISTORY_SCOPE });
      resetHistoryPagination();
      clearHistoryConfirm = false;
      await loadHistoryPage(0, true);
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

  $effect(() => {
    if (activeTab !== 'history') return;
    const signature = JSON.stringify([
      searchQuery.trim(),
      protocolFilter,
      outboundFilter,
      resultFilter,
    ]);
    if (signature === historyFilterSignature) return;
    historyFilterSignature = signature;

    const timer = window.setTimeout(() => {
      resetHistoryPagination();
      void loadHistoryPage(0, true);
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
    if (historyPageIndex === 0 && !historyLoading) void loadHistoryPage(0, true);
  });

  $effect(() => {
    const currentIds = new Set(liveView.map((connection) => connection.flowId));
    const nextSuppressed = new Set([...suppressedActiveIds].filter((flowId) => currentIds.has(flowId)));
    if (nextSuppressed.size !== suppressedActiveIds.size) suppressedActiveIds = nextSuppressed;
  });

  onMount(() => {
    void loadHistoryPage(0, true);
    const clock = window.setInterval(() => {
      now = Date.now();
    }, 1_000);
    return () => window.clearInterval(clock);
  });
</script>

<Tabs.Root bind:value={activeTab} class="connections-shell desk-card flex-1 overflow-hidden flex flex-col gap-0 animate-fade-in">
  <div class="panel-header">
    <div class="panel-title-row">
      <span class="panel-title">连接</span>
      <span class="count-badge">{activeTab === 'live' ? liveSource.length : historyView.length}</span>
    </div>
    <Tabs.List class="tab-switcher" aria-label="连接数据范围">
      <Tabs.Trigger class="tab-btn" value="live">实时连接</Tabs.Trigger>
      <Tabs.Trigger class="tab-btn" value="history">连接记录</Tabs.Trigger>
    </Tabs.List>
    <div class="stream-health" class:healthy={coreEvents.status === 'subscribed'} title={coreEvents.lastError ?? eventStatusLabel()}>
      <span class="health-dot"></span>{eventStatusLabel()}
    </div>
  </div>

  <div class="control-bar">
    <div class="search-field">
      <Search size={14} strokeWidth={1.7} />
      <input type="search" aria-label="搜索连接" placeholder="搜索目标、进程、出口、规则或 ID" bind:value={searchQuery}>
    </div>

    <select aria-label="按协议过滤" bind:value={protocolFilter}>
      <option value="all">全部协议</option>
      {#each protocolOptions as protocol}<option value={protocol}>{protocol.toUpperCase()}</option>{/each}
    </select>
    <select aria-label="按出口过滤" bind:value={outboundFilter}>
      <option value="all">全部出口</option>
      {#each outboundOptions as outbound}<option value={outbound}>{outbound}</option>{/each}
    </select>
    {#if activeTab === 'history'}
      <select aria-label="按结果过滤" bind:value={resultFilter}>
        <option value="all">全部结果</option>
        {#each resultOptions as result}<option value={result}>{result}</option>{/each}
      </select>
    {/if}

    {#if hasFilters}
      <button class="toolbar-button quiet" type="button" onclick={resetFilters} title="重置过滤"><RotateCcw size={14} />重置</button>
    {/if}

    <div class="toolbar-spacer"></div>
    {#if activeTab === 'live'}
      <button class="toolbar-button" class:active={livePaused} type="button" onclick={togglePause}>
        {#if livePaused}<Play size={14} />继续查看{:else}<Pause size={14} />暂停查看{/if}
      </button>
      <button
        class="toolbar-button danger"
        type="button"
        disabled={liveView.length === 0 || closingAll || !store.isActionOperable('core.flow.close')}
        onclick={() => closeAllConfirm = true}
      ><XCircle size={14} />关闭全部</button>
    {:else}
      <button class="toolbar-button danger" type="button" disabled={clearingHistory} onclick={() => clearHistoryConfirm = true}>
        <Trash2 size={14} />清空记录
      </button>
    {/if}
  </div>

  {#if livePaused && activeTab === 'live'}
    <div class="paused-banner" role="status">
      <span>当前列表已冻结，不影响后台事件接收。</span>
      <strong>{pendingLiveEvents > 0 ? `已收到 ${pendingLiveEvents} 个连接事件` : '暂无新事件'}</strong>
    </div>
  {/if}

  {#if historyError && activeTab === 'history'}
    <div class="warning-bar"><span>{historyError}</span><button type="button" onclick={() => loadHistoryPage(historyPageIndex, true)}>重试</button></div>
  {/if}

  {#if activeTab === 'history' && historyLoading}
    <div class="empty-state">加载连接记录...</div>
  {:else if currentSource.length === 0}
    <div class="empty-state"><strong>{activeTab === 'live' ? '暂无活动连接' : '暂无连接记录'}</strong><span>{activeTab === 'live' ? '连接事件到达后会自动显示' : '完成的连接会保存在本地记录中'}</span></div>
  {:else if filteredConnections.length === 0}
    <div class="empty-state"><strong>无匹配结果</strong><span>调整搜索条件或过滤器</span></div>
  {:else}
    <div class="list-scroll">
      {#each visibleConnections as connection (connectionKey(connection))}
        <article class="flow-row">
          <button type="button" class="flow-open" onclick={() => openDetails(connection)}>
            <div class="flow-heading">
              <div class="flow-title">
                <span class="destination" title={connection.destination}>{connection.destination}</span>
                <span class="tag">{connection.protocol.toUpperCase()}</span>
                {#if connection.policyTag}<span class="tag">{connection.policyTag}</span>{/if}
                {#if connection.outcome}<span class="tag result">{connection.outcome}</span>{/if}
              </div>
              <span class="timestamp">{formatTimestamp(connection)}</span>
            </div>
            <div class="flow-route">
              <span>{sourceLabel(connection)}</span>
              {#if connection.outboundTag}<span class="arrow">→</span><strong>{connection.outboundTag}</strong>{/if}
              <span class="flow-id">#{connection.flowId}</span>
            </div>
            <div class="flow-stats">
              <span>↑ {formatBytes(connection.bytesUp)}</span>
              <span>↓ {formatBytes(connection.bytesDown)}</span>
              <span>{formatDuration(connection)}</span>
            </div>
          </button>
          {#if connection.origin === 'active' && store.isActionOperable('core.flow.close')}
            <button
              class="row-close"
              type="button"
              disabled={terminatingIds.has(connection.flowId)}
              title="终止连接"
              aria-label={`终止连接 ${connection.destination}`}
              onclick={() => requestSingleTerminate(connection)}
            ><XCircle size={15} /></button>
          {/if}
        </article>
      {/each}
      {#if activeTab === 'live' && filteredConnections.length > visibleConnections.length}
        <div class="list-note">仅渲染前 {visibleConnections.length} / {filteredConnections.length} 条，请使用过滤器缩小范围</div>
      {/if}
    </div>
  {/if}

  {#if activeTab === 'history'}
    <footer class="pagination-bar">
      <span>第 {historyPageIndex + 1} 页 · 每页 {HISTORY_PAGE_SIZE} 条</span>
      <div>
        <button type="button" disabled={historyPageIndex === 0 || historyLoading} onclick={() => loadHistoryPage(historyPageIndex - 1)}><ChevronLeft size={14} />上一页</button>
        <button type="button" disabled={!historyPageHasMore[historyPageIndex] || historyLoading} onclick={() => loadHistoryPage(historyPageIndex + 1)}>下一页<ChevronRight size={14} /></button>
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

  {#if singleConfirmConnection}
    <div class="confirm-layer">
      <button class="confirm-scrim" type="button" aria-label="取消终止连接" onclick={() => singleConfirmKey = null}></button>
      <div class="confirm-dialog" role="alertdialog" aria-modal="true">
        <AlertTriangle size={20} />
        <h3>终止这个连接？</h3>
        <p>内核将立即关闭到 <strong>{singleConfirmConnection.destination}</strong> 的连接，对应应用可能重新建立连接。</p>
        <div><button type="button" onclick={() => singleConfirmKey = null}>取消</button><button class="danger" type="button" onclick={() => closeSingle(singleConfirmConnection)}>终止连接</button></div>
      </div>
    </div>
  {/if}

  {#if closeAllConfirm}
    <div class="confirm-layer">
      <button class="confirm-scrim" type="button" aria-label="取消关闭全部连接" onclick={() => closeAllConfirm = false}></button>
      <div class="confirm-dialog" role="alertdialog" aria-modal="true">
        <AlertTriangle size={20} />
        <h3>关闭当前全部连接？</h3>
        <p>将尝试关闭当前检测到的 <strong>{liveView.length}</strong> 条活动连接。应用可能立即重新建立部分连接。</p>
        <div><button type="button" disabled={closingAll} onclick={() => closeAllConfirm = false}>取消</button><button class="danger" type="button" disabled={closingAll} onclick={closeAllConnections}>{closingAll ? '关闭中...' : '关闭全部'}</button></div>
      </div>
    </div>
  {/if}

  {#if clearHistoryConfirm}
    <div class="confirm-layer">
      <button class="confirm-scrim" type="button" aria-label="取消清空连接记录" onclick={() => clearHistoryConfirm = false}></button>
      <div class="confirm-dialog" role="alertdialog" aria-modal="true">
        <Trash2 size={20} />
        <h3>清空本地连接记录？</h3>
        <p>只删除客户端保存的连接历史，不会关闭活动连接，也不会清空其他诊断日志。</p>
        <div><button type="button" disabled={clearingHistory} onclick={() => clearHistoryConfirm = false}>取消</button><button class="danger" type="button" disabled={clearingHistory} onclick={clearHistory}>{clearingHistory ? '清空中...' : '清空记录'}</button></div>
      </div>
    </div>
  {/if}
</Tabs.Root>

<style>
  :global(.connections-shell) { position: relative; }
  .panel-header { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .panel-title-row { display: flex; align-items: center; gap: 7px; }
  .panel-title { font-size: 13px; font-weight: 700; }
  .count-badge { min-width: 26px; padding: 2px 7px; border-radius: 5px; background: var(--muted); color: var(--muted-foreground); font-family: var(--font-mono); font-size: 11px; text-align: center; }
  :global(.tab-switcher) { justify-self: center; height: 36px; }
  :global(.tab-btn) { min-width: 78px; font-size: 12px; }
  .stream-health { justify-self: end; display: inline-flex; align-items: center; gap: 6px; color: var(--muted-foreground); font-size: 10.5px; }
  .stream-health.healthy { color: var(--primary); }
  .health-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; opacity: .7; }
  .control-bar { display: flex; align-items: center; gap: 7px; padding: 8px 14px; border-bottom: 1px solid var(--border); }
  .search-field { position: relative; min-width: 220px; flex: 1; }
  .search-field svg { position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: var(--muted-foreground); pointer-events: none; }
  .search-field input, select { height: var(--control-height); border: 1px solid var(--input); border-radius: var(--control-radius); background: var(--background); color: var(--foreground); font-size: 11.5px; outline: none; }
  .search-field input { width: 100%; padding: 0 10px 0 30px; }
  select { max-width: 150px; padding: 0 24px 0 9px; }
  .toolbar-spacer { flex: .4; }
  .toolbar-button { height: var(--control-height); display: inline-flex; align-items: center; gap: 6px; border: 1px solid var(--border); border-radius: var(--control-radius); padding: 0 10px; background: var(--background); color: var(--foreground); font-size: 11px; cursor: pointer; white-space: nowrap; }
  .toolbar-button:hover:not(:disabled), .toolbar-button.active { background: var(--muted); }
  .toolbar-button.quiet { color: var(--muted-foreground); }
  .toolbar-button.danger { color: var(--destructive); border-color: color-mix(in srgb, var(--destructive) 30%, var(--border)); }
  .toolbar-button:disabled { opacity: .45; cursor: not-allowed; }
  .paused-banner, .warning-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 7px 14px; border-bottom: 1px solid var(--border); font-size: 10.5px; }
  .paused-banner { background: color-mix(in srgb, var(--warning) 8%, transparent); color: var(--warning); }
  .warning-bar { color: var(--destructive); background: color-mix(in srgb, var(--destructive) 6%, transparent); }
  .warning-bar button { border: 0; background: transparent; color: inherit; cursor: pointer; font-weight: 700; }
  .empty-state { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px; color: var(--muted-foreground); font-size: 11.5px; }
  .empty-state strong { color: var(--foreground); font-size: 12.5px; }
  .list-scroll { flex: 1; min-height: 0; overflow-y: auto; }
  .flow-row { position: relative; display: flex; border-bottom: 1px solid color-mix(in srgb, var(--border) 72%, transparent); }
  .flow-row:hover { background: color-mix(in srgb, var(--muted) 45%, transparent); }
  .flow-open { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 6px; border: 0; padding: 10px 46px 10px 14px; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .flow-heading, .flow-title, .flow-route, .flow-stats { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .flow-heading { justify-content: space-between; }
  .flow-title { overflow: hidden; }
  .destination { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); font-size: 12px; font-weight: 700; }
  .tag { flex-shrink: 0; padding: 2px 5px; border-radius: 4px; background: var(--muted); color: var(--muted-foreground); font-size: 9.5px; font-weight: 700; }
  .tag.result { color: var(--foreground); }
  .timestamp, .flow-route, .flow-stats { color: var(--muted-foreground); font-family: var(--font-mono); font-size: 10.5px; }
  .timestamp { flex-shrink: 0; }
  .flow-route span, .flow-route strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .flow-route strong { color: var(--foreground); font-weight: 600; }
  .arrow { opacity: .5; }
  .flow-id { margin-left: auto; opacity: .55; }
  .flow-stats { gap: 14px; }
  .row-close { position: absolute; right: 12px; top: 50%; width: 28px; height: 28px; display: inline-flex; align-items: center; justify-content: center; transform: translateY(-50%); border: 0; border-radius: 6px; background: transparent; color: var(--muted-foreground); cursor: pointer; }
  .row-close:hover:not(:disabled) { background: color-mix(in srgb, var(--destructive) 10%, transparent); color: var(--destructive); }
  .row-close:disabled { opacity: .4; }
  .list-note { padding: 9px 14px; color: var(--muted-foreground); font-size: 10.5px; text-align: center; }
  .pagination-bar { display: flex; align-items: center; justify-content: space-between; padding: 8px 14px; border-top: 1px solid var(--border); color: var(--muted-foreground); font-size: 10.5px; }
  .pagination-bar div { display: flex; gap: 6px; }
  .pagination-bar button, .confirm-dialog button { display: inline-flex; align-items: center; gap: 4px; border: 1px solid var(--border); border-radius: 6px; padding: 6px 9px; background: var(--background); color: var(--foreground); font-size: 10.5px; cursor: pointer; }
  .pagination-bar button:disabled, .confirm-dialog button:disabled { opacity: .45; cursor: not-allowed; }
  .confirm-layer { position: absolute; inset: 0; z-index: 80; display: flex; align-items: center; justify-content: center; }
  .confirm-scrim { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; background: rgb(0 0 0 / .34); }
  .confirm-dialog { position: relative; width: min(390px, calc(100% - 32px)); display: flex; flex-direction: column; gap: 10px; padding: 18px; border: 1px solid var(--border); border-radius: 11px; background: var(--background); box-shadow: 0 18px 60px rgb(0 0 0 / .28); }
  .confirm-dialog > svg { color: var(--destructive); }
  .confirm-dialog h3, .confirm-dialog p { margin: 0; }
  .confirm-dialog h3 { font-size: 14px; }
  .confirm-dialog p { color: var(--muted-foreground); font-size: 11.5px; line-height: 1.6; }
  .confirm-dialog > div { display: flex; justify-content: flex-end; gap: 7px; margin-top: 3px; }
  .confirm-dialog button.danger { border-color: color-mix(in srgb, var(--destructive) 35%, var(--border)); background: color-mix(in srgb, var(--destructive) 8%, transparent); color: var(--destructive); }
  @media (max-width: 900px) { .control-bar { flex-wrap: wrap; } .toolbar-spacer { display: none; } .search-field { min-width: 100%; } }
</style>
