<script lang="ts">
  import { ArrowDown, ArrowUp, ArrowDownToLine, ArrowUpFromLine } from '@lucide/svelte';
  import { endpointStateLabel } from '$lib/features/endpoints/policy';
  import type { NetworkEndpoint } from '$lib/features/endpoints/types';
  let { endpoint }: { endpoint: NetworkEndpoint } = $props();
  function bytes(value: number | null | undefined): string {
    if (value == null || !Number.isFinite(value) || value < 0) return '—';
    if (value < 1024) return `${value} B`;
    const units = ['KB', 'MB', 'GB', 'TB'];
    const exponent = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length);
    return `${(value / 1024 ** exponent).toFixed(1)} ${units[exponent - 1]}`;
  }
  function exact(value: number | null | undefined) { return value == null ? '内核未提供' : `${value.toLocaleString()} 字节`; }
</script>
<section class="observations" aria-label="端点观测详情">
  <div class="status-line">
    <span class="state" class:running={endpoint.state === 'running'} class:failed={endpoint.state === 'failed'}><i></i>{endpointStateLabel(endpoint)}</span>
    <span class="health" class:healthy={endpoint.health === 'healthy'} class:degraded={endpoint.health === 'degraded'}>{endpoint.health === 'unknown' ? '健康未知' : endpoint.health === 'healthy' ? '健康' : '健康降级'}</span>
  </div>
  <div class="traffic-grid">
    {#each [{ label: '内层累计流量', rx: 'inner_rx_bytes', tx: 'inner_tx_bytes' }, { label: '外层累计流量', rx: 'outer_rx_bytes', tx: 'outer_tx_bytes' }] as layer}
      <section class="traffic" aria-label={layer.label}>
        <h4>{layer.label}</h4>
        <div class="transfer down" title={exact(endpoint.counters[layer.rx])}><ArrowDown size={15} /><span class="sr-only">接收 </span><strong>{bytes(endpoint.counters[layer.rx])}</strong></div>
        <div class="transfer up" title={exact(endpoint.counters[layer.tx])}><ArrowUp size={15} /><span class="sr-only">发送 </span><strong>{bytes(endpoint.counters[layer.tx])}</strong></div>
      </section>
    {/each}
  </div>
  <dl class="roles">
    <div><dt><ArrowDownToLine size={13} />入站角色</dt><dd>{#each endpoint.inbound_tags as tag}<span class="role">{tag}</span>{:else}<span class="empty">无</span>{/each}</dd></div>
    <div><dt><ArrowUpFromLine size={13} />出站角色</dt><dd>{#each endpoint.outbound_tags as tag}<span class="role">{tag}</span>{:else}<span class="empty">无</span>{/each}</dd></div>
  </dl>
  <dl class="metadata">
    <div class="resource"><dt>资源 ID</dt><dd class="mono">{endpoint.endpoint_id}</dd></div>
    <div><dt>实例版本</dt><dd>{endpoint.generation ?? '—'}</dd></div>
    <div><dt>意图版本</dt><dd>{endpoint.intent_revision}</dd></div>
    <div class="started"><dt>启动时间</dt><dd>{endpoint.started_at_unix_ms == null ? '—' : new Date(endpoint.started_at_unix_ms).toLocaleString()}</dd></div>
  </dl>
</section>
<style>
  .status-line,.state,dt,.transfer { display:flex; align-items:center; } .status-line { gap:10px; margin-bottom:18px; }
  .state { gap:7px; font-size:13px; font-weight:600; } .state i { width:7px; height:7px; border-radius:50%; background:var(--muted-foreground); } .running i { background:var(--success,#16a34a); } .failed i { background:var(--destructive); }
  .health { font-size:11px; border-radius:5px; padding:3px 7px; background:var(--muted); color:var(--muted-foreground); } .healthy { color:var(--success,#16a34a); } .degraded { color:var(--warning,#d97706); }
  .traffic-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; } .traffic { padding:16px; border:1px solid var(--border); border-radius:9px; min-width:0; }
  h4,dt { font-size:11px; color:var(--muted-foreground); font-weight:400; } h4 { margin-bottom:14px; } .transfer { gap:8px; margin-top:8px; } .transfer strong { font-size:19px; font-weight:600; font-variant-numeric:tabular-nums; }
  .down { color:var(--chart-download,#3b82f6); } .up { color:var(--chart-upload,#16a34a); }
  dl { margin:0; } dt { gap:5px; margin-bottom:7px; } dd { margin:0; font-size:12px; overflow-wrap:anywhere; min-width:0; }
  .roles { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:16px; padding:20px 0; } .roles dd { display:flex; flex-wrap:wrap; gap:5px; } .role { max-width:100%; border-radius:5px; background:var(--muted); padding:4px 7px; overflow-wrap:anywhere; } .empty { color:var(--muted-foreground); }
  .metadata { border-top:1px solid var(--border); padding-top:18px; display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:16px 24px; } .resource,.started { grid-column:1/-1; } .mono { font-family:var(--font-mono,ui-monospace,monospace); }
  @media(max-width:480px) { .traffic { padding:12px; } .transfer strong { font-size:16px; } .roles { grid-template-columns:minmax(0,1fr); } }
</style>
