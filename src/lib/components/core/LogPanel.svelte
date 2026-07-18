<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    Check,
    ChevronDown,
    CircleAlert,
    Copy,
    Pause,
    Play,
    RefreshCw,
    Search,
    Trash2,
    WrapText,
    X,
  } from '@lucide/svelte';
  import { getAppErrorMessage, getLogs, clearLogs } from '$lib/services/core';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import { createLatestRequestGate } from '$lib/services/latest-request-gate.js';
  import {
    serializeLogForClipboard,
    serializeLogsForClipboard,
  } from '$lib/services/diagnostic-copy';
  import type { LogEntry, LogLevel, LogPage, LogQuery, LogSource } from '$lib/types/logs';

  const PAGE_SIZE = 400;

  type FeedbackTone = 'success' | 'error';
  interface CopyFeedback {
    tone: FeedbackTone;
    message: string;
  }

  let logs = $state<LogEntry[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let loadingMore = $state(false);
  let clearing = $state(false);
  let liveUpdates = $state(true);
  let followLatest = $state(true);
  let wrapMessages = $state(true);
  let hasMore = $state(false);
  let selectedSource = $state<LogSource | 'all'>('all');
  let selectedLevel = $state<LogLevel>('info');
  let searchQuery = $state('');
  let expandedLogId = $state<number | null>(null);
  let unseenCount = $state(0);
  let pendingSignals = $state(0);
  let loadError = $state('');
  let copyFeedback = $state<CopyFeedback | null>(null);
  let clearArmed = $state(false);

  let queryGeneration = 0;
  let backgroundRefreshInFlight = false;
  const refreshGate = createLatestRequestGate();
  let lastLogTick = -1;
  let shouldScrollToLatest = false;
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let clearArmTimer: ReturnType<typeof setTimeout> | null = null;
  let logBodyEl: HTMLDivElement | undefined = $state();
  let searchEl: HTMLInputElement | undefined = $state();

  const sources: Array<{ value: LogSource | 'all'; label: string }> = [
    { value: 'all', label: '全部' },
    { value: 'app', label: '应用' },
    { value: 'core', label: '内核' },
  ];

  const levels: Array<{ value: LogLevel; label: string; title: string }> = [
    { value: 'error', label: '错误', title: '仅显示错误' },
    { value: 'warn', label: '警告+', title: '显示警告和错误' },
    { value: 'info', label: '信息+', title: '显示信息、警告和错误' },
    { value: 'debug', label: '调试+', title: '显示调试及以上级别' },
    { value: 'trace', label: '全部', title: '显示全部级别' },
  ];

  const orderedLogs = $derived([...logs].reverse());
  const normalizedSearch = $derived(searchQuery.trim().toLocaleLowerCase());
  const visibleLogs = $derived.by(() => {
    if (!normalizedSearch) return orderedLogs;
    return orderedLogs.filter((log) => logSearchText(log).includes(normalizedSearch));
  });
  const errorCount = $derived(logs.filter((log) => log.level === 'error').length);
  const warningCount = $derived(logs.filter((log) => log.level === 'warn').length);

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

  async function refreshLogs(options: {
    replace?: boolean;
    generation?: number;
    forceFollow?: boolean;
  } = {}) {
    const generation = options.generation ?? queryGeneration;
    const request = refreshGate.begin(generation);
    const knownIds = new Set(logs.map((entry) => entry.id));
    refreshing = true;
    try {
      const page = await getLogs(buildQuery());
      if (!refreshGate.canApply(request)) return;

      const addedCount = page.items.reduce(
        (count, entry) => count + (knownIds.has(entry.id) ? 0 : 1),
        0,
      );
      if (options.replace || (page.items.length === 0 && page.oldestAvailableId == null)) {
        logs = page.items;
      } else {
        logs = mergePage(logs, page);
      }
      syncHasMore(page, logs);
      loadError = '';

      if (options.forceFollow || (followLatest && addedCount > 0)) {
        shouldScrollToLatest = true;
      } else if (!followLatest && addedCount > 0) {
        unseenCount += addedCount;
      }
    } catch (cause) {
      if (refreshGate.canApply(request)) {
        loadError = getAppErrorMessage(cause, '读取日志失败');
      }
    } finally {
      if (refreshGate.isLatest(request)) {
        loading = false;
        refreshing = false;
      }
    }
  }

  async function refreshLogsInBackground() {
    if (backgroundRefreshInFlight || loading || refreshing || loadingMore || clearing) return;
    backgroundRefreshInFlight = true;
    try {
      await refreshLogs();
    } finally {
      backgroundRefreshInFlight = false;
    }
  }

  async function loadMoreLogs() {
    if (loadingMore || logs.length === 0) return;

    const generation = queryGeneration;
    loadingMore = true;
    try {
      const page = await getLogs(buildQuery(logs[0].id));
      if (!refreshGate.isCurrentGeneration(generation)) return;
      logs = mergePage(logs, page);
      syncHasMore(page, logs);
      loadError = '';
    } catch (cause) {
      if (refreshGate.isCurrentGeneration(generation)) {
        loadError = getAppErrorMessage(cause, '加载更多日志失败');
      }
    } finally {
      if (refreshGate.isCurrentGeneration(generation)) loadingMore = false;
    }
  }

  function armClear() {
    if (clearing) return;
    if (clearArmed) {
      void handleClear();
      return;
    }

    clearArmed = true;
    if (clearArmTimer) clearTimeout(clearArmTimer);
    clearArmTimer = setTimeout(() => {
      clearArmed = false;
      clearArmTimer = null;
    }, 4_000);
  }

  async function handleClear() {
    clearArmed = false;
    if (clearArmTimer) clearTimeout(clearArmTimer);
    clearArmTimer = null;
    clearing = true;
    queryGeneration = refreshGate.reset();
    try {
      await clearLogs();
      logs = [];
      hasMore = false;
      unseenCount = 0;
      expandedLogId = null;
      loadError = '';
      showFeedback('success', '日志已清空');
    } catch (cause) {
      loadError = getAppErrorMessage(cause, '清空日志失败');
      showFeedback('error', loadError);
    } finally {
      clearing = false;
    }
  }

  async function copyLastError() {
    const errors = logs.filter((log) => log.level === 'error');
    if (errors.length === 0) return;
    await copyLog(errors[errors.length - 1]);
  }

  function pad(value: number, width = 2): string {
    return String(value).padStart(width, '0');
  }

  function formatTime(ms: number): string {
    const date = new Date(ms);
    return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
  }

  function formatFullTime(ms: number): string {
    return `${new Date(ms).toLocaleString('zh-CN', { hour12: false })}.${pad(new Date(ms).getMilliseconds(), 3)}`;
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
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }

  function formattedFields(log: LogEntry): string {
    if (log.fields == null) return '无结构化字段';
    try {
      return JSON.stringify(log.fields, null, 2);
    } catch {
      return String(log.fields);
    }
  }

  function logSearchText(log: LogEntry): string {
    return [
      String(log.id),
      log.source,
      log.level,
      displayMessage(log),
      formattedFields(log),
    ].join('\n').toLocaleLowerCase();
  }

  function levelLabel(level: LogLevel): string {
    return ({ trace: 'TRC', debug: 'DBG', info: 'INF', warn: 'WRN', error: 'ERR' })[level];
  }

  function showFeedback(tone: FeedbackTone, message: string) {
    copyFeedback = { tone, message };
    if (feedbackTimer) clearTimeout(feedbackTimer);
    feedbackTimer = setTimeout(() => {
      copyFeedback = null;
      feedbackTimer = null;
    }, tone === 'success' ? 2_200 : 5_000);
  }

  async function copyWithFeedback(text: string, successMessage: string): Promise<void> {
    try {
      await copyTextToClipboard(text);
      // Copying is a local, reversible action. Keep its acknowledgement in
      // the log toolbar instead of interrupting the user with a toast.
      showFeedback('success', successMessage);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      showFeedback('error', `复制失败：${message}`);
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
      search: searchQuery.trim() || undefined,
      hasMore,
    });
    await copyWithFeedback(content, `已复制当前 ${visibleLogs.length} 条日志`);
  }

  function toggleLiveUpdates() {
    liveUpdates = !liveUpdates;
    if (liveUpdates) {
      pendingSignals = 0;
      void refreshLogs({ forceFollow: followLatest });
    }
  }

  function jumpToLatest() {
    followLatest = true;
    unseenCount = 0;
    shouldScrollToLatest = true;
    scrollToLatest();
  }

  function scrollToLatest() {
    if (!logBodyEl || !followLatest || !shouldScrollToLatest) return;
    requestAnimationFrame(() => {
      if (!logBodyEl) return;
      logBodyEl.scrollTop = 0;
      shouldScrollToLatest = false;
      unseenCount = 0;
    });
  }

  function handleLogScroll(event: Event) {
    const target = event.currentTarget as HTMLDivElement;
    const atLatest = target.scrollTop <= 24;
    if (atLatest) {
      unseenCount = 0;
      followLatest = true;
    } else if (event.isTrusted && followLatest) {
      followLatest = false;
    }
  }

  function toggleExpanded(logId: number) {
    expandedLogId = expandedLogId === logId ? null : logId;
  }

  onMount(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase() === 'f') {
        event.preventDefault();
        searchEl?.focus();
        searchEl?.select();
      } else if (event.key === 'Escape' && document.activeElement === searchEl) {
        searchQuery = '';
        searchEl?.blur();
      }
    };
    window.addEventListener('keydown', handleKeydown);

    return () => {
      window.removeEventListener('keydown', handleKeydown);
      if (feedbackTimer) clearTimeout(feedbackTimer);
      if (clearArmTimer) clearTimeout(clearArmTimer);
    };
  });

  $effect(() => {
    const source = selectedSource;
    const level = selectedLevel;
    void source;
    void level;

    untrack(() => {
      const generation = refreshGate.reset();
      queryGeneration = generation;
      loading = true;
      loadingMore = false;
      logs = [];
      hasMore = false;
      unseenCount = 0;
      expandedLogId = null;
      void refreshLogs({ replace: true, generation, forceFollow: true });
    });
  });

  $effect(() => {
    if (!liveUpdates) return;
    const timer = window.setInterval(() => {
      void refreshLogsInBackground();
    }, 3_000);

    return () => window.clearInterval(timer);
  });

  $effect(() => {
    const tick = coreEvents.logTick;
    if (tick <= 0 || tick === lastLogTick) return;
    const isNewSignal = lastLogTick >= 0;
    lastLogTick = tick;
    if (liveUpdates) {
      void refreshLogsInBackground();
    } else if (isNewSignal) {
      pendingSignals += 1;
    }
  });

  $effect(() => {
    void visibleLogs.length;
    scrollToLatest();
  });
</script>

<section class="log-panel" aria-label="运行日志">
  <header class="log-heading">
    <div class="heading-copy">
      <div class="title-line">
        <span class="log-title">运行日志</span>
        <span class="live-status" class:paused={!liveUpdates}>
          <span class="live-dot"></span>
          {liveUpdates ? '实时' : '已暂停'}
          {#if !liveUpdates && pendingSignals > 0}
            <span class="pending-count">{pendingSignals > 99 ? '99+' : pendingSignals}</span>
          {/if}
        </span>
      </div>
      <div class="level-summary" aria-label="已加载日志摘要">
        <span>{logs.length}{hasMore ? '+' : ''} 条已加载</span>
        {#if errorCount > 0}<span class="summary-error">{errorCount} 错误</span>{/if}
        {#if warningCount > 0}<span class="summary-warning">{warningCount} 警告</span>{/if}
      </div>
    </div>

    <div class="heading-actions">
      <div class="copy-feedback" class:error={copyFeedback?.tone === 'error'} aria-live="polite">
        {#if copyFeedback}
          {#if copyFeedback.tone === 'success'}<Check class="h-3.5 w-3.5" />{:else}<CircleAlert class="h-3.5 w-3.5" />{/if}
          <span>{copyFeedback.message}</span>
        {/if}
      </div>

      <button
        type="button"
        class="action-button icon-only"
        onclick={() => void refreshLogs({ forceFollow: followLatest })}
        disabled={refreshing}
        title="立即刷新"
        aria-label="立即刷新"
      >
        <RefreshCw class={`h-3.5 w-3.5 ${refreshing ? 'spin' : ''}`} />
      </button>

      <button
        type="button"
        class="action-button"
        class:active={!liveUpdates}
        onclick={toggleLiveUpdates}
        title={liveUpdates ? '暂停实时刷新' : '恢复实时刷新'}
      >
        {#if liveUpdates}<Pause class="h-3.5 w-3.5" />{:else}<Play class="h-3.5 w-3.5" />{/if}
        <span class="action-label">{liveUpdates ? '暂停' : '继续'}</span>
      </button>

      <button
        type="button"
        class="action-button"
        class:active={followLatest}
        onclick={jumpToLatest}
        title="回到最新日志并保持跟随"
      >
        <ChevronDown class="h-3.5 w-3.5" />
        <span class="action-label">跟随</span>
        {#if unseenCount > 0}<span class="unseen-count">{unseenCount > 99 ? '99+' : unseenCount}</span>{/if}
      </button>

      <button
        type="button"
        class="action-button icon-only"
        class:active={wrapMessages}
        onclick={() => wrapMessages = !wrapMessages}
        title={wrapMessages ? '关闭长文本换行' : '开启长文本换行'}
        aria-label={wrapMessages ? '关闭长文本换行' : '开启长文本换行'}
        aria-pressed={wrapMessages}
      >
        <WrapText class="h-3.5 w-3.5" />
      </button>

      <button
        type="button"
        class="action-button icon-only"
        onclick={copyLastError}
        disabled={errorCount === 0}
        title="复制最新错误"
        aria-label="复制最新错误"
      >
        <CircleAlert class="h-3.5 w-3.5" />
      </button>

      <button
        type="button"
        class="action-button primary-action"
        onclick={copyVisibleLogs}
        disabled={visibleLogs.length === 0}
        title="复制当前筛选和搜索结果中的完整日志"
      >
        <Copy class="h-3.5 w-3.5" />
        <span>复制当前</span>
      </button>

      <button
        type="button"
        class="action-button clear-action"
        class:armed={clearArmed}
        onclick={armClear}
        disabled={clearing}
        title={clearArmed ? '再次点击确认清空日志' : '清空日志'}
      >
        <Trash2 class="h-3.5 w-3.5" />
        {#if clearing}<span>清空中</span>{:else if clearArmed}<span>确认清空</span>{/if}
      </button>
    </div>
  </header>

  <div class="log-filters">
    <div class="filter-group" aria-label="日志来源">
      {#each sources as source}
        <button
          type="button"
          onclick={() => selectedSource = source.value}
          class="filter-button"
          class:active={selectedSource === source.value}
          aria-pressed={selectedSource === source.value}
        >
          {source.label}
        </button>
      {/each}
    </div>

    <div class="filter-group" aria-label="最低日志级别">
      {#each levels as level}
        <button
          type="button"
          onclick={() => selectedLevel = level.value}
          class="filter-button"
          class:active={selectedLevel === level.value}
          title={level.title}
          aria-pressed={selectedLevel === level.value}
        >
          {level.label}
        </button>
      {/each}
    </div>

    <label class="search-wrap">
      <Search class="search-icon h-3.5 w-3.5" />
      <input
        bind:this={searchEl}
        value={searchQuery}
        oninput={(event) => searchQuery = event.currentTarget.value}
        placeholder="搜索已加载日志（Ctrl+F）"
        aria-label="搜索已加载日志"
      />
      {#if searchQuery}
        <button type="button" class="search-clear" onclick={() => searchQuery = ''} title="清除搜索" aria-label="清除搜索">
          <X class="h-3 w-3" />
        </button>
      {/if}
    </label>

    <span class="result-count">
      {#if normalizedSearch}{visibleLogs.length} / {/if}{orderedLogs.length}{hasMore ? '+' : ''}
    </span>
  </div>

  {#if loadError}
    <div class="load-error" role="alert">
      <CircleAlert class="h-3.5 w-3.5" />
      <span>日志读取失败：{loadError}</span>
      <button type="button" onclick={() => void refreshLogs({ forceFollow: false })}>重试</button>
    </div>
  {/if}

  <div
    class="log-body"
    class:wrap={wrapMessages}
    bind:this={logBodyEl}
    onscroll={handleLogScroll}
  >
    {#if loading && visibleLogs.length === 0}
      <div class="log-empty">
        <RefreshCw class="h-4 w-4 spin" />
        <span>正在加载日志...</span>
      </div>
    {:else if visibleLogs.length === 0}
      <div class="log-empty">
        <Search class="h-5 w-5" />
        <span>{normalizedSearch ? '当前已加载日志中没有匹配项' : '当前筛选条件下暂无日志'}</span>
        <div class="empty-actions">
          {#if normalizedSearch}
            <button type="button" onclick={() => searchQuery = ''}>清除搜索</button>
          {/if}
          {#if hasMore}
            <button type="button" onclick={loadMoreLogs} disabled={loadingMore}>
              {loadingMore ? '加载中...' : '继续加载更早日志'}
            </button>
          {/if}
        </div>
      </div>
    {:else}
      {#each visibleLogs as log (log.id)}
        {@const fields = structuredFields(log)}
        {@const previewFields = fields.slice(0, 3)}
        <article
          class="log-row level-{log.level}"
          class:expanded={expandedLogId === log.id}
        >
          <button
            type="button"
            class="log-summary"
            onclick={() => toggleExpanded(log.id)}
            aria-expanded={expandedLogId === log.id}
            title={`${formatTime(log.occurredAtUnixMs)} [${log.source.toUpperCase()}] ${displayMessage(log)}`}
          >
            <ChevronDown class={`row-chevron h-3 w-3 ${expandedLogId === log.id ? 'open' : ''}`} />
            <span class="log-time">{formatTime(log.occurredAtUnixMs)}</span>
            <span class="log-source {log.source}">{log.source === 'app' ? 'APP' : 'CORE'}</span>
            <span class="log-level {log.level}">{levelLabel(log.level)}</span>
            <span class="log-message">{displayMessage(log)}</span>
            {#if previewFields.length > 0}
              <span class="log-fields" aria-label="结构化字段预览">
                {#each previewFields as [key, value]}
                  <span class="log-field" title={`${key}=${formatFieldValue(value)}`}>
                    <span class="field-key">{key}</span>
                    <span class="field-value">{formatFieldValue(value)}</span>
                  </span>
                {/each}
                {#if fields.length > previewFields.length}
                  <span class="more-fields">+{fields.length - previewFields.length}</span>
                {/if}
              </span>
            {/if}
          </button>

          <button
            type="button"
            class="row-copy"
            onclick={() => void copyLog(log)}
            title={`复制日志 #${log.id}`}
            aria-label={`复制日志 #${log.id}`}
          >
            <Copy class="h-3.5 w-3.5" />
          </button>

          {#if expandedLogId === log.id}
            <div class="log-details">
              <div class="detail-meta">
                <span><strong>ID</strong> {log.id}</span>
                <span><strong>完整时间</strong> {formatFullTime(log.occurredAtUnixMs)}</span>
                <span><strong>来源</strong> {log.source}</span>
                <span><strong>级别</strong> {log.level}</span>
              </div>
              <div class="detail-message">{displayMessage(log)}</div>
              <pre>{formattedFields(log)}</pre>
            </div>
          {/if}
        </article>
      {/each}

      {#if hasMore}
        <div class="log-more">
          <button type="button" class="load-more-button" onclick={loadMoreLogs} disabled={loadingMore}>
            {#if loadingMore}<RefreshCw class="h-3.5 w-3.5 spin" />{/if}
            <span>{loadingMore ? '加载中...' : '加载更早日志'}</span>
          </button>
        </div>
      {/if}
    {/if}
  </div>
</section>

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

  .log-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 11px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--card) 92%, var(--muted));
    flex-shrink: 0;
  }

  .heading-copy,
  .title-line,
  .level-summary,
  .heading-actions,
  .copy-feedback {
    display: flex;
    align-items: center;
  }

  .heading-copy {
    min-width: 118px;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .title-line { gap: 8px; }

  .log-title {
    color: var(--foreground);
    font-size: 13px;
    font-weight: 650;
  }

  .live-status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 19px;
    padding: 0 6px;
    border-radius: 999px;
    background: color-mix(in srgb, #22c55e 10%, transparent);
    color: #16a34a;
    font-size: 10px;
    font-weight: 650;
  }

  .live-status.paused {
    background: color-mix(in srgb, #f59e0b 11%, transparent);
    color: #d97706;
  }

  .live-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 0 2px color-mix(in srgb, currentColor 16%, transparent);
  }

  .pending-count,
  .unseen-count {
    min-width: 16px;
    height: 16px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 4px;
    border-radius: 999px;
    background: currentColor;
    color: var(--card);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
  }

  .level-summary {
    gap: 7px;
    color: var(--muted-foreground);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

  .summary-error { color: #ef4444; }
  .summary-warning { color: #d97706; }

  .heading-actions {
    justify-content: flex-end;
    gap: 5px;
    min-width: 0;
    flex-wrap: wrap;
  }

  .copy-feedback {
    justify-content: flex-end;
    gap: 5px;
    min-width: 0;
    max-width: 230px;
    color: #16a34a;
    font-size: 10.5px;
    white-space: nowrap;
  }

  .copy-feedback.error { color: var(--destructive); }
  .copy-feedback span { overflow: hidden; text-overflow: ellipsis; }

  .action-button {
    height: 27px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--card);
    color: var(--muted-foreground);
    font: inherit;
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
  }

  .action-button.icon-only {
    width: 27px;
    padding: 0;
  }

  .action-button:hover:not(:disabled) {
    background: var(--muted);
    color: var(--foreground);
  }

  .action-button.active {
    border-color: color-mix(in srgb, var(--primary) 28%, var(--border));
    background: color-mix(in srgb, var(--primary) 9%, var(--card));
    color: var(--primary);
  }

  .action-button.primary-action {
    color: var(--foreground);
  }

  .action-button.clear-action {
    width: 27px;
    padding: 0;
  }

  .action-button.clear-action.armed {
    width: auto;
    padding: 0 8px;
    border-color: color-mix(in srgb, var(--destructive) 35%, var(--border));
    background: color-mix(in srgb, var(--destructive) 8%, var(--card));
    color: var(--destructive);
  }

  .action-button:disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }

  .log-filters {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--card);
    flex-shrink: 0;
  }

  .filter-group {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 2px;
    border-radius: 7px;
    background: var(--muted);
    flex-shrink: 0;
  }

  .filter-button {
    height: 23px;
    padding: 0 7px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }

  .filter-button:hover { color: var(--foreground); }

  .filter-button.active {
    background: var(--card);
    color: var(--foreground);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }

  .search-wrap {
    position: relative;
    min-width: 150px;
    max-width: 360px;
    flex: 1;
    display: flex;
    align-items: center;
  }

  :global(.search-icon) {
    position: absolute;
    left: 9px;
    color: var(--muted-foreground);
    opacity: 0.55;
    pointer-events: none;
  }

  .search-wrap input {
    width: 100%;
    height: 28px;
    padding: 0 29px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--muted);
    color: var(--foreground);
    font: inherit;
    font-size: 11px;
    outline: none;
  }

  .search-wrap input:focus {
    border-color: color-mix(in srgb, var(--primary) 38%, var(--border));
    background: var(--card);
  }

  .search-wrap input::placeholder {
    color: var(--muted-foreground);
    opacity: 0.65;
  }

  .search-clear {
    position: absolute;
    right: 5px;
    width: 20px;
    height: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
  }

  .search-clear:hover { background: var(--border); color: var(--foreground); }

  .result-count {
    min-width: 38px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 10.5px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .load-error {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 10px;
    border-bottom: 1px solid color-mix(in srgb, var(--destructive) 20%, var(--border));
    background: color-mix(in srgb, var(--destructive) 7%, transparent);
    color: var(--destructive);
    font-size: 10.5px;
    flex-shrink: 0;
  }

  .load-error span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .load-error button {
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }

  .log-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 7px;
    background: color-mix(in srgb, var(--card) 97%, var(--muted));
    font-family: var(--font-mono, "JetBrains Mono", monospace);
    scrollbar-gutter: stable;
  }

  .log-empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-family: var(--font-sans, sans-serif);
    font-size: 11.5px;
  }

  .log-empty button {
    border: 0;
    background: transparent;
    color: var(--primary);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .empty-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .log-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 28px;
    border-left: 2px solid transparent;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 5px;
    transition: background 0.1s ease, border-color 0.1s ease;
  }

  .log-row:last-of-type { border-bottom-color: transparent; }
  .log-row:hover, .log-row.expanded { background: var(--muted); }
  .log-row.level-error {
    border-left-color: #ef4444;
    background: color-mix(in srgb, #ef4444 3.5%, transparent);
  }
  .log-row.level-warn { border-left-color: #f59e0b; }

  .log-summary {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 5px 4px 5px 5px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .wrap .log-summary { align-items: flex-start; }

  :global(.row-chevron) {
    margin-top: 3px;
    color: var(--muted-foreground);
    opacity: 0.35;
    transform: rotate(-90deg);
    transition: transform 0.12s ease, opacity 0.12s ease;
    flex-shrink: 0;
  }

  :global(.row-chevron.open) { transform: rotate(0); opacity: 0.8; }
  .log-row:hover :global(.row-chevron) { opacity: 0.8; }

  .log-time {
    width: 118px;
    color: var(--muted-foreground);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    opacity: 0.78;
    flex-shrink: 0;
  }

  .log-source,
  .log-level {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 18px;
    padding: 0 4px;
    border-radius: 4px;
    font-size: 9.5px;
    font-weight: 750;
    letter-spacing: 0.025em;
    flex-shrink: 0;
  }

  .log-source.app { background: color-mix(in srgb, #8b5cf6 12%, transparent); color: #7c3aed; }
  .log-source.core { background: color-mix(in srgb, #3b82f6 12%, transparent); color: #2563eb; }
  .log-level { color: var(--muted-foreground); background: var(--card); }
  .log-level.error { color: #dc2626; background: color-mix(in srgb, #ef4444 11%, transparent); }
  .log-level.warn { color: #d97706; background: color-mix(in srgb, #f59e0b 11%, transparent); }
  .log-level.info { color: #16a34a; background: color-mix(in srgb, #22c55e 9%, transparent); }
  .log-level.debug { color: #0891b2; background: color-mix(in srgb, #06b6d4 9%, transparent); }

  .log-message {
    min-width: 120px;
    flex: 1;
    color: var(--foreground);
    font-size: 11.5px;
    line-height: 1.5;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .wrap .log-message {
    white-space: normal;
    overflow-wrap: anywhere;
  }

  .log-fields {
    min-width: 0;
    max-width: 42%;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    overflow: hidden;
    flex-shrink: 1;
  }

  .log-field {
    min-width: 0;
    max-width: 170px;
    display: inline-flex;
    height: 18px;
    border: 1px solid color-mix(in srgb, var(--border) 75%, transparent);
    border-radius: 4px;
    overflow: hidden;
    font-size: 9.5px;
    flex-shrink: 1;
  }

  .field-key {
    padding: 1px 4px;
    background: color-mix(in srgb, var(--muted) 80%, var(--card));
    color: var(--muted-foreground);
    flex-shrink: 0;
  }

  .field-value {
    min-width: 0;
    padding: 1px 4px;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more-fields {
    color: var(--muted-foreground);
    font-size: 9.5px;
    flex-shrink: 0;
  }

  .row-copy {
    align-self: start;
    width: 24px;
    height: 24px;
    margin-top: 3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s ease, background 0.12s ease, color 0.12s ease;
  }

  .log-row:hover .row-copy,
  .log-row:focus-within .row-copy,
  .log-row.expanded .row-copy { opacity: 1; }

  .row-copy:hover { background: var(--card); color: var(--foreground); }

  .log-details {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 7px;
    margin: 0 7px 7px 22px;
    padding: 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--card);
  }

  .detail-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .detail-meta span {
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 9.5px;
  }

  .detail-meta strong { color: var(--foreground); font-weight: 650; }

  .detail-message {
    color: var(--foreground);
    font-size: 11px;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .log-details pre {
    max-height: 240px;
    margin: 0;
    padding: 8px;
    overflow: auto;
    border-radius: 5px;
    background: color-mix(in srgb, var(--muted) 72%, var(--card));
    color: var(--foreground);
    font: inherit;
    font-size: 10px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .log-more {
    display: flex;
    justify-content: center;
    padding: 11px 0 4px;
  }

  .load-more-button {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--card);
    color: var(--foreground);
    font: inherit;
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .load-more-button:hover:not(:disabled) { background: var(--muted); }
  .load-more-button:disabled { opacity: 0.6; cursor: progress; }

  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 820px) {
    .log-heading { align-items: flex-start; }
    .heading-actions { flex: 1; }
    .copy-feedback { flex: 1 1 100%; max-width: none; min-height: 0; }
    .log-fields { display: none; }
    .action-label { display: none; }
    .action-button:not(.primary-action):not(.clear-action) { width: 27px; padding: 0; }
  }

  @media (max-width: 680px) {
    .log-heading { flex-direction: column; }
    .heading-copy { width: 100%; flex-direction: row; justify-content: space-between; align-items: center; }
    .heading-actions { width: 100%; justify-content: flex-start; }
    .log-filters { flex-wrap: wrap; }
    .search-wrap { order: 3; flex-basis: calc(100% - 48px); max-width: none; }
    .log-time { width: 104px; font-size: 9.5px; }
  }
</style>
