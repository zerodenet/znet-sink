<script lang="ts">
  import { overviewData } from '$lib/services/overview-data.svelte';

  function formatSpeed(speed: number): string {
    if (speed >= 1) return `${speed.toFixed(2)} MB/s`;
    if (speed * 1000 >= 1) return `${(speed * 1000).toFixed(0)} KB/s`;
    return `0 KB/s`;
  }

  function formatTraffic(mb: number): string {
    if (mb >= 1000) return `${(mb / 1000).toFixed(2)} GB`;
    if (mb >= 1)    return `${mb.toFixed(1)} MB`;
    return `${(mb * 1000).toFixed(0)} KB`;
  }

  const { history }: {
    history: { up: number; down: number }[];
  } = $props();

  const currentDown = $derived(history.length > 0 ? history[history.length - 1].down : 0);
  const currentUp   = $derived(history.length > 0 ? history[history.length - 1].up   : 0);
  const hasTraffic  = $derived(history.some(s => s.down >= 0.01 || s.up >= 0.01));
</script>

<div class="chart-card">
  <!-- Header row: title + live speeds -->
  <div class="chart-header">
    <span class="chart-title">实时速率</span>
    <div class="chart-speeds">
      <div class="speed-item down">
        <span class="speed-dot" class:pulse={hasTraffic}></span>
        <span class="speed-val">{formatSpeed(currentDown)}</span>
        <span class="speed-label">↓</span>
      </div>
      <div class="speed-item up">
        <span class="speed-dot" class:pulse={hasTraffic}></span>
        <span class="speed-val">{formatSpeed(currentUp)}</span>
        <span class="speed-label">↑</span>
      </div>
    </div>
  </div>

  <!-- Stats row: totals + connections -->
  <div class="chart-stats">
    <div class="stat-item">
      <span class="stat-label">下行总计</span>
      <span class="stat-val down">{formatTraffic(overviewData.totalDownMB)}</span>
    </div>
    <div class="stat-divider"></div>
    <div class="stat-item">
      <span class="stat-label">上行总计</span>
      <span class="stat-val up">{formatTraffic(overviewData.totalUpMB)}</span>
    </div>
    <div class="stat-divider"></div>
    <div class="stat-item">
      <span class="stat-label">并发连接</span>
      <span class="stat-val">{overviewData.activeConnections}</span>
    </div>
  </div>

  <!-- SVG chart -->
  <div class="chart-body">
    <svg viewBox="0 0 400 120" class="chart-svg" preserveAspectRatio="xMidYMid meet" aria-hidden="true">
      <!-- Grid lines -->
      {#each [0, 30, 60, 90, 120] as y}
        <line x1="0" y1={y} x2="400" y2={y} stroke="currentColor" stroke-opacity="0.06" stroke-width="1"/>
      {/each}

      <!-- Download area -->
      {#if history.length > 1}
        <path
          d={`${history.map((s, i, arr) => {
            const x = (i / (arr.length - 1)) * 400;
            const y = 120 - Math.min(s.down * 24, 110);
            return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
          }).join(' ')} L 400 120 L 0 120 Z`}
          fill="url(#downGrad)"
          opacity="0.65"
        />
        <!-- Download line -->
        <path
          d={history.map((s, i, arr) => {
            const x = (i / (arr.length - 1)) * 400;
            const y = 120 - Math.min(s.down * 24, 110);
            return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
          }).join(' ')}
          fill="none"
          stroke="#3B82F6"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          opacity="0.85"
        />

        <!-- Upload area -->
        <path
          d={`${history.map((s, i, arr) => {
            const x = (i / (arr.length - 1)) * 400;
            const y = 120 - Math.min(s.up * 36, 110);
            return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
          }).join(' ')} L 400 120 L 0 120 Z`}
          fill="url(#upGrad)"
          opacity="0.4"
        />
      {/if}

      <defs>
        <linearGradient id="downGrad" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%"   stop-color="#3B82F6" stop-opacity="0.55"/>
          <stop offset="100%" stop-color="#3B82F6" stop-opacity="0.02"/>
        </linearGradient>
        <linearGradient id="upGrad" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%"   stop-color="#22C55E" stop-opacity="0.45"/>
          <stop offset="100%" stop-color="#22C55E" stop-opacity="0.02"/>
        </linearGradient>
      </defs>
    </svg>

    {#if !hasTraffic}
      <div class="chart-empty">等待网络数据…</div>
    {/if}
  </div>
</div>

<style>
  .chart-card {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    overflow: hidden;
    gap: 6px;
  }

  /* ---- Header ---- */
  .chart-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }

  .chart-title {
    font-size: 11px;
    font-weight: 500;
    color: var(--muted-foreground);
  }

  .chart-speeds {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .speed-item {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .speed-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .speed-item.down .speed-dot { background: #3B82F6; }
  .speed-item.up   .speed-dot { background: #22C55E; }

  .speed-dot.pulse { animation: pulse-dot 1.6s ease-in-out infinite; }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.3; }
  }

  .speed-val {
    font-size: 11px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .speed-label {
    font-size: 11.5px;
    color: var(--muted-foreground);
    opacity: 0.6;
  }

  /* ---- Stats row ---- */
  .chart-stats {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .stat-item {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .stat-label {
    font-size: 11.5px;
    color: var(--muted-foreground);
    opacity: 0.75;
  }

  .stat-val {
    font-size: 11.5px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .stat-val.down { color: #3B82F6; }
  .stat-val.up   { color: #22C55E; }

  :global(.dark) .stat-val.down { color: #60A5FA; }
  :global(.dark) .stat-val.up   { color: #4ADE80; }

  .stat-divider {
    width: 1px;
    height: 10px;
    background: var(--border);
  }

  /* ---- Chart body ---- */
  .chart-body {
    flex: 1;
    position: relative;
    overflow: hidden;
    min-height: 0;
    border-radius: 6px;
    background: var(--muted);
    color: var(--foreground);
  }

  .chart-svg {
    width: 100%;
    height: 100%;
    display: block;
  }

  .chart-empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.4;
    pointer-events: none;
  }
</style>
