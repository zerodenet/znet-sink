<script lang="ts">
  import { queryFlows, closeFlow, type FlowInfo } from '$lib/services/core';
  import { store } from '$lib/services/store.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';

  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';

  let flows = $state<FlowInfo[]>([]);
  let loading = $state(true);
  let closingId = $state<string | null>(null);

  async function refresh() {
    loading = true;
    try {
      flows = await queryFlows();
    } catch (e) {
      console.error('Failed to query flows:', e);
    } finally {
      loading = false;
    }
  }

  async function handleClose(flowId: string) {
    closingId = flowId;
    try {
      await closeFlow(flowId);
      flows = flows.filter(f => f.flowId !== flowId);
    } catch (e) {
      console.error('Failed to close flow:', e);
    } finally {
      closingId = null;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  function formatDuration(startedAtMs: number): string {
    const elapsed = Date.now() - startedAtMs;
    if (elapsed < 0) return '0s';
    const sec = Math.floor(elapsed / 1000);
    if (sec < 60) return `${sec}s`;
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ${sec % 60}s`;
    const hr = Math.floor(min / 60);
    return `${hr}h ${min % 60}m`;
  }

  $effect(() => {
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => clearInterval(interval);
  });
</script>

<div class="bg-card border border-card-border rounded-xl p-4 h-full flex flex-col gap-4 animate-fade-in overflow-hidden shadow-sm transition-all duration-200 hover:shadow">
    <div class="flex items-center justify-between flex-shrink-0">
      <div class="flex items-center gap-2">
        <h3 class="text-sm font-bold text-foreground">活跃连接</h3>
        <Badge variant="secondary" class="font-mono text-[10px]">
          {overviewData.activeConnections} 个
        </Badge>
      </div>
      <Button variant="ghost" size="sm" onclick={refresh}>
        刷新
      </Button>
    </div>

    {#if loading && flows.length === 0}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
    {:else if flows.length === 0}
      <div class="flex-1 flex flex-col items-center justify-center gap-2 text-muted-foreground py-8">
        <span class="text-xs">无活跃连接</span>
        <span class="text-[10px]">内核未运行或暂无流量</span>
      </div>
    {:else}
      <div class="flex-1 overflow-y-auto min-h-0">
        <div class="grid grid-cols-1 gap-1.5">
           {#each flows as flow (flow.flowId)}
              <div class="bg-muted/10 rounded-lg p-2.5 flex items-center justify-between gap-2 hover:bg-muted/30 transition-all duration-200 group hover:shadow-sm">
               <div class="flex flex-col gap-0.5 min-w-0 flex-1">
                 <div class="flex items-center gap-2">
                   <span class="text-[10px] font-mono font-bold text-foreground truncate">{flow.flowId}</span>
                   <Badge variant="secondary" class="text-[9px] px-1.5 py-0.5 uppercase">
                     {flow.protocol}
                   </Badge>
                 </div>
                 <div class="flex items-center gap-2 text-[9px] text-muted-foreground">
                   <span class="truncate">{flow.source} → {flow.destination}</span>
                 </div>
                 <div class="flex items-center gap-3 text-[9px] text-muted-foreground">
                   <span class="text-green-500/80 font-medium">↑ {formatBytes(flow.bytesUp)}</span>
                   <span class="text-blue-500/80 font-medium">↓ {formatBytes(flow.bytesDown)}</span>
                   <span class="text-foreground/60">{formatDuration(flow.startedAtUnixMs)}</span>
                 </div>
               </div>
                {#if store.isActionOperable('core.flow.close')}
                  <Button
                    variant="ghost"
                    size="sm"
                    class="text-red-500 hover:bg-red-500/10 hover:text-red-600"
                    onclick={() => handleClose(flow.flowId)}
                    disabled={closingId === flow.flowId}
                  >
                    {closingId === flow.flowId ? '关闭中' : '关闭'}
                  </Button>
                {/if}
             </div>
           {/each}
         </div>
       </div>
     {/if}
   </div>
