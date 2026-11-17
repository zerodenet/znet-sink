<script lang="ts">
  import { Pencil, Trash2 } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Switch } from '$lib/components/ui/switch';
  import type { RuleSetSummary } from '$lib/types/domain';
  let {items,endpoint='',busy=false,onEdit,onToggle,onRemove}: {items:RuleSetSummary[];endpoint?:string;busy?:boolean;onEdit:(item:RuleSetSummary)=>void;onToggle:(item:RuleSetSummary)=>void;onRemove:(item:RuleSetSummary)=>void}=$props();
  const rows=$derived(items.filter(i=>typeof i.commonBinding?.action==='object' && (!endpoint || i.commonBinding.action.endpoint===endpoint)).sort((a,b)=>(a.commonBinding?.order??0)-(b.commonBinding?.order??0)||a.id.localeCompare(b.id)));
  function condition(item:RuleSetSummary){const suffix=` → ${target(item)}`;return item.name.endsWith(suffix)?item.name.slice(0,-suffix.length):item.name;}
  function target(item:RuleSetSummary){return typeof item.commonBinding?.action==='object'?item.commonBinding.action.endpoint:'';}
</script>
<div class="routes-list" role="region" aria-label={endpoint ? `${endpoint} 路由规则列表` : '路由规则列表'}>
  {#if !rows.length}
    <div class="empty">{endpoint ? '还没有使用此端点的路由规则' : '还没有客户端端点路由规则'}</div>
  {:else}
    <div class="columns column-head" aria-hidden="true"><span>匹配条件</span><span>出站端点</span><span>顺序</span><span>启用</span><span></span></div>
    {#each rows as item (item.id)}
      <div class="columns route-row">
        <Button variant="ghost" class="rule-name text-foreground" data-route-id={item.id} aria-label={item.name} onclick={()=>onEdit(item)} disabled={busy} title={item.name}>{condition(item)}</Button>
        <span class="target">{target(item)}</span><span class="order">{item.commonBinding?.order??0}</span>
        <Switch size="sm" checked={item.commonBinding?.enabled??false} disabled={busy} onCheckedChange={()=>onToggle(item)} aria-label={`启用路由 ${item.name}`}/>
        <div class="actions"><Button variant="ghost" size="icon-sm" disabled={busy} onclick={()=>onEdit(item)} aria-label={`编辑路由 ${item.name}`}><Pencil size={14}/></Button>{#if !item.builtIn && !item.source}<Button variant="ghost" size="icon-sm" disabled={busy} onclick={()=>onRemove(item)} aria-label={`删除路由 ${item.name}`}><Trash2 size={14}/></Button>{/if}</div>
        <span class="scope">{item.commonBinding?.profileId?'指定配置':'所有配置'}{item.builtIn||item.source?' · 引用规则集':''}</span>
      </div>
    {/each}
  {/if}
</div>
<style>
  .routes-list {min-height:0;overflow:auto;flex:1;}
  .columns {display:grid;grid-template-columns:minmax(130px,1fr) minmax(80px,120px) 56px 40px 64px;gap:8px;align-items:center;padding:8px 14px;}
  .column-head {font-size:11px;color:var(--muted-foreground);border-bottom:1px solid var(--border);}
  .route-row {border-bottom:1px solid var(--border);font-size:12px;}
  .routes-list :global(.rule-name) {justify-content:flex-start;min-width:0;height:auto;white-space:normal;text-align:left;overflow-wrap:anywhere;padding:0;}
  .target {overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}
  .order {font-variant-numeric:tabular-nums;}
  .actions {display:flex;}
  .scope {grid-column:1/-1;font-size:11px;color:var(--muted-foreground);}
  .empty {padding:40px 16px;text-align:center;font-size:12px;color:var(--muted-foreground);}
  @media(max-width:560px){.columns {grid-template-columns:minmax(110px,1fr) 80px 40px 32px 56px;gap:4px;padding:8px;}}
</style>
