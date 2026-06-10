<script lang="ts">
  import { guiState } from '$lib/services/gui-state.svelte';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { store } from '$lib/services/store.svelte';

  let _lastTick = -1;

  // Event-driven refresh
  $effect(() => {
    const tick = coreEvents.statusTick;
    if (tick > 0 && tick !== _lastTick) {
      _lastTick = tick;
      guiState.refreshOnTick(tick);
    }
  });

  const c = $derived(guiState.connection);

  // Derived process state
  const isCoreAvailable = $derived(c?.coreAvailable === true || c?.processState === 'running');
  const isProcessRunning = $derived(c?.processState === 'running');
  const isProcessStarting = $derived(c?.processState === 'starting');
  const isProcessFailed = $derived(c?.processState === 'failed');
  const isCrashed = $derived(c?.processExitReason === 'crashed');
  const isStopped = $derived(c?.processExitReason === 'stopped');
  const isSystemProxyEnabled = $derived(c?.systemProxyEnabled === true);
  const localProxyEndpoint = $derived(
    c?.localProxyHost && c?.localProxyPort ? `${c.localProxyHost}:${c.localProxyPort}` : '已设置'
  );

  // Label
  const stateLabel = $derived(
    guiState.isConnecting    ? '启用中…' :
    guiState.isDisconnecting ? '关闭中…' :
    guiState.isStartingCore  ? '启动中'   :
    guiState.isStoppingCore  ? '停止中'   :
    isSystemProxyEnabled     ? '服务中'   :
    isCoreAvailable          ? '监听中'   :
    isProcessStarting        ? '启动中'   :
    isProcessFailed          ? '启动失败' :
    isCrashed                ? '异常退出' :
    '已断开'
  );

  const dotColor = $derived(
    isSystemProxyEnabled      ? '#22C55E' :
    isCoreAvailable           ? '#F59E0B' :
    (guiState.isConnecting || guiState.isStartingCore || isProcessStarting) ? '#F59E0B' :
    (isProcessFailed || isCrashed) ? '#EF4444' :
    'var(--muted-foreground)'
  );

  const coreActionLabel = $derived(
    guiState.isStartingCore ? '启动中…' :
    guiState.isStoppingCore ? '重启中…' :
    isProcessRunning ? '重启内核' :
    isCoreAvailable ? '外部内核' :
    '启动内核'
  );
  const proxyActionLabel = $derived(
    guiState.isSwitchingSystemProxy ? '切换中…' :
    isSystemProxyEnabled ? '关闭系统代理' : '开启系统代理'
  );
</script>

<div class="core-card">
  <!-- Header -->
  <div class="core-header">
    <span class="core-label">内核状态</span>
    <div class="core-state">
      <span class="core-dot" class:pulse={guiState.isConnecting || guiState.isStartingCore || guiState.isStoppingCore || isProcessStarting} style="background: {dotColor};"></span>
      <span class="core-state-text">{stateLabel}</span>
    </div>
  </div>

  <!-- Process info when running -->
  {#if isCoreAvailable && c}
    <div class="core-meta">
      <div class="core-meta-row">
        <span class="meta-key">PID</span>
        <span class="meta-val">{isProcessRunning ? (c.processPid ?? '—') : '外部'}</span>
      </div>
      <div class="core-meta-row">
        <span class="meta-key">系统代理</span>
        <span class="meta-val" class:connected={isSystemProxyEnabled}>
          {c.systemProxyEnabled ? localProxyEndpoint : '未设置'}
        </span>
      </div>
    </div>
  {:else if c?.processExitReason && c?.processState === 'exited'}
    <div class="core-meta">
      <div class="core-meta-row">
        <span class="meta-key">退码</span>
        <span class="meta-val">{c.processExitCode ?? '—'}</span>
      </div>
      <div class="core-meta-row">
        <span class="meta-key">原因</span>
        <span class="meta-val" class:danger={isCrashed}>
          {isStopped ? '手动停止' : isCrashed ? '崩溃' : '自行退出'}
        </span>
      </div>
      {#if isCrashed && c.message}
        <div class="core-error">{c.message}</div>
      {/if}
    </div>
  {:else if isProcessFailed && c?.message}
    <div class="core-error">{c.message}</div>
  {/if}

  <!-- Self-test warnings (when not connected) -->
  {#if !guiState.isConnected && !isCoreAvailable && !isProcessStarting && guiState.blockingIssues.length > 0}
    <div class="core-warning" title={guiState.blockingIssues.join('; ')}>
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M5 1.2L9 8.8H1Z"/>
        <line x1="5" y1="4" x2="5" y2="6"/>
        <circle cx="5" cy="7.5" r="0.4" fill="currentColor"/>
      </svg>
      <span class="truncate">{guiState.blockingIssues[0]}</span>
    </div>
    <button class="core-link" onclick={() => store.openSettings('core')}>
      配置内核
    </button>
  {/if}

  {#if store.uiMode === 'pro'}
    <div class="core-actions">
      <button
        onclick={() => isProcessRunning ? guiState.restartCore() : guiState.startCore()}
        disabled={isCoreAvailable ? !isProcessRunning || !guiState.canStopCore : !guiState.canStartCore}
        class="core-action"
        class:active={isCoreAvailable}
        class:danger={isProcessRunning}
        title={isCoreAvailable && !isProcessRunning ? '检测到外部内核，无法由本应用管理' : !isCoreAvailable && !guiState.canStartCore && guiState.blockingIssues.length ? guiState.blockingIssues.join('; ') : ''}
      >
        {coreActionLabel}
      </button>
      <button
        onclick={() => guiState.toggleSystemProxy()}
        disabled={isSystemProxyEnabled ? !guiState.canDisableSystemProxy : !guiState.canEnableSystemProxy}
        class="core-action"
        class:active={isSystemProxyEnabled}
        title={!isSystemProxyEnabled && !guiState.canEnableSystemProxy && guiState.blockingIssues.length ? guiState.blockingIssues.join('; ') : ''}
      >
        {proxyActionLabel}
      </button>
    </div>
  {:else}
    <!-- Lite mode one-click flow: start core, wait for local inbound, then set GUI-managed system proxy. -->
    <button
      onclick={() => guiState.isConnected ? guiState.disconnect() : guiState.connect()}
      disabled={guiState.isConnecting || guiState.isDisconnecting || (!guiState.isConnected && !guiState.canConnect)}
      class="core-toggle"
      class:running={guiState.isConnected}
      class:startable={guiState.canConnect && !guiState.isConnected}
      title={!guiState.canConnect && guiState.blockingIssues.length ? guiState.blockingIssues.join('; ') : ''}
    >
      {guiState.isConnecting ? '启用中…' : guiState.isDisconnecting ? '关闭中…' : guiState.isConnected ? '关闭服务' : guiState.canConnect ? '开启服务' : '配置不完整'}
    </button>
  {/if}
</div>

<style>
  .core-card {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-height: 96px;
    padding: 11px 13px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    overflow: hidden;
    transition: box-shadow 0.15s ease, transform 0.15s ease;
  }

  .core-card:hover {
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.07);
    transform: translateY(-0.5px);
  }

  :global(.dark) .core-card { box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22); }
  :global(.dark) .core-card:hover { box-shadow: 0 2px 8px rgba(0, 0, 0, 0.32); }

  .core-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }

  .core-label { font-size: 12px; font-weight: 500; color: var(--muted-foreground); }

  .core-state { display: flex; align-items: center; gap: 5px; }

  .core-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
  }
  .core-dot.pulse { animation: pulse-dot 1.4s ease-in-out infinite; }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.3; }
  }

  .core-state-text { font-size: 12.5px; font-weight: 600; color: var(--foreground); }

  .core-meta {
    display: grid; grid-template-columns: 1fr 1fr;
    gap: 1px 8px; flex-shrink: 0;
  }

  .core-meta-row {
    display: flex; align-items: center; justify-content: space-between;
    gap: 4px; overflow: hidden;
  }

  .meta-key { font-size: 11px; color: var(--muted-foreground); flex-shrink: 0; }

  .meta-val {
    font-size: 11.5px; font-family: var(--font-mono, monospace); font-weight: 600;
    color: var(--foreground); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .meta-val.danger { color: var(--destructive); }
  .meta-val.connected { color: #16A34A; }
  :global(.dark) .meta-val.connected { color: #4ADE80; }

  .core-warning {
    display: flex; align-items: center; gap: 4px;
    font-size: 11px; color: var(--warning); overflow: hidden; flex-shrink: 0;
  }

  .core-link {
    align-self: flex-start; border: none; background: transparent;
    color: var(--primary); font-size: 11.5px; font-weight: 600;
    padding: 0; cursor: pointer;
  }

  .core-error {
    font-size: 11px; color: var(--destructive); overflow: hidden;
    text-overflow: ellipsis; white-space: nowrap; flex-shrink: 0;
  }

  .core-actions {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.35fr);
    gap: 6px;
    margin-top: auto;
    flex-shrink: 0;
  }

  .core-action {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 26px;
    min-width: 0;
    padding: 0 7px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.13s ease, border-color 0.13s ease, color 0.13s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .core-action:hover:not(:disabled) {
    color: var(--foreground);
    background: var(--accent, var(--muted));
  }

  .core-action:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .core-action.active {
    background: rgba(34, 197, 94, 0.08);
    border-color: rgba(34, 197, 94, 0.25);
    color: #16A34A;
  }

  .core-action.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.25);
    color: var(--destructive);
  }

  :global(.dark) .core-action.active { color: #4ADE80; }
  :global(.dark) .core-action.danger:hover:not(:disabled) { color: #EF4444; }

  .core-toggle {
    display: flex; align-items: center; justify-content: center;
    width: 100%; height: 26px; padding: 0 8px; border-radius: 7px;
    border: 1px solid var(--border); background: var(--muted);
    color: var(--muted-foreground); font-size: 11.5px; font-weight: 600;
    cursor: pointer; transition: all 0.13s ease; white-space: nowrap;
    overflow: hidden; text-overflow: ellipsis; margin-top: auto; flex-shrink: 0;
  }

  .core-toggle:disabled { opacity: 0.4; cursor: not-allowed; }

  .core-toggle.running {
    background: rgba(34, 197, 94, 0.10);
    border-color: rgba(34, 197, 94, 0.30);
    color: #16A34A;
  }
  .core-toggle.running:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.25);
    color: var(--destructive);
  }

  .core-toggle.startable {
    background: rgba(34, 197, 94, 0.08);
    border-color: rgba(34, 197, 94, 0.25);
    color: #16A34A;
  }
  .core-toggle.startable:hover:not(:disabled) { background: rgba(34, 197, 94, 0.14); }

  :global(.dark) .core-toggle.running,
  :global(.dark) .core-toggle.startable { color: #4ADE80; }
  :global(.dark) .core-toggle.running:hover:not(:disabled) { color: #EF4444; }
</style>
