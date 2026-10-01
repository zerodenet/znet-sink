<script lang="ts">
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { onMount } from 'svelte';
  import NetworkEndpointsPanel from './NetworkEndpointsPanel.svelte';
  import TrafficStatisticsPanel from './TrafficStatisticsPanel.svelte';
  import { trafficGateway } from '$lib/features/traffic/client';
  import { TrafficSession } from '$lib/features/traffic/session';
  const session = new TrafficSession(trafficGateway, next => { view = next; });
  let view = $state(session.view);
  let tab = $state<'endpoints'|'traffic'>('endpoints');
  onMount(()=> { void session.start(); return ()=>session.dispose(); });
</script>
<div class="endpoints-workspace animate-fade-in">
  <nav aria-label="端点工作区"><SegmentedControl.Root value={tab} onValueChange={value=>tab=value as typeof tab} aria-label="端点工作区视图"><SegmentedControl.Item value="endpoints">端点</SegmentedControl.Item><SegmentedControl.Item value="traffic">流量统计</SegmentedControl.Item></SegmentedControl.Root></nav>
  {#if tab==='endpoints'}<NetworkEndpointsPanel trafficView={view} trafficSession={session}/>{:else}<TrafficStatisticsPanel {session} {view}/>{/if}
</div>
<style>
  .endpoints-workspace { flex:1; display:flex; flex-direction:column; min-width:0; min-height:0; overflow:hidden; background:var(--card); border:1px solid var(--border); border-radius:10px; } nav { display:flex; gap:4px; padding:10px 20px; border-bottom:1px solid var(--border); }
</style>
