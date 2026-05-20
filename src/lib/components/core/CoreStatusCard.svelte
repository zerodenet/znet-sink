<script lang="ts">
  import { getCoreProcessStatus, startCoreProcess, stopCoreProcess, getCoreConfigSnapshot, disableSystemProxy, getSystemProxyStatus } from '$lib/services/core';
  import type { CoreProcessStatus, CoreConfigSnapshot } from '$lib/types/core';
  import { error as toastError, success, info, warning } from '$lib/services/toast.svelte';

  let status = $state<CoreProcessStatus | null>(null);
  let snapshot = $state<CoreConfigSnapshot | null>(null);
  let loading = $state(false);
  let prevIsRunning = $state(false);
  let retryCount = $state(0);
  let retryTimer = $state<number | null>(null);

  const MAX_AUTO_RETRY = 3;
  const RETRY_DELAY_MS = 2000;

  const isRunning = $derived(status?.state === 'running');
  const isStarting = $derived(status?.state === 'starting');
  const isStopped = $derived(status?.exitReason === 'stopped');
  const isCrashed = $derived(status?.exitReason === 'crashed');
  const hasFailed = $derived(status?.state === 'failed');
  const isRetrying = $derived(retryTimer !== null);

  const canStart = $derived(!isRunning && !isStarting && !snapshot?.warnings.length && !isRetrying);

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
    if (retryTimer) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
    retryCount = 0;
    info('已取消自动重试');
  }

  async function toggleCore() {
    if (loading) return;
    
    if (isRetrying) {
      cancelRetry();
      return;
    }

    loading = true;
    try {
      if (isRunning) {
        await stopCoreProcess();
        success('内核已停止');
      } else {
        if (snapshot?.warnings.length) {
          const proceed = confirm(
            `内核配置存在以下警告:\n\n${snapshot.warnings.map(w => '• ' + w).join('\n')}\n\n是否仍然启动？`
          );
          if (!proceed) { loading = false; return; }
        }
        await startCoreProcess();
        success('内核已启动');
      }
      await refreshStatus();
    } catch (e: any) {
      const msg = e.message ?? e ?? '未知错误';
      toastError(`操作失败: ${msg}`);
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
        if (wasCrashed) {
          warning('内核崩溃，已自动关闭系统代理');
        } else {
          info('内核已停止，系统代理已关闭');
        }
      }
    } catch (e) {
      console.warn('Failed to disable system proxy:', e);
    }
  }

  async function tryRestartCore() {
    if (retryCount >= MAX_AUTO_RETRY) {
      toastError(`内核连续崩溃 ${MAX_AUTO_RETRY} 次，已停止自动重试`);
      retryCount = 0;
      return;
    }

    retryCount++;
    info(`内核崩溃，正在自动重试 (${retryCount}/${MAX_AUTO_RETRY})...`);
    
    try {
      await startCoreProcess();
      success('内核自动重启成功');
      retryCount = 0;
      retryTimer = null;
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
      if (retryTimer) {
        clearTimeout(retryTimer);
        retryTimer = null;
      }
    }
    prevIsRunning = isRunning;
  });

  $effect(() => {
    return () => {
      if (retryTimer) {
        clearTimeout(retryTimer);
      }
    };
  });
</script>

<div class="bg-card rounded-xl p-3 flex flex-col gap-2 h-24 overflow-hidden shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
  <div class="flex items-center justify-between flex-shrink-0">
    <span class="text-sm font-medium text-muted-foreground truncate">内核状态</span>
    <div class="flex items-center gap-1.5 flex-shrink-0">
      <div class="w-2.5 h-2.5 rounded-full
        {isRunning ? 'bg-green-500' : isStarting ? 'bg-yellow-500 animate-pulse' : hasFailed || isCrashed ? 'bg-red-500' : 'bg-muted'}">
      </div>
      <span class="text-sm font-bold text-foreground">
        {isRunning ? '运行中' : isStarting ? '启动中' : isRetrying ? `重试中 ${retryCount}/${MAX_AUTO_RETRY}` : hasFailed ? '启动失败' : isCrashed ? '异常退出' : '已停止'}
      </span>
    </div>
  </div>

  {#if isRunning && status}
    <div class="grid grid-cols-2 gap-1 text-xs flex-shrink-0">
      <div class="flex justify-between overflow-hidden">
        <span class="text-muted-foreground truncate">PID</span>
        <span class="font-mono text-foreground truncate ml-1">{status.pid ?? '-'}</span>
      </div>
      <div class="flex justify-between overflow-hidden">
        <span class="text-muted-foreground truncate">内核</span>
        <span class="font-mono text-foreground truncate ml-1">{status.kernel}</span>
      </div>
    </div>
  {:else if status?.exitReason && status.state === 'exited'}
    <div class="grid grid-cols-2 gap-1 text-xs flex-shrink-0">
      <div class="flex justify-between overflow-hidden">
        <span class="text-muted-foreground truncate">退码</span>
        <span class="font-mono text-foreground truncate ml-1">{status.exitCode ?? '-'}</span>
      </div>
      <div class="flex justify-between overflow-hidden">
        <span class="text-muted-foreground truncate">原因</span>
        <span class="font-mono truncate ml-1 {isCrashed ? 'text-red-500' : 'text-muted-foreground'}">
          {isStopped ? '手动停止' : isCrashed ? '崩溃' : '自行退出'}
        </span>
      </div>
      {#if isCrashed && status.lastError}
        <div class="col-span-2 flex justify-between overflow-hidden">
          <span class="text-muted-foreground truncate">错误</span>
          <span class="text-red-500 truncate ml-1 text-[10px]">{status.lastError}</span>
        </div>
      {/if}
    </div>
  {:else if hasFailed && status?.lastError}
    <div class="flex justify-between overflow-hidden text-xs flex-shrink-0">
      <span class="text-muted-foreground truncate">错误</span>
      <span class="text-red-500 truncate ml-1">{status.lastError}</span>
    </div>
  {/if}

  {#if snapshot?.warnings.length && !isRunning && !isStarting}
    <div class="flex items-center gap-1 text-[10px] text-red-400 flex-shrink-0" title={snapshot.warnings.join('; ')}>
      <span class="truncate">⚠ {snapshot.warnings[0]}</span>
    </div>
  {/if}

  <button
    onclick={toggleCore}
    disabled={loading || !canStart}
    class="w-full py-1.5 rounded-lg font-medium text-xs transition-all disabled:opacity-50 disabled:cursor-not-allowed mt-auto flex-shrink-0 truncate
           {isRunning
             ? 'bg-red-500/10 text-red-500 hover:bg-red-500/20 border border-red-500/30'
             : canStart
               ? 'bg-green-500/10 text-green-500 hover:bg-green-500/20 border border-green-500/30'
               : 'bg-muted text-muted-foreground border border-border'}"
    title={!canStart && snapshot?.warnings.length ? snapshot.warnings.join('; ') : ''}
  >
    {loading ? '处理中...' : isRunning ? '停止内核' : isRetrying ? '取消重试' : canStart ? '启动内核' : '配置不完整'}
  </button>
</div>
