<script lang="ts">
 import {onMount,untrack} from 'svelte';
 import {Button} from '$lib/components/ui/button';
 import {Textarea} from '$lib/components/ui/textarea';
 import * as Dialog from '$lib/components/ui/dialog';
 import MobileSettingsDraftGuard from '$lib/components/mobile/MobileSettingsDraftGuard.svelte';
 import {optionalMobileNavigation,optionalMobileRoute} from '$lib/features/mobile/context';
 import {routeEditor as editor} from '$lib/features/route-editor/session';
 import {listProxyConfigs} from '$lib/services/config';
 import {proxyConfigSignal} from '$lib/services/proxy-config-signal.svelte';
 import {store} from '$lib/services/store.svelte';
 let {active=true,profileId}: {active?:boolean;profileId?:string}=$props();
 const navigation=untrack(()=>optionalMobileNavigation()),route=untrack(()=>optionalMobileRoute());
 let view=$state(editor.view),profileError=$state<string|null>(null),leaveOpen=$state(false),pendingLeave=$state<(()=>void)|null>(null),sourceConfirm=$state(false),permitUnknown=$state(false);
 let alive=true,loadVersion=0;
 let checkingCurrent=$state(false);
 const busy=$derived(['loading','previewing','saving'].includes(view.phase)||view.checking||checkingCurrent);
 const writable=$derived(!!view.snapshot?.binding&&!view.snapshot.readOnlyReason&&!view.snapshot.staleReason&&!view.conflict&&!busy&&view.phase!=='unknown');
 const routeActive=()=>active&&(!navigation||!route||(navigation.activeTab===route.tab&&navigation.current?.key===route.key));
 async function loadCurrent(fromSource=false){const version=++loadVersion;profileError=null;try{const id=profileId??(await listProxyConfigs()).find(item=>item.active)?.id;if(!alive||version!==loadVersion)return;if(!id){profileError='当前没有选中的配置，请先添加或选择配置。';return;}await editor.load(id,fromSource);}catch{if(alive&&version===loadVersion)profileError='读取当前配置失败，请重试；已有草稿保留。';}}
 onMount(()=>{alive=true;const off=editor.subscribe(value=>view=value);if(view.phase==='idle')void loadCurrent();else void reconcileCurrent();const offSource=proxyConfigSignal.onActiveChanged(()=>editor.sourceChanged());const offNav=navigation?.guardBeforeLeave(next=>{if(!routeActive()||view.phase!=='unknown'||permitUnknown)return true;pendingLeave=next;leaveOpen=true;return false;});return()=>{alive=false;++loadVersion;off();offSource();offNav?.();};});
 export function requestLeave(next:()=>void){if(busy||view.dirty||view.phase==='unknown'){pendingLeave=busy?null:next;leaveOpen=true;return;}next();}
 function discardAndLeave(){if(busy)return;const next=pendingLeave;pendingLeave=null;leaveOpen=false;if(view.phase==='unknown')permitUnknown=true;else editor.resetDraft();next?.();permitUnknown=false;}
 async function reconcileCurrent(){try{const id=profileId??(await listProxyConfigs()).find(item=>item.active)?.id;if(!alive)return;if(id)await editor.reconcile(id);else editor.sourceChanged();}catch{if(alive)editor.sourceChanged();}}
 async function checkOriginal(){if(busy)return;checkingCurrent=true;try{const id=profileId??(await listProxyConfigs()).find(item=>item.active)?.id;if(alive)await editor.check(id??null);}catch{if(alive){editor.sourceChanged();profileError='当前配置无法读取；原操作仅供历史核对。';}}finally{checkingCurrent=false;}}
 function refresh(){requestLeave(()=>void loadCurrent());}
 function openWorkspace(){requestLeave(()=>{if(navigation)navigation.open('workspace');else{store.settingsSection='config';store.activeTab='settings';}});}
 function localTime(value:string){const number=Number(value);return Number.isSafeInteger(number)&&number<=8640000000000000?new Date(number).toLocaleString():'未提供';}
</script>
<MobileSettingsDraftGuard dirty={active&&view.dirty&&view.phase!=='unknown'} busy={active&&busy} label="路由规则" onDiscard={()=>editor.resetDraft()}/>
<section class="route-workspace" aria-label="原生路由规则编辑器">
 <header><div><h2>路由规则</h2><p>当前选中配置的原生规则，按列表顺序匹配。保存仅修改客户端覆盖。</p></div><Button variant="outline" disabled={busy||view.phase==='unknown'} onclick={refresh}>刷新规则来源</Button></header>
 {#if profileError}<p role="alert">{profileError}</p><Button variant="outline" disabled={busy} onclick={()=>void loadCurrent()}>重试读取</Button>{/if}
 {#if view.phase==='loading'}<p role="status">正在读取规则来源…</p>{/if}
 {#if view.snapshot}
 <p>配置：{view.snapshot.profileName} · 来源版本时间：{localTime(view.snapshot.sourceUpdatedAtUnixMs)}</p>
 {#if view.snapshot.readOnlyReason}<p role="status">{view.snapshot.readOnlyReason}；本入口未提供可编辑正文。</p>{/if}
 {#if view.snapshot.staleReason||view.conflict}<p role="alert">{view.snapshot.staleReason??'来源或当前配置已变化，旧草稿仅供核对。'} 旧规则不会自动套用到新来源。</p>{/if}
 {#if view.message}<p role="status">{view.message}</p>{/if}
 {#if view.phase==='unknown'}
 <section aria-label="原规则保存操作"><p>原操作：{view.plan?.planId??'未提供'}。核对不会重新保存，也不确认运行生效。</p><Button variant="outline" disabled={busy} onclick={()=>void checkOriginal()}>核对原保存操作</Button></section>
 {:else}
 <div class="route-actions"><Button disabled={!writable||view.rows.length>=1000} onclick={()=>editor.add()}>添加路由规则</Button><Button variant="outline" disabled={!writable} onclick={()=>void editor.preview()}>预览全部规则</Button>{#if !view.snapshot.readOnlyReason}<Button variant="outline" disabled={busy} onclick={()=>sourceConfirm=true}>使用当前来源重建草稿</Button>{/if}{#if view.snapshot.hasLocalOverlay}<Button variant="outline" disabled={!writable} onclick={()=>void editor.preview('restore_source')}>预览恢复来源规则</Button>{/if}</div>
 {/if}
 <p class="contract">支持当前原生 condition/action/mode 完整 JSON。domain 匹配域名及其子域；没有原生协议条件。启停只保存在客户端覆盖中。高级字段不会经过浏览器解析或重写。</p>
 <details><summary>当前配置允许的目标</summary><ul>{#each view.snapshot.targets as target}<li>{target.kind==='route'?target.tag:target.kind} · {target.label}</li>{/each}</ul></details>
 {#if !view.snapshot.readOnlyReason&&view.rows.length===0}<p role="status">当前规则列表为空；保存空列表与恢复来源是不同操作。</p>{/if}
 <ol class="route-list">{#each view.rows as row,index (row.id)}<li>
 <div class="row-header"><strong>规则 {index+1}</strong><span>{row.enabled?'启用':'停用'}</span><div class="row-actions"><Button variant="outline" aria-label={`上移规则 ${index+1}`} disabled={!writable||index===0} onclick={()=>editor.move(row.id,-1)}>上移</Button><Button variant="outline" aria-label={`下移规则 ${index+1}`} disabled={!writable||index===view.rows.length-1} onclick={()=>editor.move(row.id,1)}>下移</Button><Button variant="outline" aria-label={`${row.enabled?'停用':'启用'}规则 ${index+1}`} disabled={!writable} onclick={()=>editor.toggle(row.id)}>{row.enabled?'停用':'启用'}</Button><Button variant="destructive" aria-label={`删除规则 ${index+1}`} disabled={!writable} onclick={()=>editor.remove(row.id)}>删除</Button></div></div>
 <Textarea aria-label={`规则 ${index+1} 原生 JSON`} value={row.ruleJson} disabled={!writable} oninput={event=>editor.update(row.id,event.currentTarget.value)}/>
 </li>{/each}</ol>
 {#if view.phase==='saved'||view.snapshot.pendingApply}<p role="status">本地材料待应用；运行是否生效须由工作区核对。</p><Button variant="outline" disabled={busy||view.phase==='unknown'||view.conflict} onclick={openWorkspace}>前往工作区预检/应用</Button>{/if}
 {/if}
</section>
<Dialog.Root open={view.phase==='confirming'} onOpenChange={open=>{if(!open)editor.cancelPreview();}}><Dialog.Content><Dialog.Header><Dialog.Title>{view.plan?.mode==='restore_source'?'恢复来源规则覆盖？':'保存本次完整路由规则覆盖？'}</Dialog.Title><Dialog.Description>{view.plan?.mode==='restore_source'?'仅删除此配置的路由规则客户端覆盖，不删除来源；不会操作运行内核。':`固定 ${view.plan?.rows.length??0} 条规则及其顺序与启停状态；仅保存本地，仍需工作区预检和应用。`}</Dialog.Description></Dialog.Header><Dialog.Body><p>预览有效至 {view.plan?localTime(view.plan.expiresAtUnixMs):'—'}</p><ol>{#each view.plan?.rows??[] as row,index}<li>规则 {index+1} · {row.enabled?'启用':'停用'}<pre>{row.ruleJson}</pre></li>{/each}</ol></Dialog.Body><Dialog.Footer><Button variant="outline" onclick={()=>editor.cancelPreview()}>继续编辑</Button><Button onclick={()=>void editor.save()}>确认仅保存本地</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<Dialog.Root bind:open={sourceConfirm}><Dialog.Content><Dialog.Header><Dialog.Title>从当前来源重建规则草稿？</Dialog.Title><Dialog.Description>明确放弃当前未保存草稿，并读取当前来源规则。已有客户端覆盖不会立即改变；须重新预览和保存。旧操作结果未确认时不能重建。</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={()=>sourceConfirm=false}>继续编辑</Button><Button disabled={busy||view.phase==='unknown'} onclick={()=>{sourceConfirm=false;void loadCurrent(true);}}>确认重建草稿</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<Dialog.Root bind:open={leaveOpen}><Dialog.Content><Dialog.Header><Dialog.Title>{busy?'路由规则操作正在处理':view.phase==='unknown'?'离开结果未确认的规则编辑器？':'放弃未保存的路由规则？'}</Dialog.Title><Dialog.Description>{busy?'原操作尚在等待结果；不会取消、重放或在结果返回后自动离开。':view.phase==='unknown'?'原保存结果仍未确认；原操作与草稿将保留，离开不会撤回或重放。':'继续编辑保留草稿；放弃仅丢弃未保存的本地编辑。'}</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={()=>{leaveOpen=false;pendingLeave=null;}}>继续编辑</Button>{#if !busy}<Button variant="destructive" onclick={discardAndLeave}>{view.phase==='unknown'?'保留原操作并离开':'放弃修改并离开'}</Button>{/if}</Dialog.Footer></Dialog.Content></Dialog.Root>
<style>
 .route-workspace{display:flex;flex-direction:column;gap:12px;min-width:0;padding:12px;overflow:auto;height:100%;}header,.route-actions,.row-header,.row-actions{display:flex;gap:8px;flex-wrap:wrap;align-items:center;}header{justify-content:space-between;}h2{font-size:18px;font-weight:600;}p,.contract{font-size:13px;line-height:1.6;overflow-wrap:anywhere;}.contract{color:var(--muted-foreground);}.route-list{display:flex;flex-direction:column;gap:14px;min-width:0;}.route-list>li{padding:12px;border:1px solid var(--border);border-radius:12px;min-width:0;}.row-header{margin-bottom:8px;}.row-actions{width:100%;}.route-workspace :global(button){min-height:44px;}.route-workspace :global(textarea){min-height:156px;font-size:16px;}pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px;max-height:200px;overflow:auto;}details{font-size:13px;}summary{padding:12px 0;min-height:44px;cursor:pointer;}
</style>
