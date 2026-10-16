<script lang="ts">
  import {untrack} from 'svelte';
  import MobileWorkspace from '$lib/components/mobile/MobileWorkspace.svelte';
  import {provideMobileRoute, type MobileRouteOwner} from '$lib/features/mobile/context';
  let {route}: {route: MobileRouteOwner} = $props();
  // The keyed parent creates a new owner component for each route identity.
  untrack(() => provideMobileRoute(route));
</script>
<MobileWorkspace />
