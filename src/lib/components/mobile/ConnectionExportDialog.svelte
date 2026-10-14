<script lang="ts">
 import {untrack} from 'svelte';
 import * as Dialog from '$lib/components/ui/dialog';
 import {Button} from '$lib/components/ui/button';
 import {Choice} from '$lib/components/ui/choice';
 import {getAppErrorMessage} from '$lib/services/core';
 import {documentReceiptMessage} from '$lib/services/native-documents';
 import type {DebugFrameQuery} from '$lib/types/debug';
 import type {ConnectionExportGateway,ConnectionExportSource} from '$lib/features/connection-export/types';
 import {connectionExportGateway} from '$lib/features/connection-export/client';
 import {ConnectionExportSession} from '$lib/features/connection-export/session';
 let {open=$bindable(false),query=null,source='completed-history',gateway=connectionExportGateway}:{open?:boolean;query?:DebugFrameQuery|null;source?:ConnectionExportSource;gateway?:ConnectionExportGateway}=$props();
 let session=$state.raw<ConnectionExportSession|null>(null);
 let acknowledged=$state(false);
 let revision=$state(0);
 const initialGateway=untrack(()=>gateway);
 const plan=$derived.by(()=>{void revision;return session?.plan??null;});
 const receipt=$derived.by(()=>{void revision;return session?.receipt??null;});
 const error=$derived.by(()=>{void revision;return session?.error;});
 const busy=$derived.by(()=>{void revision;return session?.busy??false;});
 const submitted=$derived.by(()=>{void revision;return session?.submitted??false;});
 $effect(()=>{if(!open)return;const target=untrack(()=>query);if(!target)return;const controller=new ConnectionExportSession(initialGateway);session=controller;acknowledged=false;const fixed=structuredClone(target);const origin=untrack(()=>source);void untrack(()=>controller.preview(fixed,origin)).then(()=>revision++);revision=untrack(()=>revision)+1;return()=>controller.dispose();});
 async function deliver(kind:'save'|'share'){if(!session)return;const pending=session.deliver(kind,acknowledged);revision++;await pending;revision++;}
</script>
<Dialog.Root bind:open>
 <Dialog.Content class="max-w-md max-h-[85dvh]">
  <Dialog.Header><Dialog.Title>{source==='active'?'导出筛选活动连接':'导出筛选完成记录'}</Dialog.Title><Dialog.Description>{source==='active'?'读取当前内核一次真实活动快照并固定筛选，之后新增或结束的连接不改变材料。':'只导出预览时固定的本地完成连接快照；后续新增记录不加入。'}</Dialog.Description></Dialog.Header>
  <Dialog.Body class="space-y-3">
  {#if busy}<p role="status">{submitted?'等待系统回执…':'正在准备脱敏快照…'}</p>{/if}
  {#if error}<p role="alert">{getAppErrorMessage(error,'连接快照准备失败，请关闭后重新预览')}</p>{/if}
  {#if plan}
   <dl class="grid grid-cols-2 gap-2 text-sm"><dt>记录</dt><dd>{plan.records} 条</dd><dt>材料大小</dt><dd>{Math.ceil(plan.bytes/1024)} KiB</dd></dl>
   {#if plan.truncated}<p class="text-sm" role="status">达到 2000 条记录上限，仅导出该筛选快照的{plan.source==='active'?'查询窗口内的部分；窗口外匹配记录未知。':'最近保留部分；完整性未知。'}</p>{/if}
   {#if plan.source==='active'}<dl class="grid grid-cols-2 gap-2 text-sm"><dt>真实内核实例</dt><dd class="break-all">{plan.coreInstanceId??'—'}</dd><dt>配置版本</dt><dd>{plan.configRevision??'—'}</dd></dl>{/if}
   <p class="text-sm">此快照可能含连接 ID、来源与目标 IP、DNS 域名及流量。分享前确认接受这些隐私影响。</p>
   {#each plan.omissions as omission}<p class="text-xs text-muted-foreground">{omission}</p>{/each}
   <label class="flex items-start gap-2 text-sm"><Choice bind:checked={acknowledged} disabled={busy||submitted}/>我确认导出上述固定范围与连接事实</label>
  {/if}
  {#if receipt}<p role="status">{documentReceiptMessage(receipt)}</p>{/if}
  </Dialog.Body>
  <Dialog.Footer><Button variant="outline" onclick={()=>open=false} disabled={busy}>关闭</Button><Button variant="outline" onclick={()=>deliver('share')} disabled={!plan||!acknowledged||busy||submitted}>系统分享</Button><Button onclick={()=>deliver('save')} disabled={!plan||!acknowledged||busy||submitted}>保存文件</Button></Dialog.Footer>
 </Dialog.Content>
</Dialog.Root>
