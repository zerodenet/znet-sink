<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { NAV_TABS } from '$lib/constants/navigation';
  import Toast from '$lib/components/Toast.svelte';
  import * as Tabs from '$lib/components/AppTabs';
</script>

<!--
  AppHeader: compact desktop toolbar navigation
  Center-aligned nav tabs
-->
<header class="app-header w-full flex-shrink-0">
  <div class="w-full flex items-center justify-between" style="height: 38px;">

    <!-- Left spacer: flexible, shrinks when window is narrow -->
    <div class="flex-1 min-w-0 hidden sm:block" style="max-width: 120px;"></div>

    <!-- Center: Main navigation -->
    <nav class="app-nav flex items-center gap-0.5 flex-shrink-0 max-w-full" aria-label="主导航">
      <Tabs.Root bind:value={store.activeTab} class="app-nav-root">
        <Tabs.List surface="transparent" class="app-nav-list">
          {#each NAV_TABS as tab}
            {#if store.isNavVisible(tab.id)}
              <Tabs.Trigger
                value={tab.id}
                disabled={!store.isNavOperable(tab.id)}
                class="nav-tab-btn"
                aria-current={store.activeTab === tab.id ? 'page' : undefined}
                title={tab.comingSoon ? `${tab.label} - 敬请期待` : undefined}
              >
                <span>{tab.label}</span>
                {#if tab.comingSoon}
                  <span class="coming-soon-badge">敬请期待</span>
                {/if}
              </Tabs.Trigger>
            {/if}
          {/each}
        </Tabs.List>
      </Tabs.Root>
    </nav>

    <!-- Right spacer keeps nav centered against the titlebar controls above -->
    <div class="flex-1 min-w-0 hidden sm:block" style="max-width: 120px;" aria-hidden="true"></div>

  </div>
  <Toast />
</header>

<style>
  .app-header {
    position: relative;
  }

  .app-nav {
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .app-nav::-webkit-scrollbar {
    display: none;
  }

  :global(.app-nav-root) {
    gap: 0;
  }

  :global(.app-nav-list) {
    height: 32px;
    padding: 2px;
  }

  :global(.nav-tab-btn) {
    height: 28px;
    padding: 0 11px;
    font-size: 12.5px;
    letter-spacing: -0.005em;
  }

  :global(.coming-soon-badge) {
    margin-left: 4px;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 8px;
    font-weight: 600;
    line-height: 1.2;
  }
</style>
