<script lang="ts">
  import type { ProxyNode } from '$lib/types/protocol';
  import { gradeDelay } from '$lib/services/node-utils';
  import { meanDelay, buildSparkline, sparklinePath } from '$lib/components/tabs/nodes-delay-sparkline.js';
  import type { DelayEntry } from '$lib/services/delay-history.svelte';

  interface Props {
    node: ProxyNode;
    hist: DelayEntry[];
    positionStyle: string;
  }

  let { node, hist, positionStyle }: Props = $props();

  const delayState = $derived(gradeDelay(node.delay, node.alive));
  const spark = $derived(buildSparkline(hist));
</script>

<div class="delay-portal-popover" style={positionStyle}>
  <span class="popover-title">历史延迟 ({hist.length})</span>
  <svg class="sparkline" width="120" height="32" viewBox="0 0 120 32" preserveAspectRatio="none">
    <polyline points={sparklinePath(spark)} fill="none" stroke={delayState.bar} stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" />
    {#each spark.points as point}
      <circle cx={point.x.toFixed(1)} cy={point.y.toFixed(1)} r="1.5" fill={point.alive ? delayState.bar : 'var(--destructive)'} />
    {/each}
  </svg>
  <span class="popover-stats">
    最新 {hist[hist.length - 1].delay > 0 ? hist[hist.length - 1].delay + 'ms' : '超时'}
    · 均 {meanDelay(hist)}ms
  </span>
</div>

<style>
  .delay-portal-popover {
    position: fixed;
    z-index: 9999;
    min-width: 148px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--dialog-bg, #fff);
    border: 1px solid var(--border);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.16), 0 0 0 0.5px rgba(0, 0, 0, 0.05);
    display: flex;
    flex-direction: column;
    gap: 4px;
    pointer-events: none;
  }

  :global(.dark) .delay-portal-popover {
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5), 0 0 0 0.5px rgba(255, 255, 255, 0.06);
  }

  .popover-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .sparkline {
    display: block;
    width: 100%;
    overflow: visible;
  }

  .popover-stats {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }
</style>

