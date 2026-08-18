<script lang="ts">
  import { onMount } from 'svelte';

  import { getRuntimePerformanceSnapshot } from '$lib/services/runtime-performance';
  import type { RuntimePerformanceSnapshot, RuntimeProcessMetrics } from '$lib/types/runtime-performance';

  let { mode }: { mode: 'pro' | 'lite' } = $props();

  let snapshot = $state<RuntimePerformanceSnapshot | null>(null);
  let refreshPending = false;
  let error = $state<string | null>(null);

  function formatCpu(value: number | null | undefined): string {
    if (value == null) return snapshot ? '采样中' : '—';
    if (value < 0.1) return '<0.1%';
    return `${value.toFixed(value >= 10 ? 1 : 2)}%`;
  }

  function formatMemory(bytes: number | null | undefined): string {
    if (bytes == null) return '—';
    const mib = bytes / 1024 / 1024;
    if (mib >= 1024) return `${(mib / 1024).toFixed(2)} GB`;
    return `${mib.toFixed(mib >= 100 ? 0 : 1)} MB`;
  }

  function formatCount(value: number | null | undefined): string {
    return value == null ? '—' : String(value);
  }

  function processMeta(process: RuntimeProcessMetrics): string {
    if (!process.tracked) {
      return process.pid == null ? '外部进程 · 未纳入采样' : `PID ${process.pid} · 暂不可读取`;
    }
    return process.pid == null ? 'PID 未知' : `PID ${process.pid}`;
  }

  async function refresh(): Promise<void> {
    if (refreshPending || document.visibilityState === 'hidden') return;
    refreshPending = true;
    try {
      snapshot = await getRuntimePerformanceSnapshot();
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : '实时资源采样失败';
    } finally {
      refreshPending = false;
    }
  }

  onMount(() => {
    let timer: number | null = null;

    const start = () => {
      if (timer != null || document.visibilityState === 'hidden') return;
      void refresh();
      timer = window.setInterval(() => void refresh(), 1000);
    };
    const stop = () => {
      if (timer != null) {
        window.clearInterval(timer);
        timer = null;
      }
    };
    const handleVisibility = () => {
      if (document.visibilityState === 'hidden') stop();
      else start();
    };

    document.addEventListener('visibilitychange', handleVisibility);
    start();

    return () => {
      stop();
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  });

  const partialPrefix = $derived(snapshot?.partial ? '~' : '');
  const summaryTooltip = $derived.by(() => {
    if (error) return error;
    if (!snapshot) return '正在建立实时资源采样';
    const scope = snapshot.partial
      ? `已跟踪 ${snapshot.trackedProcessCount}/${snapshot.processCount} 个运行进程`
      : `已跟踪全部 ${snapshot.processCount} 个运行进程`;
    return `每 1 秒采样 · CPU 为整机容量占用 · 内存为 RSS 常驻集 · ${scope}`;
  });
</script>

{#if mode === 'pro'}
  <section class="runtime-pro" aria-label="实时资源损耗" title={summaryTooltip}>
    <div class="runtime-header">
      <div>
        <div class="runtime-title-row">
          <span class="runtime-title">实时资源损耗</span>
          <span class="runtime-live-dot" class:error={Boolean(error)} aria-hidden="true"></span>
          <span class="runtime-live-label">{error ? '采样异常' : '1s 实时'}</span>
        </div>
        <p class="runtime-subtitle">ZNet Sink GUI 与 Zero 内核的运行时资源占用</p>
      </div>
      {#if snapshot?.partial}
        <span class="runtime-badge" title="存在外部或暂不可读取的进程，合计值仅覆盖已跟踪部分">部分统计</span>
      {/if}
    </div>

    <div class="runtime-summary">
      <div class="runtime-metric">
        <span class="runtime-metric-label">CPU 占用</span>
        <strong>{partialPrefix}{formatCpu(snapshot?.totalCpuPercent)}</strong>
      </div>
      <div class="runtime-metric">
        <span class="runtime-metric-label">RSS 内存</span>
        <strong>{partialPrefix}{formatMemory(snapshot?.totalMemoryBytes)}</strong>
      </div>
      <div class="runtime-metric">
        <span class="runtime-metric-label">线程</span>
        <strong>{partialPrefix}{formatCount(snapshot?.totalThreadCount)}</strong>
      </div>
      <div class="runtime-metric">
        <span class="runtime-metric-label">进程</span>
        <strong>{snapshot?.processCount ?? '—'}</strong>
      </div>
    </div>

    {#if snapshot}
      <div class="runtime-processes">
        {#each [snapshot.gui, ...(snapshot.core ? [snapshot.core] : [])] as process (process.role)}
          <div class="runtime-process-row" class:untracked={!process.tracked}>
            <div class="runtime-process-name">
              <span>{process.label}</span>
              <small>{processMeta(process)}</small>
            </div>
            <span class="runtime-process-value"><small>CPU</small>{formatCpu(process.cpuPercent)}</span>
            <span class="runtime-process-value"><small>内存</small>{formatMemory(process.memoryBytes)}</span>
            <span class="runtime-process-value"><small>线程</small>{formatCount(process.threadCount)}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="runtime-placeholder">正在建立实时采样…</div>
    {/if}
  </section>
{:else}
  <div class="runtime-lite" title={summaryTooltip} aria-label="实时资源损耗">
    <div class="runtime-lite-heading">
      <span class="runtime-live-dot" class:error={Boolean(error)} aria-hidden="true"></span>
      <span>实时资源</span>
      {#if snapshot?.partial}<small>部分</small>{/if}
    </div>
    <div class="runtime-lite-metrics">
      <span><small>CPU</small><strong>{partialPrefix}{formatCpu(snapshot?.totalCpuPercent)}</strong></span>
      <span><small>内存</small><strong>{partialPrefix}{formatMemory(snapshot?.totalMemoryBytes)}</strong></span>
      <span><small>线程</small><strong>{partialPrefix}{formatCount(snapshot?.totalThreadCount)}</strong></span>
      <span><small>进程</small><strong>{snapshot?.processCount ?? '—'}</strong></span>
    </div>
  </div>
{/if}

<style>
  .runtime-pro {
    flex-shrink: 0;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--card);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  .runtime-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .runtime-title-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .runtime-title {
    font-size: 12px;
    font-weight: 650;
    color: var(--foreground);
  }

  .runtime-subtitle {
    margin: 2px 0 0;
    font-size: 10.5px;
    color: var(--muted-foreground);
  }

  .runtime-live-dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--success, #22c55e);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--success, #22c55e) 14%, transparent);
  }

  .runtime-live-dot.error {
    background: var(--destructive);
    box-shadow: none;
  }

  .runtime-live-label,
  .runtime-badge {
    font-size: 9.5px;
    color: var(--muted-foreground);
  }

  .runtime-badge {
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--muted);
  }

  .runtime-summary {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin-top: 10px;
  }

  .runtime-metric {
    min-width: 0;
    padding: 8px 9px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 55%, transparent);
  }

  .runtime-metric-label {
    display: block;
    margin-bottom: 2px;
    font-size: 9.5px;
    color: var(--muted-foreground);
  }

  .runtime-metric strong {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .runtime-processes {
    margin-top: 8px;
    border-top: 1px solid var(--border);
  }

  .runtime-process-row {
    display: grid;
    grid-template-columns: minmax(130px, 1.3fr) repeat(3, minmax(72px, 0.7fr));
    align-items: center;
    gap: 8px;
    min-height: 38px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
  }

  .runtime-process-row:last-child { border-bottom: 0; }
  .runtime-process-row.untracked { opacity: 0.65; }

  .runtime-process-name {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .runtime-process-name > span {
    font-size: 11px;
    font-weight: 600;
    color: var(--foreground);
  }

  .runtime-process-name small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 9.5px;
    color: var(--muted-foreground);
  }

  .runtime-process-value {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    min-width: 0;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .runtime-process-value small {
    font-family: inherit;
    font-size: 8.5px;
    color: var(--muted-foreground);
  }

  .runtime-placeholder {
    padding: 10px 0 2px;
    font-size: 10.5px;
    color: var(--muted-foreground);
  }

  .runtime-lite {
    width: min(100%, 500px);
    margin: 0 auto;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--card) 92%, transparent);
  }

  .runtime-lite-heading {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-bottom: 6px;
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
  }

  .runtime-lite-heading small {
    margin-left: auto;
    font-size: 9px;
    font-weight: 500;
  }

  .runtime-lite-metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 4px;
  }

  .runtime-lite-metrics > span {
    display: flex;
    min-width: 0;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }

  .runtime-lite-metrics small {
    font-size: 8.5px;
    color: var(--muted-foreground);
  }

  .runtime-lite-metrics strong {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  @media (max-width: 640px) {
    .runtime-summary { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .runtime-process-row { grid-template-columns: 1fr repeat(3, minmax(52px, auto)); }
  }
</style>
