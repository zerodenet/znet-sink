<script lang="ts">
  import EndpointRouteList from './EndpointRouteList.svelte';
  import {tick} from 'svelte';
  import {RULE_MATCH_TYPES} from '$lib/constants/rule-match-types';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import { loadUserEndpoints } from '$lib/features/endpoints/library';
  import { endpointGateway } from '$lib/features/endpoints/client';
  import { Route } from '@lucide/svelte';
  import { success as toastSuccess } from '$lib/services/toast.svelte';
  import { endpointRuleLines } from '$lib/features/endpoints/route-rules';
  import { getAppErrorMessage, getProfileSettings } from '$lib/services/core';
  import { getRuleSet, listRuleSets, upsertRuleSet, setCommonRuleBinding, removeRuleSet } from '$lib/services/config';
  import type { RuleSetSummary, ZeroRuleType } from '$lib/types/domain';
  let { onChanged = async () => {}, endpoint = '', showTrigger = true, manageTrigger = false, disabled = false, busy = $bindable(false) }: { onChanged?: () => void | Promise<void>; endpoint?: string; showTrigger?: boolean; manageTrigger?:boolean; disabled?: boolean; busy?: boolean } = $props();
  let tags = $state<{value:string;label:string}[]>([]);
  let ownedTags = $state<string[]>([]);
  let open = $state(false);
  let error = $state('');
  let selected = $state('');
  let type = $state<ZeroRuleType>('domain_suffix');
  let text = $state('');
  let draftId = $state<string>();
  let editingScopeId = $state<string>();
  let order = $state(0);
  let scope = $state('global');
  let view = $state<'list'|'edit'|'delete'>('edit');
  let fromList = $state(false);
  let items = $state<RuleSetSummary[]>([]);
  let listRevision=$state(0);
  let pendingDelete = $state<RuleSetSummary>();
  let bindingOnly = $state<RuleSetSummary>();
  let editingEnabled=$state(true);
  const fieldsId=$props.id();
  async function focusList(){await tick();const buttons=Array.from(document.getElementById(`${fieldsId}-list`)?.querySelectorAll<HTMLElement>('button[data-route-id]')??[]);(buttons.find(button=>button.dataset.routeId===draftId)??buttons[0]??document.getElementById(`${fieldsId}-close`))?.focus();}
  async function focusFields(){await tick();document.getElementById(fieldsId)?.querySelector<HTMLElement>('button:not([disabled]),input:not([readonly]):not([disabled]),textarea:not([disabled])')?.focus();}
  async function refresh() {
    try {
      const [library, catalog] = await Promise.all([loadUserEndpoints(), endpointGateway.catalog().catch(() => null)]);
      ownedTags = library.endpoints.map(e=>e.tag);
      const configured = library.endpoints.filter(e => e.directions?.outbound !== false);
      const names = [...new Set([...configured.map(e=>e.tag), ...(catalog?.endpoints ?? []).filter(e=>e.supported.directions.outbound).map(e=>e.tag)])];
      tags = names.map(tag => {
        const runtime = catalog?.endpoints.find(e=>e.tag===tag);
        const saved = configured.find(e=>e.tag===tag);
        return {value:tag,label:`${tag} · ${runtime ? runtime.enabled ? runtime.state === 'running' ? '运行中' : runtime.state : '已关闭' : saved?.enabled === false ? '已关闭' : '状态未确认'}`};
      });
    } catch (cause) { error = getAppErrorMessage(cause, '读取端点路由失败'); }
  }
  export async function manage() {
    if (busy || disabled) return;
    open=true;view='list';fromList=true;busy=true;error='';
    try {items=await listRuleSets();} catch(cause){error=getAppErrorMessage(cause,'读取路由规则失败');} finally{busy=false;await focusList();}
  }
  async function toggle(item:RuleSetSummary){
    if(!item.commonBinding || busy)return;
    busy=true;error='';
    try {await setCommonRuleBinding({ruleSetId:item.id,...item.commonBinding,enabled:!item.commonBinding.enabled});items=await listRuleSets();await onChanged();}
    catch(cause){error=getAppErrorMessage(cause,'更新路由规则失败');listRevision++;}finally{busy=false;if(error)await focusList();}
  }
  async function remove(){
    if(!pendingDelete || busy)return;
    busy=true;error='';
    try {await removeRuleSet(pendingDelete.id);items=await listRuleSets();view='list';pendingDelete=undefined;await onChanged();}
    catch(cause){error=getAppErrorMessage(cause,'删除路由规则失败');}finally{busy=false;if(view==='list')await focusList();}
  }
  function cancel(){if(busy)return;if(fromList && view!=='list'){view='list';error='';void focusList();}else open=false;}
  export async function create(listEntry=false) {
    if (busy || disabled) return;
    view='edit';fromList=listEntry;bindingOnly=undefined;editingEnabled=true;type='domain_suffix';
    draftId=undefined;editingScopeId=undefined;order=0;text='';error='';selected='';open=true;busy=true;
    try {
      await refresh();
      selected=endpoint || tags[0]?.value || '';
      scope=ownedTags.includes(selected)?'global':'current';
    } finally {busy=false;await focusFields();}
  }
  export async function edit(item: RuleSetSummary, listEntry=false) {
    if (busy || disabled) return;
    busy=true;error='';open=true;view='edit';fromList=listEntry;bindingOnly=undefined;
    try {
      draftId=undefined;selected='';text='';
      await refresh();
      const isReference=item.builtIn || !!item.source || item.editableRuleCount>1000;
      const rows=isReference?[]:(await getRuleSet(item.id)).semanticIr.rules;
      if (isReference || !rows.length || rows.some(r=>r.type!==rows[0].type)) bindingOnly=item;
      const action = item.commonBinding?.action;
      if (!action || typeof action !== 'object') {open=false;return false;}
      editingEnabled=item.commonBinding?.enabled??true;draftId=item.id;selected=action.endpoint;type=rows[0]?.type??'domain_suffix';text=rows.map(r=>r.value).join('\n');
      editingScopeId=item.commonBinding?.profileId;scope=editingScopeId?'current':'global';order=item.commonBinding?.order ?? 0;open=true;return true;
    } catch(cause) {error=getAppErrorMessage(cause,'读取路由规则失败');return true;}
    finally {busy=false;await focusFields();}
  }
  async function save() {
    busy=true;error='';
    try {
      if (!selected) throw new Error('请选择一个出站端点');
      const rules = bindingOnly?[]:endpointRuleLines(text,type);
      const name = `${rules[0]?.value}${rules.length > 1 ? ` 等 ${rules.length} 项` : ''} → ${selected}`;
      const profileId = scope === 'current' ? editingScopeId ?? (await getProfileSettings()).profileId : undefined;
      const draft = bindingOnly ? await setCommonRuleBinding({ruleSetId:bindingOnly.id, enabled:editingEnabled, action:{endpoint:selected},order:Math.max(0,Math.trunc(order)),profileId}) : await upsertRuleSet({id:draftId,name,enabled:true,semanticIr:{version:1,name,rules},commonBinding:{enabled:editingEnabled,action:{endpoint:selected},order:Math.max(0,Math.trunc(order)),profileId}});
      draftId = draft.id;
      if(fromList){items=await listRuleSets();view='list';}else open=false;toastSuccess('路由规则已保存；内核运行时已应用。');
      await refresh();await onChanged();
    } catch (cause) {
      error = getAppErrorMessage(cause,'路由修改未确认应用，当前输入已保留');
    } finally { busy=false;if(view==='list')await focusList(); }
  }
</script>
{#if showTrigger}
<Button variant="outline" size="sm" onclick={event=>{if(manageTrigger){const menu=event.currentTarget.closest('details');menu?.removeAttribute('open');menu?.querySelector('summary')?.focus();void manage();}else void create();}} disabled={busy || disabled}>
  <Route class="h-3.5 w-3.5" />{manageTrigger?'路由规则':'添加路由'}
</Button>
{/if}
<DraggableModal title={view==='list' ? `${endpoint} · 路由规则` : view==='delete' ? '删除路由规则' : draftId ? '编辑路由规则' : '添加路由规则'} description={view==='list'?'管理使用此端点的分流规则。':undefined} {open} onClose={cancel} closeDisabled={busy} width="min(680px, 92vw)">
  {#if view==='list'}
    <div id={`${fieldsId}-list`}>{#key listRevision}<EndpointRouteList {items} {endpoint} {busy} onEdit={item=>void edit(item,true)} onToggle={item=>void toggle(item)} onRemove={item=>{pendingDelete=item;view='delete';error='';}}/>{/key}</div>
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  {:else if view==='delete'}
    <p>删除“{pendingDelete?.name}”？端点配置将保留。</p>
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  {:else}
  <div class="editor-fields" id={fieldsId}>
    <div class="field-pair">
    {#if !bindingOnly}<label class="field"><span>匹配方式</span><FieldSelect value={type} onValueChange={v=>type=v as ZeroRuleType} disabled={busy} aria-label="匹配方式" options={RULE_MATCH_TYPES}/></label>{:else}<div class="field"><span>匹配条件</span><span>{bindingOnly.name}</span></div>{/if}
    <label class="field"><span>应用范围</span><FieldSelect value={scope} onValueChange={v=>scope=v} disabled={busy} aria-label="应用范围" options={[{value:'global',label:'所有配置'},{value:'current',label:'仅当前配置'}]}/></label>
    </div>
    <div class="field-pair target-pair">
    <label class="field"><span>出站端点</span>{#if fromList}<Input value={selected} readonly aria-label="目标端点"/>{:else}<FieldSelect value={selected} onValueChange={v=>selected=v} options={tags} disabled={busy} aria-label="目标端点"/>{/if}</label>
    <label class="field"><span>顺序（数值越小越优先）</span><Input type="number" bind:value={order} min="0" max="4294967295" disabled={busy} aria-label="端点规则顺序"/></label>
    </div>
    {#if !bindingOnly}<label class="field"><span>匹配内容（每行一项）</span><Textarea bind:value={text} rows={6} disabled={busy} aria-label="匹配内容" placeholder={RULE_MATCH_TYPES.find(option=>option.value===type)?.placeholder}></Textarea></label>{/if}
    <p class="hint">{type === 'domain_suffix' ? '包含主域名及其所有子域名。' : type === 'domain_exact' ? '仅匹配填写的完整域名。' : type === 'domain_keyword' ? '匹配域名中包含的关键词。' : '支持单个 IP 地址或网段，需在端点允许范围内。'}</p>
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  </div>
  {/if}
  {#snippet footer()}
    <Button id={`${fieldsId}-close`} variant="outline" onclick={cancel} disabled={busy}>{view==='list'?'关闭':'取消'}</Button>
    {#if view==='list'}<Button disabled={busy} onclick={()=>create(true)}>添加路由规则</Button>
    {:else if view==='delete'}<Button variant="destructive" disabled={busy} onclick={remove}>删除</Button>
    {:else}<Button onclick={save} disabled={busy || !selected || (!bindingOnly && !text.trim()) || !Number.isFinite(order) || order<0 || order>4294967295}>{busy ? '正在保存并应用…' : '保存并应用'}</Button>{/if}
  {/snippet}
</DraggableModal>
<style>
  .editor-fields {display:grid;gap:14px;}
  .field-pair {display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px;}
  .target-pair {grid-template-columns:minmax(0,1fr) 150px;}
  @media(max-width:500px){.field-pair {grid-template-columns:1fr;}}
  .field {display:grid;gap:6px;font-size:12px;color:var(--foreground);}
  .hint {font-size:12px;color:var(--muted-foreground);}
  .form-error {font-size:12px;color:var(--destructive);white-space:pre-wrap;}
</style>
