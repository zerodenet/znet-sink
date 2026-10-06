<script lang="ts">
  import { RefreshCw, Search, Trash2, Activity, ChevronLeft, ChevronRight, Info } from '@lucide/svelte';
  import { Input } from '$lib/components/ui/input';
  import { Choice } from '$lib/components/ui/choice';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { getAppErrorMessage } from '$lib/services/core';
  import type { TrafficView, TrafficSession } from '$lib/features/traffic/session';
  import { scopeLabel } from '$lib/features/traffic/types';
  import type { ResetInput, TrafficScope } from '$lib/features/traffic/types';
  import { supported, resetReason } from '$lib/features/traffic/policy';
  import { byteKeys, formatBytes, metric } from '$lib/features/traffic/history';
  import TrafficMiniChart from './TrafficMiniChart.svelte';
  import TrafficScopeDetails from './TrafficScopeDetails.svelte';
  let { session, view }: { session: TrafficSession; view: TrafficView } = $props();
  const kinds: {kind: TrafficScope['kind']; label:string}[] = [{kind:'global',label:'全局'},{kind:'inbound',label:'入站'},{kind:'outbound',label:'出站'},{kind:'endpoint',label:'端点'},{kind:'peer',label:'Peer'}];
  let kind = $state<TrafficScope['kind']>('global');
  let plane = $state('flow');
  let query = $state('');
  let offset = $state(0);
  let selected = $state<string[]>([]);
  let plan = $state.raw<ResetInput | null>(null);
  let confirming = $state(false);
  let inspected = $state<string | null>(null);
  let detailOpen = $state(false);
  let localError = $state<unknown>(null);
  const filtered = $derived(view.order.filter(key => view.rows[key].snapshot.scope.kind === kind && JSON.stringify(view.rows[key].snapshot.scope).toLowerCase().includes(query.toLowerCase())));
  const visible = $derived(filtered.slice(offset, offset + 24));
  const planes = $derived([...new Set(['flow','inner','outer',...view.order.flatMap(key=>view.rows[key].snapshot.planes.map(p=>p.plane))])]);
  const maximum = $derived(Math.min(256,view.discovery?.capabilities.trafficStatistics?.maximum_reset_targets ?? 0));
  const selectionReason = $derived(selected.length ? selected.map(key => view.rows[key] ? resetReason(view.discovery,view.rows[key].snapshot) : '统计范围已不存在').find(Boolean) : '先选择统计范围');
  $effect(() => { session.watch(visible); });
  $effect(() => {
    if (view.loading || view.stale) return;
    const remaining = selected.filter(key => !!view.rows[key]);
    if (remaining.length !== selected.length) selected = remaining;
  });
  $effect(() => { if (offset && offset >= filtered.length) offset = 0; });
  function toggle(key:string, checked:boolean) { selected = checked ? [...new Set([...selected,key])].slice(0,maximum) : selected.filter(k=>k!==key); }
  function prepare() {
    try { plan = session.plan(selected); confirming = true; localError = null; }
    catch(error) { localError = error; }
  }
  function confirm() { if (plan) { const submitted = plan; confirming = false; plan = null; void session.reset(submitted); } }
</script>
<section class="statistics" aria-label="流量统计">

  {#if localError || view.error}<p class="error" role="alert">{getAppErrorMessage(localError ?? view.error,'统计操作失败')}</p>{/if}
  {#if view.result}<p class="notice" role="status">{view.result}</p>{/if}
  {#if !supported(view.discovery)}<div class="unsupported-actions"><div class="actions"><Button variant="ghost" size="icon-sm" aria-label="刷新统计" disabled={view.loading || view.resetting} onclick={()=>session.refresh(true)}><RefreshCw size={15} class={view.loading ? 'animate-spin' : ''}/></Button><Button variant="outline" size="sm" aria-label="清空选中统计" disabled={view.loading || view.stale || view.resetting || !!selectionReason || selected.length>maximum} title={selectionReason ?? '清空所选范围的累计统计'} onclick={prepare}><Trash2 size={14}/>{view.resetting ? '等待确认' : '清空统计'}{selected.length ? ` (${selected.length})` : ''}</Button></div></div>{/if}
  {#if view.loading && !view.discovery}<p class="empty">正在读取内核统计能力…</p>
  {:else if view.discovery && !supported(view.discovery)}<p class="empty">当前内核未提供 Traffic Observation V1。升级内核后可查看分范围统计。</p>
  {:else if supported(view.discovery)}
    {#if view.stale}<p class="notice" role="status">{view.loading ? '正在同步统计…' : '采样中断，等待恢复'} · 上次累计值保留，速率暂不可用</p>{/if}
    <div class="toolbar">
      <div class="kinds">
        <SegmentedControl.Root value={kind} onValueChange={value=>{kind=value as TrafficScope['kind'];offset=0;}} aria-label="统计范围" class="flex-wrap max-w-full">
          {#each kinds as item}<SegmentedControl.Item value={item.kind}>{item.label}<span>{view.order.filter(key=>view.rows[key].snapshot.scope.kind===item.kind).length}</span></SegmentedControl.Item>{/each}
        </SegmentedControl.Root>
      </div>
      <div class="tools">
        <label class="search"><Search size={15} aria-hidden="true"/><Input class="pl-8" aria-label="搜索统计范围" placeholder="搜索范围" bind:value={query} oninput={()=>offset=0}/></label>
        <div class="actions"><Button variant="ghost" size="icon-sm" aria-label="刷新统计" disabled={view.loading || view.resetting} onclick={()=>session.refresh(true)}><RefreshCw size={15} class={view.loading ? 'animate-spin' : ''}/></Button><Button variant="outline" size="sm" aria-label="清空选中统计" disabled={view.loading || view.stale || view.resetting || !!selectionReason || selected.length>maximum} title={selectionReason ?? '清空所选范围的累计统计'} onclick={prepare}><Trash2 size={14}/>{view.resetting ? '等待确认' : '清空统计'}{selected.length ? ` (${selected.length})` : ''}</Button></div>
      </div>
    </div>
    <div class="plane-controls">
      <SegmentedControl.Root value={plane} onValueChange={value=>plane=value} aria-label="统计平面">{#each planes as item}<SegmentedControl.Item value={item}>{item==='flow'?'Flow':item==='inner'?'Inner':item==='outer'?'Outer':item}</SegmentedControl.Item>{/each}</SegmentedControl.Root>
      <div class="sampling"><span class:live={view.streaming} class="mode"><span class="status-dot"></span>{view.streaming ? '实时' : '定时查询'}</span><span title="Flow 为逻辑流，Inner 为设备内层，Outer 为载体外层；三个平面独立计量，不相加。"><Info size={14} aria-label="各平面独立统计"/></span></div>
    </div>
    <div class="scope-grid">
      {#each visible as key (key)}
        {@const row=view.rows[key]}
        {@const snapshot=row.snapshot}
        {@const layer=snapshot.planes.find(p=>p.plane===plane)}
        {@const keys=byteKeys(snapshot,plane)}
        {@const reason=resetReason(view.discovery,snapshot)}
        <article class="scope" aria-label={`统计 ${scopeLabel(snapshot.scope)}`}>
          <div class="identity"><label title={reason ?? '选择此统计范围'}><Choice aria-label={`选择 ${scopeLabel(snapshot.scope)}`} checked={selected.includes(key)} disabled={!!reason || view.stale || view.resetting || (!selected.includes(key)&&selected.length>=maximum)} onchange={event=>toggle(key,event.currentTarget.checked)}/><strong>{scopeLabel(snapshot.scope)}</strong></label><Button variant="ghost" size="icon-sm" aria-label={`查看 ${scopeLabel(snapshot.scope)} 统计详情`} onclick={()=>{inspected=key;detailOpen=true;}}><Info size={14}/></Button></div>
          <div class="role">{snapshot.scope.kind==='peer' ? snapshot.scope.endpoint_id : layer?.source_roles.join(' · ') || '—'}</div>
          <dl class="totals">{#each keys as field}<div><dt>{field==='bytes_down'?'下载':field==='bytes_up'?'上传':field==='rx_bytes'?'RX':'TX'}</dt><dd title={metric(layer,field)?.toString() ?? '内核未提供此指标'}>{formatBytes(metric(layer,field))}</dd></div>{/each}</dl>
          <TrafficMiniChart observation={row} {plane} stale={view.stale}/>
          <div class="activity" title="活动流 / 数据报 / Packet 路由，清空累计统计不会修改这些值"><Activity size={12}/>{snapshot.activity.active_stream_flows ?? '—'} / {snapshot.activity.active_datagram_flows ?? '—'} / {snapshot.activity.active_packet_routes ?? '—'}</div>
        </article>
      {:else}<p class="empty">{view.loading ? '正在读取统计…' : '没有匹配的统计范围'}</p>{/each}
    </div>
    {#if filtered.length}
      <footer>
        <span>共 {filtered.length} 个范围{selected.length ? ` · 已选 ${selected.length}` : ''}</span>
        {#if filtered.length>24}<div class="pagination" aria-label="统计分页"><Button variant="ghost" size="icon-sm" aria-label="统计上一页" disabled={offset===0} onclick={()=>offset=Math.max(0,offset-24)}><ChevronLeft size={14}/></Button><span>{Math.floor(offset/24)+1} / {Math.ceil(filtered.length/24)}</span><Button variant="ghost" size="icon-sm" aria-label="统计下一页" disabled={offset+24>=filtered.length} onclick={()=>offset+=24}><ChevronRight size={14}/></Button></div>{/if}
      </footer>
    {/if}
  {/if}
</section>
<Dialog.Root bind:open={confirming}><Dialog.Content><Dialog.Header><Dialog.Title>清空所选统计？</Dialog.Title><Dialog.Description>开始新的统计周期。设备与连接继续运行；其他范围保持原值。</Dialog.Description></Dialog.Header><Dialog.Body><ul class="reset-targets">{#each plan?.targets ?? [] as target}<li><strong>{scopeLabel(target.scope)}</strong><span>{kinds.find(k=>k.kind===target.scope.kind)?.label}</span></li>{/each}</ul><p class="notice">清空这些范围全部可清空的累计指标。</p></Dialog.Body><Dialog.Footer><Button variant="outline" onclick={()=>confirming=false}>取消</Button><Button onclick={confirm}>确认清空</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<Dialog.Root bind:open={detailOpen}><Dialog.Content class="sm:max-w-[720px]"><Dialog.Header><Dialog.Title>{inspected&&view.rows[inspected] ? scopeLabel(view.rows[inspected].snapshot.scope) : '统计详情'}</Dialog.Title><Dialog.Description>独立统计平面 · 当前统计周期</Dialog.Description></Dialog.Header><Dialog.Body>{#if inspected&&view.rows[inspected]}<TrafficScopeDetails observation={view.rows[inspected]} stale={view.stale}/>{:else}<p>此统计范围已不存在</p>{/if}</Dialog.Body></Dialog.Content></Dialog.Root>
<style>
  .statistics { flex:1; overflow:auto; min-width:0; padding:20px; }
  .toolbar,.tools,.actions,.kinds,.plane-controls,.sampling,.mode,.identity,.identity label,.activity,footer,footer>div { display:flex; align-items:center; gap:8px; }
  .toolbar { justify-content:space-between; flex-wrap:wrap; gap:12px; margin-bottom:12px; }
  .kinds { min-width:0; }
  .kinds span { margin-left:6px; font-size:10px; opacity:.7; }
  .tools { flex:1; justify-content:flex-end; min-width:0; }
  .actions { flex-shrink:0; }
  .unsupported-actions { display:flex; justify-content:flex-end; }
  .search { position:relative; display:block; flex:1; min-width:140px; max-width:220px; }
  .search :global(svg) { position:absolute; top:50%; left:10px; transform:translateY(-50%); color:var(--muted-foreground); pointer-events:none; }
  .search :global(input) { min-width:0; width:100%; padding-left:32px; }
  .plane-controls { justify-content:space-between; gap:12px; flex-wrap:wrap; margin-bottom:16px; }
  .sampling { color:var(--muted-foreground); gap:10px; margin-left:auto; }
  .mode { font-size:11px; color:var(--muted-foreground); gap:5px; white-space:nowrap; }
  .mode.live { color:var(--success,#16a34a); }
  .status-dot { width:6px; height:6px; border-radius:50%; background:currentColor; }
  .error { color:var(--destructive); font-size:12px; margin:10px 0; overflow-wrap:anywhere; }
  .notice { font-size:12px; color:var(--muted-foreground); margin:10px 0; }
  .scope-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,255px),1fr)); gap:12px; }
  .scope { border:1px solid var(--border); border-radius:10px; min-width:0; padding:14px; }
  .identity { justify-content:space-between; }
  .identity label { min-width:0; cursor:pointer; }
  strong { font-size:13px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .role { font-size:10px; color:var(--muted-foreground); min-height:15px; overflow-wrap:anywhere; }
  .totals { display:grid; grid-template-columns:1fr 1fr; gap:12px; margin:14px 0 5px; }
  dt { font-size:10px; color:var(--muted-foreground); }
  dd { font-variant-numeric:tabular-nums; font-size:18px; font-weight:600; margin:0; overflow-wrap:anywhere; }
  .activity { font-size:10px; color:var(--muted-foreground); margin-top:10px; }
  footer { margin-top:12px; justify-content:flex-end; flex-wrap:wrap; gap:12px; font-size:11px; color:var(--muted-foreground); }
  footer>span { white-space:nowrap; }
  .pagination { border-left:1px solid var(--border); padding-left:12px; }
  .empty { font-size:12px; color:var(--muted-foreground); padding:30px 0; }
  .reset-targets { max-height:240px; overflow:auto; }
  .reset-targets li { display:flex; justify-content:space-between; gap:12px; padding:8px 0; border-bottom:1px solid var(--border); }
  .reset-targets span { font-size:11px; color:var(--muted-foreground); }
  @media(max-width:640px) {
    .kinds { width:100%; }
    .tools { width:100%; }
    .search { max-width:none; min-width:0; }
  }
  @media(max-width:480px) { .statistics { padding:12px; } }
</style>
