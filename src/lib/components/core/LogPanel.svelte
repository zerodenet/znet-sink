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
    { value: 'error', label: 'ERROR' },
    { value: 'warn', label: 'WARN' },
    { value: 'info', label: 'INFO' },
    { value: 'debug', label: 'DEBUG' },
    { value: 'trace', label: 'TRACE' },
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
    const lastError = errors[errors.length - 1];
    const text = `[${formatTime(lastError.occurredAtUnixMs)}] [${lastError.source.toUpperCase()}] [${lastError.level.toUpperCase()}] ${lastError.message}`;
    navigator.clipboard.writeText(text);
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

<div class="h-full bg-card border border-card-border rounded-xl p-3 flex flex-col">
  <div class="flex items-center justify-between mb-2 flex-shrink-0">
    <div class="flex items-center gap-2 overflow-hidden">
      <div class="w-2.5 h-2.5 rounded-full bg-muted flex-shrink-0"></div>
      <span class="text-sm font-medium text-foreground truncate">运行日志</span>
    </div>
    <div class="flex items-center gap-2 flex-shrink-0">
      <button
        onclick={copyLastError}
        disabled={!logs.some(l => l.level === 'error')}
        class="text-xs px-2 py-1 rounded bg-muted text-muted-foreground hover:text-foreground transition-colors whitespace-nowrap disabled:opacity-50"
        title="复制最新错误"
      >
        复制错误
      </button>
      <button
        onclick={() => autoScroll = !autoScroll}
        class="text-xs px-2 py-1 rounded transition-colors whitespace-nowrap {autoScroll ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground'}"
      >
        自动滚动
      </button>
      <button
        onclick={handleClear}
        class="text-xs px-2 py-1 rounded bg-muted text-muted-foreground hover:text-foreground transition-colors whitespace-nowrap"
      >
        清空
      </button>
    </div>
  </div>

  <!-- 筛选栏 -->
  <div class="flex items-center gap-2 mb-2 flex-shrink-0">
    <select
      bind:value={selectedSource}
      class="text-xs px-2 py-1 rounded bg-muted border border-border text-foreground"
    >
      {#each sources as s}
        <option value={s.value}>{s.label}</option>
      {/each}
    </select>
    <select
      bind:value={selectedLevel}
      class="text-xs px-2 py-1 rounded bg-muted border border-border text-foreground"
    >
      {#each levels as l}
        <option value={l.value}>{l.label}</option>
      {/each}
    </select>
    <span class="text-xs text-muted-foreground ml-auto">{filteredLogs.length} 条</span>
  </div>

  <div class="flex-1 overflow-y-auto font-mono text-xs space-y-0.5 bg-muted/20 rounded p-2 min-h-0">
    {#if filteredLogs.length === 0}
      <div class="text-muted-foreground text-center py-4">暂无日志</div>
    {:else}
      {#each filteredLogs as log (log.id)}
        <div class="flex gap-2 leading-tight">
          <span class="text-muted-foreground whitespace-nowrap">[{formatTime(log.occurredAtUnixMs)}]</span>
          <span class="whitespace-nowrap {log.source === 'app' ? 'bg-purple-500/20 text-purple-400' : 'bg-blue-500/20 text-blue-400'} px-1 rounded">
            [{log.source.toUpperCase()}]
          </span>
          <span class="whitespace-nowrap {
            log.level === 'error' ? 'text-red-500' :
            log.level === 'warn' ? 'text-yellow-500' :
            log.level === 'info' ? 'text-green-500' :
            log.level === 'debug' ? 'text-cyan-500' :
            'text-muted-foreground'
          }">[{log.level.toUpperCase()}]</span>
          <span class="text-muted-foreground truncate">{log.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>
