<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { listProxyConfigs, removeProxyConfig, upsertProxyConfig, importProxyConfig } from '$lib/services/config';
  import { handleAppError } from '$lib/services/core';
  import type { ProxyConfigProfile, ProxyConfigUpsert } from '$lib/types/domain';
  import { Card, CardContent } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';

  let configs = $state<ProxyConfigProfile[]>([]);
  let loading = $state(true);
  let showForm = $state(false);
  let saving = $state(false);
  let editingId = $state<string | null>(null);

  let form = $state({ name: '', format: 'json', content: '', active: false });

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
    try {
      await removeProxyConfig(id);
      await refresh();
    } catch (e) {
      handleAppError(e, '删除代理配置失败');
    }
  }

  function openCreate() {
    editingId = null;
    form = { name: '', format: 'json', content: '', active: false };
    showForm = true;
  }

  function openEdit(config: ProxyConfigProfile) {
    editingId = config.id;
    form = {
      name: config.name,
      format: config.format,
      content: config.content ? JSON.stringify(config.content, null, 2) : '',
      active: config.active,
    };
    showForm = true;
  }

  async function handleSave() {
    if (!form.name.trim()) return;
    saving = true;
    try {
      let content: unknown = undefined;
      if (form.content.trim()) {
        try {
          content = JSON.parse(form.content);
        } catch {
          alert('代理配置内容不是有效的 JSON');
          saving = false;
          return;
        }
      }

      const input: ProxyConfigUpsert = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        format: form.format || undefined,
        content: content ?? undefined,
        active: form.active || undefined,
      };

      await upsertProxyConfig(input);
      showForm = false;
      await refresh();
    } catch (e) {
      console.error('Failed to save proxy config:', e);
      handleAppError(e, '保存代理配置失败');
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    refresh();
  });
</script>

<Card class="flex-1 overflow-hidden">
  <CardContent class="p-4 h-full flex flex-col gap-4 animate-fade-in">
    <div class="flex items-center justify-between flex-shrink-0">
      <h3 class="text-sm font-bold text-foreground">代理配置</h3>
      {#if store.isActionOperable('proxyConfig.upsert')}
        <Button size="sm" onclick={openCreate}>
          + 新增
        </Button>
      {/if}
    </div>

    {#if loading}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
    {:else if configs.length === 0 && !showForm}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无配置，点击新增导入或创建</div>
    {:else}
      <div class="flex-1 overflow-y-auto min-h-0">
        <div class="grid grid-cols-1 gap-2">
          {#each configs as config (config.id)}
            <div
              role="button"
              tabindex="0"
              onclick={() => openEdit(config)}
              onkeydown={(e) => e.key === 'Enter' && openEdit(config)}
              class="bg-muted/30 border border-card-border rounded-lg p-3 flex items-center justify-between text-left hover:bg-muted/50 transition-colors cursor-pointer"
            >
              <div class="flex flex-col gap-1">
                <div class="flex items-center gap-2">
                  <span class="text-xs font-medium text-foreground">{config.name}</span>
                  <Badge variant="secondary" class="text-[10px]">{config.format}</Badge>
                  {#if config.active}
                    <Badge variant="secondary" class="text-[10px] bg-green-500/20 text-green-600">活跃</Badge>
                  {/if}
                </div>
                <span class="text-[10px] text-muted-foreground font-mono">{config.id}</span>
              </div>
              {#if store.isActionOperable('proxyConfig.remove')}
                <Button
                  variant="ghost"
                  size="sm"
                  class="text-red-500 hover:bg-red-500/10 hover:text-red-600"
                  onclick={(e: MouseEvent) => { e.stopPropagation(); handleRemove(config.id); }}
                >
                  删除
                </Button>
              {/if}
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
      class="bg-card border border-card-border rounded-xl p-5 w-[420px] max-h-[80vh] overflow-y-auto" 
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      <h4 class="text-sm font-bold text-foreground mb-4">{editingId ? '编辑' : '新增'}代理配置</h4>

      <div class="space-y-3">
        <div>
          <label for="profile-name" class="text-[10px] text-muted-foreground block mb-1">名称 *</label>
          <input
            id="profile-name"
            bind:value={form.name}
            placeholder="例如: 香港节点配置"
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none"
          />
        </div>

        <div>
          <label for="profile-format" class="text-[10px] text-muted-foreground block mb-1">格式</label>
          <select
            id="profile-format"
            bind:value={form.format}
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border outline-none"
          >
            <option value="json">JSON (标准)</option>
            <option value="zero">Zero 内核格式</option>
          </select>
        </div>

        <div>
          <label for="profile-content" class="text-[10px] text-muted-foreground block mb-1">JSON 内容</label>
          <textarea
            id="profile-content"
            bind:value={form.content}
            placeholder='粘贴代理配置 JSON...'
            rows={10}
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none font-mono resize-y"
          ></textarea>
        </div>

        <div class="flex items-center justify-between">
          <span class="text-[10px] text-muted-foreground">设为活跃配置</span>
          <button
            onclick={() => form.active = !form.active}
            class="w-9 h-5 rounded-full relative transition-colors {form.active ? 'bg-primary' : 'bg-muted'}"
            aria-label="设为活跃配置"
            role="switch"
            aria-checked={form.active}
          >
            <div class="w-4 h-4 rounded-full bg-white absolute top-0.5 transition-all shadow {form.active ? 'left-4' : 'left-0.5'}"></div>
          </button>
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
          disabled={saving || !form.name.trim()}
          class="flex-1 py-2 rounded-lg bg-primary text-primary-foreground text-xs font-medium disabled:opacity-50"
        >
          {saving ? '保存中...' : '保存'}
        </button>
      </div>
    </div>
  </div>
{/if}
