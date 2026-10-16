<script lang="ts">
  import MobileAbout from '$lib/components/mobile/MobileAbout.svelte';
  import {provideMobileNavigation} from '$lib/features/mobile/context';
  provideMobileNavigation();
</script>
<div style="max-width:404px;margin:auto;min-width:0"><MobileAbout/></div>
