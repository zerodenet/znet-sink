<script lang="ts">
  import { ChevronRight } from '@lucide/svelte';
  import { Switch } from '$lib/components/ui/switch';
  import type { OverviewModel } from '$lib/components/overview/model';
  import type { OverviewFeedback } from './types';
  import OperationFeedback from './OperationFeedback.svelte';

  let { model, onInspect, onToggle, switchOn, canToggle, switching, stackLabel, stackReady, feedback }: { model: OverviewModel; onInspect: () => void; onToggle: () => void; switchOn: boolean; canToggle: boolean; switching: boolean; stackLabel: string; stackReady: boolean; feedback: OverviewFeedback } = $props();
  const tun = $derived(model.tunSnapshot);
  const failed = $derived(model.tunConfirmed && tun?.enabled && !tun.healthy);
  const healthy = $derived(model.tunConfirmed && tun?.enabled && tun.healthy);
  const egressReady = $derived(model.tunConfirmed && tun?.enabled && tun.ipv4Egress.availability === 'available' && (!tun.dualStack || tun.ipv6Egress.availability === 'available'));
</script>

<section class="feature-card" aria-label="TUN 与网络栈">
  <header><span class="feature-label">高级功能</span><button data-slot="surface-button" onclick={onInspect} aria-label="查看 TUN 接管详情">详情<ChevronRight size={11}/></button></header>
  <div class="feature-row">
    <span class="feature-dot" class:healthy class:failed></span>
    <div class="feature-copy"><div class="feature-main"><span class="feature-name">TUN 网卡</span><strong class:danger={failed}>{switching ? '切换中…' : model.tunLabel}</strong></div><span class="feature-meta">{model.tunConfirmed && tun?.enabled ? `${tun.name} · ${tun.autoRoute ? '自动路由' : '手动路由'}` : '接管系统网络流量'}</span></div>
    <Switch bind:checked={() => switchOn, onToggle} disabled={!canToggle} aria-label={switchOn ? '关闭 TUN 并取消自动恢复' : '开启 TUN'} />
  </div>
  <div class="feature-row"><span class="feature-dot" class:healthy={stackReady}></span><span class="feature-name">内核网络栈</span><strong>{stackLabel}</strong></div>
  <div class="egress-summary" class:danger={failed}>{!model.tunConfirmed ? '等待内核确认接管状态' : !tun?.enabled ? 'TUN 未接管流量' : failed ? '出口异常 · 下方可查看原因' : egressReady ? `${tun.dualStack ? 'IPv4 / IPv6' : 'IPv4'} 出口可用` : '出口状态待确认'}</div>
  <OperationFeedback {feedback} target="tun" />
</section>

<style>
  .feature-card { display:flex; flex-direction:column; gap:9px; min-width:0; min-height:96px; padding:11px 13px; background:var(--card); border:1px solid var(--border); border-radius:10px; box-shadow:0 1px 2px rgba(0,0,0,.04); transition:box-shadow .15s,transform .15s; }
  .feature-card:hover { box-shadow:0 2px 6px rgba(0,0,0,.07); transform:translateY(-.5px); }
  header { display:flex; align-items:center; justify-content:space-between; gap:8px; }.feature-label { font-size:12px; font-weight:500; color:var(--muted-foreground); }
  header button { display:flex; align-items:center; gap:2px; font-size:11px; color:var(--muted-foreground); cursor:pointer; }
  .feature-row { display:flex; align-items:center; gap:7px; min-width:0; }.feature-dot { width:7px; height:7px; flex-shrink:0; border-radius:50%; background:var(--muted-foreground); }.feature-dot.healthy { background:var(--success); }.feature-dot.failed { background:var(--destructive); }
  .feature-copy { min-width:0; flex:1; display:flex; flex-direction:column; gap:2px; }.feature-main { display:flex; align-items:center; flex-wrap:wrap; gap:4px 8px; }.feature-name { min-width:68px; color:var(--muted-foreground); font-size:11.5px; font-weight:500; }strong { font-size:11.5px; font-weight:600; }
  .feature-meta { color:var(--muted-foreground); font-size:10px; font-family:var(--font-mono); }.egress-summary { margin-top:auto; padding-top:8px; border-top:1px solid var(--border); color:var(--muted-foreground); font-size:11px; }.danger { color:var(--destructive); }
  button:focus-visible { outline:2px solid var(--ring); outline-offset:3px; }
</style>
