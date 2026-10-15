<script lang="ts">
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { optionalMobileNavigation, optionalMobileRoute } from '$lib/features/mobile/context';
  import type { EndpointTrafficVisit } from '$lib/features/endpoints/traffic-navigation';
  import { Button } from '$lib/components/ui/button';
  import { onMount } from 'svelte';
  import NetworkEndpointsPanel from './NetworkEndpointsPanel.svelte';
  import TrafficStatisticsPanel from './TrafficStatisticsPanel.svelte';
  import { useTrafficWorkspace } from '$lib/features/traffic/context';
  const sourceRoute = optionalMobileRoute();
  const navigation = optionalMobileNavigation();
  let endpointVisit = $state.raw<EndpointTrafficVisit | undefined>();
  function showTraffic(visit: EndpointTrafficVisit) {
    if (navigation) navigation.open('traffic', visit);
    else { endpointVisit = visit; tab = 'traffic'; workspace.tab = tab; }
  }
  function returnToEndpoint() {
    const returning = endpointVisit; endpointVisit = undefined; tab = 'endpoints'; workspace.tab = tab; returning?.restore();
  }
  const workspace = useTrafficWorkspace();
  const session = workspace.session;
  let view = $state(session.view);
  let tab = $state(workspace.tab);
  onMount(()=> workspace.connect(next => { view = next; }));
</script>
<div class="endpoints-workspace animate-fade-in">
  <nav aria-label="端点工作区"><SegmentedControl.Root value={tab} onValueChange={value=>{if(value==='endpoints' && endpointVisit) returnToEndpoint(); else {tab=value as typeof tab;workspace.tab=tab;}}} aria-label="端点工作区视图"><SegmentedControl.Item value="endpoints">端点</SegmentedControl.Item><SegmentedControl.Item value="traffic">流量统计</SegmentedControl.Item></SegmentedControl.Root></nav>
  <div class="workspace-pane" hidden={tab!=='endpoints'} inert={tab!=='endpoints'}><NetworkEndpointsPanel trafficView={view} trafficSession={session} onTraffic={showTraffic} sourceIsVisible={()=>tab==='endpoints' && (!navigation || (navigation.activeTab===sourceRoute?.tab && navigation.current?.key===sourceRoute?.key))}/></div>
  {#if tab==='traffic'}{#if endpointVisit}<Button variant="ghost" onclick={returnToEndpoint}>返回原端点详情</Button>{/if}<TrafficStatisticsPanel {session} {view} {endpointVisit}/>{/if}
</div>
<style>
  .workspace-pane { display:flex; flex:1; min-height:0; min-width:0; } .workspace-pane[hidden] { display:none; }
  .endpoints-workspace { flex:1; display:flex; flex-direction:column; min-width:0; min-height:0; overflow:hidden; background:var(--card); border:1px solid var(--border); border-radius:10px; } nav { display:flex; gap:4px; padding:10px 20px; border-bottom:1px solid var(--border); }
</style>
