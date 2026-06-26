<script lang="ts">
  import { getGuiDebugFrames, clearDebugFrames } from '$lib/services/core';
  import DiagnosticsPanel from './DiagnosticsPanel.svelte';
  import type { DebugFrame } from '$lib/types/debug';

  type SubTab = 'diagnostics' | 'frames';
  // Default to the diagnostics tools; IPC frames stay one click away.
  let subTab = $state<SubTab>('diagnostics');

  let frames = $state<DebugFrame[]>([]);
  let loading = $state(true);
  let autoRefresh = $state(true);
  let expandedIds = $state<Set<number>>(new Set());
  let expandAll = $state(false);
  let filterType = $state<string>('all');
  let _timer: ReturnType<typeof setInterval> | null = null;

  const FRAME_TYPES = ['all', 'ping', 'query', 'command', 'subscribe'];
  const MAX_DISPLAY = 500;

  async function clearAll() {
    try { await clearDebugFrames(); frames = []; expandedIds = new Set(); } catch { /* ok */ }
  }

  async function refresh() {
    try {
      frames = await getGuiDebugFrames();
    } catch { /* silent */ }
    finally { loading = false; }
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
    refresh();
    if (autoRefresh) _timer = setInterval(refresh, 2_000);
    return () => { if (_timer) clearInterval(_timer); };
  });

  $effect(() => {
    if (autoRefresh && _timer === null) _timer = setInterval(refresh, 2_000);
    else if (!autoRefresh && _timer !== null) { clearInterval(_timer); _timer = null; }
  });

  const visibleFrames = $derived(
    (filterType === 'all' ? frames : frames.filter(f => f.frameType === filterType)).slice(0, MAX_DISPLAY)
  );

  // ── Helpers ──

  function dirColor(d: string) { return d === 'tx' ? '#3B82F6' : '#22C55E'; }
  function dirLabel(d: string) { return d === 'tx' ? '发送' : '接收'; }

  /** Extract a human-readable summary from the frame payload without expanding. */
  function frameSummary(frame: DebugFrame): string {
    const p = frame.payload as Record<string, unknown> | undefined;
    if (!p) return frame.frameType;

    // ── TX frames ──
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
      return type ? `${type}` : frame.frameType;
    }

    // ── RX frames ──
    if (p['ok'] !== undefined) {
      if (p['ok']) {
        const result = p['result'] as Record<string, unknown> | undefined;
        if (result) {
          // Show first-level keys from result for context
          const keys = Object.keys(result);
          return `响应 ✓ (${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '…' : ''})`;
        }
        return '响应 ✓';
      }
      const err = p['error'] as Record<string, unknown> | undefined;
      const code = err?.['code'] || err?.['message'] || 'error';
      return `响应 ✕ (${code})`;
    }
    if (p['schema_id']) {
      const eventType = p['event_type'] as string || '?';
      return `事件: ${eventType}`;
    }

    return frame.frameType;
  }

  function fmtTime(ms: number): string {
    return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false, fractionalSecondDigits: 3 });
  }

  function fmtPayload(p: unknown): string {
    return JSON.stringify(p as object, null, 2);
  }
</script>

<div class="flex-1 w-full flex flex-col gap-2 animate-fade-in overflow-hidden min-h-0">
  <!-- Sub-tab switcher -->
  <div class="debug-subtabs">
    <button class:active={subTab === 'diagnostics'} onclick={() => (subTab = 'diagnostics')}>诊断工具</button>
    <button class:active={subTab === 'frames'} onclick={() => (subTab = 'frames')}>IPC 调试</button>
  </div>

  {#if subTab === 'diagnostics'}
    <DiagnosticsPanel />
  {:else}
  <!-- Header -->
  <div class="flex items-center justify-between flex-shrink-0">
    <div class="flex items-center gap-3">
      <h3 class="text-sm font-bold text-foreground">IPC 调试</h3>
      <span class="text-[11px] text-muted-foreground font-mono">
        TX {frames.filter(f => f.direction === 'tx').length} / RX {frames.filter(f => f.direction === 'rx').length}
      </span>
    </div>
    <div class="flex items-center gap-2">
      <button onclick={toggleExpandAll} class="debug-sm-btn" title="展开/折叠全部">
        {expandAll ? '折叠全部' : '展开全部'}
      </button>
      <select bind:value={filterType} class="debug-filter">
        {#each FRAME_TYPES as t}
          <option value={t}>{t === 'all' ? '全部' : t}</option>
        {/each}
      </select>
      <button onclick={() => autoRefresh = !autoRefresh} class="debug-toggle" class:active={autoRefresh} title={autoRefresh ? '自动刷新中' : '已暂停'}>
        {autoRefresh ? '⏸' : '▶'}
      </button>
      <button onclick={refresh} class="debug-sm-btn">刷新</button>
      <button onclick={clearAll} class="debug-sm-btn clear">清空</button>
    </div>
  </div>

  <!-- Frame list -->
  <div class="flex-1 overflow-y-auto min-h-0 space-y-0.5" style="font-size: 11px;">
    {#if loading && frames.length === 0}
      <div class="py-12 text-center text-muted-foreground" style="font-size: 12px;">暂无数据 — 等待 IPC 交互…</div>
    {:else if visibleFrames.length === 0}
      <div class="py-12 text-center text-muted-foreground" style="font-size: 12px;">无匹配帧</div>
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
              <span class="debug-err-mark" title={frame.error}>✕</span>
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

  .debug-filter { height: 22px; padding: 0 5px; border-radius: 5px; border: 1px solid var(--border); background: var(--card); color: var(--foreground); font-size: 10.5px; font-weight: 500; cursor: pointer; }
  .debug-toggle { width: 22px; height: 22px; display: flex; align-items: center; justify-content: center; border-radius: 5px; border: 1px solid var(--border); background: var(--card); color: var(--muted-foreground); font-size: 10px; cursor: pointer; transition: all 0.12s ease; }
  .debug-toggle.active { border-color: #22C55E; color: #22C55E; background: rgba(34, 197, 94, 0.06); }
  .debug-sm-btn { height: 22px; padding: 0 7px; border-radius: 5px; border: 1px solid var(--border); background: var(--card); color: var(--muted-foreground); font-size: 10.5px; font-weight: 500; cursor: pointer; transition: all 0.12s ease; white-space: nowrap; }
  .debug-sm-btn:hover { color: var(--foreground); background: var(--muted); }
  .debug-sm-btn.clear:hover { color: var(--destructive); background: rgba(239, 68, 68, 0.08); }

  .debug-row { border-radius: 5px; border: 1px solid var(--border); background: var(--card); overflow: hidden; transition: background 0.08s ease; }
  .debug-row:hover { background: var(--surface); }

  .debug-main { display: flex; align-items: center; gap: 5px; width: 100%; padding: 4px 8px; border: none; background: transparent; color: inherit; cursor: pointer; font-size: inherit; text-align: left; }
  .debug-dir { font-weight: 700; font-size: 10px; min-width: 26px; flex-shrink: 0; }
  .debug-summary { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; color: var(--foreground); min-width: 0; }
  .debug-ms { font-family: var(--font-mono); font-size: 9.5px; color: var(--muted-foreground); text-align: right; min-width: 28px; flex-shrink: 0; }
  .debug-ms.slow { color: var(--warning); font-weight: 600; }
  .debug-ms.very-slow { color: var(--destructive); font-weight: 700; }
  .debug-bar { height: 2px; border-radius: 1px; flex-shrink: 0; opacity: 0.5; }
  .debug-err-mark { color: var(--destructive); font-weight: 700; font-size: 10px; flex-shrink: 0; }
  .debug-ts { font-family: var(--font-mono); font-size: 9px; color: var(--muted-foreground); opacity: 0.6; flex-shrink: 0; }
  .debug-id { font-family: var(--font-mono); font-size: 8.5px; color: var(--muted-foreground); opacity: 0.35; flex-shrink: 0; }
  .debug-chev { flex-shrink: 0; opacity: 0.3; transition: transform 0.12s ease; }
  .debug-chev.on { transform: rotate(180deg); }
  .debug-body { padding: 0 10px 8px; border-top: 1px solid var(--border); }
  .debug-err-body { padding: 5px 7px; margin-top: 6px; border-radius: 4px; background: rgba(239, 68, 68, 0.08); color: var(--destructive); font-size: 10.5px; font-family: var(--font-mono); user-select: text; -webkit-user-select: text; }
  .debug-json { margin-top: 6px; padding: 6px 8px; border-radius: 5px; background: var(--muted); color: var(--foreground); font-size: 10px; font-family: var(--font-mono); line-height: 1.45; overflow: auto; white-space: pre; max-height: 280px; user-select: text; -webkit-user-select: text; }
</style>
