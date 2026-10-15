<script lang="ts">
  import { onMount } from 'svelte';
  import MobileVersions from '$lib/components/mobile/MobileVersions.svelte';
  import { provideMobileNavigation, provideMobileRoute } from '$lib/features/mobile/context';
  const navigation = provideMobileNavigation();
  navigation.activeTab = 'more';
  const kernel = new URLSearchParams(window.location.search).has('kernel');
  navigation.open(kernel ? 'kernel' : 'updates');
  provideMobileRoute({ ...navigation.current!, tab: 'more' });
  onMount(() => {
    Object.assign(window, { releaseNavigation: navigation });
    return () => { delete (window as unknown as { releaseNavigation?: unknown }).releaseNavigation; };
  });
</script>
<div style="max-width:404px;margin:auto;min-width:0">
  {#if navigation.current?.page === 'kernel' || navigation.current?.page === 'updates'}<MobileVersions {kernel} />{:else}<p>更多</p>{/if}
</div>
