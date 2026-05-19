<script lang="ts">
  import { listProxyConfigs, removeProxyConfig, type ProxyConfig } from '$lib/services/config';

  let configs = $state<ProxyConfig[]>([]);
  let loading = $state(true);
  let showModal = $state(false);

  async function refresh() {
    loading = true;
    try {
      configs = await listProxyConfigs();
    } catch (e) {
      console.error('Failed to load proxy configs:', e);
    } finally {
      loading = false;
    }
  }

  async function handleRemove(id: string) {
    if (!confirm('确认删除此配置？')) return;
    await removeProxyConfig(id);
    await refresh();
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="flex-1 w-full bg-card border border-card-border rounded-xl p-4 flex flex-col gap-4 animate-fade-in overflow-hidden">
  <div class="flex items-center justify-between flex-shrink-0">
    <h3 class="text-sm font-bold text-foreground">代理配置</h3>
    <button
      onclick={() => showModal = true}
      class="px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-xs font-medium"
    >
      + 新增
    </button>
  </div>

  {#if loading}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
  {:else if configs.length === 0}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无配置</div>
  {:else}
    <div class="flex-1 overflow-y-auto min-h-0">
      <div class="grid grid-cols-1 gap-2">
        {#each configs as config (config.id)}
          <div class="bg-muted/30 border border-card-border rounded-lg p-3 flex items-center justify-between">
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class="text-xs font-medium text-foreground">{config.name}</span>
                <span class="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{config.type}</span>
              </div>
              <span class="text-[10px] text-muted-foreground font-mono">{config.server}:{config.port}</span>
            </div>
            <button
              onclick={() => handleRemove(config.id)}
              class="text-[10px] px-2 py-1 rounded text-red-500 hover:bg-red-500/10"
            >
              删除
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if showModal}
    <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onclick={() => showModal = false}>
      <div class="bg-card border border-card-border rounded-xl p-4 w-96" onclick={(e) => e.stopPropagation()}>
        <h4 class="text-sm font-bold text-foreground mb-4">新增代理配置</h4>
        <p class="text-xs text-muted-foreground">功能开发中...</p>
        <button
          onclick={() => showModal = false}
          class="mt-4 w-full py-2 rounded-lg bg-muted text-muted-foreground text-xs font-medium"
        >
          关闭
        </button>
      </div>
    </div>
  {/if}
</div>
