<script lang="ts">
  const { 
    label, 
    value, 
    color = 'default', 
    chartData = null 
  }: {
    label: string;
    value: string;
    color?: 'green' | 'red' | 'yellow' | 'default';
    chartData?: number[] | null;
  } = $props();

  const parsed = $derived(value.match(/^([\d.]+)\s*(.+)$/));
  const num = $derived(parsed ? parsed[1] : value);
  const unit = $derived(parsed && parsed[2] ? parsed[2] : '');
</script>

<div class="bg-card border border-card-border rounded-xl p-3 flex flex-col h-24 overflow-hidden">
  <div class="flex items-center justify-between mb-1.5 flex-shrink-0">
    <span class="text-sm font-medium text-muted-foreground truncate">{label}</span>
  </div>
  <div class="mb-1 flex-shrink-0">
    {#if unit}
      <span class="text-3xl font-bold text-foreground">{num}</span>
      <span class="text-xs text-muted-foreground ml-1 font-medium">{unit}</span>
    {:else}
      <span class="text-2xl font-bold text-foreground">{num}</span>
    {/if}
  </div>
  {#if chartData}
    <div class="h-6 w-full mt-auto">
      <svg viewBox="0 0 100 24" class="w-full h-full" preserveAspectRatio="none">
        <path
          d={chartData.map((v, i, arr) => {
            const x = (i / (arr.length - 1)) * 100;
            const y = 24 - Math.min(v, 22);
            return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
          }).join(' ')}
          fill="none"
          stroke={color === 'green' ? '#22c55e' : color === 'red' ? '#ef4444' : color === 'yellow' ? '#eab308' : 'currentColor'}
          stroke-width="1.5"
        />
      </svg>
    </div>
  {/if}
</div>
