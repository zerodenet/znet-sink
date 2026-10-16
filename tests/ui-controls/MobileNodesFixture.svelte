<script lang="ts">
 import MobileNodesList from '$lib/components/mobile/MobileNodesList.svelte';
 import type { PolicyGroup } from '$lib/types/gui-api';
 import type { ProxyNode } from '$lib/types/protocol';
 let query=$state('');
 let action=$state('');
 const nodes: ProxyNode[]=['node-a','node-b','independent'].map(tag=>({id:tag,tag,name:tag,protocol:'vmess',delay:0,domain:''}));
 const groups: PolicyGroup[]=Array.from({length:20},(_,index)=>({name:`group-${index}`,kind:index===1?'urltest':index===2?'fallback':index===3?'relay':'selector',outbounds:[{tag:'node-a',type:'vmess'},{tag:'node-b',type:'vmess'}],selected:'node-a',available:true}));
</script>
<div class="fixture"><MobileNodesList {groups} {nodes} bind:query scopeKey="profile-1" ready={true} busy={false} probingTags={new Set()} jobs={[]} error={null} loading={false} onSelect={(node,group)=>{action=`select:${group}:${node.tag}`;}} onProbe={node=>{action=`probe:${node.tag}`;}} onProbeGroup={group=>{action=`group:${group.name}`;}}/></div>
<output aria-label="移动节点操作">{action}</output>
<style>.fixture { height:calc(100vh - 44px); min-width:0; } output { min-height:44px; display:block; overflow-wrap:anywhere; }</style>
