<script lang="ts">
 import {onMount} from 'svelte';
 import ConnectionInspectorWorkspace from '$lib/components/tabs/ConnectionInspectorWorkspace.svelte';
 import {connectionObservations} from '$lib/features/observations/connections.svelte';
 import type {GuiConnectionItem} from '$lib/types/gui-api';
 let mounted=$state(true);
 const item=(id:string):GuiConnectionItem=>({flowId:id,coreInstanceId:'old-display-core',network:'tcp',destination:`old-display-${id}.test:443`,source:'127.0.0.1:1000',selectionChain:[],relayChain:[],bytesUp:0,bytesDown:0,startedAtUnixMs:1792065500000,state:'active'});
 onMount(()=>{connectionObservations.apply({type:'snapshot',connections:[item('1')]});const push=()=>connectionObservations.apply({type:'started',connection:item('99')});const mount=(event:Event)=>{mounted=(event as CustomEvent<boolean>).detail;};window.addEventListener('fixture-flow-event',push);window.addEventListener('fixture-flow-mount',mount);return()=>{window.removeEventListener('fixture-flow-event',push);window.removeEventListener('fixture-flow-mount',mount);connectionObservations.reset();};});
</script>
<p class="fixture-note">测试宿主事实夹具；不执行真实连接关闭。</p>
<div class="fixture-workspace">{#if mounted}<ConnectionInspectorWorkspace/>{/if}</div>
<style>.fixture-note{font-size:12px;margin:4px 8px;}.fixture-workspace{height:calc(100vh - 42px);min-width:0;display:flex;}</style>
