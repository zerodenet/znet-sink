<script lang="ts">
  import { getCapabilities, type Capability } from '$lib/services/core';

  let capabilities = $state<Capability[]>([]);
  let loading = $state(true);

  async function refresh() {
    loading = true;
    try {
      capabilities = await getCapabilities();
    } catch (e) {
      console.error('Failed to load capabilities:', e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="flex-1 w-full bg-card border border-card-border rounded-xl p-4 flex flex-col gap-4 animate-fade-in overflow-hidden">
  <div class="flex items-center justify-between flex-shrink-0">
    <h3 class="text-sm font-bold text-foreground">能力快照</h3>
    <button
      onclick={refresh}
      class="px-3 py-1.5 rounded-lg bg-muted text-muted-foreground hover:text-foreground text-xs font-medium"
    >
      刷新
    </button>
  </div>

  {#if loading}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
  {:else if capabilities.length === 0}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无可用能力</div>
  {:else}
    <div class="flex-1 overflow-y-auto min-h-0">
      <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
        {#each capabilities as cap (cap.id)}
          <div class="bg-muted/30 border border-card-border rounded-lg p-3 flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <span class="text-xs font-medium text-foreground">{cap.name}</span>
              <div class="w-2 h-2 rounded-full {!cap.available ? 'bg-muted' : cap.enabled ? 'bg-green-500' : 'bg-yellow-500'}"></div>
            </div>
            <p class="text-[10px] text-muted-foreground line-clamp-2">{cap.description}</p>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
