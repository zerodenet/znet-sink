<script lang="ts">
  import { getCoreStatus, startCore, stopCore } from '$lib/services/core';
  import type { CoreStatus } from '$lib/types/core';

  let status = $state<CoreStatus>({ running: false });
  let loading = $state(false);

  async function refreshStatus() {
    try {
      status = await getCoreStatus();
    } catch (e) {
      console.error('Failed to get core status:', e);
    }
  }

  async function toggleCore() {
    if (loading) return;
    loading = true;
    try {
      if (status.running) {
        await stopCore();
      } else {
        await startCore();
      }
      await refreshStatus();
    } catch (e) {
      console.error('Failed to toggle core:', e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  });
</script>

<div class="bg-card border border-card-border rounded-xl p-3 flex flex-col gap-2 h-24 overflow-hidden">
  <div class="flex items-center justify-between flex-shrink-0">
    <span class="text-sm font-medium text-muted-foreground truncate">内核状态</span>
    <div class="flex items-center gap-1.5 flex-shrink-0">
      <div class="w-2.5 h-2.5 rounded-full {status.running ? 'bg-green-500' : 'bg-muted'}"></div>
      <span class="text-sm font-bold text-foreground">{status.running ? '运行中' : '已停止'}</span>
    </div>
  </div>

  {#if status.running}
    <div class="grid grid-cols-2 gap-1 text-xs flex-shrink-0">
      <div class="flex justify-between overflow-hidden">
        <span class="text-muted-foreground truncate">PID</span>
        <span class="font-mono text-foreground truncate ml-1">{status.pid}</span>
      </div>
      <div class="flex justify-between overflow-hidden">
        <span class="text-muted-foreground truncate">连接</span>
        <span class="font-mono text-foreground truncate ml-1">{status.connections ?? '-'}</span>
      </div>
    </div>
  {/if}

  <button
    onclick={toggleCore}
    disabled={loading}
    class="w-full py-1.5 rounded-lg font-medium text-xs transition-all disabled:opacity-50 mt-auto flex-shrink-0 truncate
           {status.running
             ? 'bg-red-500/10 text-red-500 hover:bg-red-500/20 border border-red-500/30'
             : 'bg-green-500/10 text-green-500 hover:bg-green-500/20 border border-green-500/30'}"
  >
    {loading ? '处理中...' : status.running ? '停止内核' : '启动内核'}
  </button>
</div>
