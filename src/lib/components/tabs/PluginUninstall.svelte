<script lang="ts">
  import { onMount } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Choice } from '$lib/components/ui/choice';
  import { optionalMobileNavigation, optionalMobileRoute } from '$lib/features/mobile/context';
  import { store } from '$lib/services/store.svelte';
  import { pluginUninstallSession as session } from '$lib/features/plugin-uninstall/session';
  import { itemStateLabel, uninstallTime } from '$lib/features/plugin-uninstall/state';
  import type { PluginUninstallView } from '$lib/features/plugin-uninstall/types';
  let { open, pluginId, onClose, onChanged }: { open:boolean;pluginId:string|null;onClose:()=>void;onChanged:()=>Promise<void> } = $props();
  const navigation=optionalMobileNavigation();
  const route=optionalMobileRoute();
  let view=$state<PluginUninstallView>(session.view);
  let closeReview=$state<'draft'|'unknown'|null>(null);
  let closeNotice=$state<string|null>(null);
  let pendingNavigation:(()=>void)|null=null;
  let wasOpen=false;
  let closing=false;
  const busy=$derived(['loading','submitting','checking'].includes(view.phase));
  const ownershipLabels={ 'verified-package':'已核验插件包','exact-managed-source':'精确托管来源','verified-component':'已核验组件','publisher-namespace':'发布者隔离空间' };
  const current=$derived(view.status);
  const canSubmit=$derived(view.phase==='review' && view.acknowledged && session.canExecute);
  const receiptComplete=$derived(current?.state==='completed' && current.items.length>0 && current.items.every(item=>item.state==='completed'));
  function active(){return !navigation || !route || (navigation.activeTab===route.tab && navigation.current?.key===route.key);}
  $effect(()=>{
    if(open && !wasOpen){wasOpen=true;closing=false;closeNotice=null;if(pluginId)void session.open(pluginId);else void session.recent();}
    if(!open)wasOpen=false;
  });
  function closeNow(){closing=true;if(!view.operationId)session.cancelUnsubmitted();closeReview=null;closeNotice=null;onClose();const next=pendingNavigation;pendingNavigation=null;next?.();}
  function requestClose(next?:()=>void){
    if(closing)return;
    if(busy){closeNotice='原操作仍在等待宿主结果；不会取消、重放或在结果返回后自动离开。';pendingNavigation=null;return;}
    pendingNavigation=next??null;
    if(view.operationId && current?.state!=='completed'){closeReview='unknown';return;}
    if(view.phase==='review' && view.acknowledged){closeReview='draft';return;}
    closeNow();
  }
  function cancelClose(){closeReview=null;pendingNavigation=null;}
  function escape(event:KeyboardEvent){event.preventDefault();requestClose();}
  async function execute(){await session.execute();await onChanged();}
  async function check(){await session.check();await onChanged();}
  function goProfiles(){
    if(busy || view.operationId)return;
    session.cancelUnsubmitted();closing=true;onClose();
    if(navigation)navigation.open('profiles');
    else store.activeTab='profiles';
  }
  onMount(()=>{
    const stop=session.subscribe(value=>view=value);
    const removeGuard=navigation?.guardBeforeLeave(next=>{
      if(!open || closing || !active())return true;
      requestClose(next);return false;
    });
    const visibility=()=>{if(open)session.invalidateReview();};
    document.addEventListener('visibilitychange',visibility);
    return()=>{stop();removeGuard?.();document.removeEventListener('visibilitychange',visibility);};
  });
</script>

<Dialog.Root {open} onOpenChange={value=>{if(!value)requestClose();}}>
  <Dialog.Content class="plugin-uninstall-dialog" showCloseButton={false} onEscapeKeydown={escape} onInteractOutside={event=>{event.preventDefault();requestClose();}}>
    <Dialog.Header>
      <Dialog.Title>{view.phase==='records'?'卸载记录':'卸载插件'}</Dialog.Title>
      <Dialog.Description>先核对插件及其托管的订阅、关联配置和规则。这里只处理宿主确认归属的固定资源，不按同名删除普通用户资源。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body>
      <div class="uninstall-body">
        {#if closeNotice}<p role="status">{closeNotice}</p>{/if}
        {#if view.message}<p role="status">{view.message}</p>{/if}
        {#if view.phase==='loading'}<p role="status">正在读取宿主卸载资料…</p>{/if}
        {#if view.phase==='error'}<p role="alert">本次预览不可提交；原材料尚未据此确认清理。</p>{#if pluginId}<Button variant="outline" onclick={()=>session.open(pluginId)}>重新读取卸载预览</Button>{/if}{/if}
        {#if view.preview && !view.operationId && view.phase==='review'}
          {@const preview=view.preview}
          <section aria-label="固定插件身份">
            <h3>{preview.pluginName}</h3>
            <dl><div><dt>插件</dt><dd>{preview.pluginId}</dd></div><div><dt>版本</dt><dd>{preview.version}</dd></div><div><dt>发布者</dt><dd>{preview.publisherId}</dd></div><div><dt>发布者指纹</dt><dd>{preview.publisherFingerprint}</dd></div><div><dt>包摘要</dt><dd>{preview.packageDigest}</dd></div><div><dt>预览有效至</dt><dd>{uninstallTime(preview.expiresAtUnixMs)}</dd></div></dl>
          </section>
          <p>{preview.notice}</p>
          {#if preview.blockedReasons.length}<section aria-label="卸载受限原因"><h3>暂不能执行</h3><ul>{#each preview.blockedReasons as reason}<li>{reason}</li>{/each}</ul></section>{/if}
          {#if preview.requiresManualSwitch || preview.activeProfile}
            <section aria-label="当前配置与手动处理">
              <h3>先处理当前使用的配置</h3>
              {#if preview.activeProfile}<p>当前选择：{preview.activeProfile.name}（{preview.activeProfile.id}）。此入口不会自动切换、停止 VPN 或假定替代配置已经运行。</p>{/if}
              {#if preview.replacementCandidates.length}<p>可到配置管理核对以下候选；此处没有默认选择：</p><ul>{#each preview.replacementCandidates as candidate}<li>{candidate.name}（{candidate.id}）</li>{/each}</ul>{:else}<p>未提供可用替代。请先到配置管理添加并明确选择有效替代，再重新预览；单纯断开 VPN 不会清除当前配置选择。</p>{/if}
              <Button variant="outline" onclick={goProfiles}>前往配置管理</Button>
            </section>
          {/if}
          <section aria-label="固定归属资源"><h3>本次固定资源 · {preview.resources.length} 项</h3>
            <ol>{#each preview.resources as resource (resource.resourceId)}<li>
              <strong>{resource.name}</strong><p>{resource.kind} · {ownershipLabels[resource.ownership]}</p>
              {#if resource.dependencies.length}<p>依赖与关联：</p><ul>{#each resource.dependencies as dependency}<li>{dependency}</li>{/each}</ul>{/if}
              {#if resource.protectedReason}<p role="alert">受保护，不能执行：{resource.protectedReason}</p>{/if}
              <details><summary>核对资源身份</summary><p>{resource.resourceId}</p></details>
            </li>{/each}</ol>
          </section>
          <label class="uninstall-ack"><Choice type="checkbox" checked={view.acknowledged} disabled={!preview.canExecute || preview.requiresManualSwitch || preview.blockedReasons.length>0 || preview.resources.some(r=>r.protectedReason!==null)} onchange={event=>session.acknowledge(event.currentTarget.checked)} aria-label="确认仅清理以上固定归属资源" /><span>我已核对，仅清理以上固定归属资源</span></label>
          <p>确认后按原预览提交一次。结果未知时保留原编号；不因安装列表为空就认定全部完成。</p>
        {/if}
        {#if current && view.phase!=='records' && view.operationId}
          <section aria-label="原卸载操作"><h3>{current.pluginName} · {current.version}</h3>
            <p>原操作编号：{current.operationId}</p><p>插件：{current.pluginId}</p><p>原包摘要：{current.packageDigest}</p>
            <p>开始：{uninstallTime(current.startedAtUnixMs)}；结束：{uninstallTime(current.finishedAtUnixMs)}</p>
            {#if receiptComplete}<p role="status">插件已卸载</p><p>以上结论来自原操作的逐项回执；物理缓存保留或其他剩余范围以宿主说明为准。当前安装状态另行读取，不作为替代回执。</p>{:else}<p role="status">{current.state==='executing'?'原卸载仍在执行':current.state==='residual'?'部分清理完成，仍有剩余':current.state==='manual'?'原操作需要手动处理':'原卸载结果尚未完整确认'}</p>{/if}
            {#if current.reason}<p>{current.reason}</p>{/if}<p>{current.notice}</p>
          </section>
          <section aria-label="逐项卸载结果"><h3>逐项回执 · {current.items.length} 项</h3><ol>{#each current.items as item (item.resourceId)}<li><strong>{item.name}</strong><p>{item.kind} · {itemStateLabel[item.state]}</p>{#if item.reason}<p>{item.reason}</p>{/if}</li>{/each}</ol></section>
          <Button variant="outline" disabled={busy} onclick={check}>只读核对原卸载操作</Button>
          <p>本页只读取原操作的持久回执，不重新扫描当前物理资源。“剩余”表示原记录尚未处理完成，不证明对象当前仍存在。</p>
          <p>本页不提供自动重试、继续清理或更换原操作编号。未知与剩余项只能按原记录核对；不自动重放。</p>
        {/if}
        {#if view.phase==='submitting' || view.phase==='checking'}<p role="status">{view.phase==='submitting'?'正在等待原卸载操作回执…':'正在只读核对原操作…'}</p>{/if}
        {#if view.phase==='records'}
          {#if view.recordsError}<p role="alert">{view.recordsError}</p>{/if}
          {#if !view.records.length && !view.recordsError}<p>宿主没有可展示的卸载记录。此空结果不确认任何旧卸载成功。</p>{/if}
          <section aria-label="宿主持久卸载记录"><p>最多展示 16 条宿主持久记录。旧包或原版本的结果不授予当前同名插件控制权限。</p>
            {#each view.records as record (record.operationId)}<Button variant="outline" onclick={()=>session.showRecord(record.operationId)}><span>{record.pluginName} · {record.version}<br />{record.operationId}<br />{record.state==='completed'?'原操作逐项完成':record.state==='residual'?'仍有剩余':record.state==='manual'?'需手动处理':'原操作待核对'}</span></Button>{/each}
          </section>
        {/if}
      </div>
    </Dialog.Body>
    <Dialog.Footer class="uninstall-footer">
      <Button variant="outline" disabled={busy} onclick={()=>requestClose()}>关闭</Button>
      <Button variant="outline" disabled={busy} onclick={()=>session.recent()}>卸载记录</Button>
      {#if view.phase==='review'}<Button variant="destructive" disabled={!canSubmit || busy} onclick={execute}>卸载</Button>{/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<Dialog.Root open={closeReview!==null} onOpenChange={value=>{if(!value)cancelClose();}}>
  <Dialog.Content class="plugin-uninstall-close" showCloseButton={false}>
    <Dialog.Header><Dialog.Title>{closeReview==='unknown'?'关闭仍需核对的卸载操作？':'放弃本次卸载选择？'}</Dialog.Title><Dialog.Description>{closeReview==='unknown'?'原操作可能已部分执行，关闭只隐藏面板。原编号由宿主持久保留，可从“卸载记录”只读核对；不会撤回、确认或重放。':'尚未提交卸载；关闭会放弃勾选并释放未提交预览，插件和资源保持原状。'}</Dialog.Description></Dialog.Header>
    <Dialog.Footer><Button variant="outline" onclick={cancelClose}>继续查看</Button><Button variant="outline" onclick={closeNow}>明确关闭</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  :global(.plugin-uninstall-dialog) { width:calc(100vw - 24px); max-width:640px; max-height:calc(100dvh - env(safe-area-inset-top,0px) - env(safe-area-inset-bottom,0px) - 24px); }
  :global(.plugin-uninstall-dialog button),:global(.plugin-uninstall-close button) { min-height:44px; white-space:normal; }
  :global(.plugin-uninstall-dialog [data-slot='choice']) { min-height:44px; }
  .uninstall-body { min-width:0; padding-bottom:8px; overflow-wrap:anywhere; }
  .uninstall-ack { display:flex; gap:10px; align-items:center; min-height:44px; padding:8px 0; }
  .uninstall-body h3 { font-size:1rem; font-weight:600; margin:14px 0 8px; }
  .uninstall-body p,.uninstall-body li { margin:7px 0; line-height:1.6; }
  .uninstall-body section { margin:12px 0; }
  .uninstall-body dl { border:1px solid var(--border); padding:10px; border-radius:9px; }
  .uninstall-body dl div { display:grid; grid-template-columns:5.5rem minmax(0,1fr); gap:8px; margin:6px 0; }
  .uninstall-body dd { margin:0; overflow-wrap:anywhere; } .uninstall-body dt { color:var(--muted-foreground); }
  .uninstall-body ol,.uninstall-body ul { padding-left:20px; }
  .uninstall-body ol>li { border-bottom:1px solid var(--border); padding:8px 0; }
  .uninstall-body :global(button) { width:100%; margin:6px 0; text-align:left; justify-content:flex-start; overflow-wrap:anywhere; }
  .uninstall-body summary { min-height:44px; padding:10px 0; cursor:pointer; }
  :global(.uninstall-footer) { flex-wrap:wrap; gap:8px; }
  :global(.plugin-uninstall-close) { max-width:calc(100vw - 24px); max-height:calc(100dvh - 24px); overflow:auto; }
</style>
