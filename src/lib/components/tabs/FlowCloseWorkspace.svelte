<script lang="ts">
 import {onMount} from 'svelte';
 import DraggableModal from '$lib/components/DraggableModal.svelte';
 import {Button} from '$lib/components/ui/button';
 import {Input} from '$lib/components/ui/input';
 import {Choice} from '$lib/components/ui/choice';
 import {Badge} from '$lib/components/ui/badge';
 import {flowCloseSession as session} from '$lib/features/flow-close/session';
 import {displayTime,outcomeLabel} from '$lib/features/flow-close/state';
 let open=$state(false),search=$state(''),fromOldRow=$state(false),now=$state(Date.now());
 let view=$state.raw(session.view);
 const busy=$derived(['loading','previewing','submitting'].includes(view.phase)||view.checking);
 const targets=$derived(view.inventory?.targets.filter(t=>[t.destination,t.flowId,t.network].join(' ').toLowerCase().includes(search.toLowerCase().trim()))??[]);
 const expired=$derived(!view.plan || BigInt(view.plan.expiresAtUnixMs)<=BigInt(now));
 export function begin(from:'list'|'legacy-row'='list'){fromOldRow=from==='legacy-row';search='';open=true;void session.start();}
 function close(){if(busy)return;session.cancel();open=false;}
 onMount(()=>{const unsubscribe=session.subscribe(value=>view=value);const timer=setInterval(()=>now=Date.now(),1000);return()=>{unsubscribe();clearInterval(timer);};});
</script>
{#if view.attempted && view.plan}<div class="flow-close-last" role="status"><span>保留原关闭操作的逐项回执；未确认项不会自动重放。</span><Button variant="outline" onclick={()=>open=true}>查看关闭结果</Button></div>{/if}
<DraggableModal title={view.phase==='results' ? '连接关闭结果' : view.phase==='confirming' || view.phase==='submitting' ? '确认关闭所选连接' : '选择连接关闭'} description="从宿主本次真实核对的列表明确选择；活动/暂停展示不会授予新目标控制权限。" {open} onClose={close} closeDisabled={busy} width="min(720px, 96vw)">
 <div class="flow-close-workspace">
  {#if fromOldRow}<p role="note">原活动行或暂停内容可能过期，不能据其同名 ID 关闭新连接。请在以下真实核对列表重新明确选择。</p>{/if}
  {#if view.message}<p role={view.phase==='error'?'alert':'status'}>{view.message}</p>{/if}
  {#if view.phase==='previewing'}<p role="status">正在预检原固定目标…</p>{/if}
  {#if view.phase==='loading'}<p role="status">正在读取实际连接与控制资格…</p>{/if}
  {#if view.phase==='selecting' && view.inventory}
   <dl><dt>运行实例</dt><dd>{view.inventory.coreInstanceId}</dd><dt>配置版本</dt><dd>{view.inventory.configRevision}</dd><dt>核对时间</dt><dd>{displayTime(view.inventory.observedAtUnixMs)}</dd></dl>
   <p>{view.inventory.notice}</p>
   <p>本次最多核对 {view.inventory.limit} 条。{view.inventory.truncated ? '列表已截断，窗口外还有多少连接未知；不是全部连接。' : '仅对此次返回的实际连接负责。'}</p>
   <Input aria-label="搜索待关闭连接" placeholder="搜索目标、ID 或协议" bind:value={search}/>
   <div class="flow-close-actions"><Button variant="outline" onclick={()=>session.selectAll(targets.filter(t=>t.canClose).map(t=>t.targetKey))}>选择当前核对列表</Button><Button variant="outline" onclick={()=>session.selectAll([])}>清除选择</Button></div>
   {#if !view.inventory.targets.length}<p role="status">实际快照没有活动连接。</p>{:else if !targets.length}<p role="status">当前搜索没有匹配连接。</p>{/if}
   <ul class="flow-close-targets" aria-label="本次真实连接列表">{#each targets as target(target.targetKey)}<li><label class="flow-close-choice"><Choice checked={view.selectedKeys.includes(target.targetKey)} onchange={()=>session.toggle(target.targetKey)} disabled={!target.canClose} aria-label={`选择连接 ${target.flowId} ${target.destination}`}/><span><strong>{target.destination || '目标未提供'}</strong><span>#{target.flowId} · {target.network.toUpperCase()}</span><span>开始：{displayTime(target.startedAtUnixMs)}</span>{#if !target.canClose}<span>{target.reason ?? '该连接当前不可关闭'}</span>{/if}</span></label></li>{/each}</ul>
   <p role="status">已选择 {view.selectedKeys.length} 条。尚未发出关闭命令。</p>
  {/if}
  {#if view.plan && ['confirming','submitting','results'].includes(view.phase)}
   <p class="flow-close-id">原操作：{view.plan.planId}</p><dl><dt>原运行实例</dt><dd>{view.plan.coreInstanceId}</dd><dt>原配置版本</dt><dd>{view.plan.configRevision}</dd></dl><p>这里是原操作的固定事实；当前配置或实例变化后仅供历史核对，不授予新对象控制权。</p>
   {#if view.phase!=='results'}<p>固定 {view.plan.targets.length} 个目标，新建立的连接不加入。预览有效至 {displayTime(view.plan.expiresAtUnixMs)}。{expired ? '预览已过期，不能提交。' : ''}</p><p>{view.plan.notice}</p>
    <ul class="flow-close-targets" aria-label="固定关闭目标">{#each view.plan.targets as target(target.targetKey)}<li><strong>{target.destination || '目标未提供'}</strong><span>#{target.flowId} · {target.network.toUpperCase()} · {target.state==='active'?'活动':target.state==='ended'?'已结束':'不可用'}</span></li>{/each}</ul>
   {:else}
    <div class="flow-close-counts">{#each Object.entries(outcomeLabel) as [outcome,label]}<Badge variant="outline">{label} {view.items.filter(item=>item.outcome===outcome).length}</Badge>{/each}</div>
    <ul class="flow-close-targets" aria-label="逐项关闭结果">{#each view.items as item(item.flowId)}<li><strong>#{item.flowId} · {outcomeLabel[item.outcome]}</strong><span>{item.reason}</span></li>{/each}</ul>
    <p>仅记录原操作回执，不清空统计或本地历史。活动列表仍等待实际观测更新。</p>
   {/if}
  {/if}
  {#if view.receipts.length}<details><summary>最近 {view.receipts.length} 次原操作回执</summary><p>仅保留本次应用会话最近 8 次回执投影，不重新提交。</p><div class="flow-close-receipts">{#each view.receipts as receipt(receipt.plan.planId)}<Button variant="outline" disabled={busy} onclick={()=>session.showReceipt(receipt.plan.planId)}>查看原操作 {receipt.plan.planId}</Button>{/each}</div></details>{/if}
 </div>
 {#snippet footer()}<div class="flow-close-footer">
  <Button variant="outline" disabled={busy} onclick={close}>关闭</Button>
  {#if view.phase==='selecting'}<Button disabled={!view.selectedKeys.length||busy} onclick={()=>session.preview()}>预览所选关闭</Button>{/if}
  {#if view.phase==='confirming'}<Button variant="destructive" disabled={expired||busy} onclick={()=>session.submit()}>确认关闭 {view.plan?.targets.length??0} 条</Button>{/if}
  {#if view.phase==='results' && view.attempted}<Button variant="outline" disabled={busy} onclick={()=>session.check()}>{view.checking?'核对中…':'只读核对原操作'}</Button>{/if}
  {#if ['results','error','confirming'].includes(view.phase)}<Button variant="outline" disabled={busy} onclick={()=>session.start()}>重新读取并明确选择</Button>{/if}
  {#if view.phase==='submitting'}<span role="status">等待原操作逐项回执，不会重复提交…</span>{/if}
 </div>{/snippet}
</DraggableModal>
<style>
 .flow-close-last,.flow-close-actions,.flow-close-footer,.flow-close-counts{display:flex;flex-wrap:wrap;align-items:center;gap:8px;}
 .flow-close-last{padding:8px 12px;border-bottom:1px solid var(--border);font-size:12px;}
 .flow-close-workspace{display:grid;gap:12px;min-width:0;font-size:13px;overflow-wrap:anywhere;}
 .flow-close-workspace summary{min-height:44px;padding:10px 0;}
 .flow-close-workspace p{margin:0;line-height:1.6;}.flow-close-workspace dl{display:grid;grid-template-columns:76px minmax(0,1fr);gap:4px;font-size:12px;}.flow-close-workspace dd{margin:0;overflow-wrap:anywhere;}
 .flow-close-targets{list-style:none;padding:0;margin:0;display:grid;gap:8px;max-height:45dvh;overflow:auto;overscroll-behavior:contain;}
 .flow-close-targets li{border:1px solid var(--border);border-radius:8px;padding:10px;min-width:0;display:grid;gap:4px;}.flow-close-targets li span{display:block;overflow-wrap:anywhere;}
 .flow-close-choice{display:flex;gap:10px;align-items:center;min-height:44px;cursor:pointer;}.flow-close-choice>span{min-width:0;}.flow-close-choice span span{font-size:11px;color:var(--muted-foreground);}
 .flow-close-workspace :global(button),.flow-close-footer :global(button),.flow-close-last :global(button){min-height:44px;}
 .flow-close-workspace :global(input[type='text']){font-size:16px;min-height:44px;}
 .flow-close-footer{justify-content:flex-end;min-width:0;}.flow-close-receipts{display:grid;gap:8px;}.flow-close-receipts :global(button){height:auto;white-space:normal;overflow-wrap:anywhere;}
 @media(max-width:400px){.flow-close-footer :global(button){flex:1 1 120px;}.flow-close-actions :global(button){flex:1 1 120px;}}
</style>
