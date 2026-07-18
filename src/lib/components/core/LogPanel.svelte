<script lang="ts">
  import { getLogs, clearLogs } from '$lib/services/core';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import {
    serializeLogForClipboard,
    serializeLogsForClipboard,
  } from '$lib/services/diagnostic-copy';
  import { error as toastError, success as toastSuccess } from '$lib/services/toast.svelte';
  import type { LogEntry, LogLevel, LogPage, LogQuery, LogSource } from '$lib/types/logs';

  const PAGE_SIZE = 400;

  let logs = $state<LogEntry[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let autoScroll = $state(true);
  let hasMore = $state(false);
  let selectedSource = $state<LogSource | 'all'>('all');
  let selectedLevel = $state<LogLevel>('info');

  const sources: Array<{ value: LogSource | 'all'; label: string }> = [
    { value: 'all', label: '全部' },
    { value: 'app', label: 'APP' },
    { value: 'core', label: 'CORE' },
  ];

  const levels: Array<{ value: LogLevel; label: string }> = [
    { value: 'error', label: 'ERR' },
    { value: 'warn', label: 'WRN+' },
    { value: 'info', label: 'INF+' },
    { value: 'debug', label: 'DBG+' },
    { value: 'trace', label: 'ALL' },
  ];

  const visibleLogs = $derived([...logs].reverse());

  let _lastLogTick = -1;
  let shouldScrollToLatest = false;
  let logBodyEl: HTMLDivElement | undefined = $state();

  function buildQuery(beforeId?: number): LogQuery {
    return {
      source: selectedSource === 'all' ? undefined : selectedSource,
      minLevel: selectedLevel,
      limit: PAGE_SIZE,
      beforeId,
    };
  }

  function mergePage(current: LogEntry[], page: LogPage): LogEntry[] {
    const merged = new Map<number, LogEntry>();
    const oldestAvailableId = page.oldestAvailableId;

    for (const entry of current) {
      if (oldestAvailableId == null || entry.id >= oldestAvailableId) {
        merged.set(entry.id, entry);
      }
    }
    for (const entry of page.items) {
      merged.set(entry.id, entry);
    }

    return Array.from(merged.values()).sort((a, b) => a.id - b.id);
  }

  function syncHasMore(page: LogPage, items: LogEntry[]) {
    if (page.oldestAvailableId == null || items.length === 0) {
      hasMore = false;
      return;
    }
    hasMore = page.hasMore || items[0].id > page.oldestAvailableId;
  }

  async function refreshLogs(options: { replace?: boolean } = {}) {
    try {
      const page = await getLogs(buildQuery());
      if (options.replace || (page.items.length === 0 && page.oldestAvailableId == null)) {
        logs = page.items;
      } else {
        logs = mergePage(logs, page);
      }
      syncHasMore(page, logs);
      shouldScrollToLatest = true;
    } catch (e) {
      console.error('Failed to get logs:', e);
    } finally {
      loading = false;
    }
  }

  async function loadMoreLogs() {
    if (loadingMore || logs.length === 0) return;

    loadingMore = true;
    try {
      const page = await getLogs(buildQuery(logs[0].id));
      logs = mergePage(logs, page);
      syncHasMore(page, logs);
    } catch (e) {
      console.error('Failed to load more logs:', e);
    } finally {
      loadingMore = false;
    }
  }

  async function handleClear() {
    await clearLogs();
    logs = [];
    hasMore = false;
    loading = true;
    await refreshLogs({ replace: true });
  }

  async function copyLastError() {
    const errors = logs.filter(l => l.level === 'error');
    if (errors.length === 0) return;
    const last = errors[errors.length - 1];
    await copyLog(last);
  }

  function formatTime(ms: number): string {
    const d = new Date(ms);
    return d.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' })
      + ' ' + d.toLocaleTimeString('zh-CN', { hour12: false });
  }

  function fieldObject(log: LogEntry): Record<string, unknown> | null {
    return log.fields && typeof log.fields === 'object' && !Array.isArray(log.fields)
      ? log.fields as Record<string, unknown>
      : null;
  }

  function structuredFields(log: LogEntry): Array<[string, unknown]> {
    const fields = fieldObject(log);
    if (!fields) return [];
    return Object.entries(fields).filter(([key]) =>
      !['message', 'timestamp', 'level', 'raw_line'].includes(key)
    );
  }

  function displayMessage(log: LogEntry): string {
    const message = fieldObject(log)?.['message'];
    return typeof message === 'string' && message.length > 0 ? message : log.message;
  }

  function formatFieldValue(value: unknown): string {
    if (typeof value === 'string') return value;
    return JSON.stringify(value);
  }

  function copyErrorMessage(copyError: unknown): string {
    return copyError instanceof Error ? copyError.message : String(copyError);
  }

  async function copyWithFeedback(text: string, successMessage: string): Promise<void> {
    try {
      await copyTextToClipboard(text);
      toastSuccess(successMessage, 2_500);
    } catch (copyError) {
      toastError(`复制失败：${copyErrorMessage(copyError)}`, 6_000);
    }
  }

  async function copyLog(log: LogEntry): Promise<void> {
    await copyWithFeedback(serializeLogForClipboard(log), `已复制日志 #${log.id}`);
  }

  async function copyVisibleLogs(): Promise<void> {
    if (visibleLogs.length === 0) return;
    const content = serializeLogsForClipboard(visibleLogs, {
      source: selectedSource,
      minLevel: selectedLevel,
      hasMore,
    });
    await copyWithFeedback(content, `已复制当前 ${visibleLogs.length} 条日志`);
  }

  function scrollToLatest() {
    if (!logBodyEl || !autoScroll || !shouldScrollToLatest) return;
    logBodyEl.scrollTop = 0;
    shouldScrollToLatest = false;
  }

  $effect(() => {
    const source = selectedSource;
    const level = selectedLevel;
    void source;
    void level;

    loading = true;
    logs = [];
    hasMore = false;
    void refreshLogs({ replace: true });
  });

  $effect(() => {
    const timer = window.setInterval(() => {
      void refreshLogs();
    }, 1000);

    return () => window.clearInterval(timer);
  });

  $effect(() => {
    const tick = coreEvents.logTick;
    if (tick > 0 && tick !== _lastLogTick) {
      _lastLogTick = tick;
      void refreshLogs();
    }
  });

  $effect(() => {
    void visibleLogs.length;
    scrollToLatest();
  });
</script>

<div class="log-panel">
  <div class="log-toolbar">
    <div class="flex items-center gap-2">
      <span class="log-title">运行日志</span>
      <span class="log-sep"></span>

      <div class="log-filter-group">
        {#each sources as s}
          <button
            onclick={() => selectedSource = s.value}
            class="log-filter-btn {selectedSource === s.value ? 'active' : ''}"
          >
            {s.label}
          </button>
        {/each}
      </div>

      <div class="log-filter-group">
        {#each levels as l}
          <button
            onclick={() => selectedLevel = l.value}
            class="log-filter-btn {selectedLevel === l.value ? 'active' : ''}"
          >
            {l.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="flex items-center gap-1.5">
      <span class="log-count">{hasMore ? `${visibleLogs.length}+` : visibleLogs.length}</span>

      <button
        onclick={copyVisibleLogs}
        disabled={visibleLogs.length === 0}
        class="log-copy-all-btn"
        title="复制当前筛选下已加载的完整日志"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="4.5" y="4.5" width="6" height="6" rx="1"/>
          <path d="M3.5 7.5H2.5a1 1 0 01-1-1V2.5a1 1 0 011-1h4a1 1 0 011 1v1"/>
        </svg>
        复制当前
      </button>

      <button
        onclick={() => autoScroll = !autoScroll}
        class="log-action-btn {autoScroll ? 'active' : ''}"
        title="自动滚动"
        aria-pressed={autoScroll}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="6 1.5 6 7.5"/>
          <polyline points="3.5 5 6 7.5 8.5 5"/>
          <line x1="2" y1="10.5" x2="10" y2="10.5"/>
        </svg>
      </button>

      <button
        onclick={copyLastError}
        disabled={!logs.some(l => l.level === 'error')}
        class="log-action-btn"
        title="复制最新错误"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <rect x="4.5" y="4.5" width="6" height="6" rx="1"/>
          <path d="M3.5 7.5H2.5a1 1 0 01-1-1V2.5a1 1 0 011-1h4a1 1 0 011 1v1"/>
        </svg>
      </button>

      <button
        onclick={handleClear}
        class="log-action-btn"
        title="清空日志"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="1.5 3.5 2.5 3.5 10.5 3.5"/>
          <path d="M4.5 3.5V2.5a1 1 0 011-1h1a1 1 0 011 1v1"/>
          <path d="M9.5 3.5l-.5 6.5a1 1 0 01-1 .5H4a1 1 0 01-1-.5L2.5 3.5"/>
        </svg>
      </button>
    </div>
  </div>

  <div class="log-body" bind:this={logBodyEl}>
    {#if loading && visibleLogs.length === 0}
      <div class="log-empty">加载中...</div>
    {:else if visibleLogs.length === 0}
      <div class="log-empty">暂无日志</div>
    {:else}
      {#each visibleLogs as log, index (`${log.id}-${log.occurredAtUnixMs}-${index}`)}
        {@const fields = structuredFields(log)}
        <div
          class="log-row"
          role="button"
          tabindex="0"
          title={`${formatTime(log.occurredAtUnixMs)} [${log.source.toUpperCase()}] ${displayMessage(log)}`}
          onclick={() => void copyLog(log)}
          onkeydown={(e) => { if (e.key === 'Enter') void copyLog(log); }}
        >
          <span class="log-time">{formatTime(log.occurredAtUnixMs)}</span>
          <span class="log-src" class:app={log.source === 'app'} class:core={log.source === 'core'}>
            {log.source.toUpperCase()}
          </span>
          <span class="log-lvl" class:err={log.level === 'error'} class:wrn={log.level === 'warn'} class:inf={log.level === 'info'} class:dbg={log.level === 'debug'}>
            {log.level.slice(0, 3).toUpperCase()}
          </span>
          <span class="log-msg">{displayMessage(log)}</span>
          {#if fields.length > 0}
            <span class="log-fields">
              {#each fields as [key, value]}
                <span class="log-field" title={`${key}=${formatFieldValue(value)}`}>
                  <span class="log-field-key">{key}</span>
                  <span class="log-field-value">{formatFieldValue(value)}</span>
                </span>
              {/each}
            </span>
          {/if}
          <button
            class="log-row-copy"
            type="button"
            title={`复制日志 #${log.id}`}
            aria-label={`复制日志 #${log.id}`}
            onclick={(event) => {
              event.stopPropagation();
              void copyLog(log);
            }}
          >复制</button>
        </div>
      {/each}

      {#if hasMore}
        <div class="log-more">
          <button class="log-more-btn" onclick={loadMoreLogs} disabled={loadingMore}>
            {loadingMore ? '加载中...' : '加载更多'}
          </button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .log-panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  .log-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    gap: 4px;
    row-gap: 6px;
  }

  .log-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
    white-space: nowrap;
  }

  .log-sep {
    display: block;
    width: 1px;
    height: 14px;
    background: var(--border);
    flex-shrink: 0;
  }

  .log-count {
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
    padding: 2px 6px;
    background: var(--muted);
    border-radius: 4px;
    min-width: 22px;
    text-align: center;
  }

  .log-filter-group {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    background: var(--muted);
    padding: 2px;
    border-radius: 6px;
  }

  .log-filter-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 20px;
    padding: 0 7px;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-mono, monospace);
    cursor: pointer;
    transition: all 0.12s ease;
    white-space: nowrap;
    letter-spacing: 0.02em;
  }

  .log-filter-btn:hover {
    color: var(--foreground);
  }

  .log-filter-btn.active {
    background: var(--card);
    color: var(--foreground);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }

  :global(.dark) .log-filter-btn.active {
    background: rgba(255, 255, 255, 0.1);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  }

  .log-action-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 5px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .log-copy-all-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 24px;
    padding: 0 8px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--foreground);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .log-copy-all-btn:hover:not(:disabled) {
    background: var(--muted);
  }

  .log-copy-all-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .log-action-btn:hover:not(:disabled) {
    background: var(--muted);
    color: var(--foreground);
  }

  .log-action-btn.active {
    background: var(--accent);
    color: var(--accent-foreground);
  }

  .log-action-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .log-body {
    flex: 1;
    overflow-y: auto;
    padding: 7px 9px;
    min-height: 0;
    font-family: var(--font-mono, "JetBrains Mono", monospace);
  }

  .log-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-size: 12px;
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .log-row {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 5px;
    padding: 2px 4px;
    font-size: 12.5px;
    line-height: 1.6;
    border-radius: 3px;
    transition: background 0.1s ease;
    user-select: text;
    cursor: pointer;
    min-width: 0;
  }

  .log-row:hover {
    background: var(--muted);
  }

  .log-row-copy {
    margin-left: auto;
    flex-shrink: 0;
    height: 20px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--card);
    color: var(--muted-foreground);
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s ease, color 0.12s ease, background 0.12s ease;
  }

  .log-row:hover .log-row-copy,
  .log-row:focus-within .log-row-copy {
    opacity: 1;
  }

  .log-row-copy:hover {
    color: var(--foreground);
    background: var(--muted);
  }

  .log-time {
    color: var(--muted-foreground);
    white-space: nowrap;
    flex-shrink: 0;
    opacity: 0.65;
  }

  .log-src {
    white-space: nowrap;
    flex-shrink: 0;
    font-size: 12px;
    font-weight: 700;
    padding: 0 3px;
    border-radius: 3px;
    letter-spacing: 0.02em;
  }

  .log-src.app {
    background: rgba(167, 139, 250, 0.15);
    color: #8B5CF6;
  }

  .log-src.core {
    background: rgba(59, 130, 246, 0.12);
    color: #3B82F6;
  }

  :global(.dark) .log-src.app  { color: #A78BFA; }
  :global(.dark) .log-src.core { color: #60A5FA; }

  .log-lvl {
    white-space: nowrap;
    flex-shrink: 0;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--muted-foreground);
    opacity: 0.7;
  }

  .log-lvl.err { color: #EF4444; opacity: 1; }
  .log-lvl.wrn { color: #F59E0B; opacity: 1; }
  .log-lvl.inf { color: #22C55E; opacity: 1; }
  .log-lvl.dbg { color: #06B6D4; opacity: 0.9; }

  :global(.dark) .log-lvl.err { color: #F87171; }
  :global(.dark) .log-lvl.wrn { color: #FBBF24; }
  :global(.dark) .log-lvl.inf { color: #4ADE80; }
  :global(.dark) .log-lvl.dbg { color: #22D3EE; }

  .log-msg {
    color: var(--foreground);
    opacity: 0.82;
    min-width: 0;
    flex: 0 1 auto;
    word-break: break-all;
    line-height: 1.5;
  }

  .log-fields {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 3px;
    min-width: 0;
  }

  .log-field {
    display: inline-flex;
    max-width: 280px;
    border: 1px solid color-mix(in srgb, var(--border) 82%, transparent);
    border-radius: 4px;
    overflow: hidden;
    font-size: 11px;
    line-height: 18px;
  }

  .log-field-key {
    padding: 0 4px;
    color: var(--muted-foreground);
    background: var(--muted);
  }

  .log-field-value {
    padding: 0 5px;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .log-more {
    display: flex;
    justify-content: center;
    padding: 10px 0 2px;
  }

  .log-more-btn {
    height: 26px;
    padding: 0 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--foreground);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
  }

  .log-more-btn:hover:not(:disabled) {
    background: var(--accent);
    color: var(--accent-foreground);
    border-color: color-mix(in srgb, var(--accent) 70%, var(--border));
  }

  .log-more-btn:disabled {
    opacity: 0.6;
    cursor: progress;
  }
</style>
