<script lang="ts">
  function formatSpeed(speed: number): string {
    if (speed >= 1) return `${speed.toFixed(2)} MB/s`;
    return `${(speed * 1000).toFixed(0)} KB/s`;
  }

  const { history }: {
    history: { up: number; down: number }[];
  } = $props();

  const currentDown = $derived(history.length > 0 ? history[history.length - 1].down : 0);
  const currentUp = $derived(history.length > 0 ? history[history.length - 1].up : 0);
</script>

<div class="h-full bg-card border border-card-border rounded-xl p-3 flex flex-col">
  <div class="flex items-center justify-between mb-2 flex-shrink-0">
    <div class="flex items-center gap-2 overflow-hidden">
      <span class="text-sm font-medium text-muted-foreground truncate">实时速率</span>
    </div>
    <div class="flex items-center gap-4 text-xs flex-shrink-0">
      <div class="flex items-center gap-2">
        <div class="w-2.5 h-2.5 rounded-full bg-foreground flex-shrink-0"></div>
        <span class="text-sm font-bold text-foreground font-mono">{formatSpeed(currentDown)}</span>
      </div>
      <div class="flex items-center gap-2">
        <div class="w-2.5 h-2.5 rounded-full bg-muted-foreground flex-shrink-0"></div>
        <span class="text-sm font-bold text-muted-foreground font-mono">{formatSpeed(currentUp)}</span>
      </div>
    </div>
  </div>
  
  <div class="flex-1 relative overflow-hidden min-h-0">
    <svg viewBox="0 0 400 200" class="w-full h-full" preserveAspectRatio="none">
      <!-- 网格线 -->
      {#each [0, 1, 2, 3, 4] as i}
        <line x1="0" y1={i * 50} x2="400" y2={i * 50} stroke="currentColor" stroke-opacity="0.05" stroke-width="1" />
      {/each}
      
      <!-- 下载面积 (深色前景) -->
      <path d={`${history.map((s, i, arr) => {
        const x = (i / (arr.length - 1)) * 400;
        const y = 200 - Math.min(s.down * 40, 180);
        return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
      }).join(' ')} L 400 200 L 0 200 Z`} fill="url(#downGradient)" opacity="0.5" />
      
      <!-- 上传面积 (灰色中景) -->
      <path d={`${history.map((s, i, arr) => {
        const x = (i / (arr.length - 1)) * 400;
        const y = 200 - Math.min(s.up * 60, 180);
        return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
      }).join(' ')} L 400 200 L 0 200 Z`} fill="url(#upGradient)" opacity="0.3" />
      
      <!-- 渐变定义 -->
      <defs>
        <linearGradient id="downGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="currentColor" stop-opacity="0.7" />
          <stop offset="100%" stop-color="currentColor" stop-opacity="0.05" />
        </linearGradient>
        <linearGradient id="upGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="currentColor" stop-opacity="0.4" />
          <stop offset="100%" stop-color="currentColor" stop-opacity="0.05" />
        </linearGradient>
      </defs>
    </svg>
    
    <!-- 中心提示 -->
    {#if history.every(s => s.down < 0.01 && s.up < 0.01)}
      <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
        <div class="text-[10px] text-muted-foreground/50">等待内核网络数据...</div>
      </div>
    {/if}
    
    <!-- 底部基线 -->
    <div class="absolute bottom-0 left-0 right-0 h-px bg-muted"></div>
  </div>
</div>
