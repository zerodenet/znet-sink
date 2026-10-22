<script lang="ts">
 import {onMount,untrack} from 'svelte';
 import {Button} from '$lib/components/ui/button';
 import {Input} from '$lib/components/ui/input';
 import * as Dialog from '$lib/components/ui/dialog';
 import MobileSettingsDraftGuard from '$lib/components/mobile/MobileSettingsDraftGuard.svelte';
 import {optionalMobileNavigation,optionalMobileRoute} from '$lib/features/mobile/context';
 import {runtimeNetworkSession as session} from '$lib/features/runtime-network/session';
 import {proxyConfigSignal} from '$lib/services/proxy-config-signal.svelte';
 import type {NetworkDraft,NetworkKey} from '$lib/features/runtime-network/types';
 const navigation=optionalMobileNavigation();
 const route=optionalMobileRoute();
 const active=()=>!navigation||!route||navigation.activeTab===route.tab&&navigation.current?.key===route.key;
 let discardOpen=$state(false);
 function discardAndReload(){if(busy||session.unresolved)return;session.discardDraft();discardOpen=false;void session.load();}
 $effect(()=>{if(active())untrack(()=>void session.load());});
 let view=$state.raw(session.view);
 const busy=$derived(['loading','previewing','saving'].includes(view.phase)||view.checking||view.submitting);
 const dirty=$derived(JSON.stringify(view.draft)!==JSON.stringify(view.baseline));
 const editable=$derived(!busy&&!view.stale&&!view.snapshot?.readOnlyReason&&!!view.snapshot?.binding&&!!view.snapshot.values&&['editing','saved'].includes(view.phase));
 const previewOpen=$derived(['confirming','saving'].includes(view.phase)||view.submitting);
 const origins:Record<string,string>={client:'客户端全局覆盖','source-tun':'来源 TUN 覆盖','source-network':'来源运行网络','source':'来源配置',default:'内核 schema 默认'};
 const exposure:Record<string,string>={loopback:'仅本机回环','all-interfaces':'全部对应地址族接口','specific-address':'指定地址，尚未验证设备可绑定'};
 const labels:Record<NetworkKey,string>={'tun.mtu':'MTU','runtime.udpUpstreamIdleTimeoutSeconds':'UDP 上游空闲时间','localProxy.host':'监听 IP 地址','localProxy.port':'监听端口'};
 function edit(key:keyof NetworkDraft,value:string|number|undefined){session.edit(key,String(value??''));}
 function restore(keys:NetworkKey[]){if(!busy&&!session.unresolved)void session.preview(keys);}
 function workspace(){navigation?.open('workspace');}
 onMount(()=>{const stop=session.subscribe(next=>view=next);const changed=proxyConfigSignal.onActiveChanged(()=>session.scopeChanged());const resume=()=>{if(document.visibilityState==='visible'&&active())void session.load();};document.addEventListener('visibilitychange',resume);return()=>{stop();changed();document.removeEventListener('visibilitychange',resume);};});
</script>
<MobileSettingsDraftGuard {dirty} busy={view.submitting||view.phase==='previewing'} label="运行网络设置" onDiscard={()=>session.discardDraft()}/>
<div class="network-editor">
 <p>本地保存与运行应用分开。当前入口只保存普通配置上下文中的全局客户端网络覆盖，未验证设备监听、LAN、物理出口或 VPN。</p>
 {#if view.snapshot?.profileId}<p>固定来源：{view.snapshot.profileName} · <code>{view.snapshot.profileId}</code></p>{/if}
 {#if view.message}<p role={view.phase==='error'?'alert':'status'}>{view.message}</p>{/if}
 {#if view.snapshot?.readOnlyReason}<p role="alert">{view.snapshot.readOnlyReason}</p>{/if}
 {#if view.snapshot?.values}
  <section aria-label="MTU 与 UDP 设置"><h2>MTU 与连接生命周期</h2>
   <label>MTU<Input type="text" inputmode="numeric" value={view.draft.mtu} oninput={e=>edit('mtu',e.currentTarget.value)} disabled={!editable} aria-label="运行网络 MTU"/></label>
   <p>来源：{origins[view.snapshot.values.mtuOrigin]}。576–65535 整数；IPv6、接口和实际运行仍有附加限制，不代表每台设备均可运行。TUN 页复用此最终值。</p>
   <Button variant="outline" disabled={!editable||!view.snapshot.editedFields.includes('tun.mtu')} onclick={()=>restore(['tun.mtu'])}>预览恢复来源 MTU</Button>
   <label>UDP 上游空闲时间（秒）<Input type="text" inputmode="numeric" value={view.draft.udpIdleSeconds} oninput={e=>edit('udpIdleSeconds',e.currentTarget.value)} disabled={!editable} aria-label="UDP 上游空闲时间"/></label>
   <p>来源：{origins[view.snapshot.values.udpOrigin]}。使用正整数秒，完整保留 u64；保存不会发起测速或改变手机 Wi-Fi。</p>
   <Button variant="outline" disabled={!editable||!view.snapshot.editedFields.includes('runtime.udpUpstreamIdleTimeoutSeconds')} onclick={()=>restore(['runtime.udpUpstreamIdleTimeoutSeconds'])}>预览恢复来源 UDP 时间</Button>
  </section>
  <section aria-label="本地监听与 LAN"><h2>本地代理监听</h2>
   {#if view.snapshot.values.listener}<p>原配置实际监听：{view.snapshot.values.listener.tag} · {view.snapshot.values.listener.protocol}；只修改这个原对象，不创建新监听。</p>
    <label>监听 IP 地址<Input value={view.draft.host} oninput={e=>edit('host',e.currentTarget.value)} disabled={!editable} spellcheck="false" aria-label="本地监听 IP"/></label>
    <label>监听端口<Input type="text" inputmode="numeric" value={view.draft.port} oninput={e=>edit('port',e.currentTarget.value)} disabled={!editable} aria-label="本地监听端口"/></label>
    <p>当前已保存范围：{exposure[view.snapshot.values.listener.exposure]}。127.0.0.1 / ::1 限本机；0.0.0.0 / :: 面向相应地址族所有接口。其他地址不证明当前设备拥有它；非回环可能向网络暴露代理，须在固定预览中确认。</p>
    <div class="actions"><Button variant="outline" disabled={!editable} onclick={()=>edit('host','127.0.0.1')}>仅本机 IPv4</Button><Button variant="outline" disabled={!editable} onclick={()=>edit('host','0.0.0.0')}>填写 IPv4 LAN</Button><Button variant="outline" disabled={!editable} onclick={()=>edit('host','::1')}>仅本机 IPv6</Button><Button variant="outline" disabled={!editable} onclick={()=>edit('host','::')}>填写 IPv6 LAN</Button></div>
    <Button variant="outline" disabled={!editable||!view.snapshot.editedFields.some(k=>k.startsWith('localProxy.'))} onclick={()=>restore(view.snapshot!.editedFields.filter(k=>k.startsWith('localProxy.')))}>预览恢复来源监听</Button>
   {:else}<p>{view.snapshot.values.listenerReason}</p>{/if}
  </section>
  <section aria-label="地址族与物理出口"><h2>地址族与物理出口</h2><p>这些是每个 Direct leaf 的 dial 配置，不是全局 DNS 地址族或 TUN 双栈开关。本页只读；当前移动运行服务及设备出口选择未适配。需完整原生配置时使用工作区，不据此声称已修改系统网络。</p>
   {#if !view.snapshot.dialLeaves.length}<p>当前来源没有 Direct leaf。</p>{:else}<ul>{#each view.snapshot.dialLeaves as leaf}<li><strong>{leaf.tag}</strong><span>地址族：{leaf.addressFamily}；接口：{leaf.interface??'未固定'}；源 IP：{leaf.sourceIp??'未固定'}</span></li>{/each}</ul>{/if}
   {#if navigation}<Button variant="outline" disabled={busy} onclick={workspace}>打开配置工作区</Button>{/if}
  </section>
 {/if}
 {#if (dirty||view.stale||view.phase==='error'&&view.snapshot)&&view.phase!=='unknown'}<Button variant="outline" disabled={busy} onclick={()=>discardOpen=true}>放弃草稿并重新读取网络来源</Button>{/if}
 <div class="actions"><Button variant="outline" disabled={busy||view.phase==='unknown'} onclick={()=>void session.load()}>重新核对网络来源</Button>{#if view.phase==='unknown'}<Button variant="outline" disabled={busy} onclick={()=>void session.check()}>只读核对原网络保存</Button>{:else}<Button disabled={!editable||!dirty} onclick={()=>void session.preview()}>预览本地网络设置</Button>{/if}</div>
 {#if view.phase==='unknown'&&view.plan}<p>原操作 <code>{view.plan.planId}</code> 保留在当前宿主；核对不重新保存，不补造运行回执。宿主重建后不承诺恢复这个内存操作。</p>{/if}
</div>
<Dialog.Root open={discardOpen} onOpenChange={value=>discardOpen=value}><Dialog.Content><Dialog.Header><Dialog.Title>放弃网络草稿并重读？</Dialog.Title><Dialog.Description>仅丢弃当前未保存输入，再读取当前宿主来源。不会保存、恢复来源覆盖或改变内核；旧来源不会授权新配置。</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={()=>discardOpen=false}>继续编辑网络草稿</Button><Button variant="destructive" disabled={busy} onclick={discardAndReload}>确认放弃并重新读取</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<Dialog.Root open={previewOpen} onOpenChange={value=>{if(!value&&!busy)session.cancelPreview();}}><Dialog.Content class="network-preview" showCloseButton={!busy} onEscapeKeydown={e=>{if(busy)e.preventDefault();}} onInteractOutside={e=>{if(busy)e.preventDefault();}}><Dialog.Header><Dialog.Title>确认仅保存本地网络设置</Dialog.Title><Dialog.Description>保存后仍待应用，不调用内核、监听、系统代理或 VPN。</Dialog.Description></Dialog.Header><Dialog.Body>
 {#if view.plan}<p>固定配置：{view.plan.profileId}</p><p>固定字段：{[...view.plan.changedFields,...view.plan.resetFields].map(k=>labels[k]).join('、')}</p>
  {#each [{label:'原本地视图',value:view.plan.before},{label:'保存后本地视图',value:view.plan.after}] as item}<section aria-label={item.label}><h3>{item.label}</h3><p>MTU {item.value.mtu} · UDP {item.value.udpIdleSeconds} 秒</p><p>监听：{item.value.listener?`${item.value.listener.tag} · ${item.value.listener.host}:${item.value.listener.port}`:'未声明，不新建'}</p></section>{/each}
  {#if view.plan.lanExposureWarning}<p role="alert">将保存非回环监听范围，可能向网络暴露代理；没有据此验证设备绑定、防火墙、身份认证或平台可用性。</p>{/if}<p>{view.plan.notice}</p>
 {/if}
 </Dialog.Body><Dialog.Footer><Button variant="outline" disabled={busy} onclick={()=>session.cancelPreview()}>返回编辑网络设置</Button><Button disabled={busy} onclick={()=>void session.save()}>{view.phase==='saving'?'等待原本地保存':'确认仅保存本地'}</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<style>
 .network-editor{padding:14px;display:grid;gap:14px;min-width:0;font-size:.875rem;line-height:1.6;overflow-wrap:anywhere}.network-editor p{margin:0;color:var(--muted-foreground)}.network-editor section{padding:14px;border:1px solid var(--border);border-radius:10px;display:grid;gap:10px;min-width:0}.network-editor label{display:grid;gap:6px}.network-editor h2{margin:0;font-size:1rem;font-weight:500}.actions{display:flex;gap:8px;flex-wrap:wrap}.network-editor :global(button){min-height:44px;white-space:normal}.network-editor :global(input){min-height:44px;font-size:1rem}.network-editor ul{padding:0;list-style:none;display:grid;gap:8px}.network-editor li{display:grid;gap:3px;min-width:0}code{overflow-wrap:anywhere}:global(.network-preview){max-width:min(96vw,640px);overflow-wrap:anywhere}:global(.network-preview button){min-height:44px}
</style>
