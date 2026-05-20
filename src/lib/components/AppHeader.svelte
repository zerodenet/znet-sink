<script lang="ts">
  import * as Tabs from "$lib/components/ui/tabs";
  import { store } from '$lib/services/store.svelte';
  import { NAV_TABS } from '$lib/constants/navigation';
  import ConnectionStatusBadge from '$lib/components/ConnectionStatusBadge.svelte';
</script>

<header class="w-full flex-shrink-0">
  <!-- 导航菜单 -->
  <div class="w-full flex items-center justify-between py-1">
    <div class="w-24"></div>
    <Tabs.Root value={store.activeTab} onValueChange={(v) => store.activeTab = v}>
      <Tabs.List class="flex gap-1 bg-transparent p-0">
        {#each NAV_TABS as tab}
          {#if store.isNavVisible(tab.id)}
            <Tabs.Trigger 
              value={tab.id} 
              class="px-4 py-1.5 rounded-xl font-bold transition-all text-xs 
                     data-[state=active]:bg-primary data-[state=active]:text-primary-foreground 
                     data-[state=active]:shadow-sm text-muted-foreground 
                     hover:text-foreground hover:bg-muted/30
                     {!store.isNavOperable(tab.id) ? 'opacity-50 cursor-not-allowed' : ''}"
              disabled={!store.isNavOperable(tab.id)}
            >
              {tab.label}
            </Tabs.Trigger>
          {/if}
        {/each}
      </Tabs.List>
    </Tabs.Root>
    <ConnectionStatusBadge />
  </div>
</header>
