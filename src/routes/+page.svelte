<script lang="ts">
  import { onMount } from 'svelte';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { initTheme, applyTheme } from '$lib/services/theme.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import TabContent from '$lib/components/TabContent.svelte';
  import { WelcomeGuide } from '$lib/components/WelcomeGuide';
  import Toast from '$lib/components/Toast.svelte';

  onMount(() => {
    initTheme();
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const onSystemThemeChange = () => {
      if (store.selectedTheme === 'system') applyTheme('system');
    };
    mediaQuery.addEventListener('change', onSystemThemeChange);
    return () => mediaQuery.removeEventListener('change', onSystemThemeChange);
  });

  $effect(() => {
    if (store.isInitialized) {
      guiState.initialize();
      coreEvents.start();
    } else {
      guiState.destroy();
      coreEvents.stop();
    }
    return () => {
      guiState.destroy();
      coreEvents.stop();
    };
  });
</script>

<main class="h-screen w-screen bg-background text-foreground flex flex-col select-none font-sans text-xs overflow-hidden transition-colors duration-300">
  <TitleBar />

  <div class="flex-1 w-full p-4 flex flex-col gap-3 overflow-hidden">
    <AppHeader />

    <div class="flex-1 w-full overflow-hidden flex flex-col gap-3">
      {#if !store.isInitialized}
        <WelcomeGuide />
      {:else}
        <TabContent />
      {/if}
    </div>
  </div>

  <Toast />
</main>
