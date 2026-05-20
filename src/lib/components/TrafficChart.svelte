<script lang="ts">
  import { overviewData } from '$lib/services/overview-data.svelte';

  function formatSpeed(speed: number): string {
    if (speed >= 1) return `${speed.toFixed(2)} MB/s`;
    return `${(speed * 1000).toFixed(0)} KB/s`;
  }

  function formatTraffic(mb: number): string {
    if (mb >= 1000) return `${(mb / 1000).toFixed(2)} GB`;
    if (mb >= 1) return `${mb.toFixed(1)} MB`;
    return `${(mb * 1000).toFixed(0)} KB`;
  }

  const { history }: {
    history: { up: number; down: number }[];
  } = $props();

  const currentDown = $derived(history.length > 0 ? history[history.length - 1].down : 0);
  const currentUp = $derived(history.length > 0 ? history[history.length - 1].up : 0);
  const hasTraffic = $derived(history.some(s => s.down >= 0.01 || s.up >= 0.01));
</script>

<div class="h-full bg-card border border-card-border rounded-xl p-3 flex flex-col">
  <div class="flex items-center justify-between mb-2 flex-shrink-0">
    <div class="flex items-center gap-2 overflow-hidden">
      <span class="text-sm font-medium text-muted-foreground truncate">实时速率</span>
    </div>
    <div class="flex items-center gap-4 text-xs flex-shrink-0">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-blue-500 flex-shrink-0 shadow-sm shadow-blue-500/30 {hasTraffic ? 'animate-pulse' : ''}"></div>
        <span class="text-xs font-bold text-foreground font-mono">{formatSpeed(currentDown)}</span>
      </div>
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-green-500 flex-shrink-0 shadow-sm shadow-green-500/30 {hasTraffic ? 'animate-pulse' : ''}"></div>
        <span class="text-xs font-bold text-muted-foreground font-mono">{formatSpeed(currentUp)}</span>
      </div>
    </div>
  </div>

  <div class="flex items-center gap-4 text-[10px] mb-2 flex-shrink-0">
    <div class="flex items-center gap-1.5">
      <span class="text-muted-foreground">下行总计</span>
      <span class="font-mono font-bold text-blue-500/90">{formatTraffic(overviewData.totalDownMB)}</span>
    </div>
    <div class="flex items-center gap-1.5">
      <span class="text-muted-foreground">上行总计</span>
      <span class="font-mono font-bold text-green-500/90">{formatTraffic(overviewData.totalUpMB)}</span>
    </div>
    <div class="flex items-center gap-1.5 ml-auto">
      <span class="text-muted-foreground">连接</span>
      <span class="font-mono font-bold text-foreground">{overviewData.activeConnections}</span>
    </div>
  </div>

  <div class="flex-1 relative overflow-hidden min-h-0 rounded-lg bg-muted/30">
    <svg viewBox="0 0 400 200" class="w-full h-full" preserveAspectRatio="none">
      {#each [0, 1, 2, 3, 4] as i}
        <line x1="0" y1={i * 50} x2="400" y2={i * 50} stroke="currentColor" stroke-opacity="0.08" stroke-width="1" />
      {/each}

      <path d={`${history.map((s, i, arr) => {
        const x = (i / (arr.length - 1)) * 400;
        const y = 200 - Math.min(s.down * 40, 180);
        return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
      }).join(' ')} L 400 200 L 0 200 Z`} fill="url(#downGradient)" opacity="0.6" />

      <path d={`${history.map((s, i, arr) => {
        const x = (i / (arr.length - 1)) * 400;
        const y = 200 - Math.min(s.up * 60, 180);
        return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
      }).join(' ')} L 400 200 L 0 200 Z`} fill="url(#upGradient)" opacity="0.4" />

      <defs>
        <linearGradient id="downGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="#3b82f6" stop-opacity="0.8" />
          <stop offset="100%" stop-color="#3b82f6" stop-opacity="0.05" />
        </linearGradient>
        <linearGradient id="upGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="#22c55e" stop-opacity="0.6" />
          <stop offset="100%" stop-color="#22c55e" stop-opacity="0.05" />
        </linearGradient>
      </defs>
    </svg>

    {#if !hasTraffic}
      <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
        <div class="text-[10px] text-muted-foreground/40">等待内核网络数据...</div>
      </div>
    {/if}

    <div class="absolute bottom-0 left-0 right-0 h-px bg-muted-foreground/10"></div>
  </div>
</div>
