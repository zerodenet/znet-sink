<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { listRuleSets, removeRuleSet, upsertRuleSet } from '$lib/services/config';
  import { handleAppError } from '$lib/services/core';
  import type { RuleSetProfile, RuleSetUpsert } from '$lib/types/domain';
  import { Card, CardContent } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import { Switch } from '$lib/components/ui/switch';

  let ruleSets = $state<RuleSetProfile[]>([]);
  let loading = $state(true);
  let showForm = $state(false);
  let saving = $state(false);
  let editingId = $state<string | null>(null);

  let form = $state({
    name: '',
    format: 'auto',
    kind: 'remote' as 'remote' | 'file' | 'inline',
    url: '',
    path: '',
    content: '',
    enabled: true,
  });

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
    try {
      await removeRuleSet(id);
      await refresh();
    } catch (e) {
      handleAppError(e, '删除规则集失败');
    }
  }

  function openCreate() {
    editingId = null;
    form = { name: '', format: 'auto', kind: 'remote', url: '', path: '', content: '', enabled: true };
    showForm = true;
  }

  function openEdit(rs: RuleSetProfile) {
    editingId = rs.id;
    form = {
      name: rs.name,
      format: rs.format,
      kind: rs.source.kind as 'remote' | 'file' | 'inline',
      url: rs.source.url ?? '',
      path: rs.source.path ?? '',
      content: rs.source.content ? JSON.stringify(rs.source.content, null, 2) : '',
      enabled: rs.enabled,
    };
    showForm = true;
  }

  async function handleSave() {
    if (!form.name.trim()) return;
    saving = true;
    try {
      let sourceContent: unknown = undefined;
      if (form.kind === 'inline' && form.content.trim()) {
        try {
          sourceContent = JSON.parse(form.content);
        } catch {
          alert('内联规则内容不是有效的 JSON');
          saving = false;
          return;
        }
      }

      const input: RuleSetUpsert = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        format: form.format || undefined,
        enabled: form.enabled,
        source: {
          kind: form.kind,
          url: form.kind === 'remote' ? (form.url.trim() || undefined) : undefined,
          path: form.kind === 'file' ? (form.path.trim() || undefined) : undefined,
          content: form.kind === 'inline' ? (sourceContent ?? undefined) : undefined,
        },
      };

      await upsertRuleSet(input);
      showForm = false;
      await refresh();
    } catch (e) {
      console.error('Failed to save rule set:', e);
      handleAppError(e, '保存规则集失败');
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
      <h3 class="text-sm font-bold text-foreground">规则集</h3>
      {#if store.isActionOperable('ruleSets.upsert')}
        <Button size="sm" onclick={openCreate}>
          + 新增
        </Button>
      {/if}
    </div>

    {#if loading}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
    {:else if ruleSets.length === 0 && !showForm}
      <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无规则集，点击新增添加</div>
    {:else}
      <div class="flex-1 overflow-y-auto min-h-0">
        <div class="grid grid-cols-1 gap-2">
          {#each ruleSets as rs (rs.id)}
            <div
              role="button"
              tabindex="0"
              onclick={() => openEdit(rs)}
              onkeydown={(e) => e.key === 'Enter' && openEdit(rs)}
              class="bg-muted/30 border border-card-border rounded-lg p-3 flex items-center justify-between text-left hover:bg-muted/50 transition-colors cursor-pointer"
            >
              <div class="flex flex-col gap-1">
                <div class="flex items-center gap-2">
                  <span class="text-xs font-medium text-foreground">{rs.name}</span>
                  <Badge variant="secondary" class="text-[10px]">{rs.format}</Badge>
                  <Badge variant="secondary" class="text-[10px]">{rs.source.kind}</Badge>
                  {#if !rs.enabled}
                    <Badge variant="secondary" class="text-[10px] bg-yellow-500/20 text-yellow-600">已停用</Badge>
                  {/if}
                </div>
                <span class="text-[10px] text-muted-foreground font-mono">{rs.id}</span>
              </div>
              {#if store.isActionOperable('ruleSets.remove')}
                <Button
                  variant="ghost"
                  size="sm"
                  class="text-red-500 hover:bg-red-500/10 hover:text-red-600"
                  onclick={(e: MouseEvent) => { e.stopPropagation(); handleRemove(rs.id); }}
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
      <h4 class="text-sm font-bold text-foreground mb-4">{editingId ? '编辑' : '新增'}规则集</h4>

      <div class="space-y-3">
        <div>
          <label for="rules-name" class="text-[10px] text-muted-foreground block mb-1">名称 *</label>
          <input
            id="rules-name"
            bind:value={form.name}
            placeholder="例如: 广告拦截规则"
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none"
          />
        </div>

        <div>
          <label for="rules-format" class="text-[10px] text-muted-foreground block mb-1">格式</label>
          <select
            id="rules-format"
            bind:value={form.format}
            class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border outline-none"
          >
            <option value="auto">自动检测</option>
            <option value="yaml">YAML</option>
            <option value="json">JSON</option>
            <option value="text">纯文本</option>
          </select>
        </div>

        <div>
          <span class="text-[10px] text-muted-foreground block mb-1">来源类型</span>
          <div class="flex bg-muted rounded-lg p-0.5 text-[10px] font-bold" role="group" aria-label="来源类型">
            {#each ['remote', 'file', 'inline'] as kind}
              <button
                onclick={() => form.kind = kind as typeof form.kind}
                class="flex-1 px-3 py-1 rounded-md transition-all {form.kind === kind ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
                aria-pressed={form.kind === kind}
              >
                {kind === 'remote' ? '远程' : kind === 'file' ? '文件' : '内联'}
              </button>
            {/each}
          </div>
        </div>

        {#if form.kind === 'remote'}
          <div>
            <label for="rules-url" class="text-[10px] text-muted-foreground block mb-1">URL *</label>
            <input
              id="rules-url"
              bind:value={form.url}
              placeholder="https://example.com/rules.yaml"
              class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none font-mono"
            />
          </div>
        {:else if form.kind === 'file'}
          <div>
            <label for="rules-path" class="text-[10px] text-muted-foreground block mb-1">文件路径 *</label>
            <input
              id="rules-path"
              bind:value={form.path}
              placeholder="/path/to/rules.yaml"
              class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none font-mono"
            />
          </div>
        {:else}
          <div>
            <label for="rules-content" class="text-[10px] text-muted-foreground block mb-1">内联 JSON 内容</label>
            <textarea
              id="rules-content"
              bind:value={form.content}
              placeholder={"内联规则 JSON..."}
              rows={6}
              class="w-full px-3 py-2 rounded-lg bg-muted text-xs text-foreground border border-card-border focus:border-primary outline-none font-mono resize-y"
            ></textarea>
          </div>
        {/if}

        <div class="flex items-center justify-between">
          <span class="text-[10px] text-muted-foreground">启用</span>
          <Switch
            checked={form.enabled}
            onCheckedChange={(val) => form.enabled = val}
            aria-label="启用规则"
          />
        </div>
      </div>

      <div class="flex gap-2 mt-5">
        <Button
          variant="secondary"
          size="sm"
          onclick={() => showForm = false}
          class="flex-1"
        >
          取消
        </Button>
        <Button
          size="sm"
          onclick={handleSave}
          disabled={saving || !form.name.trim()}
          class="flex-1"
        >
          {saving ? '保存中...' : '保存'}
        </Button>
      </div>
    </div>
  </div>
{/if}
