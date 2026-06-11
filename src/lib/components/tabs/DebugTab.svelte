<script lang="ts">
  import { getGuiDebugFrames } from '$lib/services/core';
  import type { DebugFrame } from '$lib/types/debug';

  let frames = $state<DebugFrame[]>([]);
  let loading = $state(true);
  let autoRefresh = $state(true);
  let expandedIds = $state<Set<number>>(new Set());
  let filterType = $state<string>('all');
  let _timer: ReturnType<typeof setInterval> | null = null;

  const FRAME_TYPES = ['all', 'ping', 'query', 'command', 'subscribe'];

  async function refresh() {
    try {
      frames = await getGuiDebugFrames();
    } catch {
      // silent
    } finally {
      loading = false;
    }
  }

  function toggleExpand(id: number) {
    const next = new Set(expandedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    expandedIds = next;
  }

  $effect(() => {
    refresh();
    if (autoRefresh) {
      _timer = setInterval(refresh, 2_000);
    }
    return () => {
      if (_timer) clearInterval(_timer);
    };
  });

  $effect(() => {
    if (autoRefresh && _timer === null) {
      _timer = setInterval(refresh, 2_000);
    } else if (!autoRefresh && _timer !== null) {
      clearInterval(_timer);
      _timer = null;
    }
  });

  const visibleFrames = $derived(
    filterType === 'all'
      ? frames
      : frames.filter(f => f.frameType === filterType)
  );

  function dirColor(d: string): string {
    return d === 'tx' ? '#3B82F6' : '#22C55E';
  }

  function dirLabel(d: string): string {
    return d === 'tx' ? '发送' : '接收';
  }

  function formatTime(ms: number): string {
    return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false, fractionalSecondDigits: 3 });
  }

  function fmtPayload(p: unknown): string {
    return JSON.stringify(p, null, 2);
  }
</script>

<div class="flex-1 w-full flex flex-col gap-3 animate-fade-in overflow-hidden min-h-0">
  <!-- Header -->
  <div class="flex items-center justify-between flex-shrink-0">
    <div class="flex items-center gap-3">
      <h3 class="text-sm font-bold text-foreground">IPC 调试</h3>
      <span class="text-[11px] text-muted-foreground font-mono">{visibleFrames.length} / {frames.length} 帧</span>
    </div>
    <div class="flex items-center gap-2">
      <!-- Type filter -->
      <select bind:value={filterType} class="debug-filter">
        {#each FRAME_TYPES as t}
          <option value={t}>{t === 'all' ? '全部' : t}</option>
        {/each}
      </select>
      <!-- Auto-refresh toggle -->
      <button
        onclick={() => autoRefresh = !autoRefresh}
        class="debug-toggle"
        class:active={autoRefresh}
        title={autoRefresh ? '自动刷新中' : '已暂停'}
      >
        {autoRefresh ? '⏸' : '▶'}
      </button>
      <button onclick={refresh} class="debug-refresh">刷新</button>
    </div>
  </div>

  <!-- Frame list -->
  <div class="flex-1 overflow-y-auto min-h-0 space-y-1">
    {#if loading && frames.length === 0}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground py-12">
        暂无数据 — 等待 IPC 交互…
      </div>
    {:else if visibleFrames.length === 0}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground py-12">
        无匹配帧
      </div>
    {:else}
      {#each visibleFrames as frame (frame.id)}
        <div class="debug-frame" class:expanded={expandedIds.has(frame.id)}>
          <button
            class="debug-frame-header"
            onclick={() => toggleExpand(frame.id)}
          >
            <span class="debug-dir" style="color: {dirColor(frame.direction)}">
              [{dirLabel(frame.direction)}]
            </span>
            <span class="debug-type">{frame.frameType}</span>
            {#if frame.elapsedMs != null}
              <span
                class="debug-elapsed"
                class:slow={frame.elapsedMs > 200}
                class:very-slow={frame.elapsedMs > 500}
                title="请求 → 响应耗时"
              >
                {frame.elapsedMs}ms
              </span>
              <!-- Timing bar: width proportional to elapsed, max 1000ms -->
              <span
                class="debug-timing-bar"
                style="width: {Math.min(frame.elapsedMs / 10, 100)}px; background: {frame.elapsedMs > 500 ? 'var(--destructive)' : frame.elapsedMs > 200 ? 'var(--warning)' : '#22C55E'};"
              ></span>
            {/if}
            <span class="debug-time">{formatTime(frame.atMs)}</span>
            <span class="debug-seq">#{frame.id}</span>
            {#if frame.error}
              <span class="debug-error-icon" title={frame.error}>✕</span>
            {/if}
            <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="debug-chevron" class:expanded={expandedIds.has(frame.id)}>
              <polyline points="3 5 7 9 11 5"/>
            </svg>
          </button>

          {#if expandedIds.has(frame.id)}
            <div class="debug-frame-body">
              {#if frame.error}
                <div class="debug-error">{frame.error}</div>
              {/if}
              <pre class="debug-payload">{fmtPayload(frame.payload)}</pre>
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .debug-filter {
    height: 24px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--foreground);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
  }

  .debug-toggle {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--muted-foreground);
    font-size: 11px;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .debug-toggle.active {
    border-color: #22C55E;
    color: #22C55E;
    background: rgba(34, 197, 94, 0.06);
  }

  .debug-refresh {
    height: 24px;
    padding: 0 10px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .debug-refresh:hover { color: var(--foreground); background: var(--muted); }

  .debug-frame {
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--card);
    overflow: hidden;
    transition: background 0.1s ease;
  }
  .debug-frame:hover { background: var(--surface); }

  .debug-frame-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 11px;
  }

  .debug-dir {
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.04em;
    min-width: 28px;
    flex-shrink: 0;
  }

  .debug-type {
    font-weight: 600;
    color: var(--foreground);
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--muted);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .debug-elapsed {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--muted-foreground);
    min-width: 32px;
    text-align: right;
  }
  .debug-elapsed.slow { color: var(--warning); font-weight: 600; }
  .debug-elapsed.very-slow { color: var(--destructive); font-weight: 700; }

  .debug-timing-bar {
    height: 3px;
    border-radius: 2px;
    flex-shrink: 0;
    opacity: 0.6;
    min-width: 0;
    transition: width 0.15s ease;
  }

  .debug-time {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--muted-foreground);
    opacity: 0.7;
    margin-left: auto;
  }

  .debug-seq {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--muted-foreground);
    opacity: 0.4;
  }

  .debug-error-icon {
    color: var(--destructive);
    font-weight: 700;
    font-size: 10px;
  }

  .debug-chevron {
    flex-shrink: 0;
    opacity: 0.35;
    transition: transform 0.15s ease;
  }
  .debug-chevron.expanded { transform: rotate(180deg); }

  .debug-frame-body {
    padding: 0 12px 10px;
    border-top: 1px solid var(--border);
    margin-top: -1px;
  }

  .debug-error {
    padding: 6px 8px;
    margin-top: 8px;
    border-radius: 4px;
    background: rgba(239, 68, 68, 0.08);
    color: var(--destructive);
    font-size: 11px;
    font-family: var(--font-mono);
    user-select: text;
    -webkit-user-select: text;
  }

  .debug-payload {
    margin-top: 8px;
    padding: 8px 10px;
    border-radius: 5px;
    background: var(--muted);
    color: var(--foreground);
    font-size: 10.5px;
    font-family: var(--font-mono);
    line-height: 1.5;
    overflow-x: auto;
    white-space: pre;
    max-height: 360px;
    overflow-y: auto;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
