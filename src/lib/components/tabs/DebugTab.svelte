<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { ChevronsUpDown, Clipboard, Pause, Radio, RefreshCcw, Trash2 } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { getAppErrorMessage, getGuiDebugFrames, clearDebugFrames } from '$lib/services/core';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import { createLatestRequestGate } from '$lib/services/latest-request-gate.js';
  import {
    serializeDebugFrameForClipboard,
    serializeDebugFramesForClipboard,
  } from '$lib/services/diagnostic-copy';
  import DiagnosticsPanel from './DiagnosticsPanel.svelte';
  import VersionManagementPanel from './VersionManagementPanel.svelte';
  import type { DebugFrame, DebugFramePage, DebugFrameQuery } from '$lib/types/debug';

  type SubTab = 'diagnostics' | 'frames' | 'versions';

  const PAGE_SIZE = 400;
  const FRAME_TYPES = [
    'all',
    'ping',
    'query',
    'command',
    'subscribe',
    'subscribe-ack',
    'response',
    'event',
    'multiplex',
    'orphan-response',
    'error',
  ];

  let subTab = $state<SubTab>('diagnostics');
  let frames = $state<DebugFrame[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let loadingMore = $state(false);
  let clearing = $state(false);
  let clearArmed = $state(false);
  let loadError = $state<string | null>(null);
  let feedback = $state<{ kind: 'success' | 'error' | 'info'; message: string } | null>(null);
  let autoRefresh = $state(true);
  let expandedIds = $state<Set<number>>(new Set());
  let expandAll = $state(false);
  let hasMore = $state(false);
  let filterType = $state<string>('all');
  let _timer: ReturnType<typeof setInterval> | null = null;
  let _feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let _clearArmTimer: ReturnType<typeof setTimeout> | null = null;
  let queryGeneration = 0;
  let backgroundRefreshInFlight = false;
  const refreshGate = createLatestRequestGate();

  const visibleFrames = $derived([...frames].reverse());

  function buildQuery(beforeId?: number): DebugFrameQuery {
    return {
      frameType: filterType === 'all' ? undefined : filterType,
      limit: PAGE_SIZE,
      beforeId,
    };
  }

  function mergePage(current: DebugFrame[], page: DebugFramePage): DebugFrame[] {
    const merged = new Map<number, DebugFrame>();
    const oldestAvailableId = page.oldestAvailableId;

    for (const frame of current) {
      if (oldestAvailableId == null || frame.id >= oldestAvailableId) {
        merged.set(frame.id, frame);
      }
    }
    for (const frame of page.items) {
      merged.set(frame.id, frame);
    }

    return Array.from(merged.values()).sort((a, b) => a.id - b.id);
  }

  function syncHasMore(page: DebugFramePage, items: DebugFrame[]) {
    if (page.oldestAvailableId == null || items.length === 0) {
      hasMore = false;
      return;
    }
    hasMore = page.hasMore || items[0].id > page.oldestAvailableId;
  }

  function showFeedback(kind: 'success' | 'error' | 'info', message: string, timeoutMs = 3_000) {
    feedback = { kind, message };
    if (_feedbackTimer) clearTimeout(_feedbackTimer);
    _feedbackTimer = setTimeout(() => {
      feedback = null;
      _feedbackTimer = null;
    }, timeoutMs);
  }

  async function requestClearAll() {
    if (clearing) return;
    if (!clearArmed) {
      clearArmed = true;
      showFeedback('info', '再次点击“确认清空”将删除当前 IPC 调试记录', 4_000);
      if (_clearArmTimer) clearTimeout(_clearArmTimer);
      _clearArmTimer = setTimeout(() => {
        clearArmed = false;
        _clearArmTimer = null;
      }, 4_000);
      return;
    }

    clearArmed = false;
    if (_clearArmTimer) {
      clearTimeout(_clearArmTimer);
      _clearArmTimer = null;
    }
    clearing = true;
    const generation = refreshGate.reset();
    queryGeneration = generation;
    try {
      await clearDebugFrames();
      frames = [];
      hasMore = false;
      expandedIds = new Set();
      expandAll = false;
      loading = true;
      await refresh({ replace: true }, generation);
      showFeedback('success', '已清空 IPC 调试记录');
    } catch (error) {
      showFeedback('error', getAppErrorMessage(error, '清空 IPC 调试记录失败'), 6_000);
    } finally {
      clearing = false;
    }
  }

  async function refresh(options: { replace?: boolean } = {}, generation = queryGeneration) {
    if (subTab !== 'frames') return;
    const request = refreshGate.begin(generation);
    const requestedFilter = filterType;
    if (options.replace && frames.length > 0) refreshing = true;

    try {
      const page = await getGuiDebugFrames(buildQuery());
      if (!refreshGate.isCurrentGeneration(generation) || subTab !== 'frames' || requestedFilter !== filterType) return;
      if (!refreshGate.canApply(request)) return;
      loadError = null;
      if (options.replace || (page.items.length === 0 && page.oldestAvailableId == null)) {
        frames = page.items;
      } else {
        frames = mergePage(frames, page);
      }
      syncHasMore(page, frames);

      if (expandAll) {
        expandedIds = new Set(visibleFrames.map(f => f.id));
      } else {
        expandedIds = new Set([...expandedIds].filter(id => frames.some(frame => frame.id === id)));
      }
    } catch (error) {
      if (requestedFilter === filterType && refreshGate.canApply(request)) {
        loadError = getAppErrorMessage(error, '加载 IPC 调试数据失败');
      }
    } finally {
      if (refreshGate.isLatest(request)) {
        loading = false;
        refreshing = false;
      }
    }
  }

  async function refreshInBackground() {
    if (backgroundRefreshInFlight || loading || refreshing || loadingMore || clearing) return;
    backgroundRefreshInFlight = true;
    try {
      await refresh();
    } finally {
      backgroundRefreshInFlight = false;
    }
  }

  async function loadMore() {
    if (loadingMore || frames.length === 0) return;

    loadingMore = true;
    const generation = queryGeneration;
    const requestedFilter = filterType;
    const beforeId = frames[0].id;
    try {
      const page = await getGuiDebugFrames(buildQuery(beforeId));
      if (!refreshGate.isCurrentGeneration(generation) || requestedFilter !== filterType || subTab !== 'frames') return;
      loadError = null;
      frames = mergePage(frames, page);
      syncHasMore(page, frames);
      if (expandAll) {
        expandedIds = new Set(visibleFrames.map(f => f.id));
      }
    } catch (error) {
      if (refreshGate.isCurrentGeneration(generation) && requestedFilter === filterType) {
        loadError = getAppErrorMessage(error, '加载更多 IPC 调试数据失败');
      }
    } finally {
      if (refreshGate.isCurrentGeneration(generation)) loadingMore = false;
    }
  }

  function toggleExpand(id: number) {
    const next = new Set(expandedIds);
    next.has(id) ? next.delete(id) : next.add(id);
    expandedIds = next;
  }

  function toggleExpandAll() {
    if (expandAll) {
      expandedIds = new Set();
      expandAll = false;
    } else {
      expandedIds = new Set(visibleFrames.map(f => f.id));
      expandAll = true;
    }
  }

  $effect(() => {
    const tab = subTab;
    const type = filterType;
    void tab;
    void type;

    // `refresh()` synchronously reads `frames` before its first await. Keep
    // those implementation details out of this effect's dependency set so
    // resetting the page cannot schedule the same effect again forever.
    untrack(() => {
      const generation = refreshGate.reset();
      queryGeneration = generation;
      frames = [];
      hasMore = false;
      expandedIds = new Set();
      expandAll = false;
      loadingMore = false;
      refreshing = false;
      loading = subTab === 'frames';
      loadError = null;

      if (subTab === 'frames') {
        void refresh({ replace: true }, generation);
      }
    });
  });

  $effect(() => {
    if (_timer) {
      clearInterval(_timer);
      _timer = null;
    }

    if (subTab === 'frames' && autoRefresh) {
      _timer = setInterval(() => {
        void refreshInBackground();
      }, 3_000);
    }

    return () => {
      if (_timer) {
        clearInterval(_timer);
        _timer = null;
      }
    };
  });

  function dirColor(d: string) {
    return d === 'tx' ? '#3B82F6' : '#22C55E';
  }

  function dirLabel(d: string) {
    return d === 'tx' ? '发送' : '接收';
  }

  function frameSummary(frame: DebugFrame): string {
    const p = frame.payload as Record<string, unknown> | undefined;
    if (!p) return frame.frameType;

    if (frame.direction === 'tx') {
      const type = p['type'] as string | undefined;
      const method = p['method'] as string | undefined;
      const request = p['request'] as Record<string, unknown> | undefined;
      const events = p['events'] as string[] | undefined;

      if (type === 'query' && request) {
        const keys = Object.keys(request).filter(k => k !== 'filter' && k !== 'limit');
        return `查询: ${keys.join(', ')}`;
      }
      if (type === 'command' && method) {
        const params = p['params'] as Record<string, unknown> | undefined;
        const paramKeys = params ? Object.keys(params).join(', ') : '';
        return `命令: ${method}${paramKeys ? ` (${paramKeys})` : ''}`;
      }
      if (type === 'subscribe') {
        return events?.length ? `订阅: ${events.join(', ')}` : '订阅: 全部事件';
      }
      if (type === 'ping') return 'Ping';
      return type ?? frame.frameType;
    }

    if (p['ok'] !== undefined) {
      if (p['ok']) {
        const result = p['result'] as Record<string, unknown> | undefined;
        if (result) {
          const keys = Object.keys(result);
          return `响应 OK (${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '…' : ''})`;
        }
        return '响应 OK';
      }
      const err = p['error'] as Record<string, unknown> | undefined;
      const code = err?.['code'] || err?.['message'] || 'error';
      return `响应 ERR (${code})`;
    }
    if (p['schema_id']) {
      const eventType = p['event_type'] as string || '?';
      return `事件: ${eventType}`;
    }

    return frame.frameType;
  }

  function fmtTime(ms: number): string {
    const date = new Date(ms);
    return date.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' })
      + ' '
      + date.toLocaleTimeString('zh-CN', { hour12: false, fractionalSecondDigits: 3 });
  }

  function fmtPayload(p: unknown): string {
    if (typeof p === 'string') return p;
    return JSON.stringify(p, null, 2) ?? '';
  }

  function copyErrorMessage(copyError: unknown): string {
    return copyError instanceof Error ? copyError.message : String(copyError);
  }

  async function copyWithFeedback(text: string, successMessage: string): Promise<void> {
    try {
      await copyTextToClipboard(text);
      showFeedback('success', successMessage);
    } catch (copyError) {
      showFeedback('error', `复制失败：${copyErrorMessage(copyError)}`, 6_000);
    }
  }

  async function copyFrame(frame: DebugFrame): Promise<void> {
    await copyWithFeedback(
      serializeDebugFrameForClipboard(frame),
      `已复制 IPC 帧 #${frame.id}`,
    );
  }

  async function copyVisibleFrames(): Promise<void> {
    if (visibleFrames.length === 0) return;
    const content = serializeDebugFramesForClipboard(visibleFrames, {
      frameType: filterType,
      hasMore,
    });
    await copyWithFeedback(content, `已复制当前 ${visibleFrames.length} 条 IPC 帧`);
  }

  onDestroy(() => {
    if (_timer) clearInterval(_timer);
    if (_feedbackTimer) clearTimeout(_feedbackTimer);
    if (_clearArmTimer) clearTimeout(_clearArmTimer);
  });
</script>

<div class="flex-1 w-full flex flex-col gap-2 animate-fade-in overflow-hidden min-h-0">
  <div class="debug-subtabs" role="tablist" aria-label="调试功能">
    <Button variant="ghost" size="xs" class="debug-subtab" role="tab" aria-selected={subTab === 'diagnostics'} aria-pressed={subTab === 'diagnostics'} onclick={() => (subTab = 'diagnostics')}>诊断工具</Button>
    <Button variant="ghost" size="xs" class="debug-subtab" role="tab" aria-selected={subTab === 'frames'} aria-pressed={subTab === 'frames'} onclick={() => (subTab = 'frames')}>IPC 调试</Button>
    <Button variant="ghost" size="xs" class="debug-subtab" role="tab" aria-selected={subTab === 'versions'} aria-pressed={subTab === 'versions'} onclick={() => (subTab = 'versions')}>版本管理</Button>
  </div>

  {#if subTab === 'diagnostics'}
    <DiagnosticsPanel />
  {:else if subTab === 'versions'}
    <VersionManagementPanel />
  {:else}
    <div class="debug-toolbar">
      <div class="flex items-center gap-3">
        <h3 class="text-sm font-bold text-foreground">IPC 调试</h3>
        <span class="text-[11px] text-muted-foreground font-mono">
          {hasMore ? `${frames.length}+` : frames.length} · TX {frames.filter(f => f.direction === 'tx').length} / RX {frames.filter(f => f.direction === 'rx').length}
        </span>
      </div>
      <div class="debug-actions">
        <Button variant="outline" size="xs" onclick={toggleExpandAll} title="展开或折叠全部">
          <ChevronsUpDown />
          {expandAll ? '折叠全部' : '展开全部'}
        </Button>
        <select bind:value={filterType} class="debug-filter">
          {#each FRAME_TYPES as t}
            <option value={t}>{t === 'all' ? '全部' : t}</option>
          {/each}
        </select>
        <Button onclick={() => autoRefresh = !autoRefresh} variant={autoRefresh ? 'secondary' : 'outline'} size="xs" class="debug-live" aria-pressed={autoRefresh} title={autoRefresh ? '自动刷新已开启' : '自动刷新已暂停'}>
          {#if autoRefresh}<Radio />{:else}<Pause />{/if}
          {autoRefresh ? 'LIVE' : 'PAUSE'}
        </Button>
        <Button
          onclick={copyVisibleFrames}
          disabled={visibleFrames.length === 0}
          variant="outline"
          size="xs"
          title="复制当前筛选下已加载的完整 IPC 帧"
        ><Clipboard />复制当前</Button>
        <Button onclick={() => refresh({ replace: true })} variant="outline" size="xs" disabled={refreshing || clearing}>
          <RefreshCcw class={refreshing ? 'animate-spin' : undefined} />
          {refreshing ? '刷新中...' : '刷新'}
        </Button>
        <Button
          onclick={requestClearAll}
          variant="destructive"
          size="xs"
          disabled={clearing}
        ><Trash2 />{clearing ? '清空中...' : clearArmed ? '确认清空' : '清空'}</Button>
      </div>
    </div>

    {#if feedback}
      <div class="debug-feedback" class:error={feedback.kind === 'error'} class:info={feedback.kind === 'info'} role="status" aria-live="polite">
        {feedback.message}
      </div>
    {/if}

    {#if loadError && visibleFrames.length > 0}
      <div class="debug-inline-error" role="alert">
        <span>刷新失败，当前仍显示上一批数据：{loadError}</span>
        <Button variant="outline" size="xs" onclick={() => refresh({ replace: true })} disabled={refreshing}>重试</Button>
      </div>
    {/if}

    <div class="flex-1 overflow-y-auto min-h-0 space-y-0.5" style="font-size: 11px;">
      {#if loading && frames.length === 0}
        <div class="py-12 text-center text-muted-foreground" style="font-size: 12px;">加载中...</div>
      {:else if loadError && visibleFrames.length === 0}
        <div class="debug-load-error" role="alert">
          <span>IPC 调试数据加载失败：{loadError}</span>
          <Button variant="outline" size="xs" onclick={() => refresh({ replace: true })}>重试</Button>
        </div>
      {:else if visibleFrames.length === 0}
        <div class="py-12 text-center text-muted-foreground" style="font-size: 12px;">暂无匹配帧</div>
      {:else}
        {#each visibleFrames as frame (frame.id)}
          <div class="debug-row" class:expanded={expandedIds.has(frame.id)}>
            <div class="debug-row-head">
              <button class="debug-main" onclick={() => toggleExpand(frame.id)}>
                <span class="debug-dir" style="color: {dirColor(frame.direction)}">{dirLabel(frame.direction)}</span>
                <span class="debug-summary">{frameSummary(frame)}</span>
                {#if frame.elapsedMs != null}
                  <span class="debug-ms" class:slow={frame.elapsedMs > 200} class:very-slow={frame.elapsedMs > 500}>
                    {frame.elapsedMs}ms
                  </span>
                  <span class="debug-bar" style="width: {Math.min(frame.elapsedMs / 10, 80)}px; background: {frame.elapsedMs > 500 ? 'var(--destructive)' : frame.elapsedMs > 200 ? 'var(--warning)' : '#22C55E'};"></span>
                {/if}
                {#if frame.error}
                  <span class="debug-err-mark" title={frame.error}>ERR</span>
                {/if}
                <span class="debug-ts">{fmtTime(frame.atMs)}</span>
                <span class="debug-id">#{frame.id}</span>
                <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="debug-chev" class:on={expandedIds.has(frame.id)}>
                  <polyline points="3 5 7 9 11 5"/>
                </svg>
              </button>
              <Button
                class="debug-frame-copy"
                variant="ghost"
                size="xs"
                title={`复制 IPC 帧 #${frame.id}`}
                aria-label={`复制 IPC 帧 #${frame.id}`}
                onclick={() => void copyFrame(frame)}
              ><Clipboard />复制</Button>
            </div>
            {#if expandedIds.has(frame.id)}
              <div class="debug-body">
                {#if frame.error}
                  <div class="debug-err-body">{frame.error}</div>
                {/if}
                <pre class="debug-json">{fmtPayload(frame.payload)}</pre>
              </div>
            {/if}
          </div>
        {/each}

        {#if hasMore}
          <div class="debug-more">
            <Button variant="outline" size="sm" class="debug-load-more" onclick={loadMore} disabled={loadingMore}>
              {loadingMore ? '加载中...' : '加载更多'}
            </Button>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .debug-subtabs {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
  }

  :global(.debug-subtab) { min-width: 84px; color: var(--muted-foreground); }
  :global(.debug-subtab[aria-pressed='true']) { background: var(--accent); color: var(--accent-foreground); }

  .debug-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-shrink: 0; }
  .debug-actions { display: flex; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }

  .debug-filter {
    height: 22px;
    padding: 0 5px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--foreground);
    font-size: 10.5px;
    font-weight: 500;
    cursor: pointer;
  }

  :global(.debug-live) { min-width: 66px; font-family: var(--font-mono); font-size: 9px; letter-spacing: 0.04em; }
  :global(.debug-load-more) { min-width: 112px; }

  .debug-row {
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    overflow: hidden;
    transition: background 0.08s ease;
  }

  .debug-row:hover {
    background: var(--surface);
  }

  .debug-row-head {
    display: flex;
    align-items: stretch;
    min-width: 0;
  }

  .debug-main {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    padding: 4px 8px;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: inherit;
    text-align: left;
    flex: 1;
    min-width: 0;
  }

  :global(.debug-frame-copy) {
    align-self: center;
    flex-shrink: 0;
    margin-right: 6px;
    font-size: 10px;
    opacity: 0;
    transition: opacity 0.12s ease;
  }

  .debug-row:hover :global(.debug-frame-copy),
  :global(.debug-frame-copy:focus-visible) {
    opacity: 1;
  }

  .debug-dir {
    font-weight: 700;
    font-size: 10px;
    min-width: 26px;
    flex-shrink: 0;
  }

  .debug-summary {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    color: var(--foreground);
    min-width: 0;
  }

  .debug-ms {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--muted-foreground);
    text-align: right;
    min-width: 28px;
    flex-shrink: 0;
  }

  .debug-ms.slow {
    color: var(--warning);
    font-weight: 600;
  }

  .debug-ms.very-slow {
    color: var(--destructive);
    font-weight: 700;
  }

  .debug-bar {
    height: 2px;
    border-radius: 1px;
    flex-shrink: 0;
    opacity: 0.5;
  }

  .debug-err-mark {
    color: var(--destructive);
    font-weight: 700;
    font-size: 10px;
    flex-shrink: 0;
  }

  .debug-ts {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--muted-foreground);
    opacity: 0.6;
    flex-shrink: 0;
  }

  .debug-id {
    font-family: var(--font-mono);
    font-size: 8.5px;
    color: var(--muted-foreground);
    opacity: 0.35;
    flex-shrink: 0;
  }

  .debug-chev {
    flex-shrink: 0;
    opacity: 0.3;
    transition: transform 0.12s ease;
  }

  .debug-chev.on {
    transform: rotate(180deg);
  }

  .debug-body {
    padding: 0 10px 8px;
    border-top: 1px solid var(--border);
  }

  .debug-err-body {
    padding: 5px 7px;
    margin-top: 6px;
    border-radius: 4px;
    background: rgba(239, 68, 68, 0.08);
    color: var(--destructive);
    font-size: 10.5px;
    font-family: var(--font-mono);
    user-select: text;
    -webkit-user-select: text;
  }

  .debug-json {
    margin-top: 6px;
    padding: 6px 8px;
    border-radius: 5px;
    background: var(--muted);
    color: var(--foreground);
    font-size: 10px;
    font-family: var(--font-mono);
    line-height: 1.45;
    overflow: auto;
    white-space: pre;
    max-height: 280px;
    user-select: text;
    -webkit-user-select: text;
  }

  .debug-more {
    display: flex;
    justify-content: center;
    padding: 10px 0 2px;
  }

  .debug-load-error {
    margin: 24px auto;
    max-width: 560px;
    padding: 10px 12px;
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 6px;
    background: rgba(239, 68, 68, 0.06);
    color: var(--destructive);
    font-size: 11px;
    line-height: 1.5;
    overflow-wrap: anywhere;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  .debug-feedback {
    flex-shrink: 0;
    padding: 6px 9px;
    border: 1px solid rgba(34, 197, 94, 0.22);
    border-radius: 6px;
    background: rgba(34, 197, 94, 0.07);
    color: var(--success);
    font-size: 10.5px;
  }

  .debug-feedback.error {
    border-color: rgba(239, 68, 68, 0.22);
    background: rgba(239, 68, 68, 0.07);
    color: var(--destructive);
  }

  .debug-feedback.info {
    border-color: color-mix(in srgb, var(--primary) 24%, var(--border));
    background: color-mix(in srgb, var(--primary) 7%, transparent);
    color: var(--foreground);
  }

  .debug-inline-error {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 9px;
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 6px;
    background: rgba(239, 68, 68, 0.06);
    color: var(--destructive);
    font-size: 10.5px;
  }

  @media (max-width: 720px) {
    .debug-subtabs { overflow-x: auto; }
    .debug-toolbar { align-items: stretch; flex-direction: column; }
    .debug-actions { justify-content: flex-start; overflow-x: auto; padding-bottom: 2px; }
  }
</style>
