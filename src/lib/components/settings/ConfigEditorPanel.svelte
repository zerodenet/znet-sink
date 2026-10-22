<script lang="ts">
  import StorageInspectionPanel from '$lib/components/settings/StorageInspectionPanel.svelte';
  import { Textarea } from '$lib/components/ui/textarea';
  import { configEditor, type ValidationError, type ConfigImpactItem } from '$lib/services/config-workspace.svelte';
  import { Button } from '$lib/components/ui/button';
  import { AlertTriangle, Check, Loader2, RefreshCcw, RotateCcw, Send, ScanSearch, Zap, Power } from '@lucide/svelte';
  import MobileSettingsDraftGuard from '$lib/components/mobile/MobileSettingsDraftGuard.svelte';
  import KernelSettingsTransfer from '$lib/components/settings/KernelSettingsTransfer.svelte';
  import * as Dialog from '$lib/components/ui/dialog';

  // Textarea.ref has a null fallback. Svelte rejects binding an undefined
  // parent value to a bindable prop with a non-undefined fallback.
  let textareaRef: HTMLTextAreaElement | null = $state(null);
  let tabSize = 2;
  let workspaceView: 'source' | 'overrides' | 'effective' = $state('source');
  let confirmRefresh = $state(false);
  let recoveryConfirm = $state(false);
  let reviewedToken = $state<string | null>(null);
  let reviewedPlanId = $state<string | null>(null);
  function recoveryDeadline(value: string): string {
    if (!/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 8640000000000000n) return '检查有效期未提供，请重新检查';
    const timestamp = Number(value);
    return timestamp <= Date.now() ? '恢复检查已过期，请重新检查' : `本次检查有效至 ${new Date(timestamp).toLocaleString('zh-CN') }；提交时再次校验，过期须重新检查`;
  }

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
    if (configEditor.dirty) { confirmRefresh = true; return; }
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

  async function formatJson() { await configEditor.format(); }

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
    configEditor.busy
  );
  const canValidate = $derived(configEditor.writable);
  const canPlanApply = $derived(configEditor.writable && configEditor.supportsPlanApply);
  const canApply = $derived(configEditor.writable && !configEditor.planResult?.applyUnavailableReason);
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

<MobileSettingsDraftGuard dirty={configEditor.dirty} busy={configEditor.busy} label="配置工作区" onDiscard={() => configEditor.reset()} />

<div class="panel">
  <!-- Header -->
  <div class="header">
    <div class="heading">
      <div class="title">内核配置编辑</div>
      <div class="desc">
        对照来源、客户端覆盖与最终配置；可独立保存本地，运行应用需真实预检和回执。
      </div>
    </div>
    <div class="actions">
      <Button variant="ghost" size="icon-sm" onclick={handleLoad} disabled={isLoading} aria-label="刷新配置来源">
        <RefreshCcw class="h-3.5 w-3.5" />
      </Button>
    </div>
  </div>

  <StorageInspectionPanel kind="workspace" scopeKey={JSON.stringify([configEditor.profileName, configEditor.readOnlyReason, configEditor.conflict])} />

  {#if confirmRefresh}
    <div role="alertdialog" aria-label="刷新来源确认" class="error-panel">
      <p>刷新会放弃当前未保存草稿；外部来源不会自动覆盖输入。</p>
      <Button variant="destructive" onclick={async () => { confirmRefresh = false; await configEditor.load(true); }}>放弃草稿并刷新</Button>
      <Button variant="outline" onclick={() => confirmRefresh = false}>继续编辑</Button>
    </div>
  {/if}
  {#if configEditor.readOnlyReason}<div role="status" class="error-panel">{configEditor.readOnlyReason}；来源、覆盖及最终正文均未提供。</div>{/if}
  {#if configEditor.conflict}<div role="alert" class="error-panel">来源已变化，原草稿保留。旧预检已失效，请明确刷新后重新预检。</div>{/if}
  {#if configEditor.pendingApply}<p role="status">{configEditor.phase === 'saved' ? '已保存，待应用' : '本地材料已保存，运行待应用或核对'}；保存不等于内核或 VPN 生效。</p>{/if}
  {#if configEditor.operationId}
    <section aria-label="原工作区操作恢复" class="error-panel">
      <p role="status">原事务结果未确认；只核对原操作，不会重新提交。</p>
      <p class="operation-id">原操作：{configEditor.operationId}</p>
      <p>{configEditor.operation?.state === 'runtime_confirmed_pending_local' ? '原运行操作已有确认回执，本地发布尚未完成；不代表当前运行态仍有效。' : '原操作的运行结果未确认；恢复元资料不代表运行成功，不会自动重放。'}</p>
      {#if configEditor.operation?.reason}<p>{configEditor.operation.reason}</p>{/if}
      <Button variant="outline" disabled={isLoading} onclick={() => configEditor.checkOperation()}>核对原事务</Button>
      <Button variant="outline" disabled={isLoading} onclick={() => configEditor.reviewRecovery()}>检查原操作恢复</Button>
      {#if configEditor.recoveryReview}
        <p role="status">{configEditor.recoveryReview.reason}</p>
        {#if configEditor.recoveryReview.expiresAtUnixMs}<p>{recoveryDeadline(configEditor.recoveryReview.expiresAtUnixMs)}</p>{/if}
        {#if configEditor.recoveryReview.canCompleteLocal}
          <p>仅完成此原操作的本地发布，不发送新的配置应用或 TUN 命令。</p>
          {#if configEditor.dirty}<p>未保存草稿仍保留；先明确放弃草稿，再重新检查原操作。</p>{/if}
          <Button disabled={!configEditor.canCompleteLocal} onclick={() => { reviewedToken = configEditor.recoveryReview?.reviewId ?? null; reviewedPlanId = configEditor.recoveryReview?.planId ?? null; recoveryConfirm = true; }}>完成原本地发布</Button>
        {/if}
      {/if}
    </section>
  {:else if configEditor.operation}
    <p role="status" class="operation-id">原操作历史：{configEditor.operation.planId} · {configEditor.operation.state === 'confirmed' ? '原事务已完成' : '原事务已拒绝'}。历史回执不代表当前运行态。</p>
  {/if}
  {#if configEditor.runtime}
    <div class="runtime-card" class:confirmed={configEditor.runtime.confirmed}>
      <div class="runtime-main">
        <span class="runtime-state">{configEditor.runtime.confirmed ? '运行态已确认' : '运行态待确认'}</span>
        <span>{configEditor.runtime.reason}</span>
      </div>
      {#if configEditor.runtime.identity}
        <code>{configEditor.runtime.identity.coreInstanceId.slice(0, 8)} · rev {configEditor.runtime.identity.configRevision}</code>
      {/if}
    </div>
  {/if}

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
      {:else if configEditor.phase === 'saved'}<span class="status-badge">已保存，待应用</span>
      {:else if configEditor.phase === 'saving'}<span class="status-badge">本地保存中</span>
      {:else if configEditor.phase === 'checking'}<span class="status-badge">原操作检查中</span>
      {:else if configEditor.phase === 'completing'}<span class="status-badge">原本地发布提交中</span>
      {:else if configEditor.phase === 'unknown'}<span class="status-badge error">结果未确认</span>
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
      <Button variant="outline" size="sm" disabled={!configEditor.writable || isLoading} onclick={() => configEditor.saveLocal()}>保存本地</Button>
      <Button
        variant="outline"
        size="sm"
        onclick={formatJson}
        disabled={!configEditor.draftJson || isLoading || !!configEditor.readOnlyReason}
        title="格式化 JSON"
      >
        格式化
      </Button>
      <Button
        variant="outline"
        size="sm"
        onclick={handleReset}
        disabled={!canReset}
        title="放弃修改，恢复已读取的本地来源文本；不改变运行态"
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
        disabled={!canApply || isLoading || needsConfirmApply}
        title={needsConfirmApply
          ? '当前客户端安全启动及固定材料通道未适配；保留影响，可保存本地待应用'
          : '校验、预检并热加载到运行中的内核'}
      >
        {#if configEditor.phase === 'applying'}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else if needsConfirmApply}
          <AlertTriangle class="h-3.5 w-3.5" />
        {:else}
          <Send class="h-3.5 w-3.5" />
        {/if}
        {needsConfirmApply ? '重启并应用' : '应用'}
      </Button>
    </div>
  </div>

  {#if configEditor.planResult?.applyUnavailableReason}<p role="status" class="error-panel">{configEditor.planResult.applyUnavailableReason}</p>{/if}

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

  {#if configEditor.compositionLayers.length > 0}
    <div class="composition-strip">
      <span class="composition-title">客户端组合层</span>
      {#each configEditor.compositionLayers as layer (layer.source)}
        <span class:changed={layer.changedPaths.length > 0} class="layer-chip">
          {layer.source}{layer.changedPaths.length > 0 ? ` · ${layer.changedPaths.length}` : ''}
        </span>
      {/each}
    </div>
  {/if}

  <div class="workspace-tabs" role="tablist" aria-label="配置层级">
    <button data-slot="surface-button" class:active={workspaceView === 'source'} onclick={() => workspaceView = 'source'}>来源编辑</button>
    <button data-slot="surface-button" class:active={workspaceView === 'overrides'} onclick={() => workspaceView = 'overrides'}>客户端覆盖</button>
    <button data-slot="surface-button" class:active={workspaceView === 'effective'} onclick={() => workspaceView = 'effective'}>最终生效</button>
  </div>

  <!-- Workspace views -->
  {#if configEditor.phase !== 'idle'}
    {#if configEditor.readOnlyReason}<p>来源正文只读且未提供，请使用来源管理入口。</p>
      {#if configEditor.dirty}<p>此前普通来源的未保存草稿仍保留，当前不可提交：</p><Textarea aria-label="此前普通来源草稿" readonly value={configEditor.draftJson} />{/if}
    {:else if workspaceView === 'source'}
      <div class="editor-container">
        <div class="line-numbers">
          {#each Array(jsonLineCount()) as _, i (i)}
            <div class="line-number">{i + 1}</div>
          {/each}
        </div>
        <Textarea
          bind:ref={textareaRef}
          class="font-mono flex-1 resize-none whitespace-pre border-0 rounded-none bg-transparent py-2.5 px-3 text-[10.5px] leading-[1.55] [tab-size:2]"
          spellcheck="false"
          aria-label="来源配置 JSON"
          value={configEditor.draftJson}
          oninput={handleInput}
          onkeydown={handleKeyDown}
          disabled={isLoading}
          placeholder={'{}'}
        ></Textarea>
      </div>
    {:else if workspaceView === 'overrides'}
      <div class="read-view">
        <div class="view-note">网络设置全局生效，端点偏好属于当前配置；不会写回订阅来源。</div>
        <pre>{configEditor.localEditsJson}</pre>
      </div>
    {:else}
      <div class="read-view effective-view">
        <div class="view-note">{configEditor.runtime?.confirmed ? '此合成版本已通过运行身份与事务回执确认。' : '这是本地合成候选，尚未确认运行生效；不是已提交成功的运行配置。'}</div>
        <pre>{configEditor.effectiveJson}</pre>
      </div>
    {/if}
  {:else}
    <div class="empty-editor">
      <span class="empty-text">点击刷新按钮加载当前活动配置</span>
    </div>
  {/if}

  <KernelSettingsTransfer />
</div>

<Dialog.Root bind:open={recoveryConfirm}>
  <Dialog.Content class="workspace-recovery-confirm">
    <Dialog.Header><Dialog.Title>完成原操作的本地发布？</Dialog.Title><Dialog.Description>只使用已检查的原操作及其冻结候选；不会重新预检、重新应用运行配置或重建 TUN。取消保留草稿与原屏障。</Dialog.Description></Dialog.Header>
    <Dialog.Body><p class="operation-id">{reviewedPlanId}</p><p>来源、覆盖、规则或运行身份变化后，原检查失效，需重新核对。</p></Dialog.Body>
    <Dialog.Footer><Button variant="outline" onclick={() => { recoveryConfirm = false; reviewedToken = null; reviewedPlanId = null; }}>取消</Button><Button disabled={!configEditor.canCompleteLocal || !reviewedToken || reviewedToken !== configEditor.recoveryReview?.reviewId || reviewedPlanId !== configEditor.recoveryReview?.planId} onclick={() => { const token = reviewedToken; recoveryConfirm = false; reviewedToken = null; reviewedPlanId = null; if (token) void configEditor.completeLocal(token); }}>确认完成本地发布</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .panel {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .operation-id { overflow-wrap: anywhere; word-break: break-word; }

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

  .runtime-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--warning, #f59e0b) 28%, var(--border));
    border-radius: var(--radius-md);
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .runtime-card.confirmed {
    border-color: color-mix(in srgb, var(--success, #22c55e) 32%, var(--border));
  }

  .runtime-main {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .runtime-state {
    color: var(--foreground);
    font-weight: 650;
    white-space: nowrap;
  }

  .runtime-card code {
    white-space: nowrap;
    font-size: 9.5px;
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

  .composition-strip {
    display: flex;
    align-items: center;
    gap: 5px;
    overflow-x: auto;
    padding: 2px 0;
  }

  .composition-title,
  .layer-chip {
    white-space: nowrap;
    font-size: 9.5px;
    color: var(--muted-foreground);
  }

  .layer-chip {
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  .layer-chip.changed {
    color: var(--primary);
    border-color: color-mix(in srgb, var(--primary) 30%, var(--border));
  }

  .workspace-tabs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    padding: 3px;
    border-radius: var(--radius-lg);
    background: var(--muted);
  }

  .workspace-tabs button {
    min-height: 44px;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--muted-foreground);
    font-size: 10.5px;
    cursor: pointer;
  }

  .workspace-tabs button.active {
    background: var(--card);
    color: var(--foreground);
    font-weight: 650;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }

  .read-view {
    min-height: 360px;
    max-height: calc(100vh - 320px);
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--card);
  }

  .view-note {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .read-view pre {
    margin: 0;
    padding: 10px 12px;
    color: var(--foreground);
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
    font-size: 10.5px;
    line-height: 1.55;
    white-space: pre;
  }

  .editor-container {
    min-width: 0;
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
  @media (max-width: 640px) {
    .header, .status-bar, .runtime-card { flex-wrap: wrap; }
    .runtime-main { flex-wrap: wrap; }
    .runtime-card code { white-space: normal; overflow-wrap: anywhere; }
    .status-left { flex-wrap: wrap; }
    .actions-bar { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); width: 100%; }
    .actions-bar :global(button), .header :global(button) { min-height: 44px; min-width: 44px; }
    .error-panel :global(button), :global(.workspace-recovery-confirm button) { min-height: 44px; min-width: 44px; }
    .editor-container :global(textarea) { font-size: 16px; min-width: 0; }
    .workspace-tabs button { font-size: 12px; }
    .runtime-card, .error-panel, .panel { overflow-wrap: anywhere; }
    .read-view pre { white-space: pre-wrap; overflow-wrap: anywhere; }
  }
</style>
