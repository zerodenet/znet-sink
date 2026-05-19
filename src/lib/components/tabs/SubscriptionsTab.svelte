<script lang="ts">
  import { listSubscriptions, syncSubscription, removeSubscription, type Subscription } from '$lib/services/config';

  let subscriptions = $state<Subscription[]>([]);
  let loading = $state(true);
  let syncingId = $state<string | null>(null);
  let showModal = $state(false);

  async function refresh() {
    loading = true;
    try {
      subscriptions = await listSubscriptions();
    } catch (e) {
      console.error('Failed to load subscriptions:', e);
    } finally {
      loading = false;
    }
  }

  async function handleSync(id: string) {
    syncingId = id;
    try {
      await syncSubscription(id);
      await refresh();
    } catch (e) {
      console.error('Failed to sync subscription:', e);
    } finally {
      syncingId = null;
    }
  }

  async function handleRemove(id: string) {
    if (!confirm('确认删除此订阅？')) return;
    await removeSubscription(id);
    await refresh();
  }

  function formatTime(ts?: number): string {
    if (!ts) return '-';
    return new Date(ts * 1000).toLocaleString('zh-CN');
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="flex-1 w-full bg-card border border-card-border rounded-xl p-4 flex flex-col gap-4 animate-fade-in overflow-hidden">
  <div class="flex items-center justify-between flex-shrink-0">
    <h3 class="text-sm font-bold text-foreground">订阅管理</h3>
    <button
      onclick={() => showModal = true}
      class="px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-xs font-medium"
    >
      + 新增
    </button>
  </div>

  {#if loading}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
  {:else if subscriptions.length === 0}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无订阅</div>
  {:else}
    <div class="flex-1 overflow-y-auto min-h-0">
      <div class="grid grid-cols-1 gap-2">
        {#each subscriptions as sub (sub.id)}
          <div class="bg-muted/30 border border-card-border rounded-lg p-3 flex items-center justify-between">
            <div class="flex flex-col gap-1">
              <span class="text-xs font-medium text-foreground">{sub.name}</span>
              <div class="flex items-center gap-2 text-[10px] text-muted-foreground">
                <span>{sub.node_count} 个节点</span>
                <span>·</span>
                <span>上次同步: {formatTime(sub.last_sync)}</span>
              </div>
            </div>
            <div class="flex items-center gap-2">
              <button
                onclick={() => handleSync(sub.id)}
                disabled={syncingId === sub.id}
                class="text-[10px] px-2 py-1 rounded text-green-500 hover:bg-green-500/10 disabled:opacity-50"
              >
                {syncingId === sub.id ? '同步中' : '同步'}
              </button>
              <button
                onclick={() => handleRemove(sub.id)}
                class="text-[10px] px-2 py-1 rounded text-red-500 hover:bg-red-500/10"
              >
                删除
              </button>
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if showModal}
    <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onclick={() => showModal = false}>
      <div class="bg-card border border-card-border rounded-xl p-4 w-96" onclick={(e) => e.stopPropagation()}>
        <h4 class="text-sm font-bold text-foreground mb-4">新增订阅</h4>
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
