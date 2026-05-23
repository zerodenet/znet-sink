<script lang="ts">
  import { getCoreProcessStatus, startCoreProcess, stopCoreProcess, getCoreConfigSnapshot, disableSystemProxy, getSystemProxyStatus } from '$lib/services/core';
  import type { CoreProcessStatus, CoreKernelInfo } from '$lib/types/core';
  import { error as toastError, success, info, warning } from '$lib/services/toast.svelte';
  import { store } from '$lib/services/store.svelte';

  let status = $state<CoreProcessStatus | null>(null);
  let snapshot = $state<CoreKernelInfo | null>(null);
  let loading = $state(false);
  let prevIsRunning = $state(false);
  let retryCount = $state(0);
  let retryTimer = $state<number | null>(null);

  const MAX_AUTO_RETRY = 3;
  const RETRY_DELAY_MS = 2000;

  const isRunning  = $derived(status?.state === 'running');
  const isStarting = $derived(status?.state === 'starting');
  const isStopped  = $derived(status?.exitReason === 'stopped');
  const isCrashed  = $derived(status?.exitReason === 'crashed');
  const hasFailed  = $derived(status?.state === 'failed');
  const isRetrying = $derived(retryTimer !== null);

  const canStart = $derived(!isRunning && !isStarting && !snapshot?.warnings.length && !isRetrying);

  const stateLabel = $derived(
    loading     ? '处理中…'  :
    isRunning   ? '运行中'   :
    isStarting  ? '启动中'   :
    isRetrying  ? `重试 ${retryCount}/${MAX_AUTO_RETRY}` :
    hasFailed   ? '启动失败' :
    isCrashed   ? '异常退出' :
    '已停止'
  );

  const dotColor = $derived(
    isRunning   ? '#22C55E' :
    isStarting  ? '#F59E0B' :
    (hasFailed || isCrashed) ? '#EF4444' :
    'var(--muted-foreground)'
  );

  const dotPulse = $derived(isStarting || isRetrying);

  async function refreshStatus() {
    try {
      status = await getCoreProcessStatus();
    } catch (e) {
      console.error('Failed to get core status:', e);
    }
  }

  async function validateConfig() {
    try {
      snapshot = await getCoreConfigSnapshot();
    } catch {
      snapshot = null;
    }
  }

  function cancelRetry() {
    if (retryTimer) { clearTimeout(retryTimer); retryTimer = null; }
    retryCount = 0;
    info('已取消自动重试');
  }

  async function toggleCore() {
    if (loading) return;
    if (isRetrying) { cancelRetry(); return; }

    loading = true;
    try {
      if (isRunning) {
        await stopCoreProcess();
        success('内核已停止');
      } else {
        if (snapshot?.warnings.length) {
          const proceed = confirm(`内核配置存在以下警告:\n\n${snapshot.warnings.map(w => '• ' + w).join('\n')}\n\n是否仍然启动？`);
          if (!proceed) { loading = false; return; }
        }
        await startCoreProcess();
        success('内核已启动');
      }
      await refreshStatus();
    } catch (e: any) {
      toastError(`操作失败: ${e.message ?? e ?? '未知错误'}`);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    refreshStatus();
    validateConfig();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  });

  async function handleCoreStopped(wasCrashed: boolean) {
    try {
      const proxyStatus = await getSystemProxyStatus();
      if (proxyStatus.enabled) {
        await disableSystemProxy();
        if (wasCrashed) warning('内核崩溃，已自动关闭系统代理');
        else info('内核已停止，系统代理已关闭');
      }
    } catch (e) {
      console.warn('Failed to disable system proxy:', e);
    }
  }

  async function tryRestartCore() {
    if (retryCount >= MAX_AUTO_RETRY) {
      toastError(`内核连续崩溃 ${MAX_AUTO_RETRY} 次，已停止自动重试`);
      retryCount = 0; return;
    }
    retryCount++;
    info(`内核崩溃，自动重试 (${retryCount}/${MAX_AUTO_RETRY})…`);
    try {
      await startCoreProcess();
      success('内核自动重启成功');
      retryCount = 0; retryTimer = null;
      await refreshStatus();
    } catch (e: any) {
      toastError(`重试失败: ${e.message ?? e}`);
      retryTimer = window.setTimeout(tryRestartCore, RETRY_DELAY_MS);
    }
  }

  $effect(() => {
    if (prevIsRunning && !isRunning) {
      if (isCrashed) {
        toastError('内核崩溃，请查看运行日志获取详情');
        handleCoreStopped(true);
        retryTimer = window.setTimeout(tryRestartCore, RETRY_DELAY_MS);
      } else if (isStopped) {
        handleCoreStopped(false);
        retryCount = 0;
      }
    }
    if (isRunning) {
      retryCount = 0;
      if (retryTimer) { clearTimeout(retryTimer); retryTimer = null; }
    }
    prevIsRunning = isRunning;
  });

  $effect(() => {
    return () => { if (retryTimer) clearTimeout(retryTimer); };
  });
</script>

<div class="core-card">
  <!-- Header row -->
  <div class="core-header">
    <span class="core-label">内核状态</span>
    <div class="core-state">
      <span class="core-dot" class:pulse={dotPulse} style="background: {dotColor};"></span>
      <span class="core-state-text">{stateLabel}</span>
    </div>
  </div>

  <!-- Info rows (when running or exited with info) -->
  {#if isRunning && status}
    <div class="core-meta">
      <div class="core-meta-row">
        <span class="meta-key">PID</span>
        <span class="meta-val">{status.pid ?? '—'}</span>
      </div>
      <div class="core-meta-row">
        <span class="meta-key">内核</span>
        <span class="meta-val">{status.kernel}</span>
      </div>
    </div>
  {:else if status?.exitReason && status.state === 'exited'}
    <div class="core-meta">
      <div class="core-meta-row">
        <span class="meta-key">退码</span>
        <span class="meta-val">{status.exitCode ?? '—'}</span>
      </div>
      <div class="core-meta-row">
        <span class="meta-key">原因</span>
        <span class="meta-val" class:danger={isCrashed}>
          {isStopped ? '手动停止' : isCrashed ? '崩溃' : '自行退出'}
        </span>
      </div>
      {#if isCrashed && status.lastError}
        <div class="core-error">{status.lastError}</div>
      {/if}
    </div>
  {:else if hasFailed && status?.lastError}
    <div class="core-error">{status.lastError}</div>
  {/if}

  <!-- Warning -->
  {#if snapshot?.warnings.length && !isRunning && !isStarting}
    <div class="core-warning" title={snapshot.warnings.join('; ')}>
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M5 1.2L9 8.8H1Z"/>
        <line x1="5" y1="4" x2="5" y2="6"/>
        <circle cx="5" cy="7.5" r="0.4" fill="currentColor"/>
      </svg>
      <span class="truncate">{snapshot.warnings[0]}</span>
    </div>
    <button class="core-link" onclick={() => store.openSettings('core')}>
      配置内核
    </button>
  {/if}

  <!-- Toggle button -->
  <button
    onclick={toggleCore}
    disabled={loading || (!isRunning && !canStart)}
    class="core-toggle"
    class:running={isRunning}
    class:startable={canStart && !isRunning}
    title={!canStart && snapshot?.warnings.length ? snapshot.warnings.join('; ') : ''}
  >
    {loading ? '处理中…' : isRunning ? '停止内核' : isRetrying ? '取消重试' : canStart ? '启动内核' : '配置不完整'}
  </button>
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

  /* ---- Header ---- */
  .core-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }

  .core-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
  }

  .core-state {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .core-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .core-dot.pulse {
    animation: pulse-dot 1.4s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.3; }
  }

  .core-state-text {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--foreground);
  }

  /* ---- Meta rows ---- */
  .core-meta {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px 8px;
    flex-shrink: 0;
  }

  .core-meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    overflow: hidden;
  }

  .meta-key {
    font-size: 11px;
    color: var(--muted-foreground);
    flex-shrink: 0;
  }

  .meta-val {
    font-size: 11.5px;
    font-family: var(--font-mono, monospace);
    font-weight: 600;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta-val.danger { color: var(--destructive); }

  /* ---- Warning ---- */
  .core-warning {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--warning);
    overflow: hidden;
    flex-shrink: 0;
  }

  .core-link {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--primary);
    font-size: 11.5px;
    font-weight: 600;
    padding: 0;
    cursor: pointer;
  }

  /* ---- Error ---- */
  .core-error {
    font-size: 11px;
    color: var(--destructive);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* ---- Toggle button ---- */
  .core-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 26px;
    padding: 0 8px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.13s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: auto;
    flex-shrink: 0;
  }

  .core-toggle:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .core-toggle.running {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.25);
    color: var(--destructive);
  }

  .core-toggle.running:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.14);
  }

  .core-toggle.startable {
    background: rgba(34, 197, 94, 0.08);
    border-color: rgba(34, 197, 94, 0.25);
    color: #16A34A;
  }

  .core-toggle.startable:hover:not(:disabled) {
    background: rgba(34, 197, 94, 0.14);
  }

  :global(.dark) .core-toggle.startable { color: #4ADE80; }
</style>
