<script lang="ts">
  import ProfilesTab from '$lib/components/tabs/ProfilesTab.svelte';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  import { provideMobileNavigation } from '$lib/features/mobile/context';
  provideMobileNavigation();
  mobilePlatform.runtimePlatform = 'android';
  mobilePlatform.isMobileLayout = true;
</script>
<div class="native-documents-fixture"><ProfilesTab mobile /></div>
<style>.native-documents-fixture { max-width:404px; min-width:0; margin:auto; padding:12px; }</style>
