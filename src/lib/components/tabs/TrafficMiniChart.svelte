<script lang="ts">
  import type { Observation } from '$lib/features/traffic/history';
  import { byteKeys, chartPath, formatRate, metric } from '$lib/features/traffic/history';
  let { observation, plane = 'flow', stale = false }: { observation?: Observation; plane?: string; stale?: boolean } = $props();
  const keys = $derived(observation ? byteKeys(observation.snapshot, plane) : ['rx_bytes', 'tx_bytes']);
  const paths = $derived(keys.map(key => `${plane}.${key}`));
  const points = $derived(observation?.points ?? []);
  const ceiling = $derived(Math.max(1024, ...points.flatMap(p => paths.map(key => p.values[key] ?? 0))));
  const physical = $derived(keys[0] === 'rx_bytes');
  function hint(key: string) {
    if (!observation) return '暂无统计快照';
    if (metric(observation.snapshot.planes.find(p => p.plane === plane), key) === null) return '内核未提供此指标';
    if (stale) return '采样中断，累计值保留';
    return observation.rates[`${plane}.${key}`] == null ? '等待下一次有效采样' : '同一统计周期的差分速率';
  }
</script>
<div class="mini-chart" aria-label={`${plane} 流量曲线`}>
  <svg viewBox="0 0 300 64" preserveAspectRatio="none" role="img" aria-label="最近两分钟的同周期差分速率">
    <path d="M0,9H300 M0,33H300 M0,57H300" class="grid" />
    {#if !stale}{#each paths as key, index}<path d={chartPath(points, key, ceiling)} class:receive={index === 0} class:send={index === 1} />{/each}{/if}
  </svg>
  <div class="rates"><span class="receive" title={hint(keys[0])}>{physical ? 'RX' : '↓'} {formatRate(stale ? null : observation?.rates[paths[0]])}</span><span class="send" title={hint(keys[1])}>{physical ? 'TX' : '↑'} {formatRate(stale ? null : observation?.rates[paths[1]])}</span></div>
</div>
<style>
  .mini-chart { min-width:0; } svg { width:100%; height:64px; display:block; } path { fill:none; stroke-width:1.7; vector-effect:non-scaling-stroke; stroke-linecap:round; } .grid { stroke:var(--border); stroke-width:.6; opacity:.5; } .receive { color:var(--chart-download,#3b82f6); stroke:var(--chart-download,#3b82f6); } .send { color:var(--chart-upload,#16a34a); stroke:var(--chart-upload,#16a34a); } .rates { display:flex; justify-content:space-between; gap:8px; font-size:11px; font-variant-numeric:tabular-nums; }
</style>
