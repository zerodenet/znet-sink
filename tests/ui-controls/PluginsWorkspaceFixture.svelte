<script lang="ts">
  import AppHeader from '$lib/components/AppHeader.svelte';
  import TabContent from '$lib/components/TabContent.svelte';
  import { store } from '$lib/services/store.svelte';
  import { NAV_TABS } from '$lib/constants/navigation';
  const pro = new URLSearchParams(window.location.search).get('mode') === 'pro';
  store.uiMode = pro ? 'pro' : 'lite';
  store.activeTab = 'plugins';
  store.interactionSurface.navigation = new Map(NAV_TABS.map(tab => [tab.id, { key: tab.id, visible: true, operable: true, readonly: false, category: 'navigation' }]));
</script>
<div class="flex h-full min-h-0 flex-col bg-background"><AppHeader /><TabContent tab={store.activeTab} /></div>
