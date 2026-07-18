<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertTriangle, Database, Plus, RefreshCw, ShieldCheck, Trash2 } from '@lucide/svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { listRuleSets, removeRuleSet, updateAllRuleSets, updateRuleSet, upsertRuleSet } from '$lib/services/config';
  import type { RuleSetProfile, ZeroRule, ZeroRuleType } from '$lib/types/domain';

  const RULE_TYPES: { value: ZeroRuleType; label: string; placeholder: string }[] = [
    { value: 'domain_exact', label: '精确域名', placeholder: 'api.example.com' },
    { value: 'domain_suffix', label: '域名后缀', placeholder: 'example.com' },
    { value: 'domain_keyword', label: '域名关键词', placeholder: 'special' },
    { value: 'ipv4_cidr', label: 'IPv4 CIDR', placeholder: '10.0.0.0/8' },
    { value: 'ipv6_cidr', label: 'IPv6 CIDR', placeholder: 'fd00::/8' },
  ];

  let items = $state<RuleSetProfile[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let updatingId = $state<string | null>(null);
  let updatingAll = $state(false);
  let deletingId = $state<string | null>(null);
  let pendingDelete = $state<RuleSetProfile | null>(null);
  let error = $state('');
  let showEditor = $state(false);
  let editingId = $state<string | undefined>();
  let name = $state('');
  let mode = $state<'visual' | 'subscription'>('visual');
  let rules = $state<ZeroRule[]>([{ type: 'domain_suffix', value: '' }]);
  let sourceUrl = $state('');
  let sourceFormat = $state<'auto' | 'zero-rule-ir-v1' | 'clash-classical-yaml'>('auto');
  let updateIntervalSecs = $state(0);
  let userAgent = $state('');
  let retainSource = $state(false);

  const sourceCount = $derived(items.filter((item) => item.source).length);
  const readyCount = $derived(items.filter((item) => item.artifact).length);
  const canSave = $derived(
    !saving
      && name.trim().length > 0
      && (mode !== 'subscription' || !!editingId || sourceUrl.trim().length > 0),
  );

  onMount(load);

  async function load() {
    loading = true;
    try {
      items = await listRuleSets();
      error = '';
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  function openNew() {
    editingId = undefined;
    name = '';
    mode = 'visual';
    rules = [{ type: 'domain_suffix', value: '' }];
    sourceUrl = '';
    sourceFormat = 'auto';
    updateIntervalSecs = 0;
    userAgent = '';
    retainSource = false;
    error = '';
    showEditor = true;
  }

  function openEdit(item: RuleSetProfile) {
    editingId = item.id;
    name = item.name;
    mode = 'visual';
    rules = item.semanticIr.rules.map((rule) => ({ ...rule }));
    sourceUrl = item.source?.url ?? '';
    sourceFormat = item.source?.format ?? 'auto';
    updateIntervalSecs = item.source?.updateIntervalSecs ?? 0;
    userAgent = item.source?.userAgent ?? '';
    retainSource = !!item.source;
    error = '';
    showEditor = true;
  }

  function closeEditor() {
    if (saving) return;
    showEditor = false;
  }

  function addRule() {
    rules = [...rules, { type: 'domain_suffix', value: '' }];
  }

  function removeRule(index: number) {
    rules = rules.filter((_, current) => current !== index);
  }

  function setRuleType(index: number, type: ZeroRuleType) {
    rules[index].type = type;
    rules = [...rules];
  }

  function setRuleValue(index: number, value: string) {
    rules[index].value = value;
    rules = [...rules];
  }

  function placeholder(type: ZeroRuleType) {
    return RULE_TYPES.find((item) => item.value === type)?.placeholder ?? '';
  }

  async function save() {
    if (!canSave) return;
    saving = true;
    error = '';
    try {
      if (mode === 'subscription' && !editingId) {
        await upsertRuleSet({
          name: name.trim(),
          enabled: true,
          source: {
            url: sourceUrl.trim(),
            format: sourceFormat,
            updateIntervalSecs: updateIntervalSecs || undefined,
            userAgent: userAgent.trim() || undefined,
          },
        });
      } else {
        const cleanRules = rules
          .map((rule) => ({ ...rule, value: rule.value.trim() }))
          .filter((rule) => rule.value);
        await upsertRuleSet({
          id: editingId,
          name: name.trim(),
          enabled: true,
          semanticIr: { version: 1, name: name.trim(), rules: cleanRules },
          source: retainSource && sourceUrl
            ? {
                url: sourceUrl,
                format: sourceFormat,
                updateIntervalSecs: updateIntervalSecs || undefined,
                userAgent: userAgent.trim() || undefined,
              }
            : undefined,
        });
      }
      showEditor = false;
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      saving = false;
    }
  }

  async function update(id: string) {
    updatingId = id;
    error = '';
    try {
      await updateRuleSet(id);
    } catch (cause) {
      error = String(cause);
    } finally {
      updatingId = null;
      await load();
    }
  }

  function requestRemove(item: RuleSetProfile) {
    pendingDelete = item;
  }

  function cancelRemove() {
    if (deletingId) return;
    pendingDelete = null;
  }

  async function confirmRemove() {
    if (!pendingDelete || deletingId) return;
    const id = pendingDelete.id;
    deletingId = id;
    try {
      await removeRuleSet(id);
      pendingDelete = null;
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      deletingId = null;
    }
  }

  async function updateAll() {
    updatingAll = true;
    error = '';
    try {
      const outcome = await updateAllRuleSets();
      await load();
      error = outcome.failed ? `${outcome.failed} 个来源更新失败，旧产物已保留` : '';
    } catch (cause) {
      error = String(cause);
    } finally {
      updatingAll = false;
    }
  }

  function formatBytes(value?: number) {
    if (!value) return '—';
    if (value < 1024) return `${value} B`;
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
    return `${(value / 1024 / 1024).toFixed(1)} MiB`;
  }

  function formatDate(value?: number) {
    if (!value) return '尚未同步';
    return new Intl.DateTimeFormat('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(value));
  }
</script>

<div class="desk-card rules-root animate-fade-in">
  <div class="panel-header">
    <div class="panel-title-group">
      <span class="panel-title">规则集</span>
      <span class="panel-subtitle">统一管理 Zero Rule IR，并构建为内核可直接加载的 ZRS 产物</span>
    </div>

    <div class="header-actions">
      {#if sourceCount > 0}
        <Button variant="outline" size="sm" onclick={updateAll} disabled={updatingAll}>
          <RefreshCw class={`h-3.5 w-3.5 ${updatingAll ? 'spin' : ''}`} />
          <span>{updatingAll ? '更新中...' : '全部更新'}</span>
        </Button>
      {/if}
      <Button size="sm" onclick={openNew}>
        <Plus class="h-3.5 w-3.5" />
        <span>新建规则集</span>
      </Button>
    </div>
  </div>

  <div class="summary-strip">
    <div class="summary-copy">
      <ShieldCheck class="summary-icon h-4 w-4" />
      <span>编辑内容保存为语义规则，运行时只加载完整校验后的不可变 ZRS 产物。</span>
    </div>
    {#if items.length > 0}
      <div class="summary-counts">
        <span>{items.length} 个规则集</span>
        <span>·</span>
        <span>{readyCount} 个产物就绪</span>
        {#if sourceCount > 0}
          <span>·</span>
          <span>{sourceCount} 个订阅来源</span>
        {/if}
      </div>
    {/if}
  </div>

  {#if error}
    <div class="error-banner" role="alert">
      <AlertTriangle class="h-3.5 w-3.5" />
      <span>{error}</span>
    </div>
  {/if}

  {#if loading}
    <div class="panel-empty">加载中...</div>
  {:else if items.length === 0}
    <div class="panel-empty">
      <div class="empty-stack">
        <Database class="empty-icon h-9 w-9" />
        <span class="empty-title">还没有规则集</span>
        <span class="empty-hint">手动创建语义规则，或从外部订阅导入</span>
        <Button size="sm" onclick={openNew}>
          <Plus class="h-3.5 w-3.5" />
          <span>新建规则集</span>
        </Button>
      </div>
    </div>
  {:else}
    <div class="list-scroll">
      {#each items as item (item.id)}
        <div
          class="list-row"
          role="button"
          tabindex="0"
          onclick={() => openEdit(item)}
          onkeydown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              openEdit(item);
            }
          }}
        >
          <div class="row-main">
            <div class="row-top">
              <span class="row-name">{item.name}</span>
              <Badge variant={item.artifact ? 'secondary' : 'outline'}>
                {item.artifact ? 'ZRS 已就绪' : '待构建'}
              </Badge>
              <Badge variant="outline">{item.source ? '订阅' : '本地'}</Badge>
            </div>

            <div class="row-meta">
              <span>{item.semanticIr.rules.length} 条源规则</span>
              <span>→</span>
              <span>{item.artifact?.entryCount ?? 0} 个索引项</span>
              <span>·</span>
              <span>{formatBytes(item.artifact?.fileSize)}</span>
              {#if item.artifact}
                <span>·</span>
                <span class="mono">CRC32 {item.artifact.checksum.toString(16).padStart(8, '0')}</span>
              {/if}
            </div>

            {#if item.source}
              <div class="source-line" title={item.source.url}>
                <span class="source-url">{item.source.url}</span>
                <span>·</span>
                <span>{formatDate(item.lastSyncAtUnixMs ?? item.sourceState.lastCheckedAtUnixMs)}</span>
                {#if item.sourceState.contentBytes}
                  <span>·</span>
                  <span>来源 {formatBytes(item.sourceState.contentBytes)}</span>
                {/if}
              </div>
            {/if}

            {#if item.lastError}
              <div class="row-error">同步失败，继续使用上一版 ZRS：{item.lastError}</div>
            {/if}
          </div>

          <div class="row-actions">
            {#if item.source}
              <Button
                variant="ghost"
                size="icon-sm"
                onclick={(event) => {
                  event.stopPropagation();
                  update(item.id);
                }}
                disabled={updatingId === item.id}
                title="下载并重建"
                aria-label="下载并重建"
              >
                <RefreshCw class={`h-3.5 w-3.5 ${updatingId === item.id ? 'spin' : ''}`} />
              </Button>
            {/if}
            <Button
              variant="ghost"
              size="icon-sm"
              class="delete-button"
              onclick={(event) => {
                event.stopPropagation();
                  requestRemove(item);
              }}
              title="删除"
              aria-label="删除"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<DraggableModal
  title={editingId ? '编辑规则集' : '新建规则集'}
  description="规则会保存为 Zero Rule IR，并在保存或同步后构建、校验 ZRS 产物。"
  open={showEditor}
  onClose={closeEditor}
  closeDisabled={saving}
  width="min(720px, 92vw)"
>
  <div class="form-item">
    <span class="form-label">名称 <span class="required">*</span></span>
    <div class="form-input-wrap">
      <Input bind:value={name} placeholder="例如：AI 服务、局域网直连" disabled={saving} />
    </div>
  </div>

  {#if !editingId}
    <div class="form-item">
      <span class="form-label">创建方式</span>
      <div class="form-input-wrap">
        <div class="source-switch">
          <button
            type="button"
            class="source-button"
            class:active={mode === 'visual'}
            onclick={() => mode = 'visual'}
            disabled={saving}
          >
            手动创建
          </button>
          <button
            type="button"
            class="source-button"
            class:active={mode === 'subscription'}
            onclick={() => mode = 'subscription'}
            disabled={saving}
          >
            订阅导入
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if mode === 'subscription' && !editingId}
    <div class="form-item">
      <span class="form-label">订阅地址 <span class="required">*</span></span>
      <div class="form-input-wrap">
        <Input bind:value={sourceUrl} placeholder="https://example.com/rules.yaml" disabled={saving} />
      </div>
    </div>

    <div class="form-row">
      <div class="form-item">
        <span class="form-label">来源格式</span>
        <div class="form-input-wrap">
          <select bind:value={sourceFormat} class="field-select" disabled={saving}>
            <option value="auto">自动识别</option>
            <option value="zero-rule-ir-v1">Zero Rule IR v1</option>
            <option value="clash-classical-yaml">Clash Classical YAML</option>
          </select>
        </div>
      </div>
      <div class="form-item">
        <span class="form-label">自动更新</span>
        <div class="form-input-wrap">
          <select bind:value={updateIntervalSecs} class="field-select" disabled={saving}>
            <option value={0}>手动</option>
            <option value={3600}>每小时</option>
            <option value={21600}>每 6 小时</option>
            <option value={86400}>每天</option>
          </select>
        </div>
      </div>
    </div>

    <div class="form-item">
      <span class="form-label">User-Agent</span>
      <div class="form-input-wrap">
        <Input bind:value={userAgent} placeholder="留空使用客户端默认值" disabled={saving} />
        <div class="form-hint">来源格式仅用于下载转换，导入后统一呈现为五种内核语义。</div>
      </div>
    </div>
  {:else}
    <div class="rules-section">
      <div class="rules-section-header">
        <div>
          <div class="section-title">语义规则</div>
          <div class="form-hint">空值不会保存；域名和 CIDR 会在构建时进行严格校验。</div>
        </div>
        <Button variant="outline" size="sm" onclick={addRule} disabled={saving}>
          <Plus class="h-3.5 w-3.5" />
          <span>添加规则</span>
        </Button>
      </div>

      <div class="rule-list">
        {#each rules as rule, index (index)}
          <div class="rule-row">
            <select
              value={rule.type}
              class="field-select"
              onchange={(event) => setRuleType(index, event.currentTarget.value as ZeroRuleType)}
              disabled={saving}
              aria-label="规则类型"
            >
              {#each RULE_TYPES as type}
                <option value={type.value}>{type.label}</option>
              {/each}
            </select>
            <Input
              value={rule.value}
              oninput={(event) => setRuleValue(index, event.currentTarget.value)}
              placeholder={placeholder(rule.type)}
              disabled={saving}
              aria-label="规则值"
            />
            <Button
              variant="ghost"
              size="icon-sm"
              class="delete-button"
              onclick={() => removeRule(index)}
              disabled={saving}
              aria-label="删除规则"
              title="删除规则"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </Button>
          </div>
        {/each}
      </div>
    </div>

    {#if sourceUrl}
      <label class="retain-source">
        <input type="checkbox" bind:checked={retainSource} disabled={saving} />
        <span>保留订阅关联；下次同步会用订阅内容覆盖当前规则</span>
      </label>
    {/if}
  {/if}

  {#snippet footer()}
    <Button variant="outline" onclick={closeEditor} disabled={saving}>取消</Button>
    <Button onclick={save} disabled={!canSave}>
      {saving ? '构建并保存中...' : mode === 'subscription' && !editingId ? '导入并构建' : '保存并构建'}
    </Button>
  {/snippet}
</DraggableModal>

<DraggableModal
  title="删除规则集"
  description="此操作会删除规则语义、订阅关联和已构建的 ZRS 产物，无法撤销。"
  open={pendingDelete !== null}
  onClose={cancelRemove}
  closeDisabled={deletingId !== null}
  width="min(440px, 90vw)"
>
  <div class="delete-confirmation">
    <div class="delete-confirmation-icon">
      <AlertTriangle class="h-5 w-5" />
    </div>
    <div class="delete-confirmation-copy">
      <span class="delete-confirmation-title">确定删除“{pendingDelete?.name}”吗？</span>
      <span>删除后，引用该规则集的配置可能无法继续正常工作。</span>
    </div>
  </div>

  {#snippet footer()}
    <Button variant="outline" onclick={cancelRemove} disabled={deletingId !== null}>取消</Button>
    <Button variant="destructive" onclick={confirmRemove} disabled={deletingId !== null}>
      {deletingId ? '删除中...' : '确认删除'}
    </Button>
  {/snippet}
</DraggableModal>

<style>
  .rules-root {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 11px 14px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .panel-title-group {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
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

  .header-actions,
  .row-actions,
  .summary-counts {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .summary-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--muted-foreground);
    font-size: 10.5px;
    flex-shrink: 0;
  }

  .summary-copy {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
  }

  :global(.summary-icon) {
    color: var(--accent-foreground);
    flex-shrink: 0;
  }

  .summary-counts {
    font-family: var(--font-mono);
    white-space: nowrap;
  }

  .error-banner {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 8px 14px;
    border-bottom: 1px solid color-mix(in srgb, var(--destructive) 18%, var(--border));
    background: color-mix(in srgb, var(--destructive) 8%, transparent);
    color: var(--destructive);
    font-size: 11px;
    flex-shrink: 0;
  }

  .panel-empty {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .empty-stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
  }

  :global(.empty-icon) { opacity: 0.28; }

  .empty-title {
    color: var(--foreground);
    font-size: 13px;
    font-weight: 600;
  }

  .empty-hint {
    margin-top: -3px;
    font-size: 11px;
    opacity: 0.75;
  }

  .list-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 5px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .list-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 11px;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    outline: none;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .list-row:hover,
  .list-row:focus-visible {
    background: var(--muted);
    border-color: var(--border);
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .row-top,
  .row-meta,
  .source-line {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }

  .row-name {
    color: var(--foreground);
    font-size: 12.5px;
    font-weight: 600;
  }

  .row-meta,
  .source-line {
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .mono { font-family: var(--font-mono); }

  .source-line { flex-wrap: nowrap; }

  .source-url {
    max-width: min(46vw, 430px);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-error {
    color: var(--destructive);
    font-size: 10.5px;
    line-height: 1.4;
  }

  :global(.delete-button:hover) {
    color: var(--destructive);
    background: color-mix(in srgb, var(--destructive) 10%, transparent);
  }

  .form-item {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .delete-confirmation {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 4px 0;
  }

  .delete-confirmation-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--destructive) 10%, transparent);
    color: var(--destructive);
    flex-shrink: 0;
  }

  .delete-confirmation-copy {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.5;
  }

  .delete-confirmation-title {
    color: var(--foreground);
    font-size: 12.5px;
    font-weight: 600;
  }

  .form-row {
    display: flex;
    gap: 12px;
  }

  .form-row .form-item { flex: 1; }

  .form-label {
    flex-shrink: 0;
    width: 76px;
    padding-top: 7px;
    color: var(--foreground);
    font-size: 12px;
    font-weight: 500;
    text-align: right;
  }

  .form-input-wrap {
    flex: 1;
    min-width: 0;
  }

  .required { color: var(--destructive); }

  .form-hint {
    margin-top: 4px;
    color: var(--muted-foreground);
    font-size: 10.5px;
    line-height: 1.45;
  }

  .source-switch {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--segment-bg);
  }

  .source-button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .source-button.active {
    background: var(--segment-active-bg);
    box-shadow: var(--segment-active-shadow);
    color: var(--foreground);
  }

  .source-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .field-select {
    width: 100%;
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--muted);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    outline: none;
  }

  .field-select:focus {
    border-color: rgba(99, 102, 241, 0.35);
  }

  .rules-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface);
  }

  .rules-section-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .section-title {
    color: var(--foreground);
    font-size: 12px;
    font-weight: 600;
  }

  .rule-list {
    max-height: 310px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
  }

  .rule-row {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr) 32px;
    align-items: center;
    gap: 7px;
  }

  .retain-source {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 88px;
    color: var(--muted-foreground);
    font-size: 11px;
    cursor: pointer;
  }

  .retain-source input { accent-color: var(--primary); }

  :global(.spin) { animation: spin 0.8s linear infinite; }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 700px) {
    .panel-header,
    .summary-strip {
      align-items: flex-start;
      flex-direction: column;
    }

    .summary-counts { display: none; }

    .form-row { flex-direction: column; }

    .rule-row { grid-template-columns: 120px minmax(0, 1fr) 32px; }

    .retain-source { padding-left: 0; }
  }
</style>
