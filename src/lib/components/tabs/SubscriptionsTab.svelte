<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { handleAppError } from '$lib/services/core';
  import { listSubscriptions, syncSubscription, removeSubscription, upsertSubscription } from '$lib/services/config';
  import type { SubscriptionProfile, SubscriptionUpsert } from '$lib/types/domain';
  import { Card, CardContent } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';

  let subscriptions = $state<SubscriptionProfile[]>([]);
  let loading = $state(true);
  let syncingId = $state<string | null>(null);
  let showForm = $state(false);
  let saving = $state(false);
  let editingId = $state<string | null>(null);

  let form = $state({ name: '', url: '', format: 'auto' });

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
      handleAppError(e, '同步订阅失败');
    } finally {
      syncingId = null;
    }
  }

  async function handleRemove(id: string) {
    if (!confirm('确认删除此订阅？')) return;
    try {
      await removeSubscription(id);
      await refresh();
    } catch (e) {
      handleAppError(e, '删除订阅失败');
    }
  }

  function openCreate() {
    editingId = null;
    form = { name: '', url: '', format: 'auto' };
    showForm = true;
  }

  function openEdit(sub: SubscriptionProfile) {
    editingId = sub.id;
    form = { name: sub.name, url: sub.url, format: sub.format };
    showForm = true;
  }

  async function handleSave() {
    if (!form.name.trim() || !form.url.trim()) return;
    saving = true;
    try {
      const input: SubscriptionUpsert = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        url: form.url.trim(),
        format: form.format || undefined,
      };

      await upsertSubscription(input);
      showForm = false;
      await refresh();
    } catch (e) {
      console.error('Failed to save subscription:', e);
      handleAppError(e, '保存订阅失败');
    } finally {
      saving = false;
    }
  }

  function formatTime(ms?: number): string {
    if (!ms) return '-';
    return new Date(ms).toLocaleString('zh-CN');
  }

  $effect(() => {
    refresh();
  });
</script>

<Card class="flex-1 overflow-hidden">
  <CardContent class="p-4 h-full flex flex-col gap-4 animate-fade-in">
    <div class="flex items-center justify-between flex-shrink-0">
      <h3 class="text-sm font-bold text-foreground">订阅管理</h3>
      <Button size="sm" onclick={openCreate}>
        + 新增
      </Button>
    </div>

    {#if loading}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
    {:else if subscriptions.length === 0 && !showForm}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无订阅，点击新增添加</div>
    {:else}
      <div class="flex-1 overflow-y-auto min-h-0">
        <div class="grid grid-cols-1 gap-2">
          {#each subscriptions as sub (sub.id)}
            <div
              role="button"
              tabindex="0"
              onclick={() => openEdit(sub)}
              onkeydown={(e) => e.key === 'Enter' && openEdit(sub)}
              class="bg-muted/30 border border-card-border rounded-lg p-3 flex items-center justify-between text-left hover:bg-muted/50 transition-colors cursor-pointer"
            >
              <div class="flex flex-col gap-1 min-w-0 flex-1">
                <span class="text-xs font-medium text-foreground">{sub.name}</span>
                <div class="flex items-center gap-2 text-[10px] text-muted-foreground">
                  <span class="font-mono truncate max-w-[250px]">{sub.url}</span>
                  <span>·</span>
                  <span>上次: {formatTime(sub.lastSyncAtUnixMs)}</span>
                </div>
                {#if sub.lastError}
                  <span class="text-[10px] text-red-500">{sub.lastError}</span>
                {/if}
              </div>
              <div class="flex items-center gap-2 flex-shrink-0 ml-2">
                <Button
                  variant="ghost"
                  size="sm"
                  class="text-green-500 hover:bg-green-500/10 hover:text-green-600"
                  onclick={(e: MouseEvent) => { e.stopPropagation(); handleSync(sub.id); }}
                  disabled={syncingId === sub.id}
                >
                  {syncingId === sub.id ? '同步中' : '同步'}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  class="text-red-500 hover:bg-red-500/10 hover:text-red-600"
                  onclick={(e: MouseEvent) => { e.stopPropagation(); handleRemove(sub.id); }}
                >
                  删除
                </Button>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </CardContent>
</Card>

{#if showForm}
  <div 
    class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" 
    onclick={() => showForm = false}
    onkeydown={(e) => e.key === 'Escape' && (showForm = false)}
    role="button"
    tabindex="0"
    aria-label="关闭弹窗"
  >
    <div 
      class="bg-card border border-card-border rounded-xl p-5 w-[420px]" 
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      <h4 class="text-sm font-bold text-foreground mb-4">{editingId ? '编辑' : '新增'}订阅</h4>

      <div class="space-y-3">
        <div>
          <label for="sub-name" class="text-[10px] text-muted-foreground block mb-1">名称 *</label>
          <input
            id="sub-name"
            bind:value={form.name}
            placeholder="例如: 官方订阅"
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none"
          />
        </div>

        <div>
          <label for="sub-url" class="text-[10px] text-muted-foreground block mb-1">订阅 URL *</label>
          <input
            id="sub-url"
            bind:value={form.url}
            placeholder="https://example.com/subscription"
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none font-mono"
          />
        </div>

        <div>
          <label for="sub-format" class="text-[10px] text-muted-foreground block mb-1">格式</label>
          <select
            id="sub-format"
            bind:value={form.format}
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border outline-none"
          >
            <option value="auto">自动检测</option>
            <option value="zero-base64-json">Zero Base64 JSON</option>
          </select>
        </div>
      </div>

      <div class="flex gap-2 mt-5">
        <button
          onclick={() => showForm = false}
          class="flex-1 py-2 rounded-lg bg-muted text-muted-foreground text-xs font-medium"
        >
          取消
        </button>
        <button
          onclick={handleSave}
          disabled={saving || !form.name.trim() || !form.url.trim()}
          class="flex-1 py-2 rounded-lg bg-primary text-primary-foreground text-xs font-medium disabled:opacity-50"
        >
          {saving ? '保存中...' : '保存'}
        </button>
      </div>
    </div>
  </div>
{/if}
