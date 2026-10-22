<script lang="ts">
 import {onMount,tick} from 'svelte';
 import * as Dialog from '$lib/components/ui/dialog';
 import {Input} from '$lib/components/ui/input';
 import type {DnsMigrationMode} from '$lib/features/dns-migration/types';
 import {Button} from '$lib/components/ui/button';
 import {dnsMigration as migration} from '$lib/features/dns-migration/session';
 import {proxyConfigSignal} from '$lib/services/proxy-config-signal.svelte';
 let {open,profileId,requestedName,requestedMode,dirty,onClose,onDiscard,onSaved,onStateChange}:{open:boolean;profileId:string|null;requestedName:string|null;requestedMode:DnsMigrationMode;dirty:boolean;onClose:()=>void;onDiscard:()=>Promise<void>;onSaved:(mode:DnsMigrationMode)=>void;onStateChange:(value:{busy:boolean;unknown:boolean;mode:DnsMigrationMode})=>void}=$props();
 let view=$state(migration.view),discarding=$state(false),wasOpen=false,handledPlan:string|null=null,alive=true;
 const busy=$derived(['loading','previewing','saving'].includes(view.phase)||view.checking||discarding);
 const mode=$derived(view.phase==='unknown'||view.phase==='saved'?view.mode:requestedMode);
 const renaming=$derived(mode==='rename');
 const selected=$derived(view.snapshot?.servers.find(server=>server.name===view.serverName)??null);
 const canChoose=$derived(view.phase==='choosing'&&!!view.snapshot?.binding&&!view.snapshot.readOnlyReason&&!view.conflict&&!busy);
 const renameIssue=$derived.by(()=>{void view.renameTo;void view.serverName;void view.snapshot;return migration.renameIssue;});
 const canPreview=$derived(canChoose&&!!selected&&(renaming?!renameIssue:(view.snapshot?.servers.length??0)>1&&(!selected.references.length||!!view.replacementName)));
 onMount(()=>{alive=true;const off=migration.subscribe(value=>view=value);const offSource=proxyConfigSignal.onActiveChanged(()=>migration.invalidate());return()=>{alive=false;off();offSource();};});
 async function begin(){if(dirty||migration.unresolved||busy)return;if(profileId)await migration.load(profileId,requestedMode);}
 $effect(()=>{if(open&&!wasOpen){wasOpen=true;void begin();}else if(!open)wasOpen=false;});
 $effect(()=>{onStateChange({busy,unknown:view.phase==='unknown',mode:view.mode});});
 $effect(()=>{if(view.phase==='saved'&&view.plan&&view.plan.planId!==handledPlan){handledPlan=view.plan.planId;onSaved(view.mode);}});
 function close(){if(busy)return;if(!migration.unresolved)migration.cancel();onClose();}
 async function discard(){if(busy)return;discarding=true;try{await onDiscard();await tick();}finally{discarding=false;}if(alive)await begin();}
 function localTime(value:string){const ms=Number(value);return Number.isSafeInteger(ms)&&ms<=8640000000000000?new Date(ms).toLocaleString():'未提供';}
</script>
<Dialog.Root {open} onOpenChange={value=>{if(!value)close();}}><Dialog.Content class="dns-migration-dialog" showCloseButton={!busy} onEscapeKeydown={event=>{if(busy)event.preventDefault();}} onInteractOutside={event=>{if(busy)event.preventDefault();}}><Dialog.Header><Dialog.Title>{renaming?'重命名 DNS 服务器并迁移引用':'删除 DNS 服务器并迁移引用'}</Dialog.Title><Dialog.Description>仅保存全局客户端 DNS 覆盖，影响全部配置；不会立即改变运行内核、系统 DNS 或 VPN。原列表入口：{requestedName??'原操作核对'}。</Dialog.Description></Dialog.Header><Dialog.Body>
 {#if dirty&&view.phase!=='unknown'&&view.phase!=='saved'}
 <section aria-label="DNS 草稿迁移限制"><p role="status">DNS 面板有未保存草稿。迁移只读取宿主持久材料，不能把当前草稿悄悄丢弃或自动保存应用。请继续编辑并保存，或明确放弃草稿并重新读取。</p><Button variant="destructive" disabled={busy} onclick={()=>void discard()}>放弃 DNS 草稿并重新读取</Button></section>
 {:else if view.phase==='loading'}<p role="status">正在读取持久 DNS 服务器与全部引用…</p>
 {:else if view.phase==='error'}<p role="alert">{view.message}</p><Button variant="outline" disabled={busy||!profileId} onclick={()=>void begin()}>重试读取持久 DNS</Button>
 {:else if view.phase==='unknown'}<section aria-label="原 DNS 迁移操作"><p role="status">{view.message}</p><p>原操作：{view.plan?.planId??'未提供'}。只读核对不会重新保存，也不确认运行生效。</p><Button variant="outline" disabled={busy} onclick={()=>void migration.check(profileId)}>核对原 DNS 迁移保存</Button></section>
 {:else if view.phase==='saved'}<p role="status">{view.message}</p><p>请关闭此窗口后，显式前往工作区预检和应用。</p>
 {:else if view.snapshot}
 <p>核对配置：{view.snapshot.profileName}。操作范围：全部配置共用的客户端 DNS 覆盖。</p>
 {#if view.snapshot.readOnlyReason}<p role="status">{view.snapshot.readOnlyReason}</p>{:else if !renaming&&view.snapshot.servers.length<2}<p role="status">至少保留一个 DNS 服务器；当前不能删除。</p>{/if}
 {#if view.conflict}<p role="alert">{view.message}</p><Button variant="outline" disabled={busy||!profileId} onclick={()=>void begin()}>重新读取持久 DNS</Button>{/if}
 {#if (view.phase==='confirming'||view.phase==='saving')&&view.plan}
 <section aria-label="固定 DNS 引用迁移预览"><h3>固定预览</h3>{#if renaming}<p>原名称：{view.plan.serverName}</p><p>新名称：{view.plan.renameTo}</p><p>同一服务器定义与全部真实引用由宿主一并迁移，不是删除后新建。</p>{:else}<p>删除：{view.plan.serverName}</p><p>替代：{view.plan.replacementName??'无需替代（无引用）'}</p>{/if}<p>有效至：{localTime(view.plan.expiresAtUnixMs)}</p><ul>{#each view.plan.references as reference}<li><strong>{reference.label}</strong><code>{reference.path}</code><span>→ {view.plan.renameTo??view.plan.replacementName??'—'}</span></li>{/each}</ul><p>确认仅提交此原预览；后续变化不会加入。所有引用顺序与冲突由宿主核对，不能用首项兜底。</p></section>
 {:else}
 <section aria-label="本次宿主持久 DNS 服务器"><h3>{renaming?'明确选择要重命名的持久服务器':'明确选择要删除的持久服务器'}</h3><p>未自动选择原列表的同名对象；请核对本次读取结果。</p><div class="server-options">{#each view.snapshot.servers as server}<Button variant={view.serverName===server.name?'secondary':'outline'} aria-pressed={view.serverName===server.name} aria-label={`${renaming?'选择重命名':'选择删除'}持久 DNS ${server.name}`} disabled={!canChoose||(!renaming&&view.snapshot.servers.length<2)} onclick={()=>migration.choose(server.name)}>{server.name} · {server.references.length} 处引用</Button>{/each}</div></section>
 {#if selected}<section aria-label="DNS 全部引用位置"><h3>全部引用：{selected.references.length} 处</h3>{#if selected.references.length===0}<p>未发现引用；仍需固定预览并确认本地变更。</p>{:else}<ul>{#each selected.references as reference}<li><strong>{reference.label}</strong><code>{reference.path}</code></li>{/each}</ul>{#if !renaming}<h3>明确选择已有替代服务器</h3><p>不会自动选择第一项。重复回退、主服务器或独立超时冲突会拒绝预览，不会静默去重覆盖。</p><div class="server-options">{#each view.snapshot.servers.filter(server=>server.name!==selected.name) as server}<Button variant={view.replacementName===server.name?'secondary':'outline'} aria-pressed={view.replacementName===server.name} aria-label={`迁移引用到 DNS ${server.name}`} disabled={!canChoose} onclick={()=>migration.replace(server.name)}>{server.name}</Button>{/each}</div>{/if}{/if}{#if renaming}<label class="rename-field"><span>明确输入 DNS 新名称</span><Input aria-label="DNS 新名称" value={view.renameTo??''} disabled={!canChoose} oninput={event=>migration.rename(event.currentTarget.value)} placeholder="输入一个未占用的新名称" /></label><p>不会修改服务器定义中的地址、出站或引导地址；全部真实引用及独立超时键由宿主校验并迁移。</p>{#if renameIssue}<p role="status">{renameIssue}</p>{/if}{/if}</section>{/if}
 {/if}
 {#if view.message&&!view.conflict}<p role="status">{view.message}</p>{/if}
 {/if}
 {#if busy}<p role="status">原操作正在等待结果；关闭、返回和重复提交均已阻拦，不会自动重放。</p>{/if}
</Dialog.Body><Dialog.Footer><Button variant="outline" disabled={busy} onclick={close}>{view.phase==='unknown'?'关闭并保留原操作':dirty?'继续编辑':'关闭迁移窗口'}</Button>{#if !dirty&&view.phase==='confirming'}<Button variant="outline" disabled={busy} onclick={()=>migration.back()}>返回选择</Button><Button disabled={busy} onclick={()=>void migration.save()}>确认仅保存 DNS 迁移</Button>{:else if !dirty&&view.phase==='choosing'}<Button disabled={!canPreview} onclick={()=>void migration.preview()}>{renaming?'预览 DNS 重命名与引用迁移':'预览 DNS 删除与引用迁移'}</Button>{/if}</Dialog.Footer></Dialog.Content></Dialog.Root>
<style>
 :global(.dns-migration-dialog){max-width:640px;max-height:calc(100dvh - 24px - env(safe-area-inset-top) - env(safe-area-inset-bottom));}.rename-field{display:flex;flex-direction:column;gap:8px;}:global(.dns-migration-dialog input){min-height:44px;font-size:16px;}section{display:flex;flex-direction:column;gap:10px;margin:8px 0 18px;min-width:0;}h3{font-size:14px;font-weight:600;}p,li{font-size:13px;line-height:1.6;overflow-wrap:anywhere;}ul{display:flex;flex-direction:column;gap:8px;}li{border:1px solid var(--border);border-radius:8px;padding:9px;display:flex;flex-direction:column;gap:4px;}code{font-size:12px;white-space:pre-wrap;overflow-wrap:anywhere;} .server-options{display:flex;gap:8px;flex-wrap:wrap;min-width:0;}.server-options :global(button){white-space:normal;overflow-wrap:anywhere;max-width:100%;height:auto;text-align:left;}:global(.dns-migration-dialog button){min-height:44px;}
</style>
