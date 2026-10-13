<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from '$lib/components/ui/button';
 import {Input} from '$lib/components/ui/input';
 import * as Dialog from '$lib/components/ui/dialog';
 import {getNativeDocumentCapabilities,documentReceiptMessage,type DocumentDelivery} from '$lib/services/native-documents';
 import {previewNativeDiagnostics,deliverNativeDiagnosticSnapshot,readNativeCapture,startNativeCapture,stopNativeCapture,validateCaptureBudget,captureReason,type DiagnosticPreview,type CaptureStatus} from '$lib/services/native-diagnostics';
 import {getAppErrorMessage} from '$lib/services/core';
 let {disabled=false}:{disabled?:boolean}=$props();
 let open=$state(false),available=$state(false),busy=$state(false),preparing=$state(false),captureBusy=$state(false);
 let message=$state<string|null>(null),captureMessage=$state<string|null>(null);
 let preview=$state.raw<DiagnosticPreview|null>(null),capture=$state.raw<CaptureStatus|null>(null);
 let duration=$state('30'),capacity=$state('1'),lastCaptureRead=$state<number|null>(null);
 let alive=true;
 async function readCapture(){if(captureBusy)return;captureBusy=true;try{const status=await readNativeCapture();if(alive){capture=status;lastCaptureRead=Date.now();captureMessage=null;}}catch(error){if(alive)captureMessage=getAppErrorMessage(error,'捕获状态读取失败，不能推断已停止');}finally{if(alive)captureBusy=false;}}
 onMount(()=>{
  void getNativeDocumentCapabilities().then(value=>{if(alive)available=value.available;}).catch(()=>{if(alive)message='系统保存与分享适配暂不可用';});
  void readCapture();const resume=()=>{if(document.visibilityState==='visible')void readCapture();};document.addEventListener('visibilitychange',resume);
  return()=>{alive=false;document.removeEventListener('visibilitychange',resume);};
 });
 async function prepare(){if(preparing||busy)return;open=true;preparing=true;message=null;preview=null;
  try{const next=await previewNativeDiagnostics();if(alive)preview=next;}catch(error){if(alive)message=getAppErrorMessage(error,'诊断快照准备失败，未交付文件');}finally{if(alive)preparing=false;}
 }
 async function deliver(delivery:DocumentDelivery){const fixed=preview;if(busy||!available||!fixed)return;busy=true;
  // Retire the preview before dispatch. Unknown system delivery is never
  // automatically replayed, including after dialog reopen.
  preview=null;
  try{const receipt=await deliverNativeDiagnosticSnapshot(fixed.planId,delivery);if(alive)message=documentReceiptMessage(receipt);}catch(error){if(alive)message=getAppErrorMessage(error,'系统导出结果未确认，请检查目标；不会自动重试');}finally{if(alive)busy=false;}
 }
 async function startCapture(){if(captureBusy)return;const budget=validateCaptureBudget(duration,capacity);if(!budget){captureMessage='时长须为1–300秒、容量须为1–10MiB整数';return;}captureBusy=true;captureMessage=null;
  try{const status=await startNativeCapture(budget.durationSeconds,budget.capacityMiB);if(alive){capture=status;lastCaptureRead=Date.now();}}catch(error){if(alive)captureMessage=getAppErrorMessage(error,'捕获开启未确认，请先重新核对');}finally{if(alive)captureBusy=false;}
 }
 async function stopCapture(){const id=capture?.sessionId;if(captureBusy||!id)return;captureBusy=true;
  try{const status=await stopNativeCapture(id);if(alive){capture=status;lastCaptureRead=Date.now();captureMessage=null;}}catch(error){if(alive)captureMessage=getAppErrorMessage(error,'捕获停止未确认，请先重新核对');}finally{if(alive)captureBusy=false;}
 }
</script>
<section class="native-diagnostics" aria-label="有限调试捕获与诊断导出">
 <h3>有限调试捕获</h3>
 <p>捕获由宿主服务维护；达到任一预算会停止。离开页面不会停止或延长捕获。记录来自实际 IPC，不提供 PCAP 全包捕获或移动内核未提供的采样。</p>
 <div class="capture-budget"><label>时长（秒）<Input type="number" min="1" max="300" step="1" bind:value={duration} disabled={captureBusy||capture?.detailed} aria-label="捕获时长（秒）"/></label><label>容量（MiB）<Input type="number" min="1" max="10" step="1" bind:value={capacity} disabled={captureBusy||capture?.detailed} aria-label="捕获容量（MiB）"/></label></div>
 {#if capture}<p role="status">{capture.state==='idle' ? '尚未捕获' : captureReason(capture)}；已存 {capture.storedBytes} / {capture.maximumBytes} 字节，{capture.storedFrames} 帧。观察到队列丢弃 {capture.queueDropsObserved ?? '—'} 帧。</p>{/if}
 {#if lastCaptureRead}<p>状态读取时间：{new Date(lastCaptureRead).toLocaleTimeString()}；显示值为最近一次宿主回执。</p>{/if}
 {#if captureMessage}<p role="alert">{captureMessage}</p>{/if}
 <div class="capture-actions"><Button variant="outline" disabled={disabled||captureBusy||capture?.detailed} onclick={startCapture}>开始有界捕获</Button><Button variant="outline" disabled={captureBusy||!capture?.detailed} onclick={stopCapture}>停止原捕获会话</Button><Button variant="outline" disabled={captureBusy} onclick={readCapture}>核对捕获状态</Button></div>
 <Button variant="outline" disabled={disabled||preparing||busy} onclick={prepare}>{preparing?'正在准备快照…':'预览脱敏诊断材料'}</Button>
</section>
<Dialog.Root bind:open>
 <Dialog.Content>
  <Dialog.Header><Dialog.Title>固定诊断快照范围</Dialog.Title><Dialog.Description>宿主已从有界存储准备脱敏材料，确认只交付此快照。原始日志消息、错误正文、配置、来源 URL、凭据及插件秘密均不导出。</Dialog.Description></Dialog.Header>
  {#if preparing}<p role="status">正在读取实际材料并脱敏…</p>{/if}
  {#if preview}<article aria-label="固定诊断快照"><p>日志 {preview.logCount} 条、有效已存 IPC 帧 {preview.frameCount ?? '—（读取失败）'} 条、任务 {preview.taskCount} 条；文件 {preview.bytes} 字节（最高2MiB）。</p><p>配置：{preview.scope.profileId ?? '无活动配置'} · 客户端实例 {preview.scope.clientInstanceId} · revision {preview.scope.configRevision}</p><p>快照时间：{new Date(Number(preview.createdAtUnixMs)).toLocaleString()}。五分钟内有效；配置或实例变化需重新预览。</p><details><summary>查看省略范围与原因</summary>{#each preview.omissions as omission}<p>{omission.category}：{omission.reason}{omission.count!=null && omission.count>0 ? `（${omission.count}项）`:''}</p>{/each}</details></article>{/if}
  {#if !available}<p role="status">系统保存与分享适配暂不可用，可查看预览但不能交付文件。</p>{/if}
  {#if message}<p role="status">{message}</p>{/if}
  <Dialog.Footer><Button variant="outline" disabled={busy} onclick={()=>open=false}>返回</Button><Button variant="outline" disabled={!available||busy||!preview} onclick={()=>deliver('share')}>系统分享原快照</Button><Button disabled={!available||busy||!preview} onclick={()=>deliver('save')}>{busy?'等待系统结果…':'保存原快照'}</Button></Dialog.Footer>
 </Dialog.Content>
</Dialog.Root>
<style>
 .native-diagnostics { min-width:0; padding:12px 0; } h3 {font-size:1rem;font-weight:500;} p {font-size:.85rem;line-height:1.6;overflow-wrap:anywhere;color:var(--muted-foreground);} p[role="alert"] {color:var(--destructive);}
 .capture-budget {display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px;}.capture-budget label {min-width:0;font-size:.85rem;}.capture-budget :global(input) {min-height:44px;width:100%;font-size:1rem;}
 .capture-actions {display:flex;flex-wrap:wrap;gap:8px;margin:10px 0;}.native-diagnostics :global(button){min-height:44px;height:auto;white-space:normal;} summary{min-height:44px;padding:12px 0;cursor:pointer;}
</style>
