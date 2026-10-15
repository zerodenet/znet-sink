<script lang="ts">
  import { optionalMobileRoute } from '$lib/features/mobile/context';
  import { onMount } from 'svelte';
  import { useTrafficWorkspace } from '$lib/features/traffic/context';
  import type { TrafficView } from '$lib/features/traffic/session';
  import TrafficStatisticsPanel from '$lib/components/tabs/TrafficStatisticsPanel.svelte';
  const visit = optionalMobileRoute()?.trafficVisit;
  const workspace = useTrafficWorkspace();
  let view = $state.raw<TrafficView>(workspace.session.view);
  onMount(() => workspace.connect(next => { view = next; }));
</script>
<TrafficStatisticsPanel session={workspace.session} {view} endpointVisit={visit} />
