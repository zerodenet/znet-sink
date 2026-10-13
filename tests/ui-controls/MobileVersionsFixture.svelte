<script lang="ts">
  import MobileVersions from '$lib/components/mobile/MobileVersions.svelte';
  import { provideMobileNavigation } from '$lib/features/mobile/context';
  provideMobileNavigation();
  const kernel = new URLSearchParams(window.location.search).has('kernel');
</script>
<div style="max-width:404px;margin:auto;min-width:0"><MobileVersions {kernel} /></div>
