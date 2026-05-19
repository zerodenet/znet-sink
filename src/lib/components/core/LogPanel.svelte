<script lang="ts">
  import { getLogs, clearLogs } from '$lib/services/core';
  import type { LogEntry } from '$lib/types/core';

  let logs = $state<LogEntry[]>([]);
  let autoScroll = $state(true);

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

  function formatTime(ts: number): string {
    return new Date(ts * 1000).toLocaleTimeString('zh-CN', { hour12: false });
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
      <span class="text-sm font-medium text-foreground truncate">内核日志</span>
    </div>
    <div class="flex items-center gap-2 flex-shrink-0">
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

  <div class="flex-1 overflow-y-auto font-mono text-xs space-y-0.5 bg-muted/20 rounded p-2 min-h-0">
    {#if logs.length === 0}
      <div class="text-muted-foreground text-center py-4">暂无日志</div>
    {:else}
      {#each logs as log (log.id)}
        <div class="flex gap-2 leading-tight">
          <span class="text-muted-foreground whitespace-nowrap">[{formatTime(log.timestamp)}]</span>
          <span class="whitespace-nowrap {
            log.level === 'error' ? 'text-red-500' :
            log.level === 'warn' ? 'text-yellow-500' :
            log.level === 'info' ? 'text-green-500' :
            'text-muted-foreground'
          }">[{log.level.toUpperCase()}]</span>
          <span class="text-muted-foreground truncate">{log.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>
