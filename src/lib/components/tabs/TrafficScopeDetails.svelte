<script lang="ts">
  import type { Observation } from '$lib/features/traffic/history';
  import { byteKeys, formatBytes, metric } from '$lib/features/traffic/history';
  import TrafficMiniChart from './TrafficMiniChart.svelte';
  let { observation, stale = false }: { observation: Observation; stale?: boolean } = $props();
  const snapshot = $derived(observation.snapshot);
  function when(value: string) { const n = Number(value); return Number.isSafeInteger(n) ? new Date(n).toLocaleString() : value; }
</script>
<div class="plane-grid">
  {#each snapshot.planes as plane (plane.plane)}
    {@const keys = byteKeys(snapshot, plane.plane)}
    <section class="plane" aria-label={`${plane.plane} 统计`}>
      <h4>{plane.plane === 'flow' ? 'Flow' : plane.plane === 'inner' ? 'Inner' : plane.plane === 'outer' ? 'Outer' : plane.plane}<span title="实际执行角色">{plane.source_roles.join(' · ') || '—'}</span></h4>
      <dl class="totals">{#each keys as key}<div><dt>{key === 'bytes_down' ? '下载' : key === 'bytes_up' ? '上传' : key === 'rx_bytes' ? 'RX' : 'TX'}</dt><dd title={metric(plane,key)?.toString() ?? '不可用'}>{formatBytes(metric(plane,key))}</dd></div>{/each}</dl>
      <TrafficMiniChart {observation} plane={plane.plane} {stale} />
      <details><summary>指标与口径</summary><p>{plane.accounting_basis}</p><dl class="metrics">{#each Object.keys(plane.counters) as key}<div><dt>{key}</dt><dd>{metric(plane,key)?.toString() ?? '—'}</dd></div>{/each}</dl></details>
    </section>
  {/each}
</div>
<dl class="period"><div><dt>统计周期开始</dt><dd>{when(snapshot.epoch_started_at_unix_ms)}</dd></div><div><dt>本次采样</dt><dd>{when(snapshot.sampled_at_unix_ms)}</dd></div><div><dt>活动流 / 数据报 / Packet</dt><dd>{snapshot.activity.active_stream_flows ?? '—'} / {snapshot.activity.active_datagram_flows ?? '—'} / {snapshot.activity.active_packet_routes ?? '—'}</dd></div></dl>
<details class="identity"><summary>统计标识</summary><dl class="metrics"><div><dt>实例</dt><dd>{snapshot.core_instance_id}</dd></div><div><dt>周期</dt><dd>{snapshot.stats_epoch}</dd></div><div><dt>generation</dt><dd>{snapshot.generation ?? '—'}</dd></div><div><dt>配置版本</dt><dd>{snapshot.config_revision}</dd></div></dl></details>
<style>
  .plane-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,180px),1fr)); gap:12px; } .plane { border:1px solid var(--border); border-radius:9px; padding:14px; min-width:0; } h4 { display:flex; align-items:center; justify-content:space-between; font-weight:600; font-size:13px; gap:6px; } h4 span { color:var(--muted-foreground); font-size:10px; font-weight:400; } .totals { display:grid; grid-template-columns:1fr 1fr; gap:10px; margin:12px 0; } dt { color:var(--muted-foreground); font-size:11px; } dd { font-size:12px; margin:0; overflow-wrap:anywhere; font-variant-numeric:tabular-nums; } .totals dd { font-size:16px; font-weight:600; } summary { cursor:pointer; color:var(--muted-foreground); font-size:11px; margin-top:12px; } .metrics { display:grid; gap:6px; margin-top:10px; } .metrics>div { display:flex; justify-content:space-between; gap:12px; } .metrics dt { overflow-wrap:anywhere; min-width:0; } .metrics dd { text-align:right; } p { font-size:11px; color:var(--muted-foreground); overflow-wrap:anywhere; margin:8px 0; } .period { display:grid; gap:8px; margin:16px 0; } .period>div { display:flex; justify-content:space-between; gap:10px; } .identity { border-top:1px solid var(--border); }
</style>
