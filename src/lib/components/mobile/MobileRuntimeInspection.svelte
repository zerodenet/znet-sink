<script lang="ts">
  import {onMount} from 'svelte';
  import {Button} from '$lib/components/ui/button';
  import {inspectRuntimeBridge,type RuntimeInspection} from '$lib/features/mobile/runtime-inspection';
  import {getAppErrorMessage} from '$lib/services/core';
  let busy=$state(false), result=$state.raw<RuntimeInspection|null>(null), message=$state<string|null>(null);
  let alive=true;
  onMount(()=>()=>{alive=false;});
  async function inspect(){
    if(busy)return;busy=true;result=null;message=null;
    try{const receipt=await inspectRuntimeBridge();if(alive)result=receipt;}
    catch(error){if(alive)message=getAppErrorMessage(error,'控制通道读取未确认，请重新核对。');}
    finally{if(alive)busy=false;}
  }
</script>
<section aria-label="运行控制通道核对" class="runtime-inspection">
  <h3>运行控制通道</h3>
  <p>只读核对当前配置的实际内核实例与版本。此结果不确认 VPN 接管或网络转发；移动运行服务尚未适配时会说明原因。</p>
  <Button variant="outline" disabled={busy} onclick={inspect}>{busy?'正在核对控制通道…':'核对运行控制通道'}</Button>
  {#if message}<p role="alert">{message}</p>{/if}
  {#if result}<article aria-label="控制通道核对回执"><p role="status">运行身份读取已确认；VPN 接管状态未提供。</p><p>实例：{result.coreInstanceId}</p><p>配置版本：{result.configRevision}</p><p>所属配置：{result.receipt.profileId}</p><p>读取回执：{result.receipt.operationId}</p></article>{/if}
</section>
<style>
  .runtime-inspection {padding:12px 0;min-width:0;} h3{font-size:1rem;font-weight:500;} p{font-size:.85rem;line-height:1.6;overflow-wrap:anywhere;color:var(--muted-foreground);} p[role="alert"]{color:var(--destructive);}.runtime-inspection :global(button){min-height:44px;height:auto;white-space:normal;}
</style>
