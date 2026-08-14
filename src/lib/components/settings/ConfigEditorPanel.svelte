<script lang="ts">
  import { configEditor, type ValidationError, type ConfigImpactItem } from '$lib/services/config-editor.svelte';
  import { Button } from '$lib/components/ui/button';
  import { AlertTriangle, Check, Loader2, RefreshCcw, RotateCcw, Send, ScanSearch, Zap, Power } from '@lucide/svelte';

  let textareaRef: HTMLTextAreaElement | undefined = $state();
  let tabSize = 2;

  $effect(() => {
    // Auto-load on mount
    if (configEditor.phase === 'idle') {
      configEditor.load();
    }
  });

  function handleInput(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    configEditor.updateDraft(target.value);
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Tab inserts spaces instead of changing focus
    if (e.key === 'Tab') {
      e.preventDefault();
      const textarea = textareaRef;
      if (!textarea) return;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const spaces = ' '.repeat(tabSize);
      const value = textarea.value;
      const newValue = value.substring(0, start) + spaces + value.substring(end);
      textarea.value = newValue;
      textarea.selectionStart = textarea.selectionEnd = start + tabSize;
      configEditor.updateDraft(newValue);
    }
  }

  async function handleLoad() {
    await configEditor.load();
  }

  async function handleValidate() {
    await configEditor.validate();
  }

  async function handlePlanApply() {
    await configEditor.planApply();
  }

  function handleReset() {
    configEditor.reset();
  }

  function formatJson() {
    const parsed = configEditor.parseDraft();
    if (parsed) {
      configEditor.updateDraft(JSON.stringify(parsed, null, 2));
    }
  }

  function jsonLineCount(): number {
    const lines = configEditor.draftJson.split('\n');
    return Math.max(lines.length, 1);
  }

  /** Section label mapping for human-friendly display. */
  const sectionLabels: Record<string, string> = {
    outbounds: '出站代理',
    listeners: '监听器',
    rules: '路由规则',
    tun: 'TUN 虚拟网卡',
    dns: 'DNS',
    inbound: '入站',
    routing: '路由',
    experimental: '实验选项',
    config: '配置',
  };

  function sectionLabel(section: string): string {
    return sectionLabels[section] ?? section;
  }

  const isLoading = $derived(
    configEditor.phase === 'validating' || configEditor.phase === 'applying' || configEditor.phase === 'planning'
  );
  const canValidate = $derived(
    configEditor.phase === 'loaded' || configEditor.phase === 'editing' || configEditor.phase === 'error'
  );
  const canPlanApply = $derived(
    configEditor.supportsPlanApply
    && (configEditor.phase === 'editing' || configEditor.phase === 'loaded' || configEditor.phase === 'error')
    && configEditor.dirty
  );
  const canApply = $derived(
    (configEditor.phase === 'editing' || configEditor.phase === 'loaded' || configEditor.phase === 'planned') && configEditor.dirty
  );
  const canReset = $derived(configEditor.dirty);
  const hasValidationErrors = $derived(configEditor.validationErrors.length > 0);
  const hasPlanResult = $derived(configEditor.planResult !== null);
  const hasRestartImpact = $derived(
    configEditor.planResult !== null && configEditor.planResult.requiresRestart.length > 0
  );
  /** Whether the "应用" button should act as a "confirm after restart-impact review". */
  const needsConfirmApply = $derived(
    configEditor.phase === 'planned' && hasRestartImpact
  );

  async function handleApply() {
    if (needsConfirmApply) {
      await configEditor.confirmApply();
    } else {
      await configEditor.apply();
    }
  }
</script>

<div class="panel">
  <!-- Header -->
  <div class="header">
    <div class="heading">
      <div class="title">内核配置编辑</div>
      <div class="desc">
        编辑当前活动的 Zero 配置。保存前由运行中内核校验，应用后同步回活动配置并自动对账。
      </div>
    </div>
    <div class="actions">
      <Button variant="ghost" size="icon-sm" onclick={handleLoad} disabled={isLoading}>
        <RefreshCcw class="h-3.5 w-3.5" />
      </Button>
    </div>
  </div>

  <!-- Status bar -->
  <div class="status-bar">
    <div class="status-left">
      {#if configEditor.phase === 'idle' || configEditor.phase === 'loaded'}
        <span class="status-badge {configEditor.phase === 'loaded' ? 'ready' : ''}">
          {configEditor.phase === 'loaded' ? '已加载' : '未加载'}
        </span>
      {:else if configEditor.phase === 'editing'}
        <span class="status-badge editing">编辑中</span>
      {:else if configEditor.phase === 'validating'}
        <span class="status-badge loading">
          <Loader2 class="h-3 w-3 animate-spin" />
          校验中
        </span>
      {:else if configEditor.phase === 'planning'}
        <span class="status-badge loading">
          <Loader2 class="h-3 w-3 animate-spin" />
          预检中
        </span>
      {:else if configEditor.phase === 'planned'}
        <span class="status-badge planned">
          <ScanSearch class="h-3 w-3" />
          {hasRestartImpact ? '部分需重启' : '可热加载'}
        </span>
      {:else if configEditor.phase === 'applying'}
        <span class="status-badge loading">
          <Loader2 class="h-3 w-3 animate-spin" />
          应用中
        </span>
      {:else if configEditor.phase === 'applied'}
        <span class="status-badge success">
          <Check class="h-3 w-3" />
          已应用
        </span>
      {:else if configEditor.phase === 'error'}
        <span class="status-badge error">
          <AlertTriangle class="h-3 w-3" />
          错误
        </span>
      {/if}

      {#if configEditor.dirty}
        <span class="dirty-badge">未保存</span>
      {/if}

      {#if configEditor.lastAppliedAt}
        <span class="applied-time">
          上次应用: {new Date(configEditor.lastAppliedAt).toLocaleTimeString('zh-CN', { hour12: false })}
        </span>
      {/if}
    </div>

    <div class="actions-bar">
      <Button
        variant="outline"
        size="sm"
        onclick={formatJson}
        disabled={!configEditor.draftJson}
        title="格式化 JSON"
      >
        格式化
      </Button>
      <Button
        variant="outline"
        size="sm"
        onclick={handleReset}
        disabled={!canReset}
        title="放弃修改，恢复到内核当前配置"
      >
        <RotateCcw class="h-3.5 w-3.5" />
        重置
      </Button>
      <Button
        variant="outline"
        size="sm"
        onclick={handleValidate}
        disabled={!canValidate || isLoading}
        title="校验配置，不应用到内核"
      >
        {#if configEditor.phase === 'validating'}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <Check class="h-3.5 w-3.5" />
        {/if}
        校验
      </Button>
      {#if configEditor.supportsPlanApply}
        <Button
          variant="outline"
          size="sm"
          onclick={handlePlanApply}
          disabled={!canPlanApply || isLoading}
          title="预检配置变更的影响范围（可热加载 vs 需重启）"
        >
          {#if configEditor.phase === 'planning'}
            <Loader2 class="h-3.5 w-3.5 animate-spin" />
          {:else}
            <ScanSearch class="h-3.5 w-3.5" />
          {/if}
          预检
        </Button>
      {/if}
      <Button
        size="sm"
        variant={needsConfirmApply ? 'destructive' : 'default'}
        onclick={handleApply}
        disabled={!canApply || isLoading}
        title={needsConfirmApply
          ? '部分变更需要重启内核才能生效，点击确认应用'
          : '校验并应用到运行中的内核（热加载，无需重启）'}
      >
        {#if configEditor.phase === 'applying'}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else if needsConfirmApply}
          <AlertTriangle class="h-3.5 w-3.5" />
        {:else}
          <Send class="h-3.5 w-3.5" />
        {/if}
        {needsConfirmApply ? '确认应用' : '应用'}
      </Button>
    </div>
  </div>

  <!-- Validation errors -->
  {#if hasValidationErrors}
    <div class="error-panel">
      <div class="error-title">
        <AlertTriangle class="h-3.5 w-3.5" />
        <span>校验失败</span>
      </div>
      {#each configEditor.validationErrors as err, i (i)}
        <div class="error-item">
          {#if err.fieldPath}
            <span class="error-field">{err.fieldPath}</span>
          {/if}
          <span class="error-msg">{err.message}</span>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Last error -->
  {#if configEditor.lastError && !hasValidationErrors}
    <div class="error-panel">
      <div class="error-title">
        <AlertTriangle class="h-3.5 w-3.5" />
        <span>{configEditor.lastError}</span>
      </div>
    </div>
  {/if}

  <!-- Plan-apply impact preview -->
  {#if hasPlanResult}
    {@const plan = configEditor.planResult!}
    <div class="plan-panel">
      <div class="plan-header">
        <ScanSearch class="h-3.5 w-3.5" />
        <span>配置变更影响分析</span>
      </div>

      <!-- Hot-reload sections -->
      {#if plan.hotReload.length > 0}
        <div class="plan-section">
          <div class="plan-section-title hot">
            <Zap class="h-3 w-3" />
            <span>可热加载（无需重启）</span>
            <span class="plan-count">{plan.hotReload.length}</span>
          </div>
          {#each plan.hotReload as item (item.section)}
            <div class="plan-item hot">
              <span class="plan-item-section">{sectionLabel(item.section)}</span>
              {#if item.tags.length > 0}
                <span class="plan-item-tags">{item.tags.join(', ')}</span>
              {/if}
              {#if item.detail}
                <span class="plan-item-detail">{item.detail}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <!-- Requires-restart sections -->
      {#if plan.requiresRestart.length > 0}
        <div class="plan-section">
          <div class="plan-section-title restart">
            <Power class="h-3 w-3" />
            <span>需要重启内核</span>
            <span class="plan-count restart">{plan.requiresRestart.length}</span>
          </div>
          {#each plan.requiresRestart as item (item.section)}
            <div class="plan-item restart">
              <span class="plan-item-section">{sectionLabel(item.section)}</span>
              {#if item.tags.length > 0}
                <span class="plan-item-tags">{item.tags.join(', ')}</span>
              {/if}
              {#if item.detail}
                <span class="plan-item-detail">{item.detail}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <!-- Warnings -->
      {#if plan.warnings.length > 0}
        <div class="plan-warnings">
          {#each plan.warnings as w (w)}
            <div class="plan-warning-item">
              <AlertTriangle class="h-3 w-3" />
              <span>{w}</span>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Empty impact (no changes detected) -->
      {#if plan.hotReload.length === 0 && plan.requiresRestart.length === 0}
        <div class="plan-empty">未检测到配置变更</div>
      {/if}
    </div>
  {/if}

  <!-- Editor -->
  {#if configEditor.phase !== 'idle'}
    <div class="editor-container">
      <div class="line-numbers">
        {#each Array(jsonLineCount()) as _, i (i)}
          <div class="line-number">{i + 1}</div>
        {/each}
      </div>
      <textarea
        bind:this={textareaRef}
        class="editor-textarea"
        spellcheck="false"
        value={configEditor.draftJson}
        oninput={handleInput}
        onkeydown={handleKeyDown}
        disabled={isLoading}
        placeholder={'{}'}
      ></textarea>
    </div>
  {:else}
    <div class="empty-editor">
      <span class="empty-text">点击刷新按钮加载当前活动配置</span>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .heading {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
  }

  .desc {
    font-size: 11px;
    color: var(--muted-foreground);
    line-height: 1.5;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
  }

  .status-left {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }

  .actions-bar {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
    background: var(--muted);
  }

  .status-badge.ready {
    color: var(--primary);
  }

  .status-badge.editing {
    color: var(--warning, #b45309);
    background: color-mix(in srgb, var(--warning, #f59e0b) 10%, transparent);
  }

  .status-badge.loading {
    color: var(--primary);
    background: color-mix(in srgb, var(--primary) 8%, transparent);
  }

  .status-badge.success {
    color: var(--success, #16a34a);
    background: color-mix(in srgb, var(--success, #22c55e) 10%, transparent);
  }

  .status-badge.error {
    color: var(--destructive);
    background: color-mix(in srgb, var(--destructive) 10%, transparent);
  }

  .status-badge.planned {
    color: #7c3aed;
    background: color-mix(in srgb, #7c3aed 10%, transparent);
  }

  .dirty-badge {
    font-size: 10px;
    color: var(--warning, #b45309);
  }

  .applied-time {
    font-size: 9.5px;
    color: var(--muted-foreground);
  }

  .error-panel {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 9px 11px;
    border: 1px solid color-mix(in srgb, var(--destructive) 25%, transparent);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--destructive) 5%, transparent);
  }

  .error-title {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--destructive);
  }

  .error-item {
    display: flex;
    align-items: baseline;
    gap: 7px;
    padding-left: 19px;
    font-size: 10.5px;
  }

  .error-field {
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
    color: var(--destructive);
    opacity: 0.8;
    white-space: nowrap;
  }

  .error-msg {
    color: var(--foreground);
    opacity: 0.8;
  }

  .plan-panel {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
  }

  .plan-header {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--foreground);
    padding-bottom: 2px;
  }

  .plan-section {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .plan-section-title {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 0;
  }

  .plan-section-title.hot {
    color: var(--success, #16a34a);
  }

  .plan-section-title.restart {
    color: var(--warning, #b45309);
  }

  .plan-count {
    font-size: 9px;
    padding: 0 4px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--success, #22c55e) 12%, transparent);
  }

  .plan-count.restart {
    background: color-mix(in srgb, var(--warning, #f59e0b) 12%, transparent);
  }

  .plan-item {
    display: flex;
    align-items: baseline;
    gap: 7px;
    padding: 4px 7px;
    border-radius: var(--radius-sm);
    font-size: 10px;
  }

  .plan-item.hot {
    background: color-mix(in srgb, var(--success, #22c55e) 4%, transparent);
  }

  .plan-item.restart {
    background: color-mix(in srgb, var(--warning, #f59e0b) 4%, transparent);
  }

  .plan-item-section {
    font-weight: 600;
    white-space: nowrap;
    color: var(--foreground);
  }

  .plan-item-tags {
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
    color: var(--primary);
    font-size: 9.5px;
  }

  .plan-item-detail {
    color: var(--muted-foreground);
    margin-left: auto;
    text-align: right;
  }

  .plan-warnings {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-top: 3px;
    border-top: 1px solid var(--border);
  }

  .plan-warning-item {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    color: var(--warning, #b45309);
  }

  .plan-empty {
    font-size: 10px;
    color: var(--muted-foreground);
    padding: 4px 0;
  }

  .editor-container {
    display: flex;
    min-height: 360px;
    max-height: calc(100vh - 320px);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--card);
  }

  .line-numbers {
    flex-shrink: 0;
    width: 38px;
    padding: 10px 7px;
    text-align: right;
    user-select: none;
    overflow: hidden;
    background: var(--surface);
    border-right: 1px solid var(--border);
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
    font-size: 10.5px;
    line-height: 1.55;
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .line-number {
    height: calc(10.5px * 1.55);
  }

  .editor-textarea {
    flex: 1;
    min-width: 0;
    resize: none;
    border: none;
    outline: none;
    padding: 10px 12px;
    margin: 0;
    background: transparent;
    color: var(--foreground);
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
    font-size: 10.5px;
    line-height: 1.55;
    tab-size: 2;
    white-space: pre;
    overflow: auto;
  }

  .editor-textarea:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .empty-editor {
    height: 200px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
  }

  .empty-text {
    font-size: 11px;
    color: var(--muted-foreground);
  }
</style>