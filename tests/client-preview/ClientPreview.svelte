<script lang="ts">
  import { onMount } from 'svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import OriginalOverview from 'virtual:original-overview';
  import OptimizedOverview from './OptimizedOverview.svelte';
  import CurrentOverview from '$lib/components/tabs/OverviewTab.svelte';
  import { Button } from '$lib/components/ui/button';
  import { NAV_TABS } from '$lib/constants/navigation';
  import { guiState, preview, store } from './state.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  overviewData.speedHistory = Array.from({ length: 120 }, (_, i) => ({ down: .5 + Math.sin(i / 8) * .3, up: .08 + Math.cos(i / 10) * .05 }));
  overviewData.applyTrafficRateSample({ sampledAtUnixMs: Date.now(), stable: true, uploadBytesPerSec: 80000, downloadBytesPerSec: 500000, totalUploadBytes: 12000000, totalDownloadBytes: 320000000, connectionCount: 26 });
  $effect(() => { document.documentElement.classList.toggle('dark', preview.dark); });
  onMount(() => {
    const timer = window.setInterval(() => {
      guiState.connectionUpdatedAt = Date.now();
      guiState.selfTestUpdatedAt = Date.now();
      overviewData.applyTrafficRateSample({ sampledAtUnixMs: Date.now(), stable: true, uploadBytesPerSec: 80000, downloadBytesPerSec: 500000, totalUploadBytes: 12000000, totalDownloadBytes: 320000000, connectionCount: 26 });
    }, 2000);
    return () => window.clearInterval(timer);
  });
</script>
<svelte:head><title>ZNet Sink · 概览优化预览</title></svelte:head>
<!-- Preserve +page.svelte's actual desktop shell. Only the process/data services are mocked. -->
<main class="w-screen flex flex-col select-none overflow-hidden transition-colors duration-200" style="height:calc(100vh - 29px); background:var(--background); color:var(--foreground); font-family:var(--font-sans, system-ui);">
  <TitleBar />
  <div class="flex-shrink-0 px-5 pt-2.5"><AppHeader /></div>
  <div class="flex-shrink-0 mx-5" style="height:1px; background:var(--border); opacity:.5;"></div>
  <div class="flex-1 min-h-0 px-3 sm:px-5 py-2 sm:py-3.5 flex flex-col overflow-hidden">
    {#if store.activeTab === 'overview'}
      {#if preview.view === 'optimized' && store.uiMode === 'pro'}<OptimizedOverview />{:else if preview.view === 'original' || store.uiMode === 'lite'}<OriginalOverview />{:else}<CurrentOverview />{/if}
    {:else}
      <div class="preview-page-notice"><strong>{NAV_TABS.find(tab => tab.id === store.activeTab)?.label}</strong><p>此预览仅加载概览，其他页面保持客户端现有实现。</p><Button size="sm" variant="outline" onclick={() => store.activeTab = 'overview'}>返回概览</Button></div>
    {/if}
  </div>
</main>
<footer class="preview-toolbar"><span>概览优化 · 模拟操作</span><select aria-label="对照版本" bind:value={preview.view} disabled={!!preview.pending}><option value="production">客户端实现</option><option value="optimized">确认的设计稿</option><option value="original">旧版概览（原样）</option></select><select aria-label="预览场景" value={preview.scenario} disabled={!!preview.pending} onchange={(event) => guiState.applyScenario(event.currentTarget.value)}><option value="normal">正常运行</option><option value="tun-failed">TUN 出口异常</option><option value="node-failed">策略出口失败</option><option value="operation-failed">切换失败</option></select><button data-slot="surface-button" onclick={() => preview.dark = !preview.dark}>切换主题</button><span role="status">{preview.feedback}</span></footer>
<style>
  :global(body) { margin:0; }.preview-toolbar { height:29px; display:flex; align-items:center; gap:12px; padding:0 12px; border-top:1px solid var(--border); background:var(--card); color:var(--muted-foreground); font-size:10px; overflow:hidden; white-space:nowrap; }.preview-toolbar select { max-width:180px; background:var(--card); color:var(--foreground); border:1px solid var(--border); border-radius:4px; font-size:10px; }.preview-toolbar button { cursor:pointer; }.preview-toolbar button:focus-visible { outline:2px solid var(--ring); }.preview-page-notice { margin:auto; text-align:center; font-size:13px; }.preview-page-notice p { color:var(--muted-foreground); margin:8px 0 15px; }
</style>
