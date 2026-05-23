<script lang="ts">
  import * as Tabs from "$lib/components/ui/tabs";
  import { store } from '$lib/services/store.svelte';
  import { NAV_TABS } from '$lib/constants/navigation';
  import ConnectionStatusBadge from '$lib/components/ConnectionStatusBadge.svelte';
</script>

<!--
  AppHeader: compact desktop toolbar navigation
  Center-aligned nav tabs, status badge on right
-->
<header class="w-full flex-shrink-0">
  <div class="w-full flex items-center justify-between" style="height: 38px;">

    <!-- Left spacer: flexible, shrinks when window is narrow -->
    <div class="flex-1 min-w-0 hidden sm:block" style="max-width: 120px;"></div>

    <!-- Center: Main navigation -->
    <nav class="flex items-center gap-0.5 overflow-x-auto flex-shrink-0 max-w-full" aria-label="主导航">
      {#each NAV_TABS as tab}
        {#if store.isNavVisible(tab.id)}
          <button
            onclick={() => store.isNavOperable(tab.id) && (store.activeTab = tab.id)}
            disabled={!store.isNavOperable(tab.id)}
            class="nav-tab-btn {store.activeTab === tab.id ? 'active' : ''} {!store.isNavOperable(tab.id) ? 'disabled' : ''}"
            aria-current={store.activeTab === tab.id ? 'page' : undefined}
          >
            {tab.label}
          </button>
        {/if}
      {/each}
    </nav>

    <!-- Right: Connection status, flexible on small windows -->
    <div class="flex items-center justify-end flex-shrink-0" style="min-width: 60px; max-width: 120px;">
      <ConnectionStatusBadge />
    </div>

  </div>
</header>

<style>
  .nav-tab-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 28px;
    padding: 0 11px;
    border-radius: 7px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.13s ease, color 0.13s ease;
    letter-spacing: -0.005em;
  }

  .nav-tab-btn:hover:not(.disabled) {
    background: var(--muted);
    color: var(--foreground);
  }

  .nav-tab-btn.active {
    background: var(--card);
    color: var(--foreground);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08), 0 0 0 0.5px rgba(0, 0, 0, 0.06);
  }

  :global(.dark) .nav-tab-btn.active {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3), 0 0 0 0.5px rgba(255, 255, 255, 0.08);
  }

  .nav-tab-btn.disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }
</style>
