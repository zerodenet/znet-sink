<script lang="ts">
  import { getCoreConfig } from '$lib/services/core';

  let config = $state<Record<string, unknown> | null>(null);

  async function refreshConfig() {
    try {
      config = await getCoreConfig();
    } catch (e) {
      console.error('Failed to get core config:', e);
    }
  }

  $effect(() => {
    refreshConfig();
  });
</script>

<div class="bg-card border border-card-border rounded-xl p-4">
  <h3 class="text-sm font-bold text-foreground mb-4">内核配置</h3>
  
  {#if !config}
    <div class="text-xs text-muted-foreground">加载中...</div>
  {:else}
    <div class="space-y-3 text-xs">
      {#each Object.entries(config) as [key, value]}
        <div class="flex justify-between items-center">
          <span class="text-muted-foreground">{key}</span>
          <span class="font-mono text-foreground truncate max-w-[200px]">
            {typeof value === 'object' ? JSON.stringify(value) : String(value)}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>
