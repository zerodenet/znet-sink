<script lang="ts">
  import { onMount } from 'svelte';

  import { getRuntimePerformanceSnapshot } from '$lib/services/runtime-performance';
  import type { RuntimePerformanceSnapshot } from '$lib/types/runtime-performance';

  let { mode }: { mode: 'pro' | 'lite' } = $props();

  const REFRESH_INTERVAL_MS = 2000;

  let snapshot = $state<RuntimePerformanceSnapshot | null>(null);
  let refreshPending = false;
  let error = $state<string | null>(null);

  function formatCpu(value: number | null | undefined): string {
    if (value == null) return '—';
    if (value < 0.1) return '<0.1%';
    return `${value.toFixed(value >= 10 ? 1 : 2)}%`;
  }

  function formatMemory(bytes: number | null | undefined): string {
    if (bytes == null) return '—';
    const mib = bytes / 1024 / 1024;
    if (mib >= 1024) return `${(mib / 1024).toFixed(2)} GB`;
    return `${mib.toFixed(mib >= 100 ? 0 : 1)} MB`;
  }

  async function refresh(): Promise<void> {
    if (mode !== 'lite' || refreshPending || document.visibilityState === 'hidden') return;
    refreshPending = true;
    try {
      snapshot = await getRuntimePerformanceSnapshot();
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : '资源占用读取失败';
    } finally {
      refreshPending = false;
    }
  }

  onMount(() => {
    if (mode !== 'lite') return;

    let timer: number | null = null;

    const start = () => {
      if (timer != null || document.visibilityState === 'hidden') return;
      void refresh();
      timer = window.setInterval(() => void refresh(), REFRESH_INTERVAL_MS);
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

  const summaryLabel = $derived.by(() => {
    if (error) return error;
    if (!snapshot) return '正在读取 CPU 和内存使用情况';
    if (!snapshot.core) return 'Zero 内核未运行';
    if (!snapshot.core.tracked) return 'Zero 进程资源占用当前无法读取';
    return 'Zero 进程 CPU 和常驻内存；每 2 秒更新';
  });

  // Keep the Lite overview on the same metric boundary as the Pro core card.
  // Snapshot totals also include the GUI process and therefore cannot be
  // compared with the Zero-only values shown in professional mode.
  const coreRuntime = $derived(snapshot?.core ?? null);
</script>

{#if mode === 'lite'}
  <div class="runtime-lite" aria-label={summaryLabel}>
    {#if error}
      <span class="runtime-lite-state error">资源占用暂时无法读取</span>
    {:else}
      <div class="runtime-lite-metrics" class:loading={!snapshot}>
        <span class="runtime-lite-metric">
          <small>CPU</small>
          <strong>{formatCpu(coreRuntime?.cpuPercent)}</strong>
        </span>
        <span class="runtime-lite-metric">
          <small>内存</small>
          <strong>{formatMemory(coreRuntime?.memoryBytes)}</strong>
        </span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .runtime-lite {
    width: min(100%, 720px);
    min-height: 22px;
    margin: 0 auto;
    padding: 0 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--muted-foreground);
  }

  .runtime-lite-metrics {
    display: flex;
    align-items: center;
    justify-content: center;
    max-width: 100%;
    min-width: 0;
  }

  .runtime-lite-metrics.loading {
    opacity: 0.5;
  }

  .runtime-lite-metric {
    position: relative;
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
    min-width: 0;
    padding: 0 12px;
    white-space: nowrap;
  }

  .runtime-lite-metric:not(:last-child)::after {
    content: '';
    position: absolute;
    top: 50%;
    right: 0;
    width: 1px;
    height: 10px;
    transform: translateY(-50%);
    background: color-mix(in srgb, var(--border) 72%, transparent);
  }

  .runtime-lite-metric small {
    font-size: 9px;
    font-weight: 500;
    color: var(--muted-foreground);
    opacity: 0.76;
  }

  .runtime-lite-metric strong {
    max-width: 100px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--foreground) 76%, var(--muted-foreground));
  }

  .runtime-lite-state {
    font-size: 9.5px;
    color: var(--muted-foreground);
    opacity: 0.72;
  }

  .runtime-lite-state.error {
    color: var(--destructive);
    opacity: 0.78;
  }

  @media (max-width: 640px) {
    .runtime-lite-metric { padding: 0 9px; gap: 3px; }
  }
</style>
