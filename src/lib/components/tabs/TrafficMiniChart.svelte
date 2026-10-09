<script lang="ts">
  import { untrack } from 'svelte';
  import type { Observation } from '$lib/features/traffic/history';
  import { byteKeys, chartPath, formatRate, metric } from '$lib/features/traffic/history';
  import { chartCeiling, displayRate, displaySeries } from '$lib/features/traffic/presentation';
  let { observation, plane = 'flow', stale = false }: { observation?: Observation; plane?: string; stale?: boolean } = $props();
  const keys = $derived(observation ? byteKeys(observation.snapshot, plane) : ['rx_bytes', 'tx_bytes']);
  const paths = $derived(keys.map(key => `${plane}.${key}`));
  const points = $derived(displaySeries(observation?.points ?? [], paths));
  const peak = $derived(Math.max(0, ...points.flatMap(p => paths.map(key => p.values[key] ?? 0))));
  let ceiling = $state(1024);
  let scaleScope = '';
  $effect(() => {
    const snapshot = observation?.snapshot;
    const scope = JSON.stringify([snapshot?.core_instance_id, snapshot?.scope, snapshot?.stats_epoch, snapshot?.generation, plane]);
    ceiling = chartCeiling(scope === scaleScope ? untrack(() => ceiling) : 0, peak);
    scaleScope = scope;
  });
  const physical = $derived(keys[0] === 'rx_bytes');
  function hint(key: string) {
    if (!observation) return '暂无统计快照';
    if (metric(observation.snapshot.planes.find(p => p.plane === plane), key) === null) return '内核未提供此指标';
    if (stale) return '采样中断，累计值保留';
    return observation.rates[`${plane}.${key}`] == null ? '等待下一次有效采样' : '最近 3 秒平均速率 · 按实际采样间隔计算';
  }
</script>
<div class="mini-chart" class:stale aria-label={`${plane} 流量曲线`}>
  <svg viewBox="0 0 300 64" preserveAspectRatio="none" role="img" aria-label="最近两分钟的同周期平均速率" data-ceiling={ceiling}>
    <path d="M0,9H300 M0,33H300 M0,57H300" class="grid" />
    {#each paths as key, index}<path d={chartPath(points, key, ceiling)} class:receive={index === 0} class:send={index === 1} />{/each}
  </svg>
  <div class="rates">{#each paths as key, index}<span class:receive={index === 0} class:send={index === 1} title={hint(keys[index])}><span class="direction">{physical ? (index === 0 ? 'RX' : 'TX') : (index === 0 ? '↓' : '↑')}</span> <span class="rate-value">{formatRate(stale ? null : displayRate(observation, key))}</span></span>{/each}</div>
</div>
<style>
  .mini-chart { min-width:0; } svg { width:100%; height:64px; display:block; } path { fill:none; stroke-width:1.7; vector-effect:non-scaling-stroke; stroke-linecap:round; } .grid { stroke:var(--border); stroke-width:.6; opacity:.5; } .receive { color:var(--chart-download,#3b82f6); stroke:var(--chart-download,#3b82f6); } .send { color:var(--chart-upload,#16a34a); stroke:var(--chart-upload,#16a34a); } .rates { display:flex; justify-content:space-between; gap:8px; font-size:11px; font-variant-numeric:tabular-nums; }
  .rates>span { display:inline-flex; align-items:baseline; gap:4px; white-space:nowrap; }
  .direction { min-width:2ch; }
  .rate-value { display:inline-block; min-width:9ch; text-align:right; }
  .stale path:not(.grid) { opacity:.35; }
</style>
