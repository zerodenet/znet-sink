<script lang="ts">
  import { getGuiCapabilitiesSnapshot } from '$lib/services/core';
  import type { GuiCapabilitySnapshot } from '$lib/types/capability';

  let snapshot = $state<GuiCapabilitySnapshot | null>(null);
  let loading = $state(true);

  async function refresh() {
    loading = true;
    try {
      snapshot = await getGuiCapabilitiesSnapshot();
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
  {:else if !snapshot}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无可用能力</div>
  {:else}
    <div class="flex-1 overflow-y-auto min-h-0 space-y-4">
      <!-- 管理能力 -->
      <div>
        <h4 class="text-xs font-medium text-foreground mb-2">管理能力</h4>
        <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
          {#each snapshot.management as item (item.key)}
            <div class="bg-muted/30 border border-card-border rounded-lg p-2 flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <span class="text-[10px] font-medium text-foreground">{item.key}</span>
                <div class="w-1.5 h-1.5 rounded-full {item.enabled ? 'bg-green-500' : 'bg-muted'}"></div>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- 代理特性 -->
      <div>
        <h4 class="text-xs font-medium text-foreground mb-2">代理特性</h4>
        <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
          {#each snapshot.proxyFeatures as item (item.key)}
            <div class="bg-muted/30 border border-card-border rounded-lg p-2 flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <span class="text-[10px] font-medium text-foreground">{item.key}</span>
                <div class="w-1.5 h-1.5 rounded-full {item.enabled ? 'bg-green-500' : 'bg-yellow-500'}"></div>
              </div>
              {#if item.reason}
                <span class="text-[9px] text-muted-foreground">{item.reason}</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      {#if snapshot.activeProxyConfigId}
        <div class="text-[10px] text-muted-foreground">
          活跃配置: <span class="font-mono">{snapshot.activeProxyConfigId}</span>
        </div>
      {/if}
    </div>
  {/if}
</div>
