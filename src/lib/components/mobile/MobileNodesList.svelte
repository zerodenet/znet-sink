<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Input } from '$lib/components/ui/input';
  import { Choice } from '$lib/components/ui/choice';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import type { PolicyGroup, ProbeJobSnapshot, ProbeObservation } from '$lib/types/gui-api';
  import type { ProxyNode } from '$lib/types/protocol';
  import { mobileGroups, ungroupedNodes } from '$lib/features/nodes/mobile-view';
  import { nodesDisplayPreferences } from '$lib/components/tabs/nodes-display-preferences.svelte';
  import { resolveEffectiveNodeSelection } from '$lib/components/tabs/nodes-view-model';
  import { formatDelay } from '$lib/services/node-utils';
  let { groups, nodes, query = $bindable(''), scopeKey, locationTags = [], ready, busy, probingTags, jobs, error, loading, onSelect, onProbe, onProbeGroup, onStop, onRetry, stopping = false, stopRequested = false, draining = null, drainingError = null, lastJob = null, historyForTag = () => [] } : {
    groups: PolicyGroup[]; nodes: ProxyNode[]; query?: string; scopeKey: string; locationTags?: string[]; ready: boolean; busy: boolean;
    probingTags: ReadonlySet<string>; jobs: ProbeJobSnapshot[]; error: string | null; loading: boolean; stopping?: boolean; stopRequested?: boolean; draining?: number | null; drainingError?: string | null; lastJob?: ProbeJobSnapshot | null; historyForTag?: (tag: string) => ProbeObservation[];
    onSelect: (node: ProxyNode, group: string) => void | Promise<void>;
    onProbe?: (node: ProxyNode) => void | Promise<void>;
    onProbeGroup?: (group: PolicyGroup) => void | Promise<void>;
    onStop?: () => void | Promise<void>; onRetry?: () => void | Promise<void>;
  } = $props();
  nodesDisplayPreferences.ensureLoaded();
  let expanded = $state<Set<string>>(new Set());
  let expandedScope = $state('');
  let detail = $state<ProxyNode | null>(null);
  let detailOpen = $state(false);
  let container:HTMLElement;
  let locateFrame=0;
  let locatedKey='';
  const revealed=$derived(new Set(locationTags));
  onDestroy(()=>cancelAnimationFrame(locateFrame));
  $effect(() => { if (scopeKey !== expandedScope) { expandedScope=scopeKey; expanded=new Set(groups.slice(0,1).map(group=>group.name)); detailOpen=false; detail=null; } });
  const visibleGroups = $derived(mobileGroups(groups, nodes, query, revealed));
  const independent = $derived(ungroupedNodes(groups, nodes, query, revealed));
  $effect(()=>{const key=JSON.stringify([scopeKey,locationTags]);if(!container || !locationTags.length || key===locatedKey)return;locatedKey=key;const root=locationTags[0];locateFrame=requestAnimationFrame(()=>{const row=[...container.querySelectorAll<HTMLElement>('[data-group-tag], [data-node-tag]')].find(element=>element.dataset.groupTag===root || element.dataset.nodeTag===root);row?.scrollIntoView({block:'nearest'});});});
  function toggle(tag: string) { const next=new Set(expanded); if (next.has(tag)) next.delete(tag); else next.add(tag); expanded=next; }
  function inspect(node: ProxyNode) { detail=node; detailOpen=true; }
  function locate(tag: string) { expanded=new Set([...expanded,tag]); query='';requestAnimationFrame(()=>[...container.querySelectorAll<HTMLElement>('[data-group-tag]')].find(element=>element.dataset.groupTag===tag)?.scrollIntoView({block:'start'})); }
  function selectionLabel(group: PolicyGroup): string {
    const selection=resolveEffectiveNodeSelection(groups,group.name);
    return selection.leafTag ? [...selection.groupPath.slice(1),selection.leafTag].join(' → ') : group.selected ?? '未确认';
  }
</script>
<section class="mobile-nodes" aria-label="全部节点组" bind:this={container}>
  {#if locationTags.length}<p class="note" role="status">正在定位概览出口；临时显示本次目标，不改变搜索与离线过滤偏好。</p>{/if}
  <Input aria-label="搜索组和节点" placeholder="搜索组和节点" bind:value={query}/>
  <div class="filters"><label><Choice checked={nodesDisplayPreferences.hideTimeout} onchange={event=>nodesDisplayPreferences.setHideTimeout(event.currentTarget.checked)}/>隐藏已确认离线</label><label><Choice checked={nodesDisplayPreferences.sortByDelay} onchange={event=>nodesDisplayPreferences.setSortByDelay(event.currentTarget.checked)}/>自动组按延迟排序</label></div>
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if !ready}<p class="note">{loading ? '正在读取节点…' : '内核未就绪，当前显示本地或上次观测；运行操作暂不可用。'}</p>{/if}
  {#if jobs.length}<div class="tasks" role="status">{#each jobs as job}<p>测速任务 #{job.id}：{job.completed}/{job.targetTags.length}，失败 {job.failed}</p>{/each}{#if onStop}<Button disabled={stopping} onclick={onStop}>{stopping ? '停止后等待在途结果' : '停止排队并等待在途'}</Button>{/if}</div>{/if}
  {#if stopRequested || (draining ?? 0)>0}<p class="note" role="status">{draining===null ? '已请求停止排队，在途结果尚未确认。' : draining>0 ? `停止后等待在途结果：${draining} 项。内核没有强制取消接口。` : '后台已确认没有等待中的在途请求。'}{drainingError ? ` ${drainingError}` : ''}</p>{/if}
  {#if lastJob && lastJob.state!=='running'}<details class="tasks"><summary>任务 #{lastJob.id} · {lastJob.state==='completed' ? '完成' : lastJob.state==='partially_failed' ? '部分失败' : lastJob.state==='timed_out' ? '超时' : lastJob.state==='cancelled' ? '已停止排队' : lastJob.state==='failed' ? '失败' : '已失效'} · {lastJob.completed}/{lastJob.targetTags.length}</summary>{#each lastJob.results as result}<p>{result.targetTag}：{result.reachable ? result.latencyMs==null ? '可达，延迟未提供' : `${result.latencyMs} ms` : result.message ?? '探测失败'}</p>{/each}</details>{/if}
  {#if onRetry}<Button variant="outline" disabled={!ready || busy} onclick={onRetry}>仅重试失败目标</Button>{/if}
  {#each visibleGroups as {group,members} (group.name)}
    {@const open=!!query.trim() || expanded.has(group.name) || revealed.has(group.name)}
    <article class="group" data-group-tag={group.name} class:location-target={revealed.has(group.name)} id={`mobile-group-${encodeURIComponent(group.name)}`}>
      <header><button data-slot="surface-button" class="group-toggle" aria-expanded={open} onclick={()=>toggle(group.name)}><strong>{group.name}</strong><span>{group.kind ?? '未知类型'} · {selectionLabel(group)}</span></button>{#if onProbeGroup}<Button variant="ghost" disabled={!ready || busy || group.outbounds.length===0 || group.outbounds.some(member=>probingTags.has(member.tag))} onclick={()=>onProbeGroup?.(group)} aria-label={`测速组 ${group.name}`}>本组测速</Button>{/if}</header>
      {#if open}<div role="list" aria-label={`${group.name} 成员`}>
        {#each members as node (node.tag)}
          {@const child=groups.find(candidate=>candidate.name===node.tag)}
          {@const selectable=group.kind?.toLowerCase()==='selector'}
          <div class="member" role="listitem" data-node-tag={node.tag} class:location-target={revealed.has(node.tag)}><button data-slot="surface-button" class="member-main" aria-label={`${selectable ? '选择' : '查看'} ${node.name}`} aria-pressed={selectable ? group.selected===node.tag : undefined} disabled={selectable && (!ready || busy || group.available===false)} title={group.reason} onclick={()=>selectable ? onSelect(node,group.name) : inspect(node)}><strong>{group.selected===node.tag ? '✓ ' : ''}{node.cleanName || node.name}</strong><span>{child?.kind ?? node.protocol} · {node.alive===false ? '离线' : formatDelay(node.delay)}{node.delay>0 && node.delay<1000 ? ' ms' : ''}</span></button><div class="member-actions">{#if child}<Button variant="ghost" onclick={()=>locate(child.name)}>查看组</Button>{:else}<Button variant="ghost" onclick={()=>inspect(node)}>详情</Button>{/if}{#if onProbe}<Button variant="ghost" disabled={!ready || probingTags.has(node.tag)} onclick={()=>onProbe?.(node)} aria-label={`测速节点 ${node.name}`}>{probingTags.has(node.tag) ? '测速中' : '测速'}</Button>{/if}</div></div>
        {:else}<p class="note">{query ? '无匹配成员' : '此组没有可显示成员'}</p>{/each}
      </div>{/if}
    </article>
  {/each}
  {#if independent.length}<article class="group"><h2>未归组节点</h2>{#each independent as node (node.tag)}<div class="member" data-node-tag={node.tag} class:location-target={revealed.has(node.tag)}><button data-slot="surface-button" class="member-main" onclick={()=>inspect(node)}><strong>{node.cleanName || node.name}</strong><span>{node.protocol} · {formatDelay(node.delay)}</span></button>{#if onProbe}<Button variant="ghost" disabled={!ready || probingTags.has(node.tag)} onclick={()=>onProbe?.(node)}>测速</Button>{/if}</div>{/each}</article>{/if}
  {#if !visibleGroups.length && !independent.length}<p class="note">{loading ? '正在读取节点…' : query ? '没有匹配的组或节点' : groups.length || nodes.length ? '当前筛选没有结果' : '当前配置没有节点；可在配置页添加本地配置或订阅。'}</p>{/if}
</section>
<Dialog.Root bind:open={detailOpen}><Dialog.Content><Dialog.Header><Dialog.Title>{detail?.name ?? '节点详情'}</Dialog.Title><Dialog.Description>当前配置公开信息与实际观测</Dialog.Description></Dialog.Header><Dialog.Body>{#if detail}<dl><dt>协议</dt><dd>{detail.protocol}</dd><dt>服务器</dt><dd>{detail.server ?? '未提供'}</dd><dt>端口</dt><dd>{detail.port ?? '未提供'}</dd><dt>延迟</dt><dd>{formatDelay(detail.delay)}{detail.delay>0 && detail.delay<1000 ? ' ms' : ''}</dd><dt>测量时间</dt><dd>{detail.lastProbeAt ? new Date(detail.lastProbeAt).toLocaleString() : '未测速'}</dd></dl><h2>近期测速记录</h2>{#each historyForTag(detail.tag).slice(-12).reverse() as entry}<p class="note">{new Date(entry.observedAtUnixMs).toLocaleString()} · {entry.reachable ? entry.latencyMs==null ? '可达，延迟未提供' : `${entry.latencyMs} ms` : entry.message ?? '探测失败'}</p>{:else}<p class="note">暂无记录</p>{/each}{/if}</Dialog.Body></Dialog.Content></Dialog.Root>
<style>
 .mobile-nodes { min-width:0; padding:12px; overflow:auto; height:100%; }
 .filters { display:flex; flex-wrap:wrap; gap:4px 12px; font-size:12px; color:var(--muted-foreground); }
 .filters label { min-height:44px; display:flex; align-items:center; gap:6px; }
 .group { background:var(--card); border:1px solid var(--border); border-radius:11px; margin:10px 0; overflow:hidden; }
 .location-target { outline:2px solid var(--ring); outline-offset:-2px; }
 header,.member,.member-actions { display:flex; align-items:center; gap:4px; }
 header { padding:8px; }
 .group-toggle,.member-main { flex:1; min-width:0; text-align:left; display:flex; flex-direction:column; gap:4px; }
 .group-toggle { padding:8px; min-height:54px; }
 strong,dd { overflow-wrap:anywhere; }
 span,.note { font-size:12px; color:var(--muted-foreground); }
 .member { border-top:1px solid var(--border); padding:4px 8px; }
 .member-main { min-height:60px; padding:8px; }
 .member-actions { flex-wrap:wrap; justify-content:flex-end; flex:0 0 88px; }
 .mobile-nodes :global(button) { min-height:44px; min-width:44px; }
 button { padding:6px; border-radius:8px; }
 button:focus-visible { outline:2px solid var(--ring); }
 button:disabled { opacity:.5; }
 .member-main[aria-pressed=true] { color:var(--primary); }
 .member-actions :global(button) { font-size:12px; color:var(--primary); }
 h2,.note { margin:12px; }
 .error { color:var(--destructive); overflow-wrap:anywhere; }
 summary { min-height:44px; display:flex; align-items:center; cursor:pointer; }
 .tasks { padding:10px; background:var(--muted); border-radius:9px; font-size:12px; }
 dl { display:grid; grid-template-columns:80px minmax(0,1fr); gap:10px; } dt { color:var(--muted-foreground); } dd { margin:0; }
</style>
