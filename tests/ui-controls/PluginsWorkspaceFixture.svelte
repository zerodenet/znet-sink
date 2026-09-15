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
<div class="flex h-full min-h-0 flex-col bg-background"><AppHeader /><div class="plugin-fixture-content"><TabContent tab={store.activeTab} /></div></div>

<style>
  .plugin-fixture-content { flex: 1; min-height: 0; display: flex; padding: 14px 20px; }
  @media (max-width: 640px) { .plugin-fixture-content { padding: 12px; } }
</style>
