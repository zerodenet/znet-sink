<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { notificationOperation,permissionLabel,type NotificationSnapshot } from '$lib/services/mobile-notifications';
  import { getAppErrorMessage } from '$lib/services/core';
  let snapshot=$state<NotificationSnapshot|null>(null);
  let busy=$state(false);
  let message=$state<string|null>(null);
  let preferenceUnconfirmed=$state(false);
  let generation=0;
  async function perform(command:'mobile_notification_status'|'mobile_notification_request'|'mobile_notification_preference'){
    if(busy)return;
    const current=++generation;busy=true;message=null;
    try{
      const args=command==='mobile_notification_preference'&&snapshot?{expectedRevision:snapshot.expectedRevision,enabled:!snapshot.enabled}:undefined;
      const next=await notificationOperation(command,args);
      if(current===generation){snapshot=next;preferenceUnconfirmed=false;if(command==='mobile_notification_preference')message='可选通知偏好已保存；系统授权与实际呈现仍单独核对。';}
    }catch(error){if(current===generation){if(command==='mobile_notification_preference')preferenceUnconfirmed=true;message=getAppErrorMessage(error,'操作结果未确认，请重新核对通知偏好与系统授权。');}}
    finally{if(current===generation)busy=false;}
  }
  onMount(()=>{
    void perform('mobile_notification_status');
    const resume=()=>{if(!document.hidden)void perform('mobile_notification_status');};
    document.addEventListener('visibilitychange',resume);
    return()=>{generation++;document.removeEventListener('visibilitychange',resume);};
  });
</script>
<div aria-label="可选系统通知设置">
  <p>应用偏好：{snapshot ? snapshot.enabled ? '开启' : '关闭' : '—'}；系统授权：{snapshot ? permissionLabel(snapshot.permission) : '—'}。</p>
  <p>系统通知是否实际呈现还受系统设置、通知渠道和来源适配影响。此偏好不改变应用内提示或 VPN 服务必需状态通知，也不授予插件权限。</p>
  {#if snapshot?.reason}<p role="status">{snapshot.reason}</p>{/if}
  {#if message}<p role="status">{message}</p>{/if}
  <Button variant="outline" disabled={busy||!snapshot||preferenceUnconfirmed} onclick={()=>perform('mobile_notification_preference')}>保存可选通知偏好为{snapshot?.enabled ? '关闭' : '开启'}</Button>
  <Button variant="outline" disabled={busy||!snapshot||snapshot.permission==='unknown'||snapshot.permission==='unsupported'||snapshot.permission==='granted'} onclick={()=>perform('mobile_notification_request')}>请求系统通知授权</Button>
  <Button variant="outline" disabled={busy} onclick={()=>perform('mobile_notification_status')}>{busy ? '正在核对…' : '重新核对偏好与系统授权'}</Button>
</div>
<style>
  p { font-size:.85rem; color:var(--muted-foreground); line-height:1.6; overflow-wrap:anywhere; }
  div :global(button) { width:100%; min-height:44px; height:auto; margin:7px 0; white-space:normal; padding:10px 12px; }
</style>
