<script lang="ts">
  import { onMount } from 'svelte';
  import { openUrl as openLink } from '@tauri-apps/plugin-opener';
  import { AlertTriangle, Database, ExternalLink, LayoutGrid, List, Plus, RefreshCw, ShieldCheck, Trash2 } from '@lucide/svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import {
    getCommonRuleInjectionStatus,
    getRuleSet,
    listRuleSets,
    removeRuleSet,
    setCommonRuleBinding,
    setCommonRuleInjectionEnabled,
    updateAllRuleSets,
    updateBuiltinRuleSets,
    updateRuleSet,
    upsertRuleSet,
  } from '$lib/services/config';
  import { getAppErrorMessage } from '$lib/services/core';
  import type { CommonRuleAction, CommonRuleInjectionStatus, RuleSetProfile, RuleSetSummary, ZeroRule, ZeroRuleType } from '$lib/types/domain';

  const RULE_TYPES: { value: ZeroRuleType; label: string; placeholder: string }[] = [
    { value: 'domain_exact', label: '精确域名', placeholder: 'api.example.com' },
    { value: 'domain_suffix', label: '域名后缀', placeholder: 'example.com' },
    { value: 'domain_keyword', label: '域名关键词', placeholder: 'special' },
    { value: 'ipv4_cidr', label: 'IPv4 CIDR', placeholder: '10.0.0.0/8' },
    { value: 'ipv6_cidr', label: 'IPv6 CIDR', placeholder: 'fd00::/8' },
  ];
  const VIEW_MODE_KEY = 'znet-rules-view-mode';
  const MAX_VISUAL_EDIT_RULES = 1000;
  type ViewMode = 'card' | 'list';

  let items = $state<RuleSetSummary[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let updatingId = $state<string | null>(null);
  let updatingAll = $state(false);
  let updatingBuiltins = $state(false);
  let deletingId = $state<string | null>(null);
  let pendingDelete = $state<RuleSetSummary | null>(null);
  let loadError = $state('');
  let pageError = $state('');
  let editorError = $state('');
  let deleteError = $state('');
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
  let viewMode = $state<ViewMode>('list');
  let commonStatus = $state<CommonRuleInjectionStatus | null>(null);
  let commonSaving = $state(false);
  let bindingId = $state<string | null>(null);
  let loadingDetailId = $state<string | null>(null);
  let builtinDetails = $state<RuleSetSummary | null>(null);
  let builtinDetailsError = $state('');

  const sourceCount = $derived(items.filter((item) => item.source).length);
  const readyCount = $derived(items.filter((item) => item.artifact).length);
  const busy = $derived(saving || updatingId !== null || updatingAll || updatingBuiltins || deletingId !== null || commonSaving || bindingId !== null || loadingDetailId !== null);
  const canSave = $derived(
    !busy
      && name.trim().length > 0
      && (mode !== 'subscription' || !!editingId || sourceUrl.trim().length > 0),
  );

  onMount(() => {
    viewMode = loadViewMode();
    void load();
  });

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

  async function load(showLoading = true) {
    if (showLoading) loading = true;
    try {
      const [nextItems, nextStatus] = await Promise.all([
        listRuleSets(),
        getCommonRuleInjectionStatus(),
      ]);
      items = nextItems;
      commonStatus = nextStatus;
      loadError = '';
    } catch (cause) {
      loadError = getAppErrorMessage(cause, '加载规则集失败');
    } finally {
      if (showLoading) loading = false;
    }
  }

  function commonStatusCopy() {
    if (!commonStatus?.enabled) return '默认关闭，不改动当前运行配置';
    if (commonStatus.effective) return `已注入 ${commonStatus.injectedCount} 个公共规则集`;
    return commonStatus.reason ?? '当前未生效';
  }

  async function toggleCommonInjection() {
    if (commonSaving) return;
    commonSaving = true;
    pageError = '';
    try {
      commonStatus = await setCommonRuleInjectionEnabled(!commonStatus?.enabled);
    } catch (cause) {
      pageError = getAppErrorMessage(cause, '切换公共规则注入失败，已保留原运行配置');
    } finally {
      commonSaving = false;
    }
  }

  async function updateCommonBinding(
    item: RuleSetSummary,
    patch: Partial<{ enabled: boolean; action: CommonRuleAction; order: number }>,
  ) {
    if (bindingId) return;
    bindingId = item.id;
    pageError = '';
    const current = item.commonBinding ?? { enabled: false, action: 'final' as const, order: items.indexOf(item) * 10 };
    const requestedOrder = patch.order ?? current.order;
    try {
      const updated = await setCommonRuleBinding({
        ruleSetId: item.id,
        enabled: patch.enabled ?? current.enabled,
        action: patch.action ?? current.action,
        order: Number.isFinite(requestedOrder) ? Math.max(0, Math.trunc(requestedOrder)) : current.order,
      });
      items = items.map((candidate) => candidate.id === updated.id
        ? { ...candidate, enabled: updated.enabled, commonBinding: updated.commonBinding, artifact: updated.artifact }
        : candidate);
      commonStatus = await getCommonRuleInjectionStatus();
    } catch (cause) {
      pageError = getAppErrorMessage(cause, '更新公共规则绑定失败，已保留原运行配置');
    } finally {
      bindingId = null;
    }
  }

  function openNew() {
    if (busy) return;
    editingId = undefined;
    name = '';
    mode = 'visual';
    rules = [{ type: 'domain_suffix', value: '' }];
    sourceUrl = '';
    sourceFormat = 'auto';
    updateIntervalSecs = 0;
    userAgent = '';
    retainSource = false;
    editorError = '';
    showEditor = true;
  }

  function openSourceEdit(item: RuleSetSummary) {
    editingId = item.id;
    name = item.name;
    mode = 'subscription';
    rules = [];
    sourceUrl = item.source?.url ?? '';
    sourceFormat = item.source?.format ?? 'auto';
    updateIntervalSecs = item.source?.updateIntervalSecs ?? 0;
    userAgent = item.source?.userAgent ?? '';
    retainSource = true;
    editorError = '';
    showEditor = true;
  }

  async function openRuleSet(item: RuleSetSummary) {
    if (busy) return;
    if (item.builtIn) {
      builtinDetails = item;
      builtinDetailsError = '';
      return;
    }
    if (item.source) {
      openSourceEdit(item);
      return;
    }
    if (item.editableRuleCount > MAX_VISUAL_EDIT_RULES) {
      pageError = `“${item.name}”包含 ${item.editableRuleCount} 条规则，不能载入可视化编辑器；请拆分为小型覆盖规则集。`;
      return;
    }
    loadingDetailId = item.id;
    pageError = '';
    try {
      const profile: RuleSetProfile = await getRuleSet(item.id);
      editingId = profile.id;
      name = profile.name;
      mode = 'visual';
      rules = profile.semanticIr.rules.map((rule) => ({ ...rule }));
      sourceUrl = '';
      sourceFormat = 'auto';
      updateIntervalSecs = 0;
      userAgent = '';
      retainSource = false;
      editorError = '';
      showEditor = true;
    } catch (cause) {
      pageError = getAppErrorMessage(cause, '读取规则集失败');
    } finally {
      loadingDetailId = null;
    }
  }

  function closeBuiltinDetails() {
    builtinDetails = null;
    builtinDetailsError = '';
  }

  async function openBuiltinSource() {
    const url = builtinDetails?.provenance?.sourceUrl;
    if (!url) return;
    builtinDetailsError = '';
    try {
      await openLink(url);
    } catch (cause) {
      builtinDetailsError = getAppErrorMessage(cause, '打开内置规则源失败');
    }
  }

  function closeEditor() {
    if (saving) return;
    showEditor = false;
    editorError = '';
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
    editorError = '';
    try {
      if (mode === 'subscription') {
        await upsertRuleSet({
          id: editingId,
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
      await load(false);
    } catch (cause) {
      editorError = getAppErrorMessage(cause, '保存规则集失败');
    } finally {
      saving = false;
    }
  }

  async function update(id: string) {
    if (busy) return;
    updatingId = id;
    pageError = '';
    try {
      await updateRuleSet(id);
    } catch (cause) {
      pageError = getAppErrorMessage(cause, '更新规则集失败');
    } finally {
      updatingId = null;
      await load(false);
    }
  }

  function requestRemove(item: RuleSetSummary) {
    if (busy) return;
    deleteError = '';
    pendingDelete = item;
  }

  function cancelRemove() {
    if (deletingId) return;
    pendingDelete = null;
    deleteError = '';
  }

  async function confirmRemove() {
    if (!pendingDelete || busy) return;
    const id = pendingDelete.id;
    deletingId = id;
    try {
      await removeRuleSet(id);
      pendingDelete = null;
      await load(false);
    } catch (cause) {
      deleteError = getAppErrorMessage(cause, '删除规则集失败');
    } finally {
      deletingId = null;
    }
  }

  async function updateAll() {
    if (busy) return;
    updatingAll = true;
    pageError = '';
    try {
      const outcome = await updateAllRuleSets();
      await load(false);
      pageError = outcome.failed ? `${outcome.failed} 个来源更新失败，旧产物已保留` : '';
    } catch (cause) {
      pageError = getAppErrorMessage(cause, '批量更新规则集失败');
    } finally {
      updatingAll = false;
    }
  }

  async function updateBuiltins() {
    if (busy) return;
    updatingBuiltins = true;
    pageError = '';
    try {
      const outcome = await updateBuiltinRuleSets();
      await load(false);
      if (outcome.updated === 0) pageError = '内置规则已是最新版本';
    } catch (cause) {
      pageError = getAppErrorMessage(cause, '更新内置规则失败，已继续使用当前版本');
    } finally {
      updatingBuiltins = false;
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
      <span class="panel-subtitle">管理规则源与 ZRS 产物</span>
    </div>

    <div class="header-actions">
      <SegmentedControl.Root
        value={viewMode}
        onValueChange={(value) => setViewMode(value as ViewMode)}
        aria-label="规则集显示方式"
      >
        <SegmentedControl.Item
          value="card"
          size="icon"
          title="卡片视图"
          aria-label="卡片视图"
        >
          <LayoutGrid class="h-3.5 w-3.5" />
        </SegmentedControl.Item>
        <SegmentedControl.Item
          value="list"
          size="icon"
          title="列表视图"
          aria-label="列表视图"
        >
          <List class="h-3.5 w-3.5" />
        </SegmentedControl.Item>
      </SegmentedControl.Root>
      {#if sourceCount > 0}
        <Button variant="outline" size="sm" onclick={updateAll} disabled={busy}>
          <RefreshCw class={`h-3.5 w-3.5 ${updatingAll ? 'spin' : ''}`} />
          <span>{updatingAll ? '更新中...' : '更新全部'}</span>
        </Button>
      {/if}
      {#if items.some((item) => item.builtIn)}
        <Button variant="outline" size="sm" onclick={updateBuiltins} disabled={busy}>
          <RefreshCw class={`h-3.5 w-3.5 ${updatingBuiltins ? 'spin' : ''}`} />
          <span>{updatingBuiltins ? '更新中...' : '更新内置'}</span>
        </Button>
      {/if}
      <Button size="sm" onclick={openNew} disabled={busy}>
        <Plus class="h-3.5 w-3.5" />
        <span>新建</span>
      </Button>
    </div>
  </div>

  <div class="summary-strip">
    <div class="summary-copy">
      <ShieldCheck class="summary-icon h-4 w-4" />
      <span>保存后生成并校验 ZRS 产物。</span>
    </div>
    {#if items.length > 0}
      <div class="summary-counts">
        <span>{items.length} 个规则集</span>
        <span>·</span>
        <span>{readyCount} 个产物就绪</span>
        {#if sourceCount > 0}
          <span>·</span>
          <span>{sourceCount} 个外部来源</span>
        {/if}
      </div>
    {/if}
  </div>

  <div class="common-injection-row">
    <div class="common-injection-copy">
      <span class="common-injection-title">在规则模式下注入公共规则</span>
      <span class="common-injection-hint">{commonStatusCopy()}。机场订阅规则优先，公共规则作为补充，且不会写回订阅原配置。</span>
    </div>
    <Switch
      checked={commonStatus?.enabled ?? false}
      onCheckedChange={toggleCommonInjection}
      disabled={loading || commonSaving}
      aria-label="在规则模式下注入公共规则"
    />
  </div>

  {#if pageError}
    <div class="error-banner" role="alert">
      <AlertTriangle class="h-3.5 w-3.5" />
      <span>{pageError}</span>
    </div>
  {/if}

  {#if loading}
    <div class="panel-empty">加载中...</div>
  {:else if loadError}
    <div class="panel-empty" role="alert">
      <div class="empty-stack error-empty">
        <AlertTriangle class="h-6 w-6" />
        <span class="empty-title">规则集加载失败</span>
        <span class="empty-hint">{loadError}</span>
        <Button variant="outline" size="sm" onclick={() => load()}>重试</Button>
      </div>
    </div>
  {:else if items.length === 0}
    <div class="panel-empty">
      <div class="empty-stack">
        <Database class="empty-icon h-9 w-9" />
        <span class="empty-title">还没有规则集</span>
        <span class="empty-hint">手动创建语义规则，或从独立规则源导入</span>
        <Button size="sm" onclick={openNew} disabled={busy}>
          <Plus class="h-3.5 w-3.5" />
          <span>新建规则集</span>
        </Button>
      </div>
    </div>
  {:else}
    <div class="list-scroll" class:card-view={viewMode === 'card'}>
      {#each items as item (item.id)}
        <div class="list-row">
          <button
            type="button"
            class="row-main"
            onclick={() => openRuleSet(item)}
            disabled={busy}
            title={item.builtIn ? '查看内置规则详情' : '编辑规则'}
          >
            <div class="row-top">
              <span class="row-name">{item.name}</span>
              <Badge variant={item.artifact ? 'secondary' : 'outline'}>
                {item.artifact ? 'ZRS 已就绪' : '待构建'}
              </Badge>
              <Badge variant="outline">{item.builtIn ? '内置' : item.source ? '外部来源' : '本地'}</Badge>
              {#if item.commonBinding?.enabled}
                <Badge variant="secondary">公共规则</Badge>
              {/if}
            </div>

            <div class="row-meta">
              {#if item.builtIn}
                <span>{item.artifact?.entryCount ?? 0} 条内置规则</span>
              {:else if item.source}
                <span>{item.artifact?.entryCount ?? 0} 条外部规则</span>
              {:else}
                <span>{item.editableRuleCount} 条本地规则</span>
                <span>→</span>
                <span>{item.artifact?.entryCount ?? 0} 个索引项</span>
              {/if}
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
          </button>

          <div class="row-actions">
            <div class="common-binding" title="仅在公共规则总开关开启且处于规则模式时生效">
                <Switch
                  size="sm"
                  checked={item.commonBinding?.enabled ?? false}
                  onCheckedChange={() => updateCommonBinding(item, { enabled: !(item.commonBinding?.enabled ?? false) })}
                  disabled={busy || !item.artifact}
                  aria-label={`将 ${item.name} 用作公共规则`}
                />
                <select
                  class="binding-select"
                  value={item.commonBinding?.action ?? 'final'}
                  onchange={(event) => updateCommonBinding(item, { action: event.currentTarget.value as CommonRuleAction })}
                  disabled={busy || !item.commonBinding?.enabled}
                  aria-label="匹配动作"
                >
                  <option value="final">沿用最终路由</option>
                  <option value="proxy">代理</option>
                  <option value="direct">直连</option>
                  <option value="reject">拒绝</option>
                </select>
                <input
                  class="binding-order"
                  type="number"
                  min="0"
                  value={item.commonBinding?.order ?? items.indexOf(item) * 10}
                  onchange={(event) => updateCommonBinding(item, { order: Number(event.currentTarget.value) })}
                  disabled={busy || !item.commonBinding?.enabled}
                  aria-label="公共规则顺序"
                  title="数值越小越优先"
                />
            </div>
            {#if item.source}
              <Button
                variant="ghost"
                size="icon-sm"
                onclick={(event) => {
                  event.stopPropagation();
                  update(item.id);
                }}
                disabled={busy}
                title="下载并重建"
                aria-label="下载并重建"
              >
                <RefreshCw class={`h-3.5 w-3.5 ${updatingId === item.id ? 'spin' : ''}`} />
              </Button>
            {/if}
            {#if !item.builtIn}
              <Button
                variant="ghost"
                size="icon-sm"
                class="delete-button"
                onclick={(event) => {
                  event.stopPropagation();
                  requestRemove(item);
                }}
                disabled={busy}
                title="删除"
                aria-label="删除"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </Button>
            {/if}
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
        <SegmentedControl.Root
          value={mode}
          onValueChange={(value) => mode = value as 'visual' | 'subscription'}
          disabled={saving}
          aria-label="创建方式"
        >
          <SegmentedControl.Item value="visual">
            手动创建
          </SegmentedControl.Item>
          <SegmentedControl.Item value="subscription">
            外部导入
          </SegmentedControl.Item>
        </SegmentedControl.Root>
      </div>
    </div>
  {/if}

  {#if mode === 'subscription'}
    <div class="form-item">
      <span class="form-label">规则源地址 <span class="required">*</span></span>
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
        <span>保留外部来源；下次同步会用远程内容覆盖当前规则</span>
      </label>
    {/if}
  {/if}

  {#if editorError}
    <div class="error-banner modal-error" role="alert">
      <AlertTriangle class="h-3.5 w-3.5" />
      <span>{editorError}</span>
    </div>
  {/if}

  {#snippet footer()}
    <Button variant="outline" onclick={closeEditor} disabled={saving}>取消</Button>
    <Button onclick={save} disabled={!canSave}>
      {saving ? '构建并保存中...' : mode === 'subscription' ? (editingId ? '更新来源并重建' : '导入并构建') : '保存并构建'}
    </Button>
  {/snippet}
</DraggableModal>

<DraggableModal
  title={builtinDetails?.name ?? '内置规则集'}
  description="内置规则以只读 ZRS 产物随应用提供；可查看版本与来源，但不能直接编辑或删除。"
  open={builtinDetails !== null}
  onClose={closeBuiltinDetails}
  width="min(680px, 92vw)"
>
  {#if builtinDetails}
    <div class="builtin-details">
      <div class="builtin-stat-grid">
        <div class="builtin-stat">
          <span class="builtin-stat-label">规则条目</span>
          <span class="builtin-stat-value">{builtinDetails.artifact?.entryCount ?? 0}</span>
        </div>
        <div class="builtin-stat">
          <span class="builtin-stat-label">产物大小</span>
          <span class="builtin-stat-value">{formatBytes(builtinDetails.artifact?.fileSize)}</span>
        </div>
        <div class="builtin-stat">
          <span class="builtin-stat-label">ZRS 版本</span>
          <span class="builtin-stat-value">
            {builtinDetails.artifact
              ? `${builtinDetails.artifact.majorVersion}.${builtinDetails.artifact.minorVersion}`
              : '—'}
          </span>
        </div>
        <div class="builtin-stat">
          <span class="builtin-stat-label">CRC32</span>
          <span class="builtin-stat-value mono">
            {builtinDetails.artifact
              ? builtinDetails.artifact.checksum.toString(16).padStart(8, '0')
              : '—'}
          </span>
        </div>
      </div>

      {#if builtinDetails.provenance}
        <dl class="builtin-provenance">
          <div>
            <dt>来源仓库</dt>
            <dd class="mono">{builtinDetails.provenance.repository}</dd>
          </div>
          <div>
            <dt>固定提交</dt>
            <dd class="mono">{builtinDetails.provenance.revision}</dd>
          </div>
          <div>
            <dt>许可证</dt>
            <dd>{builtinDetails.provenance.license}</dd>
          </div>
          <div>
            <dt>源文件 SHA-256</dt>
            <dd class="mono">{builtinDetails.provenance.sourceSha256}</dd>
          </div>
          <div>
            <dt>语义规则 SHA-256</dt>
            <dd class="mono">{builtinDetails.provenance.irSha256}</dd>
          </div>
        </dl>

        <div class="builtin-source-note">
          完整规则内容保存在固定提交对应的上游源文件中。打开后可直接搜索域名或 CIDR。
        </div>
      {/if}

      {#if builtinDetailsError}
        <div class="error-banner modal-error" role="alert">
          <AlertTriangle class="h-3.5 w-3.5" />
          <span>{builtinDetailsError}</span>
        </div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    <Button variant="outline" onclick={closeBuiltinDetails}>关闭</Button>
    <Button
      onclick={openBuiltinSource}
      disabled={!builtinDetails?.provenance?.sourceUrl}
    >
      <ExternalLink class="h-3.5 w-3.5" />
      <span>查看原始规则内容</span>
    </Button>
  {/snippet}
</DraggableModal>

<DraggableModal
  title="删除规则集"
  description="此操作会删除管理记录和订阅关联；已发布的不可变 ZRS 文件会保留，以免影响仍在映射它的内核进程。"
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

  {#if deleteError}
    <div class="error-banner modal-error" role="alert">
      <AlertTriangle class="h-3.5 w-3.5" />
      <span>{deleteError}</span>
    </div>
  {/if}

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

  .common-injection-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--card) 94%, var(--accent));
    flex-shrink: 0;
  }

  .common-injection-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .common-injection-title {
    color: var(--foreground);
    font-size: 12px;
    font-weight: 600;
  }

  .common-injection-hint {
    color: var(--muted-foreground);
    font-size: 10.5px;
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

  .modal-error {
    border: 1px solid color-mix(in srgb, var(--destructive) 22%, var(--border));
    border-radius: 7px;
  }

  .builtin-details {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .builtin-stat-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }

  .builtin-stat {
    min-width: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 55%, transparent);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .builtin-stat-label,
  .builtin-provenance dt {
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .builtin-stat-value {
    color: var(--foreground);
    font-size: 12px;
    font-weight: 600;
  }

  .builtin-provenance {
    margin: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }

  .builtin-provenance > div {
    min-width: 0;
    padding: 8px 10px;
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
  }

  .builtin-provenance > div + div {
    border-top: 1px solid var(--border);
  }

  .builtin-provenance dt,
  .builtin-provenance dd {
    margin: 0;
  }

  .builtin-provenance dd {
    min-width: 0;
    color: var(--foreground);
    font-size: 10.5px;
    overflow-wrap: anywhere;
  }

  .builtin-source-note {
    color: var(--muted-foreground);
    font-size: 10.5px;
    line-height: 1.6;
  }

  @media (max-width: 620px) {
    .builtin-stat-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .builtin-provenance > div {
      grid-template-columns: 1fr;
      gap: 3px;
    }
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

  .error-empty {
    max-width: 440px;
    color: var(--destructive);
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

  .list-scroll.card-view {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
    align-content: start;
    gap: 10px;
    padding: 10px;
  }

  .list-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 11px;
    border: 1px solid transparent;
    border-radius: 8px;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .list-row:hover,
  .list-row:focus-within {
    background: var(--muted);
    border-color: var(--border);
  }

  .card-view .list-row {
    align-items: flex-start;
    min-height: 158px;
    padding: 13px;
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
    align-self: stretch;
    min-height: 130px;
  }

  .card-view .row-top {
    padding-bottom: 3px;
  }

  .card-view .row-name {
    font-size: 13.5px;
  }

  .card-view .row-meta {
    padding: 7px 8px;
    border-radius: 6px;
    background: var(--muted);
  }

  .card-view .source-line {
    margin-top: auto;
    padding-top: 5px;
    flex-wrap: wrap;
  }

  .card-view .source-url {
    max-width: 100%;
    flex-basis: 100%;
  }

  .card-view .row-actions {
    margin: -4px -4px 0 0;
  }

  .common-binding {
    display: flex;
    align-items: center;
    gap: 5px;
    padding-right: 5px;
    border-right: 1px solid var(--border);
  }

  .binding-select,
  .binding-order {
    height: 27px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--muted);
    color: var(--foreground);
    font-size: 10.5px;
  }

  .binding-select { width: 100px; padding: 0 5px; }
  .binding-order { width: 48px; padding: 0 5px; font-family: var(--font-mono); }

  .binding-select:disabled,
  .binding-order:disabled { opacity: 0.45; }

  .card-view .common-binding {
    position: absolute;
    right: 10px;
    bottom: 10px;
    padding: 0;
    border: 0;
  }

  .card-view .list-row { position: relative; padding-bottom: 48px; }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    outline: none;
  }

  .row-main:disabled {
    cursor: wait;
    opacity: 0.65;
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

  .field-select {
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--input);
    border-radius: var(--control-radius);
    background: var(--background);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
    outline: none;
  }

  .field-select:focus {
    border-color: var(--ring);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 18%, transparent);
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

    .header-actions {
      width: 100%;
      flex-wrap: wrap;
    }

    .list-scroll.card-view {
      grid-template-columns: 1fr;
    }

    .form-row { flex-direction: column; }

    .rule-row { grid-template-columns: 120px minmax(0, 1fr) 32px; }

    .retain-source { padding-left: 0; }
  }
</style>
