<script lang="ts">
  import { onMount } from 'svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import TabContent from '$lib/components/TabContent.svelte';
  import { store } from '$lib/services/store.svelte';
  import { NAV_TABS } from '$lib/constants/navigation';

  const params = new URLSearchParams(window.location.search);
  const preview = params.has('preview');
  store.uiMode = params.get('mode') === 'lite' ? 'lite' : 'pro';
  store.activeTab = store.uiMode === 'pro' && params.get('tab') === 'endpoints' ? 'endpoints' : 'nodes';
  store.interactionSurface.navigation = new Map(NAV_TABS.map(tab => [tab.id, {
    key: tab.id, visible: true, operable: true, readonly: false, category: 'navigation',
  }]));
  let catalogReads = $state(0);
  onMount(() => {
    const count = () => { catalogReads++; };
    window.addEventListener('fixture-endpoint-catalog-read', count);
    return () => window.removeEventListener('fixture-endpoint-catalog-read', count);
  });
</script>

<div class="flex h-full min-h-0 flex-col bg-background">
  {#if preview}<div class="preview-banner" role="status">交互预览 · 模拟数据 · 不连接真实内核，刷新页面恢复初始状态</div>{/if}
  <AppHeader />
  <div class="fixture-content"><TabContent tab={store.activeTab} /></div>
  <output class="sr-only" aria-label="端点目录查询次数">{catalogReads}</output>
</div>

<style>
  .preview-banner { padding: 6px 16px; color: var(--muted-foreground); background: var(--muted); font-size: 12px; text-align: center; }
  .fixture-content { flex: 1; min-width: 0; min-height: 0; display: flex; padding: 14px 20px; }
  @media (max-width: 640px) { .fixture-content { padding: 12px; } }
</style>
