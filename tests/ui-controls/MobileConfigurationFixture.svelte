<script lang="ts">
 import MobileConfiguration from '$lib/components/mobile/MobileConfiguration.svelte';
 import { provideMobileNavigation } from '$lib/features/mobile/context';
 const navigation=provideMobileNavigation();navigation.activeTab='configuration';
</script>
<div class="fixture"><MobileConfiguration /></div>
<output aria-label="移动配置导航">{navigation.current?.page ?? 'configuration'}</output>
<style>.fixture { height:calc(100vh - 44px); min-width:0; } output { min-height:44px; display:block; }</style>
