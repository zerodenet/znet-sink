<script lang="ts">
  import DnsPanel from '../../src/lib/features/dns/Panel.svelte';
  import RoutePanel from '../../src/lib/features/routing/Panel.svelte';
  import { onMount } from 'svelte';
  let requests = $state('[]');
  const action=(detail:string)=>window.dispatchEvent(new CustomEvent('tool-fixture:action',{detail}));
  onMount(()=>{const update=(event:Event)=>requests=JSON.stringify((event as CustomEvent).detail);window.addEventListener('tool-fixture:requests',update);return()=>window.removeEventListener('tool-fixture:requests',update);});
</script>
<main>
<h1>真实工具组件的作用域夹具</h1>
<div class="fixture-actions"><button onclick={()=>action('switch')}>切换配置</button><button onclick={()=>action('restart')}>重启内核</button><button onclick={()=>action('old-clear')}>旧清理事件</button><button onclick={()=>action('dns')}>完成当前 DNS</button><button onclick={()=>action('clear')}>完成当前清理</button></div>
<pre aria-label="任务请求">{requests}</pre>
<section aria-label="DNS 页面"><DnsPanel/></section>
<section aria-label="路由页面"><RoutePanel/></section>
</main>
<style>main{padding:16px;max-width:760px;margin:auto;}section{margin-top:20px;min-width:0;}.fixture-actions{display:flex;flex-wrap:wrap;gap:8px;}button{min-height:44px;border:1px solid var(--border);padding:8px;}pre{overflow-wrap:anywhere;white-space:pre-wrap;}</style>
