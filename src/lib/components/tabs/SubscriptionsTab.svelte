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
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import type {
    SubscriptionProfile,
    SubscriptionUpsert,
    ProxyConfigProfile,
  } from '$lib/types/domain';

  const FORMAT_OPTIONS = [
    { value: 'auto', label: '自动检测' },
    { value: 'zero', label: 'Zero' },
    { value: 'clash', label: 'Clash' },
  ];

  const INTERVAL_OPTIONS = [
    { value: '0', label: '手动' },
    { value: '1800', label: '30 分钟' },
    { value: '3600', label: '1 小时' },
    { value: '21600', label: '6 小时' },
    { value: '43200', label: '12 小时' },
    { value: '86400', label: '24 小时' },
  ];

  const AUTO_CONFIG_VALUE = '__auto__';
  const VIEW_MODE_KEY = 'znet-subscriptions-view-mode';
  type ViewMode = 'card' | 'list';

  type FormState = {
    name: string;
    url: string;
    format: string;
    kernel: string;
    updateIntervalSecs: string;
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
  const proxyConfigOptions = $derived([
    { value: AUTO_CONFIG_VALUE, label: '自动创建' },
    ...proxyConfigs.map(config => ({ value: config.id, label: `${config.name} (${config.id})` })),
  ]);

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
      updateIntervalSecs: '0',
      userAgent: '',
      targetProxyConfigId: AUTO_CONFIG_VALUE,
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
      updateIntervalSecs: String(sub.updateIntervalSecs ?? 0),
      userAgent: sub.userAgent ?? '',
      targetProxyConfigId: sub.targetProxyConfigId ?? AUTO_CONFIG_VALUE,
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
      const interval = Number(form.updateIntervalSecs);
      const input: SubscriptionUpsert = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        url: form.url.trim(),
        format: canonicalFormat(form.format),
        kernel: form.kernel || undefined,
        updateIntervalSecs: interval > 0 ? interval : undefined,
        userAgent: form.userAgent.trim() || undefined,
        targetProxyConfigId:
          form.targetProxyConfigId === AUTO_CONFIG_VALUE ? undefined : form.targetProxyConfigId,
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

  function canonicalFormat(value: string): string {
    const normalized = (value || 'auto').trim().toLowerCase();
    if (normalized === 'auto') return 'auto';
    if (
      normalized === 'zero' ||
      normalized === 'zero-json' ||
      normalized === 'zero-base64-json' ||
      normalized === 'base64-json' ||
      normalized === 'znet-sink' ||
      normalized === 'znet-sink-base64'
    ) return 'zero';
    if (normalized.includes('clash') || normalized === 'yaml' || normalized === 'base64-yaml') {
      return 'clash';
    }
    return normalized;
  }

  function formatLabel(value: string): string {
    const canonical = canonicalFormat(value);
    return FORMAT_OPTIONS.find(option => option.value === canonical)?.label ?? canonical;
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
  <div class="panel-header">
    <div class="panel-title-group">
      <span class="panel-title">订阅管理</span>
      <span class="panel-subtitle">管理订阅、同步与关联配置</span>
    </div>
    <div class="header-actions">
      {#if subscriptions.length > 0}
        <SegmentedControl.Root
          value={viewMode}
          onValueChange={(value) => setViewMode(value as ViewMode)}
          aria-label="订阅显示方式"
        >
          <SegmentedControl.Item value="card" size="icon" title="卡片视图" aria-label="卡片视图">
            <LayoutGrid class="h-3.5 w-3.5" />
          </SegmentedControl.Item>
          <SegmentedControl.Item value="list" size="icon" title="列表视图" aria-label="列表视图">
            <List class="h-3.5 w-3.5" />
          </SegmentedControl.Item>
        </SegmentedControl.Root>
      {/if}
      {#if subscriptions.length > 0}
        <input bind:value={searchQuery} placeholder="搜索…" class="search-input" />
      {/if}
      {#if subscriptions.length > 0}
        <button class="action-btn" onclick={handleSyncAll} disabled={busy}>
          {#if syncingAll}
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="spin">
              <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
            </svg>
            {syncingAll.done}/{syncingAll.total}
          {:else}
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
            </svg>
            同步全部
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

<Dialog.Root bind:open={showForm}>
  <Dialog.Content class="sm:max-w-[560px]" showCloseButton={!saving}>
    <form
      class="subscription-form"
      onsubmit={(event) => {
        event.preventDefault();
        void handleSave();
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{editingId ? '编辑订阅' : '新增订阅'}</Dialog.Title>
        <Dialog.Description>
          自动检测会根据响应正文识别 Zero Base64 JSON 或 Clash；不会根据本地配置强制选择格式。
        </Dialog.Description>
      </Dialog.Header>

      <Dialog.Body class="grid gap-[15px]">
        <label class="form-field">
          <span class="form-label">名称 <span class="required">*</span></span>
          <Input bind:value={form.name} placeholder="例如：官方订阅" disabled={saving} />
        </label>

        <label class="form-field">
          <span class="form-label">订阅 URL <span class="required">*</span></span>
          <Input
            bind:value={form.url}
            placeholder="https://example.com/subscription"
            class="font-mono"
            disabled={saving}
          />
        </label>

        <div class="form-grid">
          <label class="form-field">
            <span class="form-label">源格式</span>
            <Select.Root type="single" bind:value={form.format} items={FORMAT_OPTIONS} disabled={saving}>
              <Select.Trigger class="w-full">
                <Select.Value />
              </Select.Trigger>
              <Select.Content>
                {#each FORMAT_OPTIONS as option}
                  <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            <span class="form-hint">Zero 仅接受 Base64 编码 JSON；Clash 同时兼容 YAML 与 Base64 YAML。</span>
          </label>

          <label class="form-field">
            <span class="form-label">目标内核</span>
            <Input value="Zero" disabled />
            <span class="form-hint">订阅内容会统一转换为 Zero 配置。</span>
          </label>
        </div>

        <div class="form-grid">
          <label class="form-field">
            <span class="form-label">自动同步</span>
            <Select.Root
              type="single"
              bind:value={form.updateIntervalSecs}
              items={INTERVAL_OPTIONS}
              disabled={saving}
            >
              <Select.Trigger class="w-full">
                <Select.Value />
              </Select.Trigger>
              <Select.Content>
                {#each INTERVAL_OPTIONS as option}
                  <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </label>

          <div class="form-field">
            <span class="form-label">启用</span>
            <div class="switch-row">
              <span>{form.enabled ? '参与同步' : '暂停同步'}</span>
              <Switch bind:checked={form.enabled} disabled={saving} aria-label="启用订阅" />
            </div>
          </div>
        </div>

        <label class="form-field">
          <span class="form-label">关联配置</span>
          <Select.Root
            type="single"
            bind:value={form.targetProxyConfigId}
            items={proxyConfigOptions}
            disabled={saving || proxyConfigsError !== null}
          >
            <Select.Trigger class="w-full">
              <Select.Value />
            </Select.Trigger>
            <Select.Content>
              {#each proxyConfigOptions as option}
                <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <span class="form-hint">同步时写入关联配置；选择自动创建时会新建一份。</span>
          {#if proxyConfigsError}
            <span class="form-error">关联配置加载失败：{proxyConfigsError}</span>
          {/if}
        </label>

        <label class="form-field">
          <span class="form-label">User-Agent</span>
          <Input
            bind:value={form.userAgent}
            placeholder="自定义 User-Agent（可选）"
            class="font-mono"
            disabled={saving}
          />
          <span class="form-hint">
            留空时使用 ZNet-Sink/&lt;版本&gt;；填写后完全覆盖默认 User-Agent。
          </span>
        </label>

        {#if formError}
          <div class="form-error-box" role="alert">{formError}</div>
        {/if}
      </Dialog.Body>

      <Dialog.Footer>
        <Button type="button" variant="outline" onclick={closeForm} disabled={saving}>取消</Button>
        <Button type="submit" disabled={saving || !form.name.trim() || !form.url.trim()}>
          {saving ? '保存中…' : '保存'}
        </Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root open={deleteTarget !== null}>
  <Dialog.Content class="sm:max-w-[440px]" showCloseButton={removingId === null}>
    <Dialog.Header>
      <Dialog.Title>删除订阅</Dialog.Title>
      <Dialog.Description>关联的代理配置会被保留，删除不会中断当前内核配置。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body>
      {#if deleteTarget}
        <div class="form-error-box" role="alert">
          确认删除“{deleteTarget.name}”吗？此操作无法从应用内撤销。
        </div>
      {/if}
    </Dialog.Body>
    <Dialog.Footer>
      <Button variant="outline" onclick={closeDelete} disabled={removingId !== null}>取消</Button>
      <Button variant="destructive" onclick={handleRemove} disabled={removingId !== null}>
        {removingId !== null ? '删除中…' : '确认删除'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

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
  .panel-title-group { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .panel-title { font-size: 13px; font-weight: 600; color: var(--foreground); letter-spacing: -0.01em; }
  .panel-subtitle { font-size: 10.5px; color: var(--muted-foreground); opacity: 0.8; }
  .header-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .search-input {
    width: 130px; height: var(--control-height); padding: 0 9px; border-radius: var(--control-radius);
    border: 1px solid var(--input); background: var(--background); color: var(--foreground); font-size: 12px;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04); outline: none;
    transition: border-color 0.12s ease, box-shadow 0.12s ease, width 0.15s ease;
  }
  .search-input:focus { border-color: var(--ring); box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 18%, transparent); width: 170px; }
  .panel-empty { flex: 1; display: flex; align-items: center; justify-content: center; font-size: 12px; color: var(--muted-foreground); }
  .empty-stack { display: flex; flex-direction: column; align-items: center; gap: 6px; }
  .empty-icon { opacity: 0.3; }
  .empty-hint { font-size: 11px; opacity: 0.7; }
  .error-stack { color: var(--destructive); max-width: 440px; text-align: center; }
  .action-btn {
    display: inline-flex; align-items: center; gap: 5px; height: var(--control-height); padding: 0 10px;
    border-radius: var(--control-radius); font-size: 12px; font-weight: 500; background: var(--background);
    color: var(--foreground); border: 1px solid var(--input); box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
    cursor: pointer; transition: background 0.12s ease;
  }
  .action-btn:hover { background: var(--muted); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .action-btn.primary { background: var(--primary); color: var(--primary-foreground); border-color: transparent; box-shadow: 0 1px 2px rgb(0 0 0 / 0.08); }
  .action-btn.primary:hover { opacity: 0.9; }
  .list-scroll { flex: 1; overflow-y: auto; padding: 5px; display: flex; flex-direction: column; gap: 1px; min-height: 0; }
  .list-scroll.card-view { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); align-content: start; gap: 10px; padding: 10px; }
  .list-row { display: flex; align-items: flex-start; gap: 8px; padding: 10px 11px; border-radius: 8px; border: 1px solid transparent; transition: background 0.12s ease, border-color 0.12s ease; }
  .list-row:hover { background: var(--muted); border-color: var(--border); }
  .card-view .list-row { min-height: 210px; padding: 13px; flex-direction: column; align-items: stretch; border-color: var(--border); background: color-mix(in srgb, var(--card) 94%, var(--muted)); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04); }
  .card-view .list-row:hover, .card-view .list-row:focus-within { background: var(--card); border-color: color-mix(in srgb, var(--primary) 26%, var(--border)); box-shadow: 0 5px 16px rgba(0, 0, 0, 0.07); }
  .card-view .row-main { width: 100%; }
  .card-view .row-name { font-size: 13.5px; }
  .card-view .row-url { max-width: 100%; }
  .card-view .traffic-bar-wrap { align-items: flex-start; flex-direction: column; gap: 5px; }
  .card-view .traffic-bar-track { width: 100%; max-width: none; }
  .card-view .traffic-label { white-space: normal; }
  .card-view .row-actions { width: 100%; margin-top: auto; padding-top: 8px; justify-content: flex-end; border-top: 1px solid var(--border); opacity: 1; }
  .list-row.disabled .row-name { opacity: 0.55; }
  .row-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .row-top { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .row-name { font-size: 12.5px; font-weight: 600; color: var(--foreground); }
  .row-tag { font-size: 10px; font-weight: 600; padding: 2px 6px; border-radius: 4px; background: var(--muted); color: var(--muted-foreground); white-space: nowrap; }
  .row-tag.ok-tag { background: rgba(34, 197, 94, 0.12); color: var(--success); }
  .row-tag.error-tag { background: rgba(239, 68, 68, 0.12); color: var(--destructive); }
  .row-tag.muted-tag { background: var(--muted); color: var(--muted-foreground); opacity: 0.7; }
  .row-tag.info-tag { background: rgba(99, 102, 241, 0.12); color: var(--primary); }
  .row-tag.outline-tag { background: transparent; border: 1px solid var(--border); color: var(--muted-foreground); }
  .row-meta, .row-meta-line { display: flex; align-items: center; gap: 5px; font-size: 10.5px; color: var(--muted-foreground); flex-wrap: wrap; }
  .row-meta { opacity: 0.65; }
  .row-meta-line { opacity: 0.85; }
  .row-url { font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: min(420px, 100%); }
  .row-sep { opacity: 0.4; }
  .expire-warn { color: var(--destructive); }
  .expire-days { opacity: 0.7; margin-left: 2px; }
  .traffic-bar-wrap { display: flex; align-items: center; gap: 8px; margin-top: 2px; }
  .traffic-bar-track { flex: 1; height: 5px; min-width: 60px; max-width: 220px; border-radius: 3px; background: var(--muted); overflow: hidden; }
  .traffic-bar-fill { height: 100%; background: var(--success); border-radius: 3px; transition: width 0.3s ease, background 0.2s ease; min-width: 2px; }
  .traffic-bar-fill.warn { background: var(--destructive); }
  .traffic-label { font-size: 10px; color: var(--muted-foreground); font-family: var(--font-mono); white-space: nowrap; }
  .row-error { font-size: 10.5px; color: var(--destructive); opacity: 0.85; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-actions { display: flex; align-items: center; gap: 2px; flex-shrink: 0; opacity: 0.35; transition: opacity 0.12s ease; }
  .list-row:hover .row-actions, .list-row:focus-within .row-actions { opacity: 1; }
  .row-action { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; border-radius: 6px; background: transparent; border: none; cursor: pointer; color: var(--muted-foreground); transition: background 0.12s ease, color 0.12s ease; }
  .row-action.sync-btn:hover { background: rgba(34, 197, 94, 0.12); color: var(--success); }
  .row-action.edit-btn:hover { background: rgba(99, 102, 241, 0.12); color: var(--primary); }
  .row-action.del-btn:hover { background: rgba(239, 68, 68, 0.1); color: var(--destructive); }
  .row-action:disabled { opacity: 0.35; cursor: not-allowed; }
  .spin { animation: spin 0.8s linear infinite; }

  .subscription-form { display: grid; min-height: 0; max-height: calc(100dvh - 2rem); grid-template-rows: auto minmax(0, 1fr) auto; }
  .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
  .form-field { display: grid; gap: 6px; min-width: 0; }
  .form-label { color: var(--foreground); font-size: 12px; font-weight: 600; }
  .required { color: var(--destructive); }
  .form-hint { color: var(--muted-foreground); font-size: 10.5px; line-height: 1.5; }
  .form-error { color: var(--destructive); font-size: 10.5px; }
  .switch-row { display: flex; min-height: 32px; align-items: center; justify-content: space-between; gap: 12px; padding: 0 9px; border: 1px solid var(--input); border-radius: var(--control-radius); background: var(--background); color: var(--muted-foreground); font-size: 11.5px; }
  .form-error-box { padding: 9px 11px; border: 1px solid rgba(239, 68, 68, 0.24); border-radius: 7px; background: rgba(239, 68, 68, 0.08); color: var(--destructive); font-size: 11.5px; line-height: 1.5; }

  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }

  @media (max-width: 700px) {
    .panel-header { align-items: flex-start; flex-direction: column; }
    .header-actions { width: 100%; flex-wrap: wrap; }
    .search-input { flex: 1; min-width: 120px; }
    .search-input:focus { width: auto; }
    .list-scroll.card-view { grid-template-columns: 1fr; }
    .form-grid { grid-template-columns: 1fr; }
  }
</style>
