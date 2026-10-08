<script lang="ts">
  import { Network, MoreHorizontal, LoaderCircle, RotateCw, Info, ArrowDownToLine, ArrowUpFromLine, Cable, Boxes, Route, Pin, AlertTriangle, MapPin } from '@lucide/svelte';
  import EndpointObservationView from './EndpointObservationView.svelte';
  import EndpointDetailsView from './EndpointDetailsView.svelte';
  import TrafficMiniChart from './TrafficMiniChart.svelte';
  import TrafficScopeDetails from './TrafficScopeDetails.svelte';
  import type { Observation } from '$lib/features/traffic/history';
  import EndpointTrafficChart from './EndpointTrafficChart.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { directionLabel, endpointStateLabel, operationReason, directionRequiresRestart } from '$lib/features/endpoints/policy';
  import type { EndpointTraffic } from '$lib/features/endpoints/traffic';
  import type { EndpointAction, EndpointCatalog, EndpointDetails, NetworkEndpoint } from '$lib/features/endpoints/types';
  let { endpoint, catalog, disabled, onAction, onInspect, details = null, inspecting = false, traffic, observation, observationAvailable = false, observationStale = false, pending = false, stale = false }: {
    endpoint: NetworkEndpoint; catalog: EndpointCatalog; disabled: boolean; details?: EndpointDetails | null; inspecting?: boolean;
    observation?: Observation; observationAvailable?: boolean; observationStale?: boolean;
    traffic?: EndpointTraffic; pending?: boolean; stale?: boolean;
    onAction: (endpoint: NetworkEndpoint, action: EndpointAction) => void;
    onInspect: (endpoint: NetworkEndpoint) => void;
  } = $props();
  let menuOpen = $state(false);
  let menuElement: HTMLDetailsElement | undefined = $state();
  $effect(() => {
    if (!menuOpen) return;
    const outside = (event: MouseEvent) => { if (event.target instanceof Node && !menuElement?.contains(event.target)) menuOpen = false; };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') menuOpen = false; };
    window.addEventListener('click', outside);
    window.addEventListener('keydown', escape);
    return () => { window.removeEventListener('click', outside); window.removeEventListener('keydown', escape); };
  });
  let detailOpen = $state(false);
  let detailMode = $state<'protocol' | 'observation'>('protocol');
  const stateAction = $derived<EndpointAction>({ operation: 'set_state', enabled: !endpoint.enabled });
  function directionAction(direction: 'inbound' | 'outbound'): EndpointAction {
    return { operation: 'set_directions', directions: { ...endpoint.allowed, [direction]: !endpoint.allowed[direction] } };
  }
  function directionHint(direction: 'inbound' | 'outbound'): string {
    const action = directionAction(direction);
    if (action.operation !== 'set_directions') return '';
    return directionRequiresRestart(catalog, endpoint, action.directions)
      ? '切换会重启此端点并断开现有连接'
      : '点击后直接生效并自动保留';
  }
  const inboundReason = $derived(operationReason(catalog, endpoint, directionAction('inbound')));
  const outboundReason = $derived(operationReason(catalog, endpoint, directionAction('outbound')));
  const stateReason = $derived(operationReason(catalog, endpoint, stateAction));
  const restartReason = $derived(operationReason(catalog, endpoint, { operation: 'restart' }));
  const addresses = $derived(catalog.configuredAddresses?.[endpoint.endpoint_id] ?? []);
  function count(key: string) { return observationAvailable && observation ? observation.snapshot.activity[key] ?? '—' : endpoint.counters[key] == null ? '—' : String(endpoint.counters[key]); }
  function act(action: EndpointAction) { menuOpen = false; onAction(endpoint, action); }
  function showDetail(mode: 'protocol' | 'observation') {
    menuOpen = false; detailMode = mode; detailOpen = true;
    if (mode === 'protocol') onInspect(endpoint);
  }
</script>

<article class="endpoint-card" aria-label={`端点 ${endpoint.tag}`}>
  <header>
    <div class="identity"><div class="endpoint-icon"><Network size={20} /></div><div class="min-w-0"><h3>{endpoint.tag}</h3><span class="protocol">{endpoint.protocol}</span></div></div>
    <div class="header-tools">
      {#if pending}<LoaderCircle size={13} class="animate-spin" aria-label="正在应用" />{/if}
      {#if catalog.localOverrideIds.includes(endpoint.endpoint_id)}<span class="temporary" title="当前配置的本地覆盖；重启和订阅更新后保留"><Pin size={12} />本地</span>{/if}
      <button data-slot="surface-button" class="power-switch" role="switch" aria-checked={endpoint.enabled} aria-label={endpoint.enabled ? '停用端点' : '启用端点'} disabled={disabled || !!stateReason} title={stateReason ?? '自动保留到当前配置；停用会结束现有连接'} onclick={() => act(stateAction)}><span></span></button>
      <details class="options" bind:this={menuElement} bind:open={menuOpen}>
        <summary aria-label={`${endpoint.tag} 更多操作`} title="更多操作"><MoreHorizontal size={18} /></summary>
        <div class="option-menu">
          <Button variant="ghost" size="sm" disabled={disabled || !!restartReason} aria-label="重启端点" title={restartReason ?? '重启会结束现有连接'} onclick={() => act({ operation: 'restart' })}><RotateCw size={14} />重启端点</Button>
          <div class="menu-divider"></div>
          <Button variant="ghost" size="sm" disabled={disabled || inspecting || !endpoint.supported.operations.includes('details')} aria-label="协议详情" onclick={() => showDetail('protocol')}><Network size={14} />协议详情</Button>
          <Button variant="ghost" size="sm" aria-label="观测详情" onclick={() => showDetail('observation')}><Info size={14} />观测详情</Button>
        </div>
      </details>
    </div>
  </header>
  {#if addresses.length}<div class="local-addresses" aria-label="本机隧道 IP" title={`本机隧道 IP（当前配置值）：${addresses.join(' · ')}`}><MapPin size={12}/><span>{addresses.join(' · ')}</span></div>{/if}
  <div class="status-row"><span class="status" class:running={endpoint.state === 'running'} class:failed={endpoint.state === 'failed'}><i></i>{endpointStateLabel(endpoint)}</span>{#if endpoint.health === 'degraded'}<AlertTriangle size={13} aria-label="健康降级" />{/if}<span class="effective" title="内核实际生效方向">{directionLabel(endpoint.effective)}</span></div>
  {#if observationAvailable}
    {#if observation?.snapshot.scope.kind === 'outbound' || observation?.snapshot.scope.kind === 'inbound'}<p class="observation-source" title={`内核声明的${observation.snapshot.scope.kind === 'outbound' ? '出站' : '入站'} ${observation.snapshot.scope.tag} 统计；Inner 为内层流量，与 Flow、Outer 分开计量`}>{observation.snapshot.scope.kind === 'outbound' ? '出站' : '入站'} · Inner</p>{:else if !observation}<p class="observation-source">暂无端点统计快照</p>{/if}
    <TrafficMiniChart {observation} plane="inner" stale={stale || observationStale}/>
  {:else}<EndpointTrafficChart {traffic} {stale} />{/if}
  <div class="connections" aria-label="活动连接">
    <span title="当前活动 TCP 流；不统计 Peer、握手或保活包"><Cable size={13} /><span class="sr-only">流 </span><span class="activity-count">{count('active_stream_flows')}</span></span>
    <span title="当前活动 UDP 流；不统计 WireGuard 外层 UDP 套接字"><Boxes size={13} /><span class="sr-only">数据报 </span><span class="activity-count">{count('active_datagram_flows')}</span></span>
    <span title="当前活动 Packet 路由；— 表示未提供计数，各类计数不合并"><Route size={13} /><span class="sr-only">Packet </span><span class="activity-count">{count('active_packet_routes')}</span></span>
  </div>
  {#if endpoint.last_error}<p role="alert" class="error">{endpoint.last_error.message}</p>{/if}
  <footer>
    <div class="directions" aria-label="端点方向">
      <button data-slot="surface-button" role="checkbox" aria-checked={endpoint.allowed.inbound} class:active={endpoint.allowed.inbound} aria-label={`${endpoint.tag} 允许入站`} title={inboundReason ?? directionHint('inbound')} disabled={!endpoint.supported.directions.inbound || disabled || !!inboundReason} onclick={() => act(directionAction('inbound'))}><ArrowDownToLine size={14} />入站</button>
      <button data-slot="surface-button" role="checkbox" aria-checked={endpoint.allowed.outbound} class:active={endpoint.allowed.outbound} aria-label={`${endpoint.tag} 允许出站`} title={outboundReason ?? directionHint('outbound')} disabled={!endpoint.supported.directions.outbound || disabled || !!outboundReason} onclick={() => act(directionAction('outbound'))}><ArrowUpFromLine size={14} />出站</button>
    </div>
  </footer>
</article>

<Dialog.Root bind:open={detailOpen}>
  <Dialog.Content class="sm:max-w-[640px]">
    <Dialog.Header><Dialog.Title>{endpoint.tag} · {detailMode === 'protocol' ? '协议详情' : '观测详情'}</Dialog.Title><Dialog.Description>{endpoint.protocol}</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      {#if detailMode === 'protocol'}
        <section aria-label="端点协议详情">{#if details}<EndpointDetailsView snapshot={details} />{:else}<p class="save-note">{inspecting ? '正在读取…' : '暂无详情'}</p>{/if}</section>
      {:else}
        {#if observationAvailable}{#if observation}<TrafficScopeDetails {observation} stale={stale || observationStale}/>{:else}<p class="save-note">此端点暂无统计快照</p>{/if}{:else}<EndpointObservationView {endpoint} />{/if}
      {/if}
    </Dialog.Body>
  </Dialog.Content>
</Dialog.Root>

<style>
  .endpoint-card { border:1px solid var(--border); border-radius:12px; padding:18px; background:var(--card); min-width:0; }
  header,.identity,.header-tools,.status-row,.status,.connections,.connections>span,footer,.directions,.directions>button,.temporary { display:flex; align-items:center; }
  header { justify-content:space-between; gap:12px; } .identity { gap:10px; min-width:0; } .endpoint-icon { padding:9px; border-radius:10px; background:var(--muted); color:var(--muted-foreground); }
  h3 { font-size:14px; font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; } .protocol { font-size:11px; color:var(--muted-foreground); } .header-tools { gap:10px; }
  .power-switch { width:32px; height:18px; border-radius:12px; background:var(--input); border:1px solid var(--border); padding:2px; flex-shrink:0; cursor:pointer; }
  .power-switch>span { display:block; width:12px; height:12px; border-radius:50%; background:white; box-shadow:0 1px 2px #0002; transition:transform .15s; }
  .power-switch[aria-checked=true] { background:var(--success,#16a34a); } .power-switch[aria-checked=true]>span { transform:translateX(14px); } .power-switch:disabled { opacity:.45; cursor:default; }
  .power-switch:focus-visible,summary:focus-visible { outline:2px solid var(--ring); outline-offset:3px; } .temporary { gap:3px; font-size:10px; color:var(--muted-foreground); white-space:nowrap; }
  .status-row { gap:8px; margin-top:14px; font-size:11px; color:var(--muted-foreground); } .status { gap:5px; } .status i { width:6px; height:6px; border-radius:50%; background:var(--muted-foreground); } .running i { background:var(--success,#16a34a); } .failed i { background:var(--destructive); } .effective { margin-left:auto; }
  .connections { gap:16px; font-size:11px; color:var(--muted-foreground); margin:1px 0 14px; } .connections>span { gap:5px; font-variant-numeric:tabular-nums; }
  .activity-count { display:inline-block; min-width:3ch; text-align:right; }
  .local-addresses { display:flex; align-items:center; gap:5px; margin-top:10px; color:var(--muted-foreground); font-size:11px; }
  .local-addresses :global(svg) { flex-shrink:0; }
  .local-addresses>span { min-width:0; overflow-wrap:anywhere; font-family:var(--font-mono); }
  .observation-source { font-size:10px; color:var(--muted-foreground); margin-top:8px; }
  footer { border-top:1px solid var(--border); padding-top:12px; justify-content:space-between; gap:10px; } .directions { gap:6px; } .directions>button { position:relative; gap:5px; padding:5px 8px; border-radius:6px; font-size:11px; color:var(--muted-foreground); cursor:pointer; background:var(--muted); }
  .directions>button.active { color:var(--primary); background:color-mix(in srgb,var(--primary) 9%,transparent); } .directions>button:focus-visible { outline:2px solid var(--ring); } .directions>button:disabled { opacity:.45; cursor:default; }
  .options { position:relative; } summary { display:flex; padding:4px; list-style:none; cursor:pointer; color:var(--muted-foreground); border-radius:6px; } summary::-webkit-details-marker { display:none; } summary:hover { background:var(--muted); }
  .option-menu { position:absolute; right:0; top:30px; z-index:20; min-width:165px; display:flex; flex-direction:column; padding:5px; gap:2px; background:var(--popover); border:1px solid var(--border); border-radius:9px; box-shadow:0 6px 20px #0002; }
  .option-menu :global(button) { justify-content:flex-start; gap:8px; } .menu-divider { border-top:1px solid var(--border); margin:3px; }
  .save-note { font-size:12px; line-height:1.6; color:var(--muted-foreground); margin-top:12px; } .error { font-size:11px; color:var(--destructive); overflow-wrap:anywhere; margin-top:8px; }
</style>
