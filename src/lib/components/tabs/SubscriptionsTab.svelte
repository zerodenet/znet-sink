<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { handleAppError } from '$lib/services/core';
  import { listSubscriptions, syncSubscription, removeSubscription, upsertSubscription } from '$lib/services/config';
  import type { SubscriptionProfile, SubscriptionUpsert } from '$lib/types/domain';

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
      handleAppError(e, '保存订阅失败');
    } finally {
      saving = false;
    }
  }

  function formatTime(ms?: number): string {
    if (!ms) return '—';
    return new Date(ms).toLocaleString('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="desk-card flex-1 overflow-hidden flex flex-col animate-fade-in">
  <!-- Panel header -->
  <div class="panel-header">
    <span class="panel-title">订阅管理</span>
    <button class="action-btn" onclick={openCreate}>
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
        <line x1="6" y1="1" x2="6" y2="11"/><line x1="1" y1="6" x2="11" y2="6"/>
      </svg>
      新增
    </button>
  </div>

  <!-- Content -->
  {#if loading}
    <div class="panel-empty">加载中...</div>
  {:else if subscriptions.length === 0 && !showForm}
    <div class="panel-empty">暂无订阅，点击新增添加</div>
  {:else}
    <div class="list-scroll">
      {#each subscriptions as sub (sub.id)}
        <div
          role="button"
          tabindex="0"
          onclick={() => openEdit(sub)}
          onkeydown={(e) => e.key === 'Enter' && openEdit(sub)}
          class="list-row"
        >
          <div class="row-main">
            <div class="row-top">
              <span class="row-name">{sub.name}</span>
              {#if sub.lastError}
                <span class="row-tag error-tag">同步失败</span>
              {:else}
                <span class="row-tag ok-tag">正常</span>
              {/if}
            </div>
            <div class="row-meta">
              <span class="font-mono row-url">{sub.url}</span>
              <span class="row-sep">·</span>
              <span>{formatTime(sub.lastSyncAtUnixMs)}</span>
            </div>
            {#if sub.lastError}
              <span class="row-error">{sub.lastError}</span>
            {/if}
          </div>

          <!-- Actions -->
          <div class="row-actions">
            <button
              class="row-action sync-btn"
              onclick={(e: MouseEvent) => { e.stopPropagation(); handleSync(sub.id); }}
              disabled={syncingId === sub.id}
              title="同步订阅"
            >
              <svg
                width="14" height="14" viewBox="0 0 12 12" fill="none" stroke="currentColor"
                stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"
                class="{syncingId === sub.id ? 'spin' : ''}"
              >
                <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
              </svg>
            </button>
            <button
              class="row-action del-btn"
              onclick={(e: MouseEvent) => { e.stopPropagation(); handleRemove(sub.id); }}
              title="删除订阅"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
                <line x1="2" y1="2" x2="10" y2="10"/><line x1="10" y1="2" x2="2" y2="10"/>
              </svg>
            </button>
          </div>
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
        <h4 class="modal-title">{editingId ? '编辑' : '新增'}订阅</h4>
      </div>

      <div class="modal-body">
        <div class="form-item">
          <span class="form-label">名称 <span class="required">*</span></span>
          <div class="form-input-wrap">
            <input id="sub-name" bind:value={form.name} placeholder="例如: 官方订阅" class="field-input" />
          </div>
        </div>

        <div class="form-item">
          <span class="form-label">订阅 URL <span class="required">*</span></span>
          <div class="form-input-wrap">
            <input id="sub-url" bind:value={form.url} placeholder="https://example.com/subscription" class="field-input field-mono" />
          </div>
        </div>

        <div class="form-item">
          <span class="form-label">格式</span>
          <div class="form-input-wrap">
            <select id="sub-format" bind:value={form.format} class="field-input">
              <option value="auto">自动检测</option>
              <option value="zero-base64-json">Zero Base64 JSON</option>
            </select>
          </div>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn-ghost" onclick={() => showForm = false}>取消</button>
        <button class="btn-primary" onclick={handleSave} disabled={saving || !form.name.trim() || !form.url.trim()}>
          {saving ? '保存中...' : '保存'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
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

  .row-tag.ok-tag {
    background: rgba(34, 197, 94, 0.1);
    color: var(--success);
  }

  .row-tag.error-tag {
    background: rgba(239, 68, 68, 0.1);
    color: var(--destructive);
  }

  .row-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    color: var(--muted-foreground);
    opacity: 0.65;
    overflow: hidden;
  }

  .row-url {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: min(320px, 100%);
  }

  .row-sep { opacity: 0.4; }

  .row-error {
    font-size: 10px;
    color: var(--destructive);
    opacity: 0.8;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.12s ease;
  }

  .list-row:hover .row-actions {
    opacity: 1;
  }

  .row-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--muted-foreground);
    transition: background 0.12s ease, color 0.12s ease;
  }

  .row-action.sync-btn:hover {
    background: rgba(34, 197, 94, 0.12);
    color: var(--success);
  }

  .row-action.del-btn:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--destructive);
  }

  .row-action:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .spin {
    animation: spin 0.8s linear infinite;
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
    padding: 20px 24px;
    width: min(440px, 90vw);
    max-height: 85vh;
    overflow-y: auto;
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.4);
  }

  :global(.dark) .modal-box { box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5); }

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
    width: 80px;
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

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
