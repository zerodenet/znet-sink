<script lang="ts">
  import { listRuleSets, removeRuleSet, type RuleSet } from '$lib/services/config';

  let ruleSets = $state<RuleSet[]>([]);
  let loading = $state(true);
  let showModal = $state(false);

  async function refresh() {
    loading = true;
    try {
      ruleSets = await listRuleSets();
    } catch (e) {
      console.error('Failed to load rule sets:', e);
    } finally {
      loading = false;
    }
  }

  async function handleRemove(id: string) {
    if (!confirm('确认删除此规则集？')) return;
    await removeRuleSet(id);
    await refresh();
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="flex-1 w-full bg-card border border-card-border rounded-xl p-4 flex flex-col gap-4 animate-fade-in overflow-hidden">
  <div class="flex items-center justify-between flex-shrink-0">
    <h3 class="text-sm font-bold text-foreground">规则集</h3>
    <button
      onclick={() => showModal = true}
      class="px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-xs font-medium"
    >
      + 新增
    </button>
  </div>

  {#if loading}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
  {:else if ruleSets.length === 0}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无规则集</div>
  {:else}
    <div class="flex-1 overflow-y-auto min-h-0">
      <div class="grid grid-cols-1 gap-2">
        {#each ruleSets as ruleSet (ruleSet.id)}
          <div class="bg-muted/30 border border-card-border rounded-lg p-3 flex items-center justify-between">
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class="text-xs font-medium text-foreground">{ruleSet.name}</span>
                <span class="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{ruleSet.type}</span>
              </div>
              <span class="text-[10px] text-muted-foreground font-mono">{ruleSet.rule_count} 条规则</span>
            </div>
            <button
              onclick={() => handleRemove(ruleSet.id)}
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
        <h4 class="text-sm font-bold text-foreground mb-4">新增规则集</h4>
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
