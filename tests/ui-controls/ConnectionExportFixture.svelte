<script lang="ts">
 import {onMount} from 'svelte';
 import ConnectionExportDialog from '$lib/components/mobile/ConnectionExportDialog.svelte';
 import type {ConnectionExportGateway} from '$lib/features/connection-export/types';
 import type {DebugFrameQuery} from '$lib/types/debug';
 const params=new URLSearchParams(window.location.search);
 const mode=params.get('export-result')??'saved';
 const source=params.get('export-source')==='active'?'active':'completed-history';
 let open=$state(true);
 let query=$state.raw<DebugFrameQuery>({search:'first-filter',protocol:'tcp'});
 let calls=$state<string[]>([]);
 let previews=$state<string[]>([]);
 // Explicit browser fixture only: no filesystem, Android, iOS or real flow execution.
 const gateway:ConnectionExportGateway={
  preview:async fixed=>{previews=[...previews,JSON.stringify(fixed)];return{source:'completed-history',coreInstanceId:null,configRevision:null,observedAtUnixMs:null,planId:'fixed-test-snapshot',records:2,truncated:false,bytes:640,expiresAtUnixMs:'999999999',filtersFingerprint:'test-only-digest',omissions:['排除URL、凭据、自由文本错误、进程与协议材料。']};},
  previewActive:async fixed=>{previews=[...previews,JSON.stringify(fixed)];if(params.has('runtime-unavailable'))throw {code:'unsupported',message:'移动运行服务控制桥尚未适配，不能读取活动连接'};return{source:'active',coreInstanceId:'test-core-instance',configRevision:'9007199254740993',observedAtUnixMs:'1791936000000',planId:'fixed-active-snapshot',records:2,truncated:false,bytes:640,expiresAtUnixMs:'999999999',filtersFingerprint:'test-only-digest',omissions:['真实快照的固定测试契约；不改变后续活动对象。']};},
  deliver:async(planId,delivery)=>{calls=[...calls,JSON.stringify({planId,delivery})];return{operationId:'test-document-operation',status:mode as 'saved'|'cancelled'|'result-unknown'|'share-accepted',content:null,bytesRead:null,reason:null};},
 };
 onMount(()=>{window.addEventListener('fixture-connection-export-filter',changeFilter);return()=>window.removeEventListener('fixture-connection-export-filter',changeFilter);});
 function changeFilter(){query={search:'later-filter',protocol:'udp'};}
</script>
<p>测试夹具：模拟原生文档回执；不读写实际连接或VPN。</p>
<button data-slot="surface-button" onclick={changeFilter}>更改外部筛选</button>
<output id="connection-export-calls">{JSON.stringify(calls)}</output>
<output id="connection-export-previews">{JSON.stringify(previews)}</output>
<ConnectionExportDialog bind:open {query} {source} {gateway}/>
