<script lang="ts">
  import { getLogs, clearLogs } from '$lib/services/core';
  import type { LogEntry, LogLevel, LogSource } from '$lib/types/logs';

  let logs = $state<LogEntry[]>([]);
  let autoScroll = $state(true);
  let selectedSource = $state<LogSource | 'all'>('all');
  let selectedLevel = $state<LogLevel | 'all'>('all');

  const sources: Array<{ value: LogSource | 'all'; label: string }> = [
    { value: 'all', label: '全部' },
    { value: 'app', label: 'APP' },
    { value: 'core', label: 'CORE' },
  ];

  const levels: Array<{ value: LogLevel | 'all'; label: string }> = [
    { value: 'all', label: '全部' },
    { value: 'error', label: 'ERR' },
    { value: 'warn', label: 'WRN' },
    { value: 'info', label: 'INF' },
    { value: 'debug', label: 'DBG' },
    { value: 'trace', label: 'TRC' },
  ];

  const filteredLogs = $derived(logs.filter(log => {
    if (selectedSource !== 'all' && log.source !== selectedSource) return false;
    if (selectedLevel !== 'all' && log.level !== selectedLevel) return false;
    return true;
  }));

  async function refreshLogs() {
    try {
      logs = await getLogs();
    } catch (e) {
      console.error('Failed to get logs:', e);
    }
  }

  async function handleClear() {
    await clearLogs();
    await refreshLogs();
  }

  function copyLastError() {
    const errors = logs.filter(l => l.level === 'error');
    if (errors.length === 0) return;
    const last = errors[errors.length - 1];
    navigator.clipboard.writeText(
      `[${formatTime(last.occurredAtUnixMs)}] [${last.source.toUpperCase()}] [${last.level.toUpperCase()}] ${last.message}`
    );
  }

  function formatTime(ms: number): string {
    return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false });
  }

  $effect(() => {
    refreshLogs();
    const interval = setInterval(refreshLogs, 2000);
    return () => clearInterval(interval);
  });
</script>

<div class="log-panel">
  <!-- Toolbar -->
  <div class="log-toolbar">
    <!-- Left: title + filters -->
    <div class="flex items-center gap-2">
      <span class="log-title">运行日志</span>
      <span class="log-sep"></span>

      <!-- Source filter -->
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

      <!-- Level filter -->
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

    <!-- Right: count + actions -->
    <div class="flex items-center gap-1.5">
      <span class="log-count">{filteredLogs.length}</span>

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

  <!-- Log body: terminal-style -->
  <div class="log-body">
    {#if filteredLogs.length === 0}
      <div class="log-empty">暂无日志</div>
    {:else}
      {#each filteredLogs as log, index (`${log.id}-${log.occurredAtUnixMs}-${index}`)}
        <div class="log-row">
          <span class="log-time">{formatTime(log.occurredAtUnixMs)}</span>
          <span class="log-src" class:app={log.source === 'app'} class:core={log.source === 'core'}>
            {log.source.toUpperCase()}
          </span>
          <span class="log-lvl" class:err={log.level === 'error'} class:wrn={log.level === 'warn'} class:inf={log.level === 'info'} class:dbg={log.level === 'debug'}>
            {log.level.slice(0,3).toUpperCase()}
          </span>
          <span class="log-msg">{log.message}</span>
        </div>
      {/each}
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

  /* ---- Toolbar ---- */
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

  /* ---- Filter groups ---- */
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

  /* ---- Action buttons ---- */
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

  /* ---- Log body ---- */
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
    gap: 5px;
    padding: 2px 0;
    font-size: 12.5px;
    line-height: 1.6;
    border-radius: 3px;
    transition: background 0.1s ease;
  }

  .log-row:hover {
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }
</style>
