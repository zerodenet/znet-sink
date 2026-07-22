<script lang="ts">
  import { onMount } from 'svelte';
  import { LayoutGrid, List } from '@lucide/svelte';
  import { getAppErrorMessage, handleAppError } from '$lib/services/core';
  import {
    listSubscriptions,
    syncSubscription,
    removeSubscription,
    upsertSubscription,
    listProxyConfigs,
    syncAllSubscriptions,
  } from '$lib/services/config';
  import * as toast from '$lib/services/toast.svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import { Switch } from '$lib/components/ui/switch';
  import type {
    SubscriptionProfile,
    SubscriptionUpsert,
    ProxyConfigProfile,
  } from '$lib/types/domain';

  // Format presets exposed in the form. Values map to the Rust
  // `parse_subscription_content` format string.
  const FORMAT_OPTIONS = [
    { value: 'auto', label: '自动检测' },
    { value: 'zero-json', label: 'Zero JSON' },
    { value: 'zero-base64-json', label: 'Zero Base64 JSON' },
    { value: 'clash-yaml', label: 'Clash YAML' },
    { value: 'clash-base64-yaml', label: 'Clash Base64 YAML' },
  ];

  // Auto-sync interval presets (seconds). `0` means manual only.
  const INTERVAL_OPTIONS: Array<{ value: number; label: string }> = [
    { value: 0, label: '手动' },
    { value: 1800, label: '30 分钟' },
    { value: 3600, label: '1 小时' },
    { value: 21600, label: '6 小时' },
    { value: 43200, label: '12 小时' },
    { value: 86400, label: '24 小时' },
  ];

  const KERNEL_OPTIONS = [
    { value: 'zero', label: 'Zero' },
  ];
  const VIEW_MODE_KEY = 'znet-subscriptions-view-mode';
  type ViewMode = 'card' | 'list';

  type FormState = {
    name: string;
    url: string;
    format: string;
    kernel: string;
    updateIntervalSecs: number;
    userAgent: string;
    targetProxyConfigId: string;
    enabled: boolean;
  };

  let subscriptions = $state<SubscriptionProfile[]>([]);
  let proxyConfigs = $state<ProxyConfigProfile[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let proxyConfigsError = $state<string | null>(null);
  let syncingId = $state<string | null>(null);
  let syncingAll = $state<{ done: number; total: number } | null>(null);
  let togglingId = $state<string | null>(null);
  let showForm = $state(false);
  let saving = $state(false);
  let formError = $state<string | null>(null);
  let removingId = $state<string | null>(null);
  let deleteTarget = $state<SubscriptionProfile | null>(null);
  let editingId = $state<string | null>(null);
  let searchQuery = $state('');
  let viewMode = $state<ViewMode>('list');

  let form = $state<FormState>(emptyForm());
  const busy = $derived(
    syncingId !== null || syncingAll !== null || togglingId !== null || saving || removingId !== null
  );

  const filtered = $derived(
    searchQuery.trim()
      ? subscriptions.filter(s =>
          s.name.toLowerCase().includes(searchQuery.trim().toLowerCase()) ||
          s.url.toLowerCase().includes(searchQuery.trim().toLowerCase())
        )
      : subscriptions
  );

  function loadViewMode(): ViewMode {
    try {
      return localStorage.getItem(VIEW_MODE_KEY) === 'card' ? 'card' : 'list';
    } catch {
      return 'list';
    }
  }

  function setViewMode(mode: ViewMode) {
    viewMode = mode;
    try {
      localStorage.setItem(VIEW_MODE_KEY, mode);
    } catch {
      // View preference persistence is best effort.
    }
  }

  function emptyForm(): FormState {
    return {
      name: '',
      url: '',
      format: 'auto',
      kernel: 'zero',
      updateIntervalSecs: 0,
      userAgent: '',
      targetProxyConfigId: '',
      enabled: true,
    };
  }

  async function refresh(showLoading = true) {
    if (showLoading) loading = true;
    loadError = null;
    proxyConfigsError = null;
    const [subscriptionsResult, configsResult] = await Promise.allSettled([
      listSubscriptions(),
      listProxyConfigs(),
    ]);

    if (subscriptionsResult.status === 'fulfilled') {
      subscriptions = subscriptionsResult.value;
    } else {
      loadError = getAppErrorMessage(subscriptionsResult.reason, '加载订阅列表失败');
      handleAppError(subscriptionsResult.reason, '加载订阅列表失败');
    }
    if (configsResult.status === 'fulfilled') {
      proxyConfigs = configsResult.value;
    } else {
      proxyConfigs = [];
      proxyConfigsError = getAppErrorMessage(configsResult.reason, '加载关联配置失败');
    }
    if (showLoading) loading = false;
  }

  async function handleSync(id: string) {
    if (busy) return;
    syncingId = id;
    try {
      await syncSubscription(id);
      await refresh(false);
      toast.success('订阅同步完成');
    } catch (e) {
      handleAppError(e, '同步订阅失败');
    } finally {
      syncingId = null;
    }
  }

  async function handleSyncAll() {
    const list = subscriptions;
    if (list.length === 0 || busy) return;
    const eligible = list.filter(s => s.enabled);
    if (eligible.length === 0) {
      toast.warning('没有已启用的订阅可同步');
      return;
    }
    syncingAll = { done: 0, total: eligible.length };
    try {
      const outcome = await syncAllSubscriptions();
      syncingAll = { done: outcome.total, total: outcome.total };
      await refresh(false);
      if (outcome.failed === 0) {
        toast.success(`全部同步完成 (${outcome.succeeded}/${outcome.total})`);
      } else {
        toast.warning(`同步完成：成功 ${outcome.succeeded}/${outcome.total}，失败 ${outcome.failed}`);
      }
    } catch (e) {
      handleAppError(e, '批量同步订阅失败');
    } finally {
      syncingAll = null;
    }
  }

  async function handleToggleEnabled(sub: SubscriptionProfile) {
    if (busy) return;
    togglingId = sub.id;
    try {
      // Send the full editable payload so the backend upsert (which
      // replaces editable fields) preserves interval/UA/format/etc.
      await upsertSubscription({
        id: sub.id,
        name: sub.name,
        url: sub.url,
        enabled: !sub.enabled,
        format: canonicalFormat(sub.format),
        kernel: sub.kernel || 'zero',
        updateIntervalSecs: sub.updateIntervalSecs,
        userAgent: sub.userAgent,
        targetProxyConfigId: sub.targetProxyConfigId,
      });
      await refresh(false);
    } catch (e) {
      handleAppError(e, '切换启用状态失败');
    } finally {
      togglingId = null;
    }
  }

  function requestRemove(sub: SubscriptionProfile) {
    if (busy) return;
    deleteTarget = sub;
  }

  function closeDelete() {
    if (removingId !== null) return;
    deleteTarget = null;
  }

  async function handleRemove() {
    if (!deleteTarget || busy) return;
    const target = deleteTarget;
    removingId = target.id;
    try {
      await removeSubscription(target.id);
      deleteTarget = null;
      await refresh(false);
      toast.success('订阅已删除');
    } catch (e) {
      handleAppError(e, '删除订阅失败');
    } finally {
      removingId = null;
    }
  }

  function openCreate() {
    if (busy) return;
    editingId = null;
    form = emptyForm();
    formError = null;
    showForm = true;
  }

  function openEdit(sub: SubscriptionProfile) {
    if (busy) return;
    editingId = sub.id;
    form = {
      name: sub.name,
      url: sub.url,
      format: canonicalFormat(sub.format),
      kernel: sub.kernel || 'zero',
      updateIntervalSecs: sub.updateIntervalSecs ?? 0,
      userAgent: sub.userAgent ?? '',
      targetProxyConfigId: sub.targetProxyConfigId ?? '',
      enabled: sub.enabled,
    };
    formError = null;
    showForm = true;
  }

  function closeForm() {
    if (saving) return;
    showForm = false;
    formError = null;
  }

  async function handleSave() {
    if (!form.name.trim() || !form.url.trim() || busy) return;
    saving = true;
    formError = null;
    try {
      const input: SubscriptionUpsert = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        url: form.url.trim(),
        format: form.format || undefined,
        kernel: form.kernel || undefined,
        updateIntervalSecs: form.updateIntervalSecs || undefined,
        userAgent: form.userAgent.trim() || undefined,
        targetProxyConfigId: form.targetProxyConfigId || undefined,
        enabled: form.enabled,
      };

      await upsertSubscription(input);
      showForm = false;
      await refresh(false);
      toast.success(editingId ? '订阅已更新' : '订阅已创建');
    } catch (e) {
      formError = getAppErrorMessage(e, '保存订阅失败');
      handleAppError(e, '保存订阅失败');
    } finally {
      saving = false;
    }
  }

  // ── Formatting helpers ──

  function formatTime(ms?: number): string {
    if (!ms) return '从未同步';
    return new Date(ms).toLocaleString('zh-CN', {
      month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  function formatExpiry(ms?: number): string {
    if (!ms) return '';
    const date = new Date(ms);
    return date.toLocaleDateString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit' });
  }

  function daysUntil(ms?: number): number | null {
    if (!ms) return null;
    const diff = ms - Date.now();
    return Math.ceil(diff / 86_400_000);
  }

  function formatBytes(bytes?: number): string {
    if (bytes === undefined || bytes === null) return '—';
    if (bytes < 1024) return `${bytes} B`;
    const units = ['KB', 'MB', 'GB', 'TB'];
    let value = bytes / 1024;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit++;
    }
    return `${value.toFixed(1)} ${units[unit]}`;
  }

  function usedBytes(sub: SubscriptionProfile): number {
    return (sub.uploadBytes ?? 0) + (sub.downloadBytes ?? 0);
  }

  function usagePercent(sub: SubscriptionProfile): number | null {
    if (!sub.totalBytes) return null;
    return Math.min(100, (usedBytes(sub) / sub.totalBytes) * 100);
  }

  function formatLabel(value: string): string {
    const found = FORMAT_OPTIONS.find(o => o.value === value);
    return found?.label ?? value;
  }

  /** Map a stored format string back to a selectable source-format
   *  value so the form and toggle payloads stay consistent. */
  function canonicalFormat(value: string): string {
    if (value === 'clash-yaml-converted') return 'clash-yaml';
    if (value === 'clash-base64-yaml-converted') return 'clash-base64-yaml';
    return value || 'auto';
  }

  function proxyConfigName(id?: string): string {
    if (!id) return '自动创建';
    return proxyConfigs.find(c => c.id === id)?.name ?? '自动创建';
  }

  onMount(() => {
    viewMode = loadViewMode();
    void refresh();
  });
</script>

<div class="desk-card flex-1 overflow-hidden flex flex-col animate-fade-in">
  <!-- Panel header -->
  <div class="panel-header">
    <div class="panel-title-group">
      <span class="panel-title">订阅管理</span>
      <span class="panel-subtitle">订阅链接会自动转换为 Zero 内核配置并关联代理配置</span>
    </div>
    <div class="header-actions">
      {#if subscriptions.length > 0}
        <div class="view-switch" role="group" aria-label="订阅显示方式">
          <button
            type="button"
            class="view-switch-button"
            class:active={viewMode === 'card'}
            onclick={() => setViewMode('card')}
            title="卡片视图"
            aria-label="卡片视图"
            aria-pressed={viewMode === 'card'}
          >
            <LayoutGrid class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            class="view-switch-button"
            class:active={viewMode === 'list'}
            onclick={() => setViewMode('list')}
            title="列表视图"
            aria-label="列表视图"
            aria-pressed={viewMode === 'list'}
          >
            <List class="h-3.5 w-3.5" />
          </button>
        </div>
      {/if}
      {#if subscriptions.length > 0}
        <input
          bind:value={searchQuery}
          placeholder="搜索…"
          class="search-input"
        />
      {/if}
      {#if subscriptions.length > 0}
        <button
          class="action-btn"
          onclick={handleSyncAll}
          disabled={busy}
        >
          {#if syncingAll}
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="spin">
              <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
            </svg>
            {syncingAll.done}/{syncingAll.total}
          {:else}
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
            </svg>
            全部同步
          {/if}
        </button>
      {/if}
      <button class="action-btn primary" onclick={openCreate} disabled={loading || busy}>
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <line x1="6" y1="1" x2="6" y2="11"/><line x1="1" y1="6" x2="11" y2="6"/>
        </svg>
        新增
      </button>
    </div>
  </div>

  <!-- Content -->
  {#if loading}
    <div class="panel-empty">加载中...</div>
  {:else if loadError}
    <div class="panel-empty" role="alert">
      <div class="empty-stack error-stack">
        <span>订阅列表加载失败</span>
        <span class="empty-hint">{loadError}</span>
        <button class="action-btn" onclick={() => refresh()}>重试</button>
      </div>
    </div>
  {:else if subscriptions.length === 0 && !showForm}
    <div class="panel-empty">
      <div class="empty-stack">
        <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="empty-icon">
          <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
          <path d="M12 7v5l3 3"/>
        </svg>
        <span>暂无订阅</span>
        <span class="empty-hint">添加订阅链接以自动获取节点配置</span>
      </div>
    </div>
  {:else}
    <div class="list-scroll" class:card-view={viewMode === 'card'}>
      {#if filtered.length === 0 && searchQuery}
        <div class="panel-empty">无匹配结果</div>
      {/if}
      {#each filtered as sub (sub.id)}
        <div class="list-row" class:disabled={!sub.enabled}>
          <div class="row-main">
            <div class="row-top">
              <Switch
                size="sm"
                checked={sub.enabled}
                onCheckedChange={() => handleToggleEnabled(sub)}
                title={sub.enabled ? '已启用' : '已禁用'}
                disabled={busy}
                aria-label={sub.enabled ? '禁用订阅' : '启用订阅'}
              />

              <span class="row-name">{sub.name}</span>

              {#if sub.lastError}
                <span class="row-tag error-tag">同步失败</span>
              {:else if !sub.enabled}
                <span class="row-tag muted-tag">已禁用</span>
              {:else if sub.lastSyncAtUnixMs}
                <span class="row-tag ok-tag">正常</span>
              {:else}
                <span class="row-tag muted-tag">未同步</span>
              {/if}

              {#if sub.nodeCount !== undefined}
                <span class="row-tag info-tag">{sub.nodeCount} 节点</span>
              {/if}
              <span class="row-tag outline-tag">{formatLabel(sub.format)}</span>
              {#if sub.updateIntervalSecs}
                <span class="row-tag outline-tag" title="自动同步间隔">⏱ 自动</span>
              {/if}
            </div>

            <div class="row-meta">
              <span class="font-mono row-url" title={sub.url}>{sub.url}</span>
            </div>

            <div class="row-meta-line">
              <span>同步: {formatTime(sub.lastSyncAtUnixMs)}</span>
              <span class="row-sep">·</span>
              <span>配置: {proxyConfigName(sub.targetProxyConfigId)}</span>
              {#if sub.expireAtUnixMs}
                <span class="row-sep">·</span>
                <span class:expire-warn={daysUntil(sub.expireAtUnixMs) !== null && daysUntil(sub.expireAtUnixMs)! < 7}>
                  到期: {formatExpiry(sub.expireAtUnixMs)}
                  {#if daysUntil(sub.expireAtUnixMs) !== null}
                    <span class="expire-days">(剩 {daysUntil(sub.expireAtUnixMs)} 天)</span>
                  {/if}
                </span>
              {/if}
            </div>

            {#if sub.totalBytes}
              <div class="traffic-bar-wrap">
                <div class="traffic-bar-track">
                  <div
                    class="traffic-bar-fill"
                    class:warn={usagePercent(sub) !== null && usagePercent(sub)! >= 90}
                    style="width: {usagePercent(sub)}%"
                  ></div>
                </div>
                <span class="traffic-label">
                  {formatBytes(usedBytes(sub))} / {formatBytes(sub.totalBytes)}
                  (↑{formatBytes(sub.uploadBytes)} ↓{formatBytes(sub.downloadBytes)})
                </span>
              </div>
            {/if}

            {#if sub.lastError}
              <span class="row-error" title={sub.lastError}>{sub.lastError}</span>
            {/if}
          </div>

          <!-- Actions -->
          <div class="row-actions">
            <button
              class="row-action sync-btn"
              onclick={(e: MouseEvent) => { e.stopPropagation(); handleSync(sub.id); }}
              disabled={busy || !sub.enabled}
              title="同步订阅"
              aria-label="同步订阅"
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
              class="row-action edit-btn"
              onclick={(e: MouseEvent) => { e.stopPropagation(); openEdit(sub); }}
              disabled={busy}
              title="编辑订阅"
              aria-label="编辑订阅"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                <path d="M8.5 1.5l2 2L4 10H2V8z"/>
              </svg>
            </button>
            <button
              class="row-action del-btn"
              onclick={(e: MouseEvent) => { e.stopPropagation(); requestRemove(sub); }}
              disabled={busy}
              title="删除订阅"
              aria-label="删除订阅"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
                <path d="M2 3h8M4.5 3V2h3v1M3 3l.5 7h5L9 3"/>
              </svg>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Modal -->
<DraggableModal
  title="{editingId ? '编辑' : '新增'}订阅"
  open={showForm}
  onClose={closeForm}
  closeDisabled={saving}
  width="min(500px, 92vw)"
>
    <div class="form-item">
      <span class="form-label">名称 <span class="required">*</span></span>
      <div class="form-input-wrap">
        <input bind:value={form.name} placeholder="例如: 官方订阅" class="field-input" disabled={saving} />
      </div>
    </div>

    <div class="form-item">
      <span class="form-label">订阅 URL <span class="required">*</span></span>
      <div class="form-input-wrap">
        <input bind:value={form.url} placeholder="https://example.com/subscription" class="field-input field-mono" disabled={saving} />
      </div>
    </div>

    <div class="form-row">
      <div class="form-item">
        <span class="form-label">源格式</span>
        <div class="form-input-wrap">
          <select bind:value={form.format} class="field-input" disabled={saving}>
            {#each FORMAT_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </div>
      </div>

      <div class="form-item">
        <span class="form-label">目标内核</span>
        <div class="form-input-wrap">
          <select bind:value={form.kernel} class="field-input" disabled>
            {#each KERNEL_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
          <span class="form-hint">GUI 仅管理 Zero 内核，订阅会转换为 Zero 配置</span>
        </div>
      </div>
    </div>

    <div class="form-row">
      <div class="form-item">
        <span class="form-label">自动同步</span>
        <div class="form-input-wrap">
          <select bind:value={form.updateIntervalSecs} class="field-input" disabled={saving}>
            {#each INTERVAL_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </div>
      </div>

      <div class="form-item">
        <span class="form-label">启用</span>
        <div class="form-input-wrap toggle-row">
          <span class="toggle-copy">{form.enabled ? '参与同步' : '暂停同步'}</span>
          <Switch
            bind:checked={form.enabled}
            disabled={saving}
            aria-label="启用订阅"
          />
        </div>
      </div>
    </div>

    <div class="form-item">
      <span class="form-label">关联配置</span>
      <div class="form-input-wrap">
        <select bind:value={form.targetProxyConfigId} class="field-input" disabled={saving || proxyConfigsError !== null}>
          <option value="">自动创建</option>
          {#each proxyConfigs as cfg}
            <option value={cfg.id}>{cfg.name} ({cfg.id})</option>
          {/each}
        </select>
        <span class="form-hint">同步时写入关联的代理配置；留空则自动新建一份</span>
        {#if proxyConfigsError}
          <span class="form-error">关联配置加载失败：{proxyConfigsError}</span>
        {/if}
      </div>
    </div>

    <div class="form-item">
      <span class="form-label">User-Agent</span>
      <div class="form-input-wrap">
        <input bind:value={form.userAgent} placeholder="留空使用默认 UA" class="field-input field-mono" disabled={saving} />
        <span class="form-hint">部分机场需要特定 UA 才能返回节点</span>
      </div>
    </div>

    {#if formError}
      <div class="form-error-box" role="alert">{formError}</div>
    {/if}

  {#snippet footer()}
    <button class="btn-ghost" onclick={closeForm} disabled={saving}>取消</button>
    <button class="btn-primary" onclick={handleSave} disabled={saving || !form.name.trim() || !form.url.trim()}>
      {saving ? '保存中...' : '保存'}
    </button>
  {/snippet}
</DraggableModal>

<DraggableModal
  title="删除订阅"
  description="关联的代理配置会被保留，删除操作不会中断当前内核配置。"
  open={deleteTarget !== null}
  onClose={closeDelete}
  closeDisabled={removingId !== null}
  width="min(440px, 90vw)"
>
  {#if deleteTarget}
    <div class="form-error-box" role="alert">确认删除“{deleteTarget.name}”吗？此操作无法从应用内撤销。</div>
  {/if}

  {#snippet footer()}
    <button class="btn-ghost" onclick={closeDelete} disabled={removingId !== null}>取消</button>
    <button class="btn-danger" onclick={handleRemove} disabled={removingId !== null}>
      {removingId !== null ? '删除中...' : '确认删除'}
    </button>
  {/snippet}
</DraggableModal>

<style>
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 11px 14px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    gap: 12px;
  }

  .panel-title-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .panel-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
    letter-spacing: -0.01em;
  }

  .panel-subtitle {
    font-size: 10.5px;
    color: var(--muted-foreground);
    opacity: 0.8;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .view-switch {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--muted);
  }

  .view-switch-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 27px;
    height: 25px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease, box-shadow 0.12s ease;
  }

  .view-switch-button:hover {
    color: var(--foreground);
  }

  .view-switch-button.active {
    background: var(--card);
    color: var(--foreground);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }

  .search-input {
    width: 130px;
    height: 28px;
    padding: 0 9px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--foreground);
    font-size: 12px;
    outline: none;
    transition: border-color 0.12s ease, width 0.15s ease;
  }

  .search-input:focus {
    border-color: rgba(99, 102, 241, 0.4);
    width: 170px;
  }

  .panel-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .empty-stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .empty-icon {
    opacity: 0.3;
  }

  .empty-hint {
    font-size: 11px;
    opacity: 0.7;
  }

  .error-stack {
    color: var(--destructive);
    max-width: 440px;
    text-align: center;
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
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .action-btn.primary {
    background: var(--primary);
    color: var(--primary-foreground);
    border-color: transparent;
  }

  .action-btn.primary:hover { opacity: 0.9; }

  .list-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 5px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-height: 0;
  }

  .list-scroll.card-view {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    align-content: start;
    gap: 10px;
    padding: 10px;
  }

  .list-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 11px;
    border-radius: 8px;
    border: 1px solid transparent;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .list-row:hover {
    background: var(--muted);
    border-color: var(--border);
  }

  .card-view .list-row {
    min-height: 210px;
    padding: 13px;
    flex-direction: column;
    align-items: stretch;
    border-color: var(--border);
    background: color-mix(in srgb, var(--card) 94%, var(--muted));
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  .card-view .list-row:hover,
  .card-view .list-row:focus-within {
    background: var(--card);
    border-color: color-mix(in srgb, var(--primary) 26%, var(--border));
    box-shadow: 0 5px 16px rgba(0, 0, 0, 0.07);
  }

  .card-view .row-main {
    width: 100%;
  }

  .card-view .row-name {
    font-size: 13.5px;
  }

  .card-view .row-url {
    max-width: 100%;
  }

  .card-view .traffic-bar-wrap {
    align-items: flex-start;
    flex-direction: column;
    gap: 5px;
  }

  .card-view .traffic-bar-track {
    width: 100%;
    max-width: none;
  }

  .card-view .traffic-label {
    white-space: normal;
  }

  .card-view .row-actions {
    width: 100%;
    margin-top: auto;
    padding-top: 8px;
    justify-content: flex-end;
    border-top: 1px solid var(--border);
    opacity: 1;
  }

  .list-row.disabled .row-name {
    opacity: 0.55;
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .row-top {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .row-name {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--foreground);
  }

  .row-tag {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
    white-space: nowrap;
  }

  .row-tag.ok-tag {
    background: rgba(34, 197, 94, 0.12);
    color: var(--success);
  }

  .row-tag.error-tag {
    background: rgba(239, 68, 68, 0.12);
    color: var(--destructive);
  }

  .row-tag.muted-tag {
    background: var(--muted);
    color: var(--muted-foreground);
    opacity: 0.7;
  }

  .row-tag.info-tag {
    background: rgba(99, 102, 241, 0.12);
    color: var(--primary);
  }

  .row-tag.outline-tag {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--muted-foreground);
  }

  .row-meta,
  .row-meta-line {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    color: var(--muted-foreground);
    flex-wrap: wrap;
  }

  .row-meta { opacity: 0.65; }
  .row-meta-line { opacity: 0.85; }

  .row-url {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: min(420px, 100%);
  }

  .row-sep { opacity: 0.4; }

  .expire-warn { color: var(--destructive); }
  .expire-days { opacity: 0.7; margin-left: 2px; }

  .traffic-bar-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 2px;
  }

  .traffic-bar-track {
    flex: 1;
    height: 5px;
    min-width: 60px;
    max-width: 220px;
    border-radius: 3px;
    background: var(--muted);
    overflow: hidden;
  }

  .traffic-bar-fill {
    height: 100%;
    background: var(--success);
    border-radius: 3px;
    transition: width 0.3s ease, background 0.2s ease;
    min-width: 2px;
  }

  .traffic-bar-fill.warn { background: var(--destructive); }

  .traffic-label {
    font-size: 10px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    white-space: nowrap;
  }

  .row-error {
    font-size: 10.5px;
    color: var(--destructive);
    opacity: 0.85;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    opacity: 0.35;
    transition: opacity 0.12s ease;
  }

  .list-row:hover .row-actions,
  .list-row:focus-within .row-actions {
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

  .row-action.edit-btn:hover {
    background: rgba(99, 102, 241, 0.12);
    color: var(--primary);
  }

  .row-action.del-btn:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--destructive);
  }

  .row-action:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .spin {
    animation: spin 0.8s linear infinite;
  }

  /* Modal form styles */
  .form-item {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .form-row {
    display: flex;
    gap: 12px;
  }

  .form-row .form-item { flex: 1; }

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
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .required { color: var(--destructive); }

  .form-hint {
    font-size: 10.5px;
    color: var(--muted-foreground);
    opacity: 0.75;
  }

  .form-error {
    font-size: 10.5px;
    color: var(--destructive);
  }

  .form-error-box {
    padding: 9px 11px;
    border: 1px solid rgba(239, 68, 68, 0.24);
    border-radius: 7px;
    background: rgba(239, 68, 68, 0.08);
    color: var(--destructive);
    font-size: 11.5px;
    line-height: 1.5;
  }

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

  .toggle-row {
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }

  .toggle-copy {
    font-size: 11.5px;
    color: var(--muted-foreground);
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
  .btn-ghost:disabled { opacity: 0.45; cursor: not-allowed; }

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

  .btn-danger {
    flex: 1;
    padding: 8px 14px;
    border-radius: 8px;
    background: var(--destructive);
    color: white;
    font-size: 12px;
    font-weight: 600;
    border: none;
    cursor: pointer;
  }

  .btn-danger:disabled { opacity: 0.45; cursor: not-allowed; }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  @media (max-width: 700px) {
    .panel-header {
      align-items: flex-start;
      flex-direction: column;
    }

    .header-actions {
      width: 100%;
      flex-wrap: wrap;
    }

    .search-input {
      flex: 1;
      min-width: 120px;
    }

    .search-input:focus {
      width: auto;
    }

    .list-scroll.card-view {
      grid-template-columns: 1fr;
    }

    .form-row {
      flex-direction: column;
    }
  }
</style>
