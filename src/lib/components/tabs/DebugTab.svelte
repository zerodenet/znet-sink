<script lang="ts">
  import { getGuiDebugFrames, clearDebugFrames } from '$lib/services/core';
  import DiagnosticsPanel from './DiagnosticsPanel.svelte';
  import type { DebugFrame, DebugFramePage, DebugFrameQuery } from '$lib/types/debug';

  type SubTab = 'diagnostics' | 'frames';

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
  let loadingMore = $state(false);
  let loadError = $state<string | null>(null);
  let autoRefresh = $state(true);
  let expandedIds = $state<Set<number>>(new Set());
  let expandAll = $state(false);
  let hasMore = $state(false);
  let filterType = $state<string>('all');
  let _timer: ReturnType<typeof setInterval> | null = null;

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

  async function clearAll() {
    try {
      await clearDebugFrames();
      frames = [];
      hasMore = false;
      expandedIds = new Set();
      expandAll = false;
      loading = true;
      await refresh({ replace: true });
    } catch {
      /* ignore */
    }
  }

  async function refresh(options: { replace?: boolean } = {}) {
    if (subTab !== 'frames') return;

    try {
      const page = await getGuiDebugFrames(buildQuery());
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
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (loadingMore || frames.length === 0) return;

    loadingMore = true;
    try {
      const page = await getGuiDebugFrames(buildQuery(frames[0].id));
      loadError = null;
      frames = mergePage(frames, page);
      syncHasMore(page, frames);
      if (expandAll) {
        expandedIds = new Set(visibleFrames.map(f => f.id));
      }
    } catch (error) {
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      loadingMore = false;
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

    frames = [];
    hasMore = false;
    expandedIds = new Set();
    expandAll = false;
    loading = subTab === 'frames';

    if (subTab === 'frames') {
      void refresh({ replace: true });
    }
  });

  $effect(() => {
    if (_timer) {
      clearInterval(_timer);
      _timer = null;
    }

    if (subTab === 'frames' && autoRefresh) {
      _timer = setInterval(() => {
        void refresh();
      }, 2_000);
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
</script>

<div class="flex-1 w-full flex flex-col gap-2 animate-fade-in overflow-hidden min-h-0">
  <div class="debug-subtabs">
    <button class:active={subTab === 'diagnostics'} onclick={() => (subTab = 'diagnostics')}>诊断工具</button>
    <button class:active={subTab === 'frames'} onclick={() => (subTab = 'frames')}>IPC 调试</button>
  </div>

  {#if subTab === 'diagnostics'}
    <DiagnosticsPanel />
  {:else}
    <div class="flex items-center justify-between flex-shrink-0">
      <div class="flex items-center gap-3">
        <h3 class="text-sm font-bold text-foreground">IPC 调试</h3>
        <span class="text-[11px] text-muted-foreground font-mono">
          {hasMore ? `${frames.length}+` : frames.length} · TX {frames.filter(f => f.direction === 'tx').length} / RX {frames.filter(f => f.direction === 'rx').length}
        </span>
      </div>
      <div class="flex items-center gap-2">
        <button onclick={toggleExpandAll} class="debug-sm-btn" title="展开或折叠全部">
          {expandAll ? '折叠全部' : '展开全部'}
        </button>
        <select bind:value={filterType} class="debug-filter">
          {#each FRAME_TYPES as t}
            <option value={t}>{t === 'all' ? '全部' : t}</option>
          {/each}
        </select>
        <button onclick={() => autoRefresh = !autoRefresh} class="debug-toggle" class:active={autoRefresh} title={autoRefresh ? '自动刷新已开启' : '自动刷新已暂停'}>
          {autoRefresh ? 'LIVE' : 'PAUSE'}
        </button>
        <button onclick={() => refresh({ replace: true })} class="debug-sm-btn">刷新</button>
        <button onclick={clearAll} class="debug-sm-btn clear">清空</button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto min-h-0 space-y-0.5" style="font-size: 11px;">
      {#if loading && frames.length === 0}
        <div class="py-12 text-center text-muted-foreground" style="font-size: 12px;">加载中...</div>
      {:else if loadError && visibleFrames.length === 0}
        <div class="debug-load-error">IPC 调试数据加载失败：{loadError}</div>
      {:else if visibleFrames.length === 0}
        <div class="py-12 text-center text-muted-foreground" style="font-size: 12px;">暂无匹配帧</div>
      {:else}
        {#each visibleFrames as frame (frame.id)}
          <div class="debug-row" class:expanded={expandedIds.has(frame.id)}>
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
            <button class="debug-sm-btn wide" onclick={loadMore} disabled={loadingMore}>
              {loadingMore ? '加载中...' : '加载更多'}
            </button>
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

  .debug-subtabs button {
    padding: 3px 12px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    border-radius: 5px;
    transition: all 0.12s ease;
  }

  .debug-subtabs button:hover {
    color: var(--foreground);
    background: var(--muted);
  }

  .debug-subtabs button.active {
    color: var(--primary);
    background: var(--muted);
  }

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

  .debug-toggle {
    min-width: 48px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--muted-foreground);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .debug-toggle.active {
    border-color: #22C55E;
    color: #22C55E;
    background: rgba(34, 197, 94, 0.06);
  }

  .debug-sm-btn {
    height: 22px;
    padding: 0 7px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--muted-foreground);
    font-size: 10.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.12s ease;
    white-space: nowrap;
  }

  .debug-sm-btn:hover:not(:disabled) {
    color: var(--foreground);
    background: var(--muted);
  }

  .debug-sm-btn.clear:hover {
    color: var(--destructive);
    background: rgba(239, 68, 68, 0.08);
  }

  .debug-sm-btn:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  .debug-sm-btn.wide {
    min-width: 112px;
    justify-content: center;
  }

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
  }
</style>
