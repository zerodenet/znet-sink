<script lang="ts">
 import {onMount} from 'svelte';
 import DraggableModal from '$lib/components/DraggableModal.svelte';
 import {Button} from '$lib/components/ui/button';
 import {Badge} from '$lib/components/ui/badge';
 import FieldSelect from '$lib/components/ui/select/field-select.svelte';
 import {listProxyConfigs} from '$lib/services/config';
 import {proxyConfigSignal} from '$lib/services/proxy-config-signal.svelte';
 import {ruleMigrationSession as session} from '$lib/features/rule-migration/session';
 let {onSaved=()=>{}}:{onSaved?:()=>void|Promise<void>}=$props();
 let open=$state(false),loadingProfile=$state(false),profileError=$state<string|null>(null),requestedId=$state<string|null>(null);
 let view=$state.raw(session.view),now=$state(Date.now());
 const busy=$derived(loadingProfile||['loading','previewing','saving'].includes(view.phase)||view.checking);
 const writable=$derived(view.phase==='selecting'&&view.sourceConfirmed&&!!view.snapshot?.binding&&view.snapshot.resourceExists&&!view.snapshot.blockers.length&&!view.conflict);
 const expired=$derived(!view.plan||BigInt(view.plan.expiresAtUnixMs)<=BigInt(now));
 const options=$derived(view.snapshot?.targets.map(t=>({value:t.id,label:`${t.name} · ${t.tag}`,disabled:t.unavailableReason!==null}))??[]);
 let notifiedPlan:string|null=null;
 const reasonLabels:Record<string,string>={
  cross_profile_reference:'其他配置仍引用此资源，不能在此迁移',
  source_uninspectable:'此来源不能安全检查，不能据未知状态删除',
  managed_source:'插件或订阅托管来源不能由普通迁移修改',
  unsupported_source:'此来源格式尚不支持引用迁移',
  source_schema_invalid:'来源结构未通过检查',
  metadata_invalid:'客户端覆盖结构无法核对',
  metadata_source_changed:'覆盖所属来源已变化，须重新核对',
  generated_tag_collision:'规则集原生标签冲突，不能猜测归属',
  global_dns_scope:'全局 DNS 仍有引用，当前范围不能完整迁移',
  replacement_required:'仍有引用，须明确选择替换资源',
  replacement_unavailable:'替换资源尚不满足此迁移条件',
  dns_incompatible:'替换资源不兼容现有 DNS 引用',
  reference_budget_exceeded:'引用检查超出预算，未证明删除安全',
  legacy_dns_scope:'旧 DNS 覆盖仍有引用，须先处理对应范围',
 };
 export async function begin(ruleSetId:string){open=true;if(loadingProfile||session.unresolved||session.busy)return;requestedId=ruleSetId;profileError=null;loadingProfile=true;try{const active=(await listProxyConfigs()).find(p=>p.active);if(!active){session.scopeChanged();profileError='请先选择一份普通配置；当前没有配置可固定本次引用范围。';return;}await session.load(active.id,ruleSetId);}catch{session.scopeChanged();profileError='当前配置无法核对；未读取秘密或提交删除，请重新检查。';}finally{loadingProfile=false;}}
 async function refresh(){if(requestedId&&!busy)await begin(requestedId);}
 function close(){if(busy||!session.close())return;open=false;}
 async function check(){if(busy)return;let id:string|null=null;try{id=(await listProxyConfigs()).find(p=>p.active)?.id??null;}catch{profileError='当前配置读取失败；只保留原操作历史。';}await session.check(id);}
 onMount(()=>{const stop=session.subscribe(next=>view=next);const changed=proxyConfigSignal.onActiveChanged(()=>{if(view.snapshot||view.plan)session.scopeChanged();});const clock=window.setInterval(()=>now=Date.now(),1000);return()=>{stop();changed();window.clearInterval(clock);};});
 $effect(()=>{if(view.phase==='saved'&&view.plan&&notifiedPlan!==view.plan.planId){notifiedPlan=view.plan.planId;void onSaved();}});
</script>
{#if view.phase==='unknown'&&view.plan}<div class="migration-retained" role="status"><span>规则集迁移结果未确认，原操作保留。</span><Button variant="outline" onclick={()=>open=true}>核对原规则集迁移</Button></div>{/if}
<DraggableModal title={view.phase==='saved'?'规则集本地结果':view.phase==='confirming'||view.phase==='saving'?'确认迁移或删除规则集':'核对规则集引用'} description="先明确核对本次来源和替换对象；本地迁移与删除分阶段保存，不自动应用运行配置。" {open} onClose={close} closeDisabled={busy} width="min(660px, 96vw)">
 <div class="migration-body">
  {#if profileError}<p role="alert">{profileError}</p>{/if}
  {#if loadingProfile||view.phase==='loading'}<p role="status">正在核对当前配置、规则集和引用范围…</p>{/if}
  {#if view.message}<p role={view.phase==='error'?'alert':'status'}>{view.message}</p>{/if}
  {#if view.snapshot}
   <dl><dt>配置范围</dt><dd>{view.snapshot.profileName} · {view.snapshot.profileId}</dd><dt>本次来源</dt><dd>{view.snapshot.ruleSetName??'资源未登记'} · {view.snapshot.ruleSetId}</dd><dt>原生 tag</dt><dd>{view.snapshot.tag}</dd></dl>
   <p>{view.snapshot.notice}</p>
   {#if view.snapshot.blockers.length}<section aria-label="迁移阻止范围"><h3>当前不能迁移或删除</h3><ul>{#each view.snapshot.blockers as blocker}<li>{blocker.profileId??'宿主范围'}：{reasonLabels[blocker.reasonCode]??'此范围尚不能安全修改'} <code>{blocker.reasonCode}</code></li>{/each}</ul></section>{/if}
   {#if view.phase==='selecting'}
    {#if view.snapshot.resourceExists&&!view.sourceConfirmed}<Button variant="outline" disabled={busy} onclick={()=>session.confirmSource()}>使用本次核对来源</Button>{/if}
    {#if view.sourceConfirmed}<Badge variant="secondary">已明确选择本次来源</Badge>{/if}
    <section aria-label="原规则集引用位置"><h3>引用位置 · {view.snapshot.references.length}</h3>{#if !view.snapshot.references.length}<p>本次范围未发现引用；删除仍需独立预览确认。</p>{:else}<ul>{#each view.snapshot.references as reference}<li><strong>{reference.profileId??'全局范围'}</strong><code>{reference.fieldPath}</code><span>{reference.kind} · {reference.tag}</span></li>{/each}</ul>{/if}</section>
    {#if view.snapshot.references.length}<label class="target-field">替换规则集<FieldSelect value={view.targetId??''} onValueChange={id=>session.chooseTarget(id||null)} {options} disabled={!writable||busy} placeholder="请明确选择替换规则集" aria-label="替换规则集"/></label><p>仅提供宿主允许的现有公共资源；不会创建规则集，也不改变替换对象自己的匹配动作或顺序。</p>{#each view.snapshot.targets.filter(t=>t.unavailableReason) as target}<p>{target.name}：{target.unavailableReason}</p>{/each}{/if}
   {/if}
  {/if}
  {#if view.plan&&['confirming','saving','unknown','saved'].includes(view.phase)}
   <section aria-label="原规则集迁移操作"><p>原操作：<code>{view.plan.planId}</code></p><dl><dt>删除来源</dt><dd>{view.plan.ruleSetName} · {view.plan.ruleSetId}</dd><dt>引用替换</dt><dd>{view.plan.targetName??'没有替换目标'}{view.plan.targetId?` · ${view.plan.targetId}`:''}</dd><dt>原生标签</dt><dd>{view.plan.fromTag} → {view.plan.toTag??'不迁移引用'}</dd></dl><p>固定 {view.plan.references.length} 处引用；后续新引用不会扩大本次确认范围。</p>{#if view.plan.commonBindingRemoved}<p>将移除原资源的公共规则绑定；替换对象的动作及顺序保持不变。</p>{/if}<p>{view.plan.notice}</p>
   {#if view.phase==='confirming'}<ul>{#each view.plan.references as reference}<li>{reference.profileId}<code>{reference.fieldPath}</code></li>{/each}</ul>{#if expired}<p role="alert">原预览已过期，不能提交。</p>{/if}{/if}
   </section>
  {/if}
  {#if view.receipt}<section aria-label="原操作本地阶段事实"><h3>原操作本地阶段事实</h3><p>引用设置已发布：{view.receipt.appConfigPublished?'已由原操作确认':'未确认'}</p><p>原规则集已删除：{view.receipt.ruleSetRemoved?'已由原操作确认':'未确认'}</p><p>这些事实不代表当前材料仍一致，也不代表运行配置已应用。阶段失败不声明全部回滚。</p></section>{/if}
 </div>
 {#snippet footer()}<div class="migration-footer"><Button variant="outline" disabled={busy} onclick={close}>返回规则集</Button>
 {#if view.phase==='selecting'}<Button variant="destructive" disabled={!writable||busy||(!!view.snapshot?.references.length&&!view.targetId)} onclick={()=>void session.preview()}>预览本地迁移/删除</Button>{/if}
 {#if view.phase==='confirming'}<Button variant="outline" onclick={()=>session.cancelPreview()}>返回修改选择</Button><Button variant="destructive" disabled={busy||expired} onclick={()=>void session.save()}>确认仅保存本地</Button>{/if}
 {#if view.phase==='unknown'}<Button variant="outline" disabled={busy} onclick={()=>void check()}>只读核对原迁移操作</Button>{/if}
 {#if ['selecting','error'].includes(view.phase)}<Button variant="outline" disabled={busy} onclick={()=>void refresh()}>重新核对来源</Button>{/if}
 {#if view.phase==='saving'}<span role="status">等待原本地操作；不会取消、重放或自动再删。</span>{/if}
 </div>{/snippet}
</DraggableModal>
<style>
 .migration-body{display:grid;gap:12px;min-width:0;font-size:.875rem;line-height:1.6;overflow-wrap:anywhere;}.migration-body p{margin:0;}.migration-body h3{font-size:1rem;font-weight:600;}.migration-body dl{display:grid;grid-template-columns:78px minmax(0,1fr);gap:6px;}.migration-body dd{margin:0;}.migration-body dt{color:var(--muted-foreground);}.migration-body ul{display:grid;gap:8px;padding:0;list-style:none;max-height:34dvh;overflow:auto;}.migration-body li{padding:10px;border:1px solid var(--border);border-radius:8px;display:grid;gap:4px;}.migration-body code{white-space:pre-wrap;overflow-wrap:anywhere;font-size:.75rem;}.target-field{display:grid;gap:6px;}.migration-footer,.migration-retained{display:flex;flex-wrap:wrap;gap:8px;align-items:center;}.migration-retained{padding:8px 12px;font-size:.75rem;}.migration-body :global(button),.migration-footer :global(button),.migration-retained :global(button){min-height:44px;}.migration-footer{justify-content:flex-end;}@media(max-width:400px){.migration-footer :global(button){flex:1 1 120px;}}
</style>
