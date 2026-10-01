<script lang="ts">
  import { KeyRound, Network, Clock3, ShieldCheck, ChevronRight } from '@lucide/svelte';
  import type { EndpointDetails } from '$lib/features/endpoints/types';
  let { snapshot }: { snapshot: EndpointDetails } = $props();
  type Peer = {
    peer_id: string | null; public_key: string; allowed_ips: string[];
    configured_endpoint: string | null; authenticated_endpoint: string | null;
    source_known: boolean | null; last_authenticated_packet_age_ms?: number | null;
    health?: { state: string; last_handshake_age_ms?: number | null } | null;
  };
  const peers = $derived.by<Peer[] | null>(() => {
    if (snapshot.schema_id !== 'zero.endpoint.wireguard.v1' || snapshot.schema_version !== 1) return null;
    const data = snapshot.details as { peers?: Peer[] } | null;
    return Array.isArray(data?.peers) ? data.peers : null;
  });
  function age(value: number | null | undefined) { return value == null ? '—' : `${(value / 1000).toFixed(1)} 秒前`; }
</script>
{#if peers}
  {#each peers as peer, index}
    <section class="peer" aria-label={`Peer ${index + 1}`}>
      <div class="peer-heading">
        <span class="peer-icon"><Network size={17} /></span>
        <h4>{peer.peer_id ?? `Peer ${index + 1}`}</h4>
        <span class="health" title="内核报告的健康事实">{peer.health?.state === 'healthy' ? '健康' : peer.health?.state === 'degraded' ? '降级' : peer.health?.state ?? '健康未知'}</span>
      </div>
      <dl class="peer-fields">
        <div class="key-field"><dt><KeyRound size={13} />公钥</dt><dd class="mono key">{peer.public_key}</dd></div>
        <div><dt>配置远端</dt><dd class="mono">{peer.configured_endpoint ?? '未配置'}</dd></div>
        <div><dt>认证远端</dt><dd class="mono" class:unknown={peer.authenticated_endpoint == null}>{peer.authenticated_endpoint ?? '未提供'}</dd></div>
        <div class="networks"><dt>允许网段</dt><dd>{#each peer.allowed_ips as ip}<span class="network mono">{ip}</span>{:else}<span class="unknown">无</span>{/each}</dd></div>
      </dl>
      <dl class="peer-facts">
        <div><dt><Clock3 size={13} />最近握手</dt><dd title={peer.health?.last_handshake_age_ms == null ? '内核未提供' : undefined}>{age(peer.health?.last_handshake_age_ms)}</dd></div>
        <div><dt><Clock3 size={13} />认证报文</dt><dd title={peer.last_authenticated_packet_age_ms == null ? '内核未提供' : undefined}>{age(peer.last_authenticated_packet_age_ms)}</dd></div>
        <div><dt><ShieldCheck size={13} />来源已知</dt><dd title={peer.source_known == null ? '内核未提供' : undefined}>{peer.source_known == null ? '—' : peer.source_known ? '是' : '否'}</dd></div>
      </dl>
    </section>
  {/each}
{/if}
<details class="raw-data"><summary><ChevronRight size={14} />公开协议数据</summary><div class="schema">{snapshot.schema_id} · v{snapshot.schema_version}</div><pre>{JSON.stringify(snapshot.details, null, 2)}</pre></details>
<style>
  .peer { border:1px solid var(--border); border-radius:10px; padding:18px; margin-bottom:14px; min-width:0; }
  .peer-heading { display:flex; align-items:center; gap:9px; margin-bottom:18px; min-width:0; }
  .peer-icon { display:flex; color:var(--muted-foreground); padding:7px; background:var(--muted); border-radius:8px; }
  h4 { font-size:14px; font-weight:600; overflow-wrap:anywhere; min-width:0; }
  .health { margin-left:auto; font-size:11px; color:var(--muted-foreground); white-space:nowrap; }
  dl { margin:0; } dt { display:flex; align-items:center; gap:5px; font-size:11px; color:var(--muted-foreground); margin-bottom:7px; }
  dd { margin:0; font-size:13px; overflow-wrap:anywhere; min-width:0; }
  .peer-fields { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:18px 24px; }
  .key-field,.networks { grid-column:1/-1; }
  .mono { font-family:var(--font-mono,ui-monospace,monospace); font-size:12px; }
  .key { padding:10px 12px; background:var(--muted); border-radius:6px; line-height:1.6; user-select:all; }
  .unknown { color:var(--muted-foreground); }
  .networks dd { display:flex; flex-wrap:wrap; gap:6px; }
  .network { padding:4px 8px; border:1px solid var(--border); border-radius:5px; max-width:100%; overflow-wrap:anywhere; }
  .peer-facts { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; border-top:1px solid var(--border); padding-top:16px; margin-top:18px; }
  .peer-facts dd { font-variant-numeric:tabular-nums; }
  summary { display:flex; align-items:center; gap:6px; list-style:none; cursor:pointer; font-size:12px; color:var(--muted-foreground); padding:4px 0; }
  summary::-webkit-details-marker { display:none; } .raw-data[open] :global(summary svg) { transform:rotate(90deg); }
  .schema { color:var(--muted-foreground); font-size:11px; overflow-wrap:anywhere; margin-top:10px; }
  pre { max-height:280px; overflow:auto; font-size:11px; background:var(--muted); border-radius:8px; padding:12px; margin-top:8px; }
  @media(max-width:480px) { .peer { padding:14px; } .peer-fields { grid-template-columns:minmax(0,1fr); gap:14px; } .peer-facts { gap:8px; } .peer-facts dt { font-size:10px; } .peer-facts dt :global(svg) { display:none; } }
</style>
