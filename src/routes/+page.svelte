<script lang="ts">
  import { onMount } from 'svelte';
  import { store } from '$lib/services/store.svelte';
  import { initTheme, applyTheme } from '$lib/services/theme.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import TabContent from '$lib/components/TabContent.svelte';
  import WelcomeGuide from '$lib/components/WelcomeGuide.svelte';
  import type { ProxyNode } from '$lib/types/protocol';

  let speedHistory: { up: number; down: number }[] = $state(
    Array(45).fill(0).map(() => ({ up: Math.random() * 3, down: Math.random() * 8 }))
  );

  let nodes: ProxyNode[] = $state([
    { id: 'node-1', name: 'HK-Hysteria-01', protocol: 'ZNet', delay: 18, domain: 'zerodenet.org' },
    { id: 'node-2', name: 'SG-Tuic-02', protocol: 'ZNet', delay: 35, domain: 'zerodenet.org' },
    { id: 'node-3', name: 'US-Vless-03', protocol: 'ZNet', delay: 142, domain: 'zerodenet.org' }
  ]);

  onMount(() => {
    initTheme();
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', () => store.selectedTheme === 'system' && applyTheme('system'));

    const interval = setInterval(() => {
      if (!store.isInitialized) return;
      speedHistory.push({ up: Math.random() * 5, down: Math.random() * 15 });
      if (speedHistory.length > 45) speedHistory.shift();
      nodes.forEach(n => n.delay = Math.max(5, n.delay + (Math.floor(Math.random() * 4) - 2)));
    }, 1000);

    return () => clearInterval(interval);
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
        <TabContent {speedHistory} {nodes} />
      {/if}
    </div>
  </div>
</main>
