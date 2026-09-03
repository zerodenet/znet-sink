<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import type { ProxyNode } from '$lib/types/protocol';
  import { gradeDelay, formatProbeTime, parseNodeName } from '$lib/services/node-utils';
  import { meanDelay, buildSparkline, sparklinePath } from '$lib/components/tabs/nodes-delay-sparkline.js';

  type DelayEntry = { delay: number; at: number; selectedTag?: string };

  interface Props {
    node: ProxyNode;
    hist: DelayEntry[];
  }

  let { node, hist }: Props = $props();

  type ViewMode = 'chart' | 'list';
  let viewMode = $state<ViewMode>('chart');
  let hoveredIndex = $state<number | null>(null);

  // `node` is the card snapshot captured when the popover opened. The node
  // list is rebuilt after every policy probe, so an already-open popover must
  // use the live history tail rather than keeping that snapshot's delay/time.
  const latestEntry = $derived(hist.length > 0 ? hist[hist.length - 1] : undefined);
  const currentDelay = $derived(latestEntry?.delay ?? node.delay);
  const currentAlive = $derived(latestEntry ? latestEntry.delay >= 0 : node.alive);
  const currentProbeAt = $derived(latestEntry?.at ?? node.lastProbeAt);
  const delayState = $derived(gradeDelay(currentDelay, currentAlive));
  const spark = $derived(buildSparkline(hist));
  const lastProbeLabel = $derived(formatProbeTime(currentProbeAt));
  const averageDelay = $derived(meanDelay(hist));
  const averageDelayLabel = $derived(averageDelay === '-' ? '—' : `${averageDelay}ms`);
  const latestTarget = $derived(
    latestEntry?.selectedTag ? parseNodeName(latestEntry.selectedTag) : undefined,
  );

  // Reversed history for list view (newest first)
  const reversedHist = $derived([...hist].reverse());

  function toggleView() {
    viewMode = viewMode === 'chart' ? 'list' : 'chart';
    hoveredIndex = null;
  }

  function isSameDay(a: Date, b: Date): boolean {
    return a.getFullYear() === b.getFullYear()
      && a.getMonth() === b.getMonth()
      && a.getDate() === b.getDate();
  }

  function formatHistoryTime(ts: number): string {
    const date = new Date(ts);
    const now = new Date();
    if (isSameDay(date, now)) {
      return new Intl.DateTimeFormat('zh-CN', {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false,
      }).format(date);
    }

    if (date.getFullYear() === now.getFullYear()) {
      return new Intl.DateTimeFormat('zh-CN', {
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        hour12: false,
      }).format(date);
    }

    return new Intl.DateTimeFormat('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    }).format(date);
  }

  function formatHistoryTooltip(ts: number): string {
    return new Intl.DateTimeFormat('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    }).format(new Date(ts));
  }

  function handleSparklineHover(event: MouseEvent) {
    const svg = event.currentTarget as SVGElement;
    const rect = svg.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const ratio = x / rect.width;
    const index = Math.round(ratio * (hist.length - 1));
    hoveredIndex = Math.max(0, Math.min(hist.length - 1, index));
  }

  function handleSparklineLeave() {
    hoveredIndex = null;
  }

  const hoveredEntry = $derived(hoveredIndex !== null ? hist[hoveredIndex] : null);
  const hoveredTarget = $derived(
    hoveredEntry?.selectedTag ? parseNodeName(hoveredEntry.selectedTag) : undefined,
  );
</script>

<div class="delay-portal-popover">
  <div class="popover-header">
    <span class="popover-title">历史延迟 ({hist.length})</span>
    <Button variant="ghost" size="icon-xs" onclick={toggleView} title={viewMode === 'chart' ? '切换列表' : '切换图表'}>
      {#if viewMode === 'chart'}
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/>
          <line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>
        </svg>
      {:else}
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="22,12 18,12 15,21 9,3 6,12 2,12"/>
        </svg>
      {/if}
    </Button>
  </div>

  {#if viewMode === 'chart'}
    <div class="chart-container">
      <svg
        class="sparkline"
        width="120"
        height="32"
        viewBox="0 0 120 32"
        preserveAspectRatio="none"
        role="img"
        aria-label="延迟历史折线图"
        onmousemove={handleSparklineHover}
        onmouseleave={handleSparklineLeave}
      >
        <polyline points={sparklinePath(spark)} fill="none" stroke={delayState.bar} stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" />
        {#each spark.points as point, i}
          <circle
            cx={point.x.toFixed(1)}
            cy={point.y.toFixed(1)}
            r={hoveredIndex === i ? 3 : 1.5}
            fill={point.alive ? delayState.bar : 'var(--destructive)'}
            opacity={hoveredIndex !== null && hoveredIndex !== i ? 0.4 : 1}
          />
        {/each}
        {#if hoveredEntry && hoveredIndex !== null}
          <line
            x1={spark.points[hoveredIndex].x.toFixed(1)}
            y1="0"
            x2={spark.points[hoveredIndex].x.toFixed(1)}
            y2="32"
            stroke="var(--muted-foreground)"
            stroke-width="0.5"
            stroke-dasharray="2,2"
          />
        {/if}
      </svg>
      {#if hoveredEntry}
        <span class="hover-tooltip">
          {#if hoveredTarget}
            <span class="target-label">
              {#if hoveredTarget.flagCode}
                <span
                  class="target-country-flag fi fi-{hoveredTarget.flagCode.toLowerCase()}"
                  role="img"
                  aria-label="国旗 {hoveredTarget.flagCode}"
                ></span>
              {:else if hoveredTarget.emoji}
                <span class="target-emoji">{hoveredTarget.emoji}</span>
              {/if}
              <span>{hoveredTarget.cleanName}</span>
            </span>
            <span> · </span>
          {/if}
          {formatHistoryTooltip(hoveredEntry.at)} · {hoveredEntry.delay > 0 ? hoveredEntry.delay + 'ms' : '超时'}
        </span>
      {/if}
    </div>
  {:else}
    <div class="delay-list">
      {#each reversedHist as entry, i}
        <div class="delay-list-item">
          <span class="delay-list-time" title={formatHistoryTooltip(entry.at)}>{formatHistoryTime(entry.at)}</span>
          {#if entry.selectedTag}
            {@const entryTarget = parseNodeName(entry.selectedTag)}
            <span class="delay-list-target" title={entry.selectedTag}>
              {#if entryTarget.flagCode}
                <span
                  class="target-country-flag fi fi-{entryTarget.flagCode.toLowerCase()}"
                  role="img"
                  aria-label="国旗 {entryTarget.flagCode}"
                ></span>
              {:else if entryTarget.emoji}
                <span class="target-emoji">{entryTarget.emoji}</span>
              {/if}
              <span class="delay-list-target-name">{entryTarget.cleanName}</span>
            </span>
          {/if}
          <span class="delay-list-bar-wrap">
            <span
              class="delay-list-bar"
              style="width: {entry.delay > 0 ? Math.min(100, (entry.delay / spark.maxDelay) * 100) : 0}%; background: {entry.delay > 0 ? delayState.bar : 'var(--destructive)'};"
            ></span>
          </span>
          <span class="delay-list-value" style="color: {entry.delay > 0 ? delayState.color : 'var(--destructive)'};">
            {entry.delay > 0 ? entry.delay + 'ms' : '超时'}
          </span>
        </div>
      {/each}
    </div>
  {/if}

  {#if latestEntry}
    <div class="popover-stats">
      <span class="popover-stats-summary">
        最新 {latestEntry.delay > 0 ? latestEntry.delay + 'ms' : '超时'} · 均 {averageDelayLabel}
      </span>
      {#if latestTarget && latestEntry.selectedTag}
        <span class="popover-stats-target" title={latestEntry.selectedTag}>
          {#if latestTarget.flagCode}
            <span
              class="target-country-flag popover-target-flag fi fi-{latestTarget.flagCode.toLowerCase()}"
              role="img"
              aria-label="国旗 {latestTarget.flagCode}"
            ></span>
          {:else if latestTarget.emoji}
            <span class="target-emoji">{latestTarget.emoji}</span>
          {/if}
          <span>{latestTarget.cleanName}</span>
        </span>
      {/if}
    </div>
  {/if}
  {#if currentProbeAt}
    <span class="popover-time">测速: {lastProbeLabel}</span>
  {/if}
</div>

<style>
  .delay-portal-popover {
    min-width: 180px;
    max-width: 280px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--dialog-bg, #fff);
    border: 1px solid var(--border);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.16), 0 0 0 0.5px rgba(0, 0, 0, 0.05);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  :global(.dark) .delay-portal-popover {
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5), 0 0 0 0.5px rgba(255, 255, 255, 0.06);
  }

  .popover-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .popover-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

.toggle-btn:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  .chart-container {
    position: relative;
  }

  .sparkline {
    display: block;
    width: 100%;
    overflow: visible;
    cursor: crosshair;
  }

  .hover-tooltip {
    position: absolute;
    top: -4px;
    right: 0;
    max-width: 260px;
    font-size: 9px;
    font-family: var(--font-mono);
    color: var(--foreground);
    background: var(--card);
    padding: 1px 4px;
    border-radius: 3px;
    border: 1px solid var(--border);
    white-space: nowrap;
    pointer-events: none;
  }

  .target-label,
  .popover-stats-target,
  .delay-list-target {
    display: inline-flex;
    align-items: center;
  }

  .target-country-flag {
    display: inline-block;
    width: 12px;
    height: 9px;
    margin-right: 3px;
    border-radius: 1.5px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--border) 82%, transparent);
    overflow: hidden;
    flex-shrink: 0;
  }

  .popover-target-flag {
    width: 14px;
    height: 10.5px;
    margin-right: 4px;
  }

  .target-emoji {
    margin-right: 2px;
    font-family: "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif;
  }

  .delay-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    height: 150px;
    overflow-y: auto;
    padding-right: 2px;
  }

  .delay-list::-webkit-scrollbar {
    width: 3px;
  }

  .delay-list::-webkit-scrollbar-thumb {
    background: var(--muted-foreground);
    border-radius: 2px;
    opacity: 0.3;
  }

  .delay-list-item {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 18px;
  }

  .delay-list-time {
    font-size: 9px;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    flex-shrink: 0;
    width: 92px;
    text-align: right;
  }

  .delay-list-bar-wrap {
    flex: 1;
    height: 4px;
    background: var(--muted);
    border-radius: 2px;
    overflow: hidden;
    min-width: 40px;
  }

  .delay-list-target {
    width: 72px;
    flex-shrink: 0;
    min-width: 0;
    font-size: 9px;
    font-family: var(--font-mono);
    color: var(--foreground);
  }

  .delay-list-target-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .delay-list-bar {
    height: 100%;
    border-radius: 2px;
    transition: width 0.2s ease;
  }

  .delay-list-value {
    font-size: 9px;
    font-family: var(--font-mono);
    font-weight: 600;
    flex-shrink: 0;
    width: 36px;
    text-align: right;
  }

  .popover-stats {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }

  .popover-stats-summary {
    white-space: nowrap;
  }

  .popover-stats-target {
    min-width: 0;
    color: var(--foreground);
    font-family: inherit;
    font-size: 10px;
    line-height: 1.35;
  }

  .popover-stats-target > span:last-child {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .popover-time {
    font-size: 9px;
    color: var(--muted-foreground);
    opacity: 0.7;
  }
</style>
