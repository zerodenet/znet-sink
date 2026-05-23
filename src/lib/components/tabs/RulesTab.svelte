<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { listRuleSets, removeRuleSet, upsertRuleSet } from '$lib/services/config';
  import { handleAppError } from '$lib/services/core';
  import type { RuleSetProfile, RuleSetUpsert } from '$lib/types/domain';

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

  const kindLabels: Record<string, string> = { remote: '远程', file: '文件', inline: '内联' };

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

<div class="desk-card flex-1 overflow-hidden flex flex-col animate-fade-in">
  <!-- Panel header -->
  <div class="panel-header">
    <span class="panel-title">规则集</span>
    {#if store.isActionOperable('ruleSets.upsert')}
      <button class="action-btn" onclick={openCreate}>
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <line x1="6" y1="1" x2="6" y2="11"/><line x1="1" y1="6" x2="11" y2="6"/>
        </svg>
        新增
      </button>
    {/if}
  </div>

  <!-- Content -->
  {#if loading}
    <div class="panel-empty">加载中...</div>
  {:else if ruleSets.length === 0 && !showForm}
    <div class="panel-empty">暂无规则集，点击新增添加</div>
  {:else}
    <div class="list-scroll">
      {#each ruleSets as rs (rs.id)}
        <div
          role="button"
          tabindex="0"
          onclick={() => openEdit(rs)}
          onkeydown={(e) => e.key === 'Enter' && openEdit(rs)}
          class="list-row"
        >
          <div class="row-main">
            <div class="row-top">
              <span class="row-name">{rs.name}</span>
              <span class="row-tag">{rs.format}</span>
              <span class="row-tag">{kindLabels[rs.source.kind] ?? rs.source.kind}</span>
              {#if !rs.enabled}
                <span class="row-tag disabled-tag">已停用</span>
              {/if}
            </div>
            <span class="row-sub">{rs.id}</span>
          </div>
          {#if store.isActionOperable('ruleSets.remove')}
            <button
              class="row-del"
              onclick={(e: MouseEvent) => { e.stopPropagation(); handleRemove(rs.id); }}
              title="删除"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
                <line x1="2" y1="2" x2="10" y2="10"/><line x1="10" y1="2" x2="2" y2="10"/>
              </svg>
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Modal -->
{#if showForm}
  <div class="modal-overlay" role="presentation" onkeydown={(e) => e.key === 'Escape' && (showForm = false)}>
    <div class="modal-box" role="dialog" aria-modal="true">
      <div class="modal-header">
        <h4 class="modal-title">{editingId ? '编辑' : '新增'}规则集</h4>
      </div>

      <div class="modal-body">
        <div class="form-item">
          <span class="form-label">名称 <span class="required">*</span></span>
          <div class="form-input-wrap">
            <input id="rules-name" bind:value={form.name} placeholder="例如: 广告拦截规则" class="field-input" />
          </div>
        </div>

        <div class="form-item">
          <span class="form-label">格式</span>
          <div class="form-input-wrap">
            <select id="rules-format" bind:value={form.format} class="field-input">
              <option value="auto">自动检测</option>
              <option value="yaml">YAML</option>
              <option value="json">JSON</option>
              <option value="text">纯文本</option>
            </select>
          </div>
        </div>

        <div class="form-item">
          <span class="form-label">来源类型</span>
          <div class="form-input-wrap">
            <div class="kind-seg">
              {#each ['remote', 'file', 'inline'] as kind}
                <button
                  onclick={() => form.kind = kind as typeof form.kind}
                  class="kind-btn {form.kind === kind ? 'on' : ''}"
                  aria-pressed={form.kind === kind}
                >
                  {kindLabels[kind]}
                </button>
              {/each}
            </div>
          </div>
        </div>

        {#if form.kind === 'remote'}
          <div class="form-item">
            <span class="form-label">URL <span class="required">*</span></span>
            <div class="form-input-wrap">
              <input id="rules-url" bind:value={form.url} placeholder="https://example.com/rules.yaml" class="field-input field-mono" />
            </div>
          </div>
        {:else if form.kind === 'file'}
          <div class="form-item">
            <span class="form-label">文件路径 <span class="required">*</span></span>
            <div class="form-input-wrap">
              <input id="rules-path" bind:value={form.path} placeholder="/path/to/rules.yaml" class="field-input field-mono" />
            </div>
          </div>
        {:else}
          <div class="form-item">
            <span class="form-label">内联内容</span>
            <div class="form-input-wrap">
              <textarea id="rules-content" bind:value={form.content} placeholder="内联规则 JSON..." rows={6} class="field-input field-mono resize-y"></textarea>
            </div>
          </div>
        {/if}

        <div class="form-item">
          <span class="form-label">启用</span>
          <div class="form-input-wrap flex items-center">
            <button
              onclick={() => form.enabled = !form.enabled}
              class="toggle-btn {form.enabled ? 'on' : ''}"
              role="switch"
              aria-checked={form.enabled}
              aria-label="启用规则集"
            >
              <span class="toggle-thumb"></span>
            </button>
          </div>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn-ghost" onclick={() => showForm = false}>取消</button>
        <button class="btn-primary" onclick={handleSave} disabled={saving || !form.name.trim()}>
          {saving ? '保存中...' : '保存'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* Panel */
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 11px 14px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .panel-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
    letter-spacing: -0.01em;
  }

  .panel-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border-radius: 7px;
    font-size: 12px;
    font-weight: 500;
    background: var(--muted);
    color: var(--foreground);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: background 0.12s ease;
  }

  .action-btn:hover { background: var(--surface); }

  /* List */
  .list-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 5px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-height: 0;
  }

  .list-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 11px;
    border-radius: 8px;
    border: 1px solid transparent;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .list-row:hover {
    background: var(--muted);
    border-color: var(--border);
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .row-top {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .row-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--foreground);
  }

  .row-tag {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
  }

  .row-tag.disabled-tag {
    background: rgba(234, 179, 8, 0.12);
    color: var(--warning, #ca8a04);
  }

  .row-sub {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    opacity: 0.55;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-del {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    border: none;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s ease, background 0.12s ease, color 0.12s ease;
    flex-shrink: 0;
  }

  .list-row:hover .row-del { opacity: 1; }

  .row-del:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--destructive);
  }

  /* Modal */
  .modal-overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }

  .modal-box {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 18px;
    width: min(420px, 90vw);
    max-height: 85vh;
    overflow-y: auto;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.15);
  }

  :global(.dark) .modal-box { box-shadow: 0 24px 80px rgba(0, 0, 0, 0.5); }

  .modal-header {
    padding-bottom: 14px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 16px;
  }

  .modal-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--foreground);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .form-item {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .form-label {
    flex-shrink: 0;
    width: 72px;
    padding-top: 7px;
    font-size: 12px;
    font-weight: 500;
    color: var(--foreground);
    text-align: right;
  }

  .form-input-wrap {
    flex: 1;
    min-width: 0;
  }

  .required { color: var(--destructive); }

  .field-input {
    width: 100%;
    padding: 7px 10px;
    border-radius: 7px;
    background: var(--muted);
    border: 1px solid var(--border);
    color: var(--foreground);
    font-size: 12.5px;
    outline: none;
    transition: border-color 0.12s ease;
  }

  .field-input:focus { border-color: rgba(99, 102, 241, 0.4); }
  .field-mono { font-family: var(--font-mono); font-size: 12px; }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  /* Kind segmented control */
  .kind-seg {
    display: flex;
    background: var(--muted);
    border-radius: 8px;
    padding: 3px;
  }

  .kind-btn {
    flex: 1;
    padding: 6px 0;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    text-align: center;
    background: transparent;
    color: var(--muted-foreground);
    border: none;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .kind-btn.on {
    background: var(--segment-active-bg);
    color: var(--segment-active-fg, var(--foreground));
    box-shadow: var(--segment-active-shadow, 0 1px 2px rgba(0,0,0,0.06));
  }

  .kind-btn:not(.on):hover { color: var(--foreground); }

  /* Toggle */
  .toggle-btn {
    position: relative;
    width: 34px;
    height: 20px;
    border-radius: 10px;
    background: var(--muted);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
    flex-shrink: 0;
  }

  .toggle-btn.on {
    background: var(--primary);
    border-color: transparent;
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--muted-foreground);
    transition: transform 0.15s ease, background 0.15s ease;
  }

  .toggle-btn.on .toggle-thumb {
    transform: translateX(14px);
    background: var(--primary-foreground);
  }

  .btn-ghost {
    flex: 1;
    padding: 8px 14px;
    border-radius: 8px;
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
    border: 1px solid var(--border);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .btn-ghost:hover { background: var(--surface); color: var(--foreground); }

  .btn-primary {
    flex: 1;
    padding: 8px 14px;
    border-radius: 8px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 12px;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: opacity 0.12s ease;
  }

  .btn-primary:disabled { opacity: 0.4; cursor: not-allowed; }
  .btn-primary:not(:disabled):hover { opacity: 0.88; }
</style>
