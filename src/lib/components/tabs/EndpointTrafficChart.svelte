<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowDown, ArrowUp, Activity } from '@lucide/svelte';
  import { formatEndpointRate, trafficPaths, type EndpointTraffic } from '$lib/features/endpoints/traffic';
  let { traffic, stale = false }: { traffic?: EndpointTraffic; stale?: boolean } = $props();
  let now = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(timer);
  });
  const expired = $derived(stale || (!!traffic?.baseline && now - traffic.baseline.at > 15000));
  const points = $derived(traffic?.points ?? []);
  const latest = $derived(points.at(-1));
  const ceiling = $derived(Math.max(1024, ...points.flatMap(point => [point.rx ?? 0, point.tx ?? 0])) * 1.1);
  const sampled = $derived(points.some(point => point.rx !== null || point.tx !== null));
  const label = $derived(expired ? '采样中断' : !traffic?.source ? '暂无流量数据' : !sampled ? '采样中…' : null);
</script>

<div class="traffic" class:stale={expired} aria-label="端点实时流量">
  <div class="traffic-heading"><span><Activity size={13} />{traffic?.source === 'outer' ? '外层流量' : '实时流量'}</span><small>2 分钟</small></div>
  <svg viewBox="0 0 300 64" preserveAspectRatio="none" role="img" aria-label={label ?? '最近两分钟收发速率'}>
    <path d="M0 57H300 M0 33H300 M0 9H300" class="grid" />
    {#if sampled}<path d={trafficPaths(points, 'rx', ceiling)} class="rx" /><path d={trafficPaths(points, 'tx', ceiling)} class="tx" />{/if}
    {#if label}<text x="150" y="35" text-anchor="middle">{label}</text>{/if}
  </svg>
  <div class="rates"><span class="download"><ArrowDown size={13} />{formatEndpointRate(expired ? null : latest?.rx)}</span><span class="upload"><ArrowUp size={13} />{formatEndpointRate(expired ? null : latest?.tx)}</span></div>
</div>

<style>
  .traffic { padding: 12px 0; } .traffic-heading, .traffic-heading>span, .rates, .rates>span { display:flex; align-items:center; gap:6px; }
  .traffic-heading { justify-content:space-between; color:var(--muted-foreground); font-size:11px; } small { font-size:10px; opacity:.7; }
  svg { width:100%; height:64px; margin:8px 0; overflow:visible; } path { fill:none; vector-effect:non-scaling-stroke; } .grid { stroke:var(--border); stroke-width:.6; }
  .rx { stroke:var(--info,#3b82f6); stroke-width:1.7; } .tx { stroke:var(--success,#16a34a); stroke-width:1.7; } text { font-size:11px; fill:var(--muted-foreground); }
  .rates { justify-content:space-between; font-size:12px; font-variant-numeric:tabular-nums; } .download { color:var(--info,#3b82f6); } .upload { color:var(--success,#16a34a); } .stale path:not(.grid) { opacity:.35; }
</style>
