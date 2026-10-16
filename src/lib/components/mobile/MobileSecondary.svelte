<script lang="ts">
  import { untrack, type Component } from 'svelte';
  import { provideMobileRoute, type MobilePage, type MobileTab } from '$lib/features/mobile/context';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  import MobileMenu from './MobileMenu.svelte';
  import MobileVersions from './MobileVersions.svelte';
  import MobileApplicationSettings from './MobileApplicationSettings.svelte';
  import { Button } from '$lib/components/ui/button';
  import UiErrorBoundary from '$lib/components/UiErrorBoundary.svelte';
  let { page, routeKey, tab, trafficVisit, locationTarget, origin }: { page: MobilePage; routeKey: number; tab: MobileTab; trafficVisit?: import('$lib/features/endpoints/traffic-navigation').EndpointTrafficVisit; locationTarget?: import('$lib/features/mobile/location').MobileLocationTarget; origin?: import('$lib/features/mobile/navigation.svelte').MobileRouteOrigin } = $props();
  // The keyed host route owns this scope even if its async panel loads while hidden.
  untrack(() => provideMobileRoute({ key: routeKey, page, tab, trafficVisit, locationTarget, origin }));
  type Loader = () => Promise<{ default: Component }>;
  const loaders: Partial<Record<MobilePage, Loader>> = {
    traffic: () => import('./MobileTraffic.svelte'),
    endpoints: () => import('$lib/components/tabs/EndpointsTab.svelte'),
    connections: () => import('$lib/components/tabs/ConnectionsTab.svelte'),
    packet: () => import('$lib/components/tabs/PacketRoutesPanel.svelte'),
    plugins: () => import('$lib/components/tabs/PluginsTab.svelte'),
    logs: () => import('$lib/components/tabs/LogsTab.svelte'),
    diagnostics: () => import('./MobileDiagnostics.svelte'),
    about: () => mobilePlatform.runtimePlatform !== 'desktop'
      ? import('./MobileAbout.svelte') : import('$lib/components/settings/AboutPanel.svelte'),
    tun: () => import('$lib/components/settings/TunSettingsPanel.svelte'),
    dns: () => import('$lib/components/settings/DnsSettingsPanel.svelte'),
    network: () => import('$lib/components/settings/RuntimeNetworkSettingsPanel.svelte'),
    probes: () => import('$lib/components/settings/UrlTestSettingsPanel.svelte'),
    workspace: () => import('./MobileWorkspace.svelte'),
    migration: () => import('./MobileSettingsMigration.svelte'),
    data: () => import('./MobileLocalData.svelte'),
    rules: () => import('$lib/components/tabs/RulesTab.svelte'),
    profiles: () => import('$lib/components/tabs/ProfilesTab.svelte'),
    nodes: () => import('$lib/components/tabs/NodesTab.svelte'),
    subscriptions: () => import('$lib/components/tabs/SubscriptionsTab.svelte'),
  };
  let ActivePanel = $state<Component | null>(null);
  let error = $state<string | null>(null);
  let generation = 0;
  async function load() {
    const current = ++generation;
    ActivePanel = null; error = null;
    try {
      const module = await loaders[page]?.();
      if (current === generation && module) ActivePanel = module.default;
    } catch (cause) { if (current === generation) error = cause instanceof Error ? cause.message : String(cause); }
  }
  $effect(() => {
    if (page === 'plugins' && mobilePlatform.runtimePlatform !== 'desktop') return;
    if (loaders[page]) void load();
  });
</script>
{#if page === 'settings'}
  <div class="settings-scroll"><MobileMenu settings /><MobileApplicationSettings /></div>
{:else if page === 'kernel' || page === 'updates'}
  <MobileVersions kernel={page === 'kernel'} />
{:else if page === 'plugins' && mobilePlatform.runtimePlatform !== 'desktop'}
  <div class="unavailable"><h2>移动插件执行待适配</h2><p>安装审阅、逐组件授权、管理页面和托管资源契约保留。移动执行容器、系统 IO、敏感材料加载与分发尚未验收，当前不能安装或运行组件。宿主导航与手动断开独立于插件。</p></div>
{:else if error}
  <div class="unavailable" role="alert"><p>页面加载失败：{error}</p><Button onclick={load}>重试</Button></div>
{:else if ActivePanel}
  <UiErrorBoundary context={{ tab: page, presentation: 'mobile' }}>{#if page === 'profiles' || page === 'nodes'}<ActivePanel mobile={true} {locationTarget}/>{:else}<ActivePanel />{/if}</UiErrorBoundary>
{:else}
  <p class="unavailable" role="status">正在加载…</p>
{/if}
<style>
  .unavailable { padding:16px 14px; color:var(--muted-foreground); overflow-wrap:anywhere; line-height:1.6; }
  h2 { font-size:1rem; font-weight:500; color:var(--foreground); }
  .settings-scroll { flex:1; min-height:0; overflow:auto; }
</style>
