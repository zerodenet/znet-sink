<script lang="ts">
  import { overviewData } from '$lib/services/overview-data.svelte';

  type TrafficPoint = { up: number; down: number };
  type ChartScale = {
    min: number;
    max: number;
    ticks: number[];
  };

  const DISPLAY_HISTORY = 120;
  const SCALE_HISTORY = 45;
  const CHART_WIDTH = 400;
  const CHART_HEIGHT = 120;
  const PLOT_LEFT = 6;
  const PLOT_RIGHT = 6;
  const PLOT_TOP = 8;
  const PLOT_BOTTOM = 114;
  const PLOT_WIDTH = CHART_WIDTH - PLOT_LEFT - PLOT_RIGHT;
  const PLOT_HEIGHT = PLOT_BOTTOM - PLOT_TOP;
  const TICK_COUNT = 4;
  const MIN_VISIBLE_SCALE_MBPS = 0.016; // 16 KB/s

  function formatSpeed(speed: number): string {
    if (speed >= 1) return `${speed.toFixed(2)} MB/s`;
    if (speed * 1000 >= 1) return `${(speed * 1000).toFixed(speed * 1000 >= 100 ? 0 : 1)} KB/s`;
    if (speed > 0) return '<1 KB/s';
    return '0 KB/s';
  }

  function formatScaleSpeed(speed: number): string {
    if (speed >= 10) return `${speed.toFixed(0)} MB/s`;
    if (speed >= 1) return `${speed.toFixed(speed >= 5 ? 1 : 2)} MB/s`;
    const kb = speed * 1000;
    if (kb >= 100) return `${kb.toFixed(0)} KB/s`;
    if (kb >= 10) return `${kb.toFixed(0)} KB/s`;
    if (kb >= 1) return `${kb.toFixed(1)} KB/s`;
    return speed <= 0 ? '0 KB/s' : '<1 KB/s';
  }

  function formatScaleLabel(scale: ChartScale): string {
    if (scale.min <= 0) return `上限 ${formatScaleSpeed(scale.max)}`;
    return `${formatScaleSpeed(scale.min)} – ${formatScaleSpeed(scale.max)}`;
  }

  function formatTraffic(mb: number): string {
    if (mb >= 1000) return `${(mb / 1000).toFixed(2)} GB`;
    if (mb >= 1) return `${mb.toFixed(1)} MB`;
    return `${(mb * 1000).toFixed(0)} KB`;
  }

  function niceStep(value: number): number {
    if (!Number.isFinite(value) || value <= 0) return MIN_VISIBLE_SCALE_MBPS / TICK_COUNT;
    const exponent = Math.floor(Math.log10(value));
    const power = 10 ** exponent;
    const fraction = value / power;
    const niceFraction = fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10;
    return niceFraction * power;
  }

  function buildScale(points: TrafficPoint[]): ChartScale {
    const recent = points.slice(-SCALE_HISTORY);
    const source = recent.length > 0 ? recent : points;
    const values = source
      .flatMap((point) => [point.up, point.down])
      .filter((value) => Number.isFinite(value) && value >= 0);
    const positive = values.filter((value) => value > 0);
    const peak = positive.length > 0 ? Math.max(...positive) : 0;

    if (peak <= 0) {
      const max = MIN_VISIBLE_SCALE_MBPS;
      return {
        min: 0,
        max,
        ticks: Array.from({ length: TICK_COUNT + 1 }, (_, index) => max * index / TICK_COUNT),
      };
    }

    // The chart is a trend surface, while the header carries the exact current
    // upload/download values. Detect a steady dominant traffic band separately
    // from the smaller direction so a quiet upload stream does not force a
    // stable multi-MB/s download curve onto an unnecessarily large zero-based
    // scale. The smaller series remains clamped to the visual baseline.
    const dominantValues = source
      .map((point) => Math.max(point.up, point.down))
      .filter((value) => value > 0);
    const dominantPeak = dominantValues.length > 0 ? Math.max(...dominantValues) : peak;
    const dominantMin = dominantValues.length > 0 ? Math.min(...dominantValues) : 0;
    const steadyEnough = dominantValues.length >= 15 && dominantMin / dominantPeak >= 0.72;

    if (steadyEnough) {
      const observedRange = Math.max(dominantPeak - dominantMin, dominantPeak * 0.06);
      const padding = observedRange * 0.7;
      const targetMin = Math.max(0, dominantMin - padding);
      const targetMax = dominantPeak + padding;
      const step = niceStep((targetMax - targetMin) / TICK_COUNT);
      const min = Math.max(0, Math.floor(targetMin / step) * step);
      const max = Math.max(min + step * TICK_COUNT, Math.ceil(targetMax / step) * step);
      return {
        min,
        max,
        ticks: Array.from({ length: TICK_COUNT + 1 }, (_, index) => min + (max - min) * index / TICK_COUNT),
      };
    }

    const targetMax = Math.max(MIN_VISIBLE_SCALE_MBPS, peak * 1.12);
    const step = niceStep(targetMax / TICK_COUNT);
    const max = Math.max(MIN_VISIBLE_SCALE_MBPS, Math.ceil(targetMax / step) * step);
    return {
      min: 0,
      max,
      ticks: Array.from({ length: TICK_COUNT + 1 }, (_, index) => max * index / TICK_COUNT),
    };
  }

  function pointX(index: number, length: number): number {
    if (length <= 1) return PLOT_LEFT;
    return PLOT_LEFT + (index / (length - 1)) * PLOT_WIDTH;
  }

  function pointY(value: number, scale: ChartScale): number {
    const range = Math.max(Number.EPSILON, scale.max - scale.min);
    const normalized = Math.max(0, Math.min(1, (value - scale.min) / range));
    return PLOT_BOTTOM - normalized * PLOT_HEIGHT;
  }

  function linePath(points: TrafficPoint[], key: keyof TrafficPoint, scale: ChartScale): string {
    return points.map((point, index) => {
      const x = pointX(index, points.length);
      const y = pointY(point[key], scale);
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    }).join(' ');
  }

  function areaPath(points: TrafficPoint[], key: keyof TrafficPoint, scale: ChartScale): string {
    if (points.length < 2) return '';
    const path = linePath(points, key, scale);
    const lastX = pointX(points.length - 1, points.length);
    const firstX = pointX(0, points.length);
    return `${path} L ${lastX.toFixed(2)} ${PLOT_BOTTOM} L ${firstX.toFixed(2)} ${PLOT_BOTTOM} Z`;
  }

  const { history, unsupported = false }: {
    history: TrafficPoint[];
    unsupported?: boolean;
  } = $props();

  const displayHistory = $derived(history.slice(-DISPLAY_HISTORY));
  const scale = $derived(buildScale(displayHistory));
  const scaleLabel = $derived(formatScaleLabel(scale));
  const downLine = $derived(linePath(displayHistory, 'down', scale));
  const upLine = $derived(linePath(displayHistory, 'up', scale));
  const downArea = $derived(areaPath(displayHistory, 'down', scale));
  const upArea = $derived(areaPath(displayHistory, 'up', scale));

  const currentDown = $derived(history.length > 0 ? history[history.length - 1].down : 0);
  const currentUp = $derived(history.length > 0 ? history[history.length - 1].up : 0);
  const hasTraffic = $derived(displayHistory.some((sample) => sample.down >= 0.001 || sample.up >= 0.001));
  const lastPointIndex = $derived(Math.max(0, displayHistory.length - 1));
  const lastPointX = $derived(pointX(lastPointIndex, displayHistory.length));
  const lastDownY = $derived(pointY(currentDown, scale));
  const lastUpY = $derived(pointY(currentUp, scale));
</script>

<div class="chart-card">
  <div class="chart-header">
    <div class="chart-title">
      <span class="chart-title-text">实时速率</span>
      <span class="chart-subtitle">最近 2 分钟 · {scaleLabel}</span>
    </div>
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

  <div class="chart-body">
    <svg viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`} class="chart-svg" preserveAspectRatio="none" aria-hidden="true">
      {#each scale.ticks.slice(1, -1) as tick}
        {@const y = pointY(tick, scale)}
        <line
          x1={PLOT_LEFT}
          y1={y}
          x2={CHART_WIDTH - PLOT_RIGHT}
          y2={y}
          class="grid-line"
          vector-effect="non-scaling-stroke"
        />
      {/each}

      {#if displayHistory.length > 1}
        <path d={downArea} fill="url(#downGrad)" class="traffic-area down" />
        <path d={upArea} fill="url(#upGrad)" class="traffic-area up" />

        <path
          d={downLine}
          fill="none"
          class="traffic-line down"
          vector-effect="non-scaling-stroke"
        />
        <path
          d={upLine}
          fill="none"
          class="traffic-line up"
          vector-effect="non-scaling-stroke"
        />

        {#if hasTraffic}
          <circle cx={lastPointX} cy={lastDownY} r="2" class="traffic-point down" vector-effect="non-scaling-stroke" />
          <circle cx={lastPointX} cy={lastUpY} r="2.2" class="traffic-point up" vector-effect="non-scaling-stroke" />
        {/if}
      {/if}

      <defs>
        <linearGradient id="downGrad" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="var(--traffic-down)" stop-opacity="0.24"/>
          <stop offset="100%" stop-color="var(--traffic-down)" stop-opacity="0.01"/>
        </linearGradient>
        <linearGradient id="upGrad" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stop-color="var(--traffic-up)" stop-opacity="0.24"/>
          <stop offset="100%" stop-color="var(--traffic-up)" stop-opacity="0.01"/>
        </linearGradient>
      </defs>
    </svg>

    {#if !hasTraffic && !unsupported}
      <div class="chart-empty">等待网络数据…</div>
    {:else if !hasTraffic && unsupported}
      <div class="chart-empty">内核不支持流量查询</div>
    {/if}
  </div>
</div>

<style>
  .chart-card {
    --traffic-down: #3b82f6;
    --traffic-up: #22c55e;
    width: 100%;
    min-width: 0;
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

  :global(.dark) .chart-card {
    --traffic-down: #60a5fa;
    --traffic-up: #4ade80;
  }

  .chart-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }

  .chart-title {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .chart-title-text,
  .chart-subtitle {
    font-size: 11px;
    font-weight: 500;
    color: var(--muted-foreground);
  }

  .chart-title-text { flex-shrink: 0; }

  .chart-subtitle {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 400;
    font-size: 10px;
    opacity: 0.58;
  }

  .chart-speeds {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
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

  .speed-item.down .speed-dot { background: var(--traffic-down); }
  .speed-item.up .speed-dot { background: var(--traffic-up); }
  .speed-dot.pulse { animation: pulse-dot 1.6s ease-in-out infinite; }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }

  .speed-val {
    font-size: 11px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .speed-item.up .speed-val {
    color: color-mix(in srgb, var(--traffic-up) 86%, var(--foreground));
  }

  .speed-label {
    font-size: 11.5px;
    color: var(--muted-foreground);
    opacity: 0.6;
  }

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

  .stat-val.down { color: var(--traffic-down); }
  .stat-val.up { color: var(--traffic-up); }

  .stat-divider {
    width: 1px;
    height: 10px;
    background: var(--border);
  }

  .chart-body {
    flex: 1;
    width: 100%;
    position: relative;
    overflow: hidden;
    min-height: 0;
    border-radius: 7px;
    background: color-mix(in srgb, var(--muted) 38%, transparent);
    color: var(--foreground);
  }

  .chart-svg {
    width: 100%;
    height: 100%;
    display: block;
  }

  .grid-line {
    stroke: currentColor;
    stroke-opacity: 0.055;
    stroke-width: 1;
  }

  .traffic-area,
  .traffic-line,
  .traffic-point {
    pointer-events: none;
  }

  .traffic-line {
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0.94;
  }

  .traffic-line.down { stroke: var(--traffic-down); }
  .traffic-line.up {
    stroke: var(--traffic-up);
    stroke-width: 2.05;
    opacity: 1;
  }

  .traffic-point {
    stroke: var(--card);
    stroke-width: 1.2;
  }

  .traffic-point.down { fill: var(--traffic-down); }
  .traffic-point.up { fill: var(--traffic-up); }

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

  @media (max-width: 640px) {
    .chart-title { gap: 4px; }
    .chart-subtitle { display: none; }
    .chart-speeds { gap: 7px; }
    .speed-val { font-size: 10.5px; }
  }
</style>
